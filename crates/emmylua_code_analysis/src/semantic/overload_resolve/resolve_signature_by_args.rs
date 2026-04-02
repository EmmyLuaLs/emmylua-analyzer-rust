use std::sync::Arc;

use crate::{
    InferFailReason, check_type_compact,
    db_index::{DbIndex, LuaFunctionType, LuaType},
    semantic::infer::InferCallFuncResult,
};

pub(crate) fn callable_accepts_args(
    db: &DbIndex,
    func: &LuaFunctionType,
    expr_types: &[LuaType],
    is_colon_call: bool,
    arg_count: Option<usize>,
) -> bool {
    let arg_count = arg_count.unwrap_or(expr_types.len());
    if func.get_params().len() < arg_count && !is_func_last_param_variadic(func) {
        return false;
    }

    for (arg_index, expr_type) in expr_types.iter().enumerate() {
        let param_type = match get_call_arg_param(func, arg_index, is_colon_call) {
            CallArgParam::Skip => continue,
            CallArgParam::Present { param_type, .. } => param_type,
            CallArgParam::Missing => return false,
        };

        if !param_type.is_any() && check_type_compact(db, &param_type, expr_type).is_err() {
            return false;
        }
    }

    true
}

pub fn resolve_signature_by_args(
    db: &DbIndex,
    overloads: &[Arc<LuaFunctionType>],
    expr_types: &[LuaType],
    is_colon_call: bool,
    arg_count: Option<usize>,
) -> InferCallFuncResult {
    let expr_len = expr_types.len();
    let arg_count = arg_count.unwrap_or(expr_len);
    let mut need_resolve_funcs = match overloads.len() {
        0 => return Err(InferFailReason::None),
        1 => return Ok(Arc::clone(&overloads[0])),
        _ => overloads
            .iter()
            .map(|it| Some(it.clone()))
            .collect::<Vec<_>>(),
    };

    if expr_len == 0 {
        for overload in overloads {
            let param_len = overload.get_params().len();
            if param_len == 0 {
                return Ok(overload.clone());
            }
        }
    }

    let mut best_match_result = need_resolve_funcs[0]
        .clone()
        .expect("Match result should exist");
    for (arg_index, expr_type) in expr_types.iter().enumerate() {
        let mut current_match_result = ParamMatchResult::Not;
        for opt_func in &mut need_resolve_funcs {
            let func = match opt_func.as_ref() {
                None => continue,
                Some(func) => func,
            };
            if func.get_params().len() < arg_count && !is_func_last_param_variadic(func) {
                *opt_func = None;
                continue;
            }

            let (param_index, param_type) = match get_call_arg_param(func, arg_index, is_colon_call)
            {
                CallArgParam::Skip => continue,
                CallArgParam::Present {
                    param_index,
                    param_type,
                } => (param_index, param_type),
                CallArgParam::Missing => {
                    *opt_func = None;
                    continue;
                }
            };

            let match_result = if param_type.is_any() {
                ParamMatchResult::Any
            } else if check_type_compact(db, &param_type, expr_type).is_ok() {
                ParamMatchResult::Type
            } else {
                ParamMatchResult::Not
            };

            if match_result > current_match_result {
                current_match_result = match_result;
                best_match_result = func.clone();
            }

            if match_result == ParamMatchResult::Not {
                *opt_func = None;
                continue;
            }

            if match_result > ParamMatchResult::Any
                && arg_index + 1 == expr_len
                && param_index + 1 == func.get_params().len()
            {
                return Ok(func.clone());
            }
        }

        if current_match_result == ParamMatchResult::Not {
            break;
        }
    }

    let mut rest_need_resolve_funcs = need_resolve_funcs
        .iter()
        .filter_map(|it| it.clone())
        .map(Some)
        .collect::<Vec<_>>();

    let rest_len = rest_need_resolve_funcs.len();
    match rest_len {
        0 => return Ok(best_match_result),
        1 => {
            return Ok(rest_need_resolve_funcs[0]
                .clone()
                .expect("Resolve function should exist"));
        }
        _ => {}
    }

    let start_param_index = expr_len;
    let mut max_param_len = 0;
    for func in rest_need_resolve_funcs.iter().flatten() {
        let param_len = func.get_params().len();
        if param_len > max_param_len {
            max_param_len = param_len;
        }
    }

    for param_index in start_param_index..max_param_len {
        let mut current_match_result = ParamMatchResult::Not;
        for (i, opt_func) in rest_need_resolve_funcs.iter_mut().enumerate() {
            let func = match opt_func.as_ref() {
                None => continue,
                Some(func) => func,
            };
            let (param_index, param_type) =
                match get_call_arg_param(func, param_index, is_colon_call) {
                    CallArgParam::Skip => continue,
                    CallArgParam::Present {
                        param_index,
                        param_type,
                    } => (param_index, param_type),
                    CallArgParam::Missing => return Ok(func.clone()),
                };

            let match_result = if param_type.is_any() {
                ParamMatchResult::Any
            } else if param_type.is_nullable() {
                ParamMatchResult::Type
            } else {
                ParamMatchResult::Not
            };

            if match_result > current_match_result {
                current_match_result = match_result;
                best_match_result = func.clone();
            }

            if match_result == ParamMatchResult::Not {
                *opt_func = None;
                continue;
            }

            if match_result >= ParamMatchResult::Any
                && i + 1 == rest_len
                && param_index + 1 == func.get_params().len()
            {
                return Ok(func.clone());
            }
        }

        if current_match_result == ParamMatchResult::Not {
            break;
        }
    }

    Ok(best_match_result)
}

fn get_call_arg_param(
    func: &LuaFunctionType,
    arg_index: usize,
    is_colon_call: bool,
) -> CallArgParam {
    let mut param_index = arg_index;
    match (func.is_colon_define(), is_colon_call) {
        (true, false) => {
            if param_index == 0 {
                return CallArgParam::Skip;
            }
            param_index -= 1;
        }
        (false, true) => {
            param_index += 1;
        }
        _ => {}
    }

    if let Some((_, ty)) = func.get_params().get(param_index) {
        return CallArgParam::Present {
            param_index,
            param_type: ty.clone().unwrap_or(LuaType::Any),
        };
    }

    if let Some((name, ty)) = func.get_params().last()
        && name == "..."
    {
        return CallArgParam::Present {
            param_index,
            param_type: ty.clone().unwrap_or(LuaType::Any),
        };
    }

    CallArgParam::Missing
}

fn is_func_last_param_variadic(func: &LuaFunctionType) -> bool {
    if let Some(last_param) = func.get_params().last() {
        last_param.0 == "..."
    } else {
        false
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum ParamMatchResult {
    Not,
    Any,
    Type,
}

enum CallArgParam {
    Skip,
    Present {
        param_index: usize,
        param_type: LuaType,
    },
    Missing,
}
