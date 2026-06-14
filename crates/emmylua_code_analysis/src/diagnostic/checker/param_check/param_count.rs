use std::sync::Arc;

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaCallExpr, LuaClosureExpr, LuaExpr, LuaGeneralToken,
};

use crate::{DiagnosticCode, LuaFunctionType, LuaSignatureId, LuaType, SemanticModel};

use super::super::DiagnosticContext;
use super::call_facts::{
    CallFacts, adjusted_params, count_ranges_overlap, get_param_count_range, is_dots_expr,
    is_nullable,
};

pub(super) fn check_call_param_count(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    facts: &CallFacts,
) -> Option<()> {
    let mut missing_candidates = Vec::new();
    let mut redundant_candidates = Vec::new();
    let mut arg_types = None;

    for func in facts.funcs() {
        let Some(call_count) = facts.call_arg_count_range(semantic_model, func) else {
            return Some(());
        };
        let param_count = get_param_count_range(context.get_db(), &facts.call_expr, func);

        if count_ranges_overlap(call_count, param_count) {
            let arg_types = arg_types.get_or_insert_with(|| facts.arg_types(semantic_model));
            if semantic_model.callable_accepts_args(
                func,
                arg_types,
                facts.call_expr.is_colon_call(),
                None,
            ) {
                return Some(());
            }

            continue;
        }

        if let Some(max_call_count) = call_count.max
            && max_call_count < param_count.min
        {
            missing_candidates.push((
                param_count.min - max_call_count,
                param_count.min,
                func.clone(),
            ));
            continue;
        }

        if let Some(max_param_count) = param_count.max
            && call_count.min > max_param_count
        {
            redundant_candidates.push((
                call_count.min - max_param_count,
                max_param_count,
                func.clone(),
            ));
        }
    }

    match (
        missing_candidates.is_empty(),
        redundant_candidates.is_empty(),
    ) {
        (false, true) => {
            if let Some((_, _, func)) =
                missing_candidates
                    .into_iter()
                    .min_by_key(|(missing_count, min_param_count, _)| {
                        (*missing_count, *min_param_count)
                    })
            {
                emit_missing_parameter(
                    context,
                    semantic_model,
                    &facts.call_expr,
                    &facts.arg_exprs,
                    &func,
                );
            }
        }
        (true, false) => {
            if let Some((_, _, func)) = redundant_candidates
                .into_iter()
                .max_by_key(|(_, max_param_count, _)| *max_param_count)
            {
                emit_redundant_parameter(
                    context,
                    semantic_model,
                    &facts.call_expr,
                    &facts.arg_exprs,
                    &func,
                );
            }
        }
        _ => {}
    }

    Some(())
}

fn emit_missing_parameter(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
    call_args: &[LuaExpr],
    func: &Arc<LuaFunctionType>,
) -> Option<()> {
    let fake_params = adjusted_params(call_expr, func);
    let mut call_args_count = adjusted_call_args_count(call_expr, func, call_args.len());

    if call_args_count >= fake_params.len() {
        return Some(());
    }

    // 调用参数包含 `...`.
    if call_args.iter().any(is_dots_expr) {
        return Some(());
    }

    // 对调用参数的最后一个参数进行特殊处理.
    if let Some(last_arg) = call_args.last()
        && let Ok(LuaType::Variadic(variadic)) = semantic_model.infer_expr(last_arg.clone())
    {
        let len = match variadic.get_max_len() {
            Some(len) => len,
            None => {
                return Some(());
            }
        };
        call_args_count = call_args_count + len - 1;
        if call_args_count >= fake_params.len() {
            return Some(());
        }
    }

    let mut miss_parameter_info = Vec::new();

    for i in call_args_count..fake_params.len() {
        let param_info = fake_params.get(i)?;
        if param_info.0 == "..." {
            break;
        }

        if let Some(typ) = param_info.1.clone()
            && !is_nullable(context.get_db(), &typ)
        {
            miss_parameter_info.push(t!("missing parameter: %{name}", name = param_info.0,));
        }
    }

    if !miss_parameter_info.is_empty() {
        let right_paren = call_expr
            .get_args_list()?
            .tokens::<LuaGeneralToken>()
            .last()?;
        context.add_diagnostic(
            DiagnosticCode::MissingParameter,
            right_paren.get_range(),
            t!(
                "expected %{num} parameters but found %{found_num}. %{infos}",
                num = fake_params.len(),
                found_num = call_args_count,
                infos = miss_parameter_info.join(" \n ")
            )
            .to_string(),
            None,
        );
    }
    Some(())
}

fn emit_redundant_parameter(
    context: &mut DiagnosticContext,
    _semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
    call_args: &[LuaExpr],
    func: &Arc<LuaFunctionType>,
) -> Option<()> {
    if func.is_variadic() {
        return Some(());
    }

    let fake_params = adjusted_params(call_expr, func);
    let call_args_count = adjusted_call_args_count(call_expr, func, call_args.len());
    let last_arg_is_dots = call_args.last().is_some_and(is_dots_expr);

    let mut min_call_args_count = call_args_count;
    if last_arg_is_dots {
        min_call_args_count = min_call_args_count.saturating_sub(1);
    }

    if min_call_args_count <= fake_params.len() {
        return Some(());
    }

    // 参数定义中最后一个参数是 `...`.
    if fake_params.last().is_some_and(|(name, typ)| {
        name == "..." || typ.as_ref().is_some_and(|typ| typ.is_variadic())
    }) {
        return Some(());
    }

    let mut adjusted_index = 0;
    if call_expr.is_colon_call() != func.is_colon_define() {
        adjusted_index = if func.is_colon_define() && !call_expr.is_colon_call() {
            -1
        } else {
            1
        };
    }

    for (i, arg) in call_args.iter().enumerate() {
        if last_arg_is_dots && i + 1 == call_args.len() {
            continue;
        }

        let param_index = i as isize + adjusted_index;

        if param_index < 0 || param_index < fake_params.len() as isize {
            continue;
        }

        context.add_diagnostic(
            DiagnosticCode::RedundantParameter,
            arg.get_range(),
            t!(
                "expected %{num} parameters but found %{found_num}",
                num = fake_params.len(),
                found_num = min_call_args_count,
            )
            .to_string(),
            None,
        );
    }

    Some(())
}

fn adjusted_call_args_count(
    call_expr: &LuaCallExpr,
    func: &LuaFunctionType,
    call_args_count: usize,
) -> usize {
    let mut count = call_args_count;
    if call_expr.is_colon_call() && !func.is_colon_define() {
        count += 1;
    }

    count
}

fn get_params_len(params: &[(String, Option<LuaType>)]) -> Option<usize> {
    if let Some((name, typ)) = params.last() {
        // 如果最后一个参数是可变参数, 则直接返回, 不需要检查.
        if name == "..." || typ.as_ref().is_some_and(|typ| typ.is_variadic()) {
            return None;
        }
    }
    Some(params.len())
}

pub(super) fn check_closure_param_count(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    closure_expr: &LuaClosureExpr,
) -> Option<()> {
    let current_signature =
        context
            .get_db()
            .get_signature_index()
            .get(&LuaSignatureId::from_closure(
                semantic_model.get_file_id(),
                closure_expr,
            ))?;

    let source_typ = semantic_model.infer_bind_value_type(closure_expr.clone().into())?;

    let source_params_len = match &source_typ {
        LuaType::DocFunction(func_type) => get_params_len(func_type.get_params()),
        LuaType::Signature(signature_id) => {
            let signature = context.get_db().get_signature_index().get(signature_id)?;
            let params = signature.get_type_params();
            get_params_len(&params)
        }
        _ => return Some(()),
    }?;

    // 只检查右值参数多于左值参数的情况, 右值参数少于左值参数的情况是能够接受的.
    if source_params_len > current_signature.params.len() {
        return Some(());
    }
    let found_num = current_signature.params.len();
    let params = closure_expr
        .get_params_list()?
        .get_params()
        .collect::<Vec<_>>();

    for param in params[source_params_len..].iter() {
        context.add_diagnostic(
            DiagnosticCode::RedundantParameter,
            param.get_range(),
            t!(
                "expected %{num} parameters but found %{found_num}",
                num = source_params_len,
                found_num = found_num,
            )
            .to_string(),
            None,
        );
    }

    Some(())
}
