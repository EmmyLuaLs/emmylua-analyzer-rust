use std::sync::Arc;

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaCallExpr, LuaClosureExpr, LuaExpr, LuaGeneralToken,
};

use crate::{DiagnosticCode, LuaFunctionType, LuaSignatureId, LuaType, SemanticModel};

use super::super::DiagnosticContext;
use super::call_facts::{CallFacts, count_ranges_overlap, get_param_count_range, is_nullable};

pub(super) fn check_call_param_count(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    facts: &CallFacts,
) {
    let mut best_candidate = None;
    for func in facts.funcs() {
        let Some(call_count) = facts.call_arg_count_range(semantic_model, func) else {
            return;
        };
        let param_count = get_param_count_range(context.get_db(), func, &facts.call_expr);

        if count_ranges_overlap(call_count, param_count) {
            return;
        }

        if let Some(max_call_count) = call_count.max
            && max_call_count < param_count.min
        {
            update_best_candidate(
                &mut best_candidate,
                CountDiagnosticCandidate::Missing {
                    mismatch: param_count.min - max_call_count,
                    expected_count: param_count.min,
                    found_count: max_call_count,
                    func,
                },
            );
            continue;
        }

        if let Some(max_param_count) = param_count.max
            && call_count.min > max_param_count
        {
            update_best_candidate(
                &mut best_candidate,
                CountDiagnosticCandidate::Redundant {
                    mismatch: call_count.min - max_param_count,
                    expected_count: max_param_count,
                    found_count: call_count.min,
                    func,
                },
            );
        }
    }

    let Some(candidate) = best_candidate else {
        return;
    };

    match candidate {
        CountDiagnosticCandidate::Missing {
            expected_count,
            found_count,
            func,
            ..
        } => emit_missing_parameter(context, &facts.call_expr, expected_count, found_count, func),
        CountDiagnosticCandidate::Redundant {
            expected_count,
            found_count,
            func,
            ..
        } => {
            emit_redundant_parameter(
                context,
                &facts.call_expr,
                &facts.arg_exprs,
                expected_count,
                found_count,
                func,
            );
        }
    }
}

enum CountDiagnosticCandidate<'a> {
    Missing {
        mismatch: usize,
        expected_count: usize,
        found_count: usize,
        func: &'a Arc<LuaFunctionType>,
    },
    Redundant {
        mismatch: usize,
        expected_count: usize,
        found_count: usize,
        func: &'a Arc<LuaFunctionType>,
    },
}

fn update_best_candidate<'a>(
    best_candidate: &mut Option<CountDiagnosticCandidate<'a>>,
    candidate: CountDiagnosticCandidate<'a>,
) {
    if best_candidate
        .as_ref()
        .is_none_or(|current| candidate.is_better_than(current))
    {
        *best_candidate = Some(candidate);
    }
}

impl CountDiagnosticCandidate<'_> {
    fn is_better_than(&self, other: &Self) -> bool {
        match self.mismatch().cmp(&other.mismatch()) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => self.is_better_tie_than(other),
        }
    }

    fn mismatch(&self) -> usize {
        match self {
            CountDiagnosticCandidate::Missing { mismatch, .. }
            | CountDiagnosticCandidate::Redundant { mismatch, .. } => *mismatch,
        }
    }

    fn is_better_tie_than(&self, other: &Self) -> bool {
        match (self, other) {
            (
                CountDiagnosticCandidate::Missing {
                    expected_count: left,
                    ..
                },
                CountDiagnosticCandidate::Missing {
                    expected_count: right,
                    ..
                },
            ) => left < right,
            (
                CountDiagnosticCandidate::Redundant {
                    expected_count: left,
                    ..
                },
                CountDiagnosticCandidate::Redundant {
                    expected_count: right,
                    ..
                },
            ) => left > right,
            (
                CountDiagnosticCandidate::Missing { .. },
                CountDiagnosticCandidate::Redundant { .. },
            ) => true,
            (
                CountDiagnosticCandidate::Redundant { .. },
                CountDiagnosticCandidate::Missing { .. },
            ) => false,
        }
    }
}

fn emit_missing_parameter(
    context: &mut DiagnosticContext,
    call_expr: &LuaCallExpr,
    expected_count: usize,
    found_count: usize,
    func: &Arc<LuaFunctionType>,
) {
    let mut miss_parameter_info = Vec::new();

    for param_index in found_count..expected_count {
        add_missing_parameter_info(
            context,
            call_expr,
            func,
            param_index,
            &mut miss_parameter_info,
        );
    }

    if !miss_parameter_info.is_empty() {
        let Some(args_list) = call_expr.get_args_list() else {
            return;
        };
        let Some(right_paren) = args_list.tokens::<LuaGeneralToken>().last() else {
            return;
        };
        context.add_diagnostic(
            DiagnosticCode::MissingParameter,
            right_paren.get_range(),
            t!(
                "expected %{num} parameters but found %{found_num}. %{infos}",
                num = expected_count,
                found_num = found_count,
                infos = miss_parameter_info.join(" \n ")
            )
            .to_string(),
            None,
        );
    }
}

fn emit_redundant_parameter(
    context: &mut DiagnosticContext,
    call_expr: &LuaCallExpr,
    call_args: &[LuaExpr],
    expected_count: usize,
    found_count: usize,
    func: &Arc<LuaFunctionType>,
) {
    let implicit_receiver_offset =
        usize::from(call_expr.is_colon_call() && !func.is_colon_define());
    for (i, arg) in call_args.iter().enumerate() {
        if i + implicit_receiver_offset < expected_count {
            continue;
        }

        context.add_diagnostic(
            DiagnosticCode::RedundantParameter,
            arg.get_range(),
            t!(
                "expected %{num} parameters but found %{found_num}",
                num = expected_count,
                found_num = found_count,
            )
            .to_string(),
            None,
        );
    }
}

fn add_missing_parameter_info(
    context: &DiagnosticContext,
    call_expr: &LuaCallExpr,
    func: &LuaFunctionType,
    adjusted_index: usize,
    miss_parameter_info: &mut Vec<String>,
) {
    if needs_implicit_self_param(call_expr, func) {
        if adjusted_index == 0 {
            if !is_nullable(context.get_db(), &LuaType::SelfInfer) {
                miss_parameter_info
                    .push(t!("missing parameter: %{name}", name = "self",).to_string());
            }
            return;
        }
        let Some((name, typ)) = func.get_params().get(adjusted_index - 1) else {
            return;
        };
        if let Some(typ) = typ
            && !is_nullable(context.get_db(), typ)
        {
            miss_parameter_info.push(t!("missing parameter: %{name}", name = name,).to_string());
        }
        return;
    }

    let Some((name, typ)) = func.get_params().get(adjusted_index) else {
        return;
    };
    if let Some(typ) = typ
        && !is_nullable(context.get_db(), typ)
    {
        miss_parameter_info.push(t!("missing parameter: %{name}", name = name,).to_string());
    }
}

fn needs_implicit_self_param(call_expr: &LuaCallExpr, func: &LuaFunctionType) -> bool {
    !call_expr.is_colon_call() && func.is_colon_define()
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
) {
    let Some(current_signature) =
        context
            .get_db()
            .get_signature_index()
            .get(&LuaSignatureId::from_closure(
                semantic_model.get_file_id(),
                closure_expr,
            ))
    else {
        return;
    };

    let Some(source_typ) = semantic_model.infer_bind_value_type(closure_expr.clone().into()) else {
        return;
    };

    let Some(source_params_len) = (match &source_typ {
        LuaType::DocFunction(func_type) => get_params_len(func_type.get_params()),
        LuaType::Signature(signature_id) => {
            let Some(signature) = context.get_db().get_signature_index().get(signature_id) else {
                return;
            };
            let params = signature.get_type_params();
            get_params_len(&params)
        }
        _ => return,
    }) else {
        return;
    };

    // 只检查右值参数多于左值参数的情况, 右值参数少于左值参数的情况是能够接受的.
    if source_params_len > current_signature.params.len() {
        return;
    }
    let found_num = current_signature.params.len();
    let Some(params_list) = closure_expr.get_params_list() else {
        return;
    };
    let params = params_list.get_params().collect::<Vec<_>>();

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
}
