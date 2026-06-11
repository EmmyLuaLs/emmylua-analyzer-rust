//! 函数调用推断 — `f(args)`, `obj:method(args)`

use emmylua_parser::{LuaAstNode, LuaCallExpr};
use smol_str::SmolStr;

use crate::{LuaIntersectionType, LuaType, LuaUnionType};

use super::{InferFailReason, InferQuery, InferResult};

pub(super) fn infer_call_expr(
    infer: &InferQuery,
    call_expr: LuaCallExpr,
) -> InferResult {
    let prefix_expr = call_expr.get_prefix_expr().ok_or(InferFailReason::None)?;
    let prefix_type = infer.infer_expr(prefix_expr)?;
    extract_return_type(infer, &prefix_type)
}

fn extract_return_type(infer: &InferQuery, prefix_type: &LuaType) -> InferResult {
    match prefix_type {
        LuaType::Function => Ok(LuaType::Any),

        LuaType::DocFunction(func) => Ok(func.get_ret().clone()),

        LuaType::Signature(sig_id) => {
            let db = infer.read_db();
            let explain = db.doc().signature().explain(infer.get_file_id(), sig_id.get_position());
            if let Some(e) = &explain {
                if let Some(ret) = e.returns.first() {
                    if let Some(item) = ret.items.first() {
                        if let Some(lt) = item.doc_type.lowered.as_ref() {
                            if let Some(ty) = super::lowered_node_to_lua_type(lt) {
                                return Ok(ty);
                            }
                        }
                    }
                }
            }
            Err(InferFailReason::NotImplemented)
        }

        LuaType::Union(u) => {
            let mut types = Vec::new();
            for sub in u.into_vec() {
                if let Ok(t) = extract_return_type(infer, &sub) {
                    if !t.is_unknown() { types.push(t); }
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
            let results: Vec<LuaType> = inter.get_types().iter()
                .filter_map(|t| extract_return_type(infer, t).ok())
                .collect();
            match results.len() {
                0 => Ok(LuaType::Unknown),
                1 => Ok(results.into_iter().next().expect("len checked")),
                _ => Ok(LuaType::Intersection(LuaIntersectionType::new(results).into())),
            }
        }

        _ => Err(InferFailReason::NotImplemented),
    }
}
