use std::{ops::Deref, sync::Arc};

use emmylua_parser::{LuaCallExpr, LuaExpr};

use crate::{
    VariadicType,
    db_index::{DbIndex, LuaFunctionType, LuaType},
    infer_expr,
};

use super::{
    LuaInferCache,
    generic::instantiate_func_generic,
    infer::{InferCallFuncResult, InferFailReason},
    type_check::check_type_compact,
};

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

fn resolve_signature_by_args(
    db: &DbIndex,
    overloads: &[Arc<LuaFunctionType>],
    expr_types: &[LuaType],
    is_colon_call: bool,
    arg_count: Option<usize>,
) -> InferCallFuncResult {
    let arg_count = arg_count.unwrap_or(0);
    let mut opt_funcs = Vec::with_capacity(overloads.len());

    for func in overloads {
        let params = func.get_params();
        if params.len() < arg_count {
            continue;
        }
        let mut total_weight = 0; // 总权重

        let mut fake_expr_len = expr_types.len();
        let jump_param;
        if is_colon_call && !func.is_colon_define() {
            jump_param = 1;
            fake_expr_len += 1;
        } else {
            jump_param = 0;
        };

        // 如果在不计算可空参数的情况下, 参数数量完全匹配, 则认为其权重更高
        if params.len() == fake_expr_len {
            total_weight += params.len() as i32 + 1;
        }

        // 冒号定义且冒号调用
        if is_colon_call && func.is_colon_define() {
            total_weight += 100;
        }

        // 检查每个参数的匹配情况
        for (i, param) in params.iter().enumerate() {
            if i == 0 && jump_param > 0 {
                // 非冒号定义但是冒号调用, 直接认为匹配
                total_weight += 100;
                continue;
            }
            let param_type = param.1.as_ref().unwrap_or(&LuaType::Any);
            let expr_idx = i - jump_param;

            if expr_idx >= expr_types.len() {
                // 没有传入参数, 但参数是可空类型
                if param_type.is_nullable() {
                    total_weight += 1;
                    fake_expr_len += 1;
                }
                continue;
            }

            let expr_type = &expr_types[expr_idx];
            if *param_type == LuaType::Any || check_type_compact(db, param_type, expr_type).is_ok()
            {
                total_weight += 100; // 类型完全匹配
            }
        }
        // 如果参数数量完全匹配, 则认为其权重更高
        if params.len() == fake_expr_len {
            total_weight += 50000;
        }

        opt_funcs.push((func, total_weight));
    }

    // 按权重降序排序
    opt_funcs.sort_by(|a, b| b.1.cmp(&a.1));
    // 返回权重最高的签名，若无则取最后一个重载作为默认
    opt_funcs
        .first()
        .filter(|(_, weight)| *weight > i32::MIN) // 确保不是无效签名
        .map(|(func, _)| Arc::clone(func))
        .or_else(|| overloads.last().cloned())
        .ok_or(InferFailReason::None)
}

fn infer_expr_list_types(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    exprs: &[LuaExpr],
    var_count: Option<usize>,
) -> Vec<LuaType> {
    let mut value_types = Vec::new();
    for (idx, expr) in exprs.iter().enumerate() {
        let expr_type = infer_expr(db, cache, expr.clone()).unwrap_or(LuaType::Unknown);
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
