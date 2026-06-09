//! 函数调用推断 — `f(args)`, `obj:method(args)`

use emmylua_parser::{LuaAstNode, LuaCallExpr};

use crate::{LuaIntersectionType, LuaType, LuaUnionType};

use super::{InferFailReason, InferQuery, InferResult};

pub(super) fn infer_call_expr(
    infer: &InferQuery,
    call_expr: LuaCallExpr,
) -> InferResult {
    let prefix_expr = call_expr.get_prefix_expr().ok_or(InferFailReason::None)?;
    let prefix_type = infer.infer_expr(prefix_expr)?;
    extract_return_type(&prefix_type)
}

fn extract_return_type(prefix_type: &LuaType) -> InferResult {
    match prefix_type {
        LuaType::Function => Ok(LuaType::Any),

        LuaType::DocFunction(func) => Ok(func.get_ret().clone()),

        LuaType::Signature(_sig_id) => Err(InferFailReason::NotImplemented),

        LuaType::Union(union_type) => {
            let mut return_types = Vec::new();
            for sub in union_type.into_vec() {
                if let Ok(ty) = extract_return_type(&sub) {
                    if !ty.is_unknown() {
                        return_types.push(ty);
                    }
                }
            }
            match return_types.len() {
                0 => Ok(LuaType::Unknown),
                1 => Ok(return_types.into_iter().next().expect("len checked above")),
                _ => Ok(LuaType::Union(LuaUnionType::from_vec(return_types).into())),
            }
        }

        LuaType::Generic(generic) => {
            let base = generic.get_base_type();
            extract_return_type(&base)
        }

        LuaType::Intersection(intersection) => {
            let results: Vec<LuaType> = intersection
                .get_types()
                .iter()
                .filter_map(|ty| extract_return_type(ty).ok())
                .collect();
            match results.len() {
                0 => Ok(LuaType::Unknown),
                1 => Ok(results.into_iter().next().expect("len checked above")),
                _ => Ok(LuaType::Intersection(
                    LuaIntersectionType::new(results).into(),
                )),
            }
        }

        _ => Err(InferFailReason::NotImplemented),
    }
}
