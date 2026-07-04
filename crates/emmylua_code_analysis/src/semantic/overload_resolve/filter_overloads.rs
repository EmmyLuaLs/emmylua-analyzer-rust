use std::sync::Arc;

use emmylua_parser::LuaCallExpr;

use crate::{
    DbIndex, LuaFunctionType, LuaType, infer_call_generic,
    semantic::{LuaInferCache, infer::InferFailReason},
};

use super::{
    collect_overloads::collect_callable_overload_groups,
    resolve_signature_by_args::callable_accepts_args,
};

pub fn filter_callable_overloads(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    callable_type: &LuaType,
    call_arg_types: &[LuaType],
    call_expr: &LuaCallExpr,
    args_count: Option<usize>,
    return_instantiated_generic: bool,
) -> Result<Vec<Arc<LuaFunctionType>>, InferFailReason> {
    let mut overload_groups = Vec::new();
    collect_callable_overload_groups(db, callable_type, &mut overload_groups)?;

    Ok(overload_groups
        .into_iter()
        .flatten()
        .filter_map(|func| {
            match_callable_by_arg_types(
                db,
                cache,
                func,
                call_arg_types,
                call_expr,
                args_count,
                return_instantiated_generic,
            )
        })
        .collect())
}

pub fn find_callable_overload(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    callable_type: &LuaType,
    call_arg_types: &[LuaType],
    call_expr: &LuaCallExpr,
    args_count: Option<usize>,
    return_instantiated_generic: bool,
) -> Result<Option<Arc<LuaFunctionType>>, InferFailReason> {
    let mut overload_groups = Vec::new();
    collect_callable_overload_groups(db, callable_type, &mut overload_groups)?;

    Ok(overload_groups.into_iter().flatten().find_map(|func| {
        match_callable_by_arg_types(
            db,
            cache,
            func,
            call_arg_types,
            call_expr,
            args_count,
            return_instantiated_generic,
        )
    }))
}

pub(crate) fn match_callable_by_arg_types(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    func: Arc<LuaFunctionType>,
    call_arg_types: &[LuaType],
    call_expr: &LuaCallExpr,
    args_count: Option<usize>,
    return_instantiated_generic: bool,
) -> Option<Arc<LuaFunctionType>> {
    let has_tpls = func.contain_tpl();
    let match_func = if has_tpls {
        infer_call_generic(db, cache, func.as_ref(), call_expr.clone())
            .map(Arc::new)
            .unwrap_or_else(|_| func.clone())
    } else {
        func.clone()
    };

    if !callable_accepts_args(
        db,
        &match_func,
        call_arg_types,
        call_expr.is_colon_call(),
        args_count,
    ) {
        return None;
    }

    if has_tpls && return_instantiated_generic {
        Some(match_func)
    } else {
        Some(func)
    }
}
