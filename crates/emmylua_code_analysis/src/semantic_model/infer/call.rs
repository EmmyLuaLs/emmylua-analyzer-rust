//! 函数调用推断 — `f(args)`, `obj:method(args)`

use emmylua_parser::LuaCallExpr;

use crate::semantic_model::signature::SignatureInfo;
use crate::{LuaIntersectionType, LuaType, LuaUnionType};

use super::{InferFailReason, InferQuery, InferResult};

pub(super) fn infer_call_expr(infer: &InferQuery, call_expr: LuaCallExpr) -> InferResult {
    let prefix_expr = call_expr.get_prefix_expr().ok_or(InferFailReason::None)?;
    let prefix_type = infer.infer_expr(prefix_expr)?;

    if let Ok(ty) = extract_return_type(infer, &prefix_type) {
        return Ok(ty);
    }

    if let Some(call_info) = infer.infer_call_expr_func(call_expr, None) {
        return Ok(call_info.return_type);
    }

    Err(InferFailReason::NotImplemented)
}

fn extract_return_type(infer: &InferQuery, prefix_type: &LuaType) -> InferResult {
    match prefix_type {
        LuaType::Function => Ok(LuaType::Any),
        LuaType::DocFunction(func) => Ok(func.get_ret().clone()),
        // Use SignatureInfo if available; fall through to Path B on failure
        // (Path B has resolve_call_info with generic substitution)
        LuaType::Signature(sig_id) => {
            let db = infer.read_db();
            if let Some(info) =
                SignatureInfo::query(&db, infer.get_file_id(), sig_id.get_position())
            {
                let ret = info.return_type();
                if !ret.is_unknown() {
                    return Ok(ret);
                }
            }
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
