use emmylua_parser::{LuaAstNode, LuaAstToken, LuaCallExpr};
use rowan::TextRange;

use crate::{
    AssignabilityResult, DiagnosticCode, LuaFunctionType, LuaType, RenderLevel, SemanticModel,
    TypeMismatch,
    diagnostic::checker::table::check_table_assignment_diagnostics,
    humanize_type, render_type_mismatch_reason,
    semantic::{RelationOutcome, get_func_param_type, probe_assignable},
};

use super::{
    super::{DiagnosticContext, DiagnosticMessage},
    call_analysis::CallAnalysis,
};

pub(super) fn check_param_type_mismatch(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call: &CallAnalysis,
    // `Some` 只检查指定候选, `None` 保留全部候选.
    candidate_indices: Option<&[usize]>,
) {
    // 根据参数数量检查结果选择待检查的实例化候选.
    let candidate_capacity = candidate_indices
        .map(|indices| indices.len())
        .unwrap_or_else(|| call.candidates().len());
    let mut current_candidates = Vec::with_capacity(candidate_capacity);
    match candidate_indices {
        Some(indices) => current_candidates.extend(indices.iter().filter_map(|index| {
            call.candidates()
                .get(*index)
                .map(|candidate| candidate.instantiated.as_ref())
        })),
        None => current_candidates.extend(
            call.candidates()
                .iter()
                .map(|candidate| candidate.instantiated.as_ref()),
        ),
    }
    if current_candidates.is_empty() {
        return;
    }

    // 推断显式实参类型, 并准备调用形式不一致时需要的隐式 self 信息.
    let arg_types = semantic_model.infer_expr_list_types(&call.arg_exprs, None);
    let call_is_colon = call.call_expr.is_colon_call();
    let call_form_mismatch = current_candidates
        .iter()
        .any(|func| call_is_colon != func.is_colon_define());
    let self_type = call_form_mismatch
        .then(|| semantic_model.resolve_call_self_type(&call.call_expr))
        .flatten();
    let needs_implicit_receiver = call_is_colon
        && current_candidates
            .iter()
            .any(|func| !func.is_colon_define());
    let colon_range = needs_implicit_receiver.then(|| {
        call.call_expr
            .get_colon_token()
            .map(|token| token.get_range())
            .or_else(|| {
                call.call_expr
                    .get_prefix_expr()
                    .map(|expr| expr.get_range())
            })
    });
    let colon_range = colon_range.flatten();

    // 按实参位置逐步收窄候选, 只有所有候选都在同一位置失败时才生成诊断.
    let mut next_candidates = Vec::with_capacity(candidate_capacity);
    let mut failed_param_types = Vec::with_capacity(candidate_capacity);
    let mut arg_index = 0;
    loop {
        let arg_index_result = check_arg_index_candidates(
            semantic_model,
            &call.call_expr,
            &current_candidates,
            &arg_types,
            self_type.as_ref(),
            colon_range,
            arg_index,
            &mut next_candidates,
            &mut failed_param_types,
        );

        let (failed_arg, param_type) = match arg_index_result {
            ArgIndexCheckResult::NoDiagnostic => return,
            ArgIndexCheckResult::MatchedCandidates => {
                std::mem::swap(&mut current_candidates, &mut next_candidates);
                arg_index += 1;
                continue;
            }
            ArgIndexCheckResult::Mismatch {
                failed_arg,
                param_type,
            } => (failed_arg, param_type),
        };

        // 表字面量优先交给专用表检查, 避免重复报告普通参数类型错误.
        if let Some(arg_expr_index) = failed_arg.expr_index
            && let Some(arg_expr) = call.arg_exprs.get(arg_expr_index)
            && check_table_assignment_diagnostics(
                context,
                semantic_model,
                arg_expr,
                failed_arg.typ,
                &param_type,
            )
            .is_handled()
        {
            return;
        }

        // 表检查未处理时, 补充通用可赋值关系和参数类型诊断.
        let mismatch = match semantic_model.check_assignable(failed_arg.typ, &param_type) {
            AssignabilityResult::NotAssignable(mismatch) => Some(mismatch),
            AssignabilityResult::Assignable | AssignabilityResult::Indeterminate(_) => None,
        };
        add_param_type_diagnostic(
            context,
            semantic_model,
            failed_arg.range,
            &param_type,
            failed_arg.typ,
            mismatch.as_ref(),
        );
        return;
    }
}

enum ArgIndexCheckResult<'arg> {
    NoDiagnostic,
    MatchedCandidates,
    Mismatch {
        failed_arg: DiagnosticArg<'arg>,
        param_type: LuaType,
    },
}

#[derive(Clone, Copy)]
struct DiagnosticArg<'a> {
    typ: &'a LuaType,
    range: TextRange,
    expr_index: Option<usize>,
}

#[allow(clippy::too_many_arguments)]
fn check_arg_index_candidates<'func, 'arg>(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
    candidates: &[&'func LuaFunctionType],
    arg_types: &'arg [(LuaType, TextRange)],
    self_type: Option<&'arg LuaType>,
    colon_range: Option<TextRange>,
    arg_index: usize,
    next_candidates: &mut Vec<&'func LuaFunctionType>,
    failed_param_types: &mut Vec<LuaType>,
) -> ArgIndexCheckResult<'arg> {
    // 清空上一轮的复用缓冲区, 避免每个参数位置重复分配.
    next_candidates.clear();
    failed_param_types.clear();
    let mut checked_call_arg = false;
    let mut failed_arg = None;

    // 检查当前参数位置, 同时收集仍可匹配的候选和失败候选的期望类型.
    for func in candidates.iter().copied() {
        let Some(arg) = get_diagnostic_arg(
            call_expr,
            func,
            arg_types,
            self_type,
            colon_range,
            arg_index,
        ) else {
            next_candidates.push(func);
            continue;
        };
        checked_call_arg = true;

        // 点调用冒号定义函数时, 第 0 个位置对应隐式 self.
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
            failed_arg.get_or_insert(arg);
            continue;
        };

        // Any, 整数型浮点常量和关系上可兼容的类型都保留当前候选.
        if param_type.is_any()
            || matches!((&param_type, arg.typ), (LuaType::Integer, LuaType::FloatConst(value)) if value.fract() == 0.0)
            || matches!(
                probe_assignable(semantic_model.get_db(), arg.typ, &param_type),
                RelationOutcome::Related
            )
        {
            next_candidates.push(func);
            continue;
        }

        failed_param_types.push(param_type);
        failed_arg.get_or_insert(arg);
    }

    // 没有可检查的实参或仍有匹配候选时, 本轮不生成类型诊断.
    if !checked_call_arg {
        return ArgIndexCheckResult::NoDiagnostic;
    }
    if !next_candidates.is_empty() {
        return ArgIndexCheckResult::MatchedCandidates;
    }
    let Some(failed_arg) = failed_arg else {
        return ArgIndexCheckResult::NoDiagnostic;
    };
    if failed_param_types.is_empty() {
        return ArgIndexCheckResult::NoDiagnostic;
    }

    ArgIndexCheckResult::Mismatch {
        failed_arg,
        param_type: LuaType::from_vec(std::mem::take(failed_param_types)),
    }
}

fn get_diagnostic_arg<'a>(
    call_expr: &LuaCallExpr,
    func: &LuaFunctionType,
    arg_types: &'a [(LuaType, TextRange)],
    self_type: Option<&'a LuaType>,
    colon_range: Option<TextRange>,
    arg_index: usize,
) -> Option<DiagnosticArg<'a>> {
    // 冒号调用普通函数时, 隐式 receiver 作为第 0 个实参参与类型检查.
    if call_expr.is_colon_call() && !func.is_colon_define() {
        if arg_index == 0 {
            return Some(DiagnosticArg {
                typ: self_type?,
                range: colon_range?,
                expr_index: None,
            });
        }

        let index = arg_index - 1;
        let (typ, range) = arg_types.get(index)?;
        return Some(DiagnosticArg {
            typ,
            range: *range,
            expr_index: Some(index),
        });
    }

    // 其他调用形式直接映射显式实参.
    let (typ, range) = arg_types.get(arg_index)?;
    Some(DiagnosticArg {
        typ,
        range: *range,
        expr_index: Some(arg_index),
    })
}

fn add_param_type_diagnostic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    param_type: &LuaType,
    expr_type: &LuaType,
    mismatch: Option<&TypeMismatch>,
) {
    // 整数参数接受整数值的浮点常量, 不报告参数类型错误.
    if let (LuaType::Integer, LuaType::FloatConst(value)) = (param_type, expr_type)
        && value.fract() == 0.0
    {
        return;
    }

    // 输出首个失败参数位置的类型不匹配诊断.
    let db = semantic_model.get_db();
    context.add_diagnostic(
        DiagnosticCode::ParamTypeMismatch,
        range,
        DiagnosticMessage::with_detail(
            t!(
                "expected `%{source}` but found `%{found}`.",
                source = humanize_type(db, param_type, RenderLevel::Simple),
                found = humanize_type(db, expr_type, RenderLevel::Simple),
            )
            .to_string(),
            mismatch.and_then(|mismatch| render_type_mismatch_reason(db, mismatch)),
        ),
        None,
    );
}
