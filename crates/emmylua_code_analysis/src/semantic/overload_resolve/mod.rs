mod resolve_signature_by_args;

use std::{ops::Deref, sync::Arc};

use emmylua_parser::{LuaCallExpr, LuaExpr};

use crate::{
    VariadicType,
    db_index::{DbIndex, LuaFunctionType, LuaType},
    infer_expr_root,
};

use super::{
    LuaInferCache,
    generic::instantiate_func_generic,
    infer::{InferCallFuncResult, InferFailReason},
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
    let expr_types = infer_expr_list_types(
        db,
        cache,
        args.get_args().collect::<Vec<_>>().as_slice(),
        arg_count,
    );
    if is_generic {
        resolve_signature_by_generic(db, cache, overloads, call_expr, expr_types, arg_count)
    } else {
        resolve_signature_by_args(
            db,
            &overloads,
            &expr_types,
            call_expr.is_colon_call(),
            arg_count,
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
    )
}

fn infer_expr_list_types(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    exprs: &[LuaExpr],
    var_count: Option<usize>,
) -> Vec<LuaType> {
    let mut value_types = Vec::new();
    for (idx, expr) in exprs.iter().enumerate() {
        let expr_type = infer_expr_root(db, cache, expr.clone()).unwrap_or(LuaType::Unknown);
        match expr_type {
            LuaType::Variadic(variadic) => {
                if let Some(var_count) = var_count {
                    if idx < var_count {
                        for i in idx..var_count {
                            if let Some(typ) = variadic.get_type(i - idx) {
                                value_types.push(typ.clone());
                            } else {
                                break;
                            }
                        }
                    }
                } else {
                    match variadic.deref() {
                        VariadicType::Base(base) => {
                            value_types.push(base.clone());
                        }
                        VariadicType::Multi(vecs) => {
                            for typ in vecs {
                                value_types.push(typ.clone());
                            }
                        }
                    }
                }

                break;
            }
            _ => value_types.push(expr_type.clone()),
        }
    }

    value_types
}
