use std::cmp::Ordering;
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
        let Some(param_type) = get_func_param_type(func, param_index) else {
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
            let Some(param_type) = get_func_param_type(func, param_index) else {
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

    if !has_declined_no_flow_arg
        && let Some(func) = choose_more_specific_callable(
            db,
            &rest_need_resolve_funcs,
            expr_types,
            is_colon_call,
            declined_no_flow_args,
        )
    {
        return Ok(func);
    }

    let start_param_index = expr_len;
    let mut max_param_len = 0;
    for func in rest_need_resolve_funcs.iter().flatten() {
        let mut param_len = func.get_params().len();
        if func
            .get_params()
            .last()
            .is_some_and(|last_param| last_param.0 == "...")
        {
            param_len = param_len.saturating_sub(1);
        }
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

    if remaining_funcs.all(|func| func.get_return_row() == first.get_return_row()) {
        Ok(first)
    } else {
        Err(InferFailReason::None)
    }
}

fn choose_more_specific_callable(
    db: &DbIndex,
    funcs: &[Option<Arc<LuaFunctionType>>],
    expr_types: &[LuaType],
    is_colon_call: bool,
    declined_no_flow_args: &[bool],
) -> Option<Arc<LuaFunctionType>> {
    if expr_types.is_empty()
        || expr_types.iter().enumerate().all(|(i, expr_type)| {
            declined_no_flow_args.get(i).copied().unwrap_or(false)
                || expr_type.is_any()
                || expr_type.is_unknown()
        })
    {
        return None;
    }

    let mut best: Option<Arc<LuaFunctionType>> = None;
    let mut has_strict_better = false;
    for func in funcs.iter().flatten() {
        let Some(best_func) = best.as_ref() else {
            best = Some(func.clone());
            continue;
        };

        match compare_callable_specificity(
            db,
            func,
            best_func,
            expr_types,
            is_colon_call,
            declined_no_flow_args,
        ) {
            Some(Ordering::Greater) => {
                best = Some(func.clone());
                has_strict_better = true;
            }
            Some(Ordering::Less) => {
                has_strict_better = true;
            }
            Some(Ordering::Equal) => {}
            None => return None,
        }
    }

    if has_strict_better { best } else { None }
}

fn compare_callable_specificity(
    db: &DbIndex,
    a: &LuaFunctionType,
    b: &LuaFunctionType,
    expr_types: &[LuaType],
    is_colon_call: bool,
    declined_no_flow_args: &[bool],
) -> Option<Ordering> {
    let mut result = Ordering::Equal;
    for (arg_index, expr_type) in expr_types.iter().enumerate() {
        if declined_no_flow_args
            .get(arg_index)
            .copied()
            .unwrap_or(false)
            || expr_type.is_any()
            || expr_type.is_unknown()
        {
            continue;
        }

        let param_index = get_call_param_index(a, arg_index, is_colon_call)?;
        let a_param = get_func_param_type(a, param_index)?;
        let b_param = get_func_param_type(b, param_index)?;
        let param_order = compare_param_specificity(db, &a_param, &b_param, expr_type);
        match (result, param_order) {
            (Ordering::Equal, order) => result = order,
            (Ordering::Greater, Ordering::Less) | (Ordering::Less, Ordering::Greater) => {
                return None;
            }
            _ => {}
        }
    }

    Some(result)
}

fn compare_param_specificity(
    db: &DbIndex,
    a: &LuaType,
    b: &LuaType,
    expr_type: &LuaType,
) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }

    // 字面量实参直接命中对应 overload 时, 该 overload 比基础类型主签名更具体.
    match (expr_type, a, b) {
        (
            LuaType::IntegerConst(expr) | LuaType::DocIntegerConst(expr),
            LuaType::DocIntegerConst(a),
            LuaType::Integer | LuaType::Number,
        ) if expr == a => return Ordering::Greater,
        (
            LuaType::IntegerConst(expr) | LuaType::DocIntegerConst(expr),
            LuaType::Integer | LuaType::Number,
            LuaType::DocIntegerConst(b),
        ) if expr == b => return Ordering::Less,
        (
            LuaType::StringConst(expr) | LuaType::DocStringConst(expr),
            LuaType::DocStringConst(a),
            LuaType::String,
        ) if expr == a => return Ordering::Greater,
        (
            LuaType::StringConst(expr) | LuaType::DocStringConst(expr),
            LuaType::String,
            LuaType::DocStringConst(b),
        ) if expr == b => return Ordering::Less,
        (
            LuaType::BooleanConst(expr) | LuaType::DocBooleanConst(expr),
            LuaType::DocBooleanConst(a),
            LuaType::Boolean,
        ) if expr == a => return Ordering::Greater,
        (
            LuaType::BooleanConst(expr) | LuaType::DocBooleanConst(expr),
            LuaType::Boolean,
            LuaType::DocBooleanConst(b),
        ) if expr == b => return Ordering::Less,
        _ => {}
    }

    match (a.is_any() || a.is_unknown(), b.is_any() || b.is_unknown()) {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }

    let a_sub_b = check_type_compact(db, b, a).is_ok();
    let b_sub_a = check_type_compact(db, a, b).is_ok();
    match (a_sub_b, b_sub_a) {
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => Ordering::Equal,
    }
}

pub(crate) fn is_func_last_param_variadic(func: &LuaFunctionType) -> bool {
    if let Some(last_param) = func.get_params().last() {
        last_param.0 == "..."
    } else {
        false
    }
}

pub(crate) fn get_call_param_index(
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

pub(crate) fn get_func_param_type(func: &LuaFunctionType, param_index: usize) -> Option<LuaType> {
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
