mod resolve_signature_by_args;

use std::sync::Arc;

use emmylua_parser::LuaCallExpr;

use crate::db_index::{DbIndex, LuaFunctionType, LuaType};

use super::{
    LuaInferCache,
    generic::instantiate_func_generic,
    infer::{InferCallFuncResult, InferFailReason, infer_expr_list_types, try_infer_expr_no_flow},
};

use resolve_signature_by_args::resolve_signature_by_args;

pub fn resolve_signature(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    overloads: Vec<Arc<LuaFunctionType>>,
    call_expr: LuaCallExpr,
    is_generic: bool,
    arg_count: Option<usize>,
) -> InferCallFuncResult {
    let args = call_expr.get_args_list().ok_or(InferFailReason::None)?;
    let mut has_unknown_no_flow_arg = false;
    let expr_types: Vec<_> = infer_expr_list_types(
        db,
        cache,
        args.get_args().collect::<Vec<_>>().as_slice(),
        arg_count,
        |db, cache, expr| {
            if cache.is_no_flow() {
                let Some(expr_type) = try_infer_expr_no_flow(db, cache, expr)? else {
                    if !is_generic {
                        has_unknown_no_flow_arg = true;
                        return Ok(LuaType::Unknown);
                    }
                    return Err(InferFailReason::None);
                };
                Ok(expr_type)
            } else {
                Ok(crate::infer_expr(db, cache, expr).unwrap_or(LuaType::Unknown))
            }
        },
    )?
    .into_iter()
    .map(|(ty, _)| ty)
    .collect();

    if is_generic {
        resolve_signature_by_generic(db, cache, overloads, call_expr, expr_types, arg_count)
    } else {
        resolve_signature_by_args(
            db,
            &overloads,
            &expr_types,
            call_expr.is_colon_call(),
            arg_count,
            has_unknown_no_flow_arg,
        )
    }
}

fn resolve_signature_by_generic(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    overloads: Vec<Arc<LuaFunctionType>>,
    call_expr: LuaCallExpr,
    expr_types: Vec<LuaType>,
    arg_count: Option<usize>,
) -> InferCallFuncResult {
    let mut instantiate_funcs = Vec::new();
    for func in overloads {
        let instantiate_func = instantiate_func_generic(db, cache, &func, call_expr.clone())?;
        instantiate_funcs.push(Arc::new(instantiate_func));
    }
    resolve_signature_by_args(
        db,
        &instantiate_funcs,
        &expr_types,
        call_expr.is_colon_call(),
        arg_count,
        false,
    )
}
