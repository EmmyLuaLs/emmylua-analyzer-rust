use std::sync::Arc;

use emmylua_parser::{LuaAstNode, LuaAstToken, LuaCallExpr};
use rowan::{NodeOrToken, TextRange};

use crate::{
    DiagnosticCode, LuaFunctionType, LuaType, RenderLevel, SemanticModel, TypeCheckFailReason,
    TypeCheckResult, diagnostic::checker::assign_type_mismatch::check_table_expr, humanize_type,
    semantic::get_func_param_type,
};

use super::{super::DiagnosticContext, call_facts::CallFacts};

pub(super) fn check_param_types(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    facts: &CallFacts,
    candidates: &[Arc<LuaFunctionType>],
) -> Option<()> {
    if candidates.is_empty() {
        return Some(());
    }
    let mut candidates = candidates
        .iter()
        .map(Arc::as_ref)
        .collect::<Vec<&LuaFunctionType>>();

    let (arg_types, arg_ranges): (Vec<LuaType>, Vec<TextRange>) = semantic_model
        .infer_expr_list_types(&facts.arg_exprs, None)
        .into_iter()
        .unzip();

    let self_type = semantic_model.infer_call_self_type(&facts.call_expr);
    let colon_range = facts
        .call_expr
        .get_colon_token()
        .map(|token| token.get_range())
        .or_else(|| {
            facts
                .call_expr
                .get_prefix_expr()
                .map(|expr| expr.get_range())
        });
    let mut arg_index = 0;
    while !candidates.is_empty() {
        let arg_index_result = check_arg_index_candidates(
            semantic_model,
            &facts.call_expr,
            &candidates,
            &arg_types,
            &arg_ranges,
            self_type.as_ref(),
            colon_range,
            arg_index,
        );

        let (failed_arg, param_type, result) = match arg_index_result {
            ArgIndexCheckResult::NoDiagnostic => return Some(()),
            ArgIndexCheckResult::MatchedCandidates(next_candidates) => {
                candidates = next_candidates;
                arg_index += 1;
                continue;
            }
            ArgIndexCheckResult::Mismatch {
                failed_arg,
                param_type,
                result,
            } => (failed_arg, param_type, result),
        };

        // 表字段已经报错了, 则不添加参数不匹配的诊断避免干扰.
        if failed_arg.typ.is_table()
            && let Some(arg_expr_idx) = failed_arg.expr_index
            && let Some(arg_expr) = facts.arg_exprs.get(arg_expr_idx)
            && let Some(add_diagnostic) = check_table_expr(
                context,
                semantic_model,
                NodeOrToken::Node(arg_expr.syntax().clone()),
                arg_expr,
                Some(&param_type),
            )
            && add_diagnostic
        {
            return Some(());
        }

        add_diagnostic(
            context,
            semantic_model,
            failed_arg.range,
            &param_type,
            failed_arg.typ,
            result,
        );
        return Some(());
    }

    Some(())
}

enum ArgIndexCheckResult<'func, 'arg> {
    NoDiagnostic,
    MatchedCandidates(Vec<&'func LuaFunctionType>),
    Mismatch {
        failed_arg: DiagnosticArg<'arg>,
        param_type: LuaType,
        result: TypeCheckResult,
    },
}

#[derive(Clone, Copy)]
struct DiagnosticArg<'a> {
    typ: &'a LuaType,
    range: TextRange,
    expr_index: Option<usize>,
}

fn check_arg_index_candidates<'func, 'arg>(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
    candidates: &[&'func LuaFunctionType],
    arg_types: &'arg [LuaType],
    arg_ranges: &[TextRange],
    self_type: Option<&'arg LuaType>,
    colon_range: Option<TextRange>,
    arg_index: usize,
) -> ArgIndexCheckResult<'func, 'arg> {
    let mut checked_call_arg = false;
    let mut next_candidates = Vec::with_capacity(candidates.len());
    let mut failed_param_types = Vec::with_capacity(candidates.len());
    let mut failed_arg = None;
    let mut failed_result = None;

    // 按参数位置逐步收窄候选, 第一处全体失败的位置就是本次诊断的位置.
    for func in candidates.iter().copied() {
        let Some(arg) = get_diagnostic_arg(
            call_expr,
            func,
            arg_types,
            arg_ranges,
            self_type,
            colon_range,
            arg_index,
        ) else {
            next_candidates.push(func);
            continue;
        };
        checked_call_arg = true;

        // 点调用到冒号定义时, self 是第 0 个形参, 后续形参整体右移.
        let param_type = if !call_expr.is_colon_call() && func.is_colon_define() {
            if arg_index == 0 {
                self_type.cloned().or(Some(LuaType::SelfInfer))
            } else {
                get_func_param_type(func, arg_index - 1)
            }
        } else {
            get_func_param_type(func, arg_index)
        };
        let Some(param_type) = param_type else {
            if failed_arg.is_none() {
                failed_arg = Some(arg);
            }
            continue;
        };

        if param_type.is_any()
            || matches!((&param_type, arg.typ), (LuaType::Integer, LuaType::FloatConst(f)) if f.fract() == 0.0)
        {
            next_candidates.push(func);
            continue;
        }

        let type_check_result = semantic_model.type_check_detail(&param_type, arg.typ);
        if type_check_result.is_ok() {
            next_candidates.push(func);
            continue;
        }

        failed_param_types.push(param_type);
        if failed_arg.is_none() {
            failed_arg = Some(arg);
        }
        if failed_result.is_none() {
            failed_result = Some(type_check_result);
        }
    }

    if !checked_call_arg {
        return ArgIndexCheckResult::NoDiagnostic;
    }

    if !next_candidates.is_empty() {
        return ArgIndexCheckResult::MatchedCandidates(next_candidates);
    }

    let Some(failed_arg) = failed_arg else {
        return ArgIndexCheckResult::NoDiagnostic;
    };

    if failed_param_types.is_empty() {
        return ArgIndexCheckResult::NoDiagnostic;
    }
    let Some(result) = failed_result else {
        return ArgIndexCheckResult::NoDiagnostic;
    };

    ArgIndexCheckResult::Mismatch {
        failed_arg,
        param_type: LuaType::from_vec(failed_param_types),
        result,
    }
}

fn get_diagnostic_arg<'a>(
    call_expr: &LuaCallExpr,
    func: &LuaFunctionType,
    arg_types: &'a [LuaType],
    arg_ranges: &[TextRange],
    self_type: Option<&'a LuaType>,
    colon_range: Option<TextRange>,
    arg_index: usize,
) -> Option<DiagnosticArg<'a>> {
    // 冒号调用到非冒号定义时, 隐式 receiver 作为第 0 个实参参与类型检查.
    if call_expr.is_colon_call() && !func.is_colon_define() {
        if arg_index == 0 {
            return Some(DiagnosticArg {
                typ: self_type?,
                range: colon_range?,
                expr_index: None,
            });
        }

        let index = arg_index - 1;
        return Some(DiagnosticArg {
            typ: arg_types.get(index)?,
            range: *arg_ranges.get(index)?,
            expr_index: Some(index),
        });
    }

    let typ = arg_types.get(arg_index)?;
    Some(DiagnosticArg {
        typ,
        range: *arg_ranges.get(arg_index)?,
        expr_index: Some(arg_index),
    })
}

fn add_diagnostic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    param_type: &LuaType,
    expr_type: &LuaType,
    result: TypeCheckResult,
) {
    if let (LuaType::Integer, LuaType::FloatConst(f)) = (param_type, expr_type)
        && f.fract() == 0.0
    {
        return;
    }
    let db = semantic_model.get_db();
    match result {
        Ok(_) => (),
        Err(reason) => {
            let reason_message = match reason {
                TypeCheckFailReason::TypeNotMatchWithReason(reason) => reason,
                TypeCheckFailReason::TypeNotMatch | TypeCheckFailReason::DonotCheck => {
                    "".to_string()
                }
                TypeCheckFailReason::TypeRecursion => "type recursion".to_string(),
            };
            context.add_diagnostic(
                DiagnosticCode::ParamTypeMismatch,
                range,
                t!(
                    "expected `%{source}` but found `%{found}`. %{reason}",
                    source = humanize_type(db, param_type, RenderLevel::Simple),
                    found = humanize_type(db, expr_type, RenderLevel::Simple),
                    reason = reason_message
                )
                .to_string(),
                None,
            );
        }
    }
}
