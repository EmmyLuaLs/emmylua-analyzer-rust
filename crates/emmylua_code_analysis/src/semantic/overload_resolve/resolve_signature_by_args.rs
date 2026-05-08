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
        let Some(param_index) = get_call_param_index(func, arg_index, is_colon_call) else {
            continue;
        };
        let Some(param_type) = get_call_arg_param_type(func, param_index) else {
            return false;
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
    declined_no_flow_args: &[bool],
) -> InferCallFuncResult {
    let expr_len = expr_types.len();
    let arg_count = arg_count.unwrap_or(expr_len);
    let has_declined_no_flow_arg = declined_no_flow_args.iter().any(|declined| *declined);
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
        let declined_no_flow_arg = declined_no_flow_args
            .get(arg_index)
            .copied()
            .unwrap_or(false);
        for opt_func in &mut need_resolve_funcs {
            let func = match opt_func.as_ref() {
                None => continue,
                Some(func) => func,
            };
            let param_len = func.get_params().len();
            if param_len < arg_count && !is_func_last_param_variadic(func) {
                *opt_func = None;
                continue;
            }

            let Some(param_index) = get_call_param_index(func, arg_index, is_colon_call) else {
                continue;
            };
            let Some(param_type) = get_call_arg_param_type(func, param_index) else {
                *opt_func = None;
                continue;
            };

            let match_result = if declined_no_flow_arg && expr_type.is_unknown() {
                // Declined no-flow args are compatible with any overload, but they do
                // not prove that a specific row won.
                ParamMatchResult::Any
            } else if param_type.is_any() {
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

            if !has_declined_no_flow_arg
                && match_result > ParamMatchResult::Any
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
        0 => {
            if has_declined_no_flow_arg {
                return Err(InferFailReason::None);
            }
            return Ok(best_match_result);
        }
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
            let param_len = func.get_params().len();
            let Some(param_index) = get_call_param_index(func, param_index, is_colon_call) else {
                continue;
            };
            let match_result = if param_index >= param_len {
                if func
                    .get_params()
                    .last()
                    .is_some_and(|last_param_info| last_param_info.0 == "...")
                {
                    ParamMatchResult::Any
                } else if has_declined_no_flow_arg {
                    ParamMatchResult::Type
                } else {
                    return Ok(func.clone());
                }
            } else {
                let param_info = func
                    .get_params()
                    .get(param_index)
                    .expect("Param index should exist");
                let param_type = param_info.1.clone().unwrap_or(LuaType::Any);
                if param_type.is_any() {
                    ParamMatchResult::Any
                } else if param_type.is_nullable() {
                    ParamMatchResult::Type
                } else {
                    ParamMatchResult::Not
                }
            };

            if match_result > current_match_result {
                current_match_result = match_result;
                best_match_result = func.clone();
            }

            if match_result == ParamMatchResult::Not {
                *opt_func = None;
                continue;
            }

            if !has_declined_no_flow_arg
                && match_result >= ParamMatchResult::Any
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

    if !has_declined_no_flow_arg {
        return Ok(best_match_result);
    }

    let mut remaining_funcs = rest_need_resolve_funcs.into_iter().flatten();
    let Some(first) = remaining_funcs.next() else {
        return Err(InferFailReason::None);
    };

    if remaining_funcs.all(|func| func.get_ret() == first.get_ret()) {
        Ok(first)
    } else {
        Err(InferFailReason::None)
    }
}

fn is_func_last_param_variadic(func: &LuaFunctionType) -> bool {
    if let Some(last_param) = func.get_params().last() {
        last_param.0 == "..."
    } else {
        false
    }
}

fn get_call_param_index(
    func: &LuaFunctionType,
    arg_index: usize,
    is_colon_call: bool,
) -> Option<usize> {
    let mut param_index = arg_index;
    match (func.is_colon_define(), is_colon_call) {
        (true, false) => {
            if param_index == 0 {
                return None;
            }
            param_index -= 1;
        }
        (false, true) => {
            param_index += 1;
        }
        _ => {}
    }
    Some(param_index)
}

fn get_call_arg_param_type(func: &LuaFunctionType, param_index: usize) -> Option<LuaType> {
    if let Some(param_info) = func.get_params().get(param_index) {
        return Some(param_info.1.clone().unwrap_or(LuaType::Any));
    }

    let last_param_info = func.get_params().last()?;
    if last_param_info.0 == "..." {
        Some(last_param_info.1.clone().unwrap_or(LuaType::Any))
    } else {
        None
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum ParamMatchResult {
    Not,
    Any,
    Type,
}
