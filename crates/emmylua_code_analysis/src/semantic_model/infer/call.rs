//! 函数调用推断 — `f(args)`, `obj:method(args)`

use emmylua_parser::LuaCallExpr;

use crate::{LuaIntersectionType, LuaType, LuaUnionType};

use super::{InferFailReason, InferQuery, InferResult};

pub(super) fn infer_call_expr(infer: &InferQuery, call_expr: LuaCallExpr) -> InferResult {
    let prefix_expr = call_expr.get_prefix_expr().ok_or(InferFailReason::None)?;
    let prefix_type = infer.infer_expr(prefix_expr)?;

    // Path A: 基础返回类型提取
    if let Ok(ty) = extract_return_type(infer, &prefix_type) {
        return Ok(ty);
    }

    // Path B: 通过完整签名解析获取返回类型（处理 Ref/Def/Alias 等 extract_return_type 未覆盖的情况）
    if let Some(call_info) = infer.infer_call_expr_func(call_expr, None) {
        return Ok(call_info.return_type);
    }

    Err(InferFailReason::NotImplemented)
}

fn extract_return_type(infer: &InferQuery, prefix_type: &LuaType) -> InferResult {
    match prefix_type {
        LuaType::Function => Ok(LuaType::Any),

        LuaType::DocFunction(func) => Ok(func.get_ret().clone()),

        LuaType::Signature(_sig_id) => {
            // Generic substitution requires the full call_explain pipeline.
            // Defer to Path B which uses resolve_call_info.
            Err(InferFailReason::NotImplemented)
        }

        LuaType::Union(u) => {
            let mut types = Vec::new();
            for sub in u.into_vec() {
                if let Ok(t) = extract_return_type(infer, &sub) {
                    if !t.is_unknown() {
                        types.push(t);
                    }
                }
            }
            match types.len() {
                0 => Ok(LuaType::Unknown),
                1 => Ok(types.into_iter().next().expect("len checked")),
                _ => Ok(LuaType::Union(LuaUnionType::from_vec(types).into())),
            }
        }

        LuaType::Generic(g) => extract_return_type(infer, &g.get_base_type()),

        LuaType::Intersection(inter) => {
            let results: Vec<LuaType> = inter
                .get_types()
                .iter()
                .filter_map(|t| extract_return_type(infer, t).ok())
                .collect();
            match results.len() {
                0 => Ok(LuaType::Unknown),
                1 => Ok(results.into_iter().next().expect("len checked")),
                _ => Ok(LuaType::Intersection(
                    LuaIntersectionType::new(results).into(),
                )),
            }
        }

        _ => Err(InferFailReason::NotImplemented),
    }
}
