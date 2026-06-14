use emmylua_parser::{LuaAstNode, LuaAstToken, LuaCallExpr};
use rowan::{NodeOrToken, TextRange};

use crate::{
    DiagnosticCode, LuaFunctionType, LuaType, RenderLevel, SemanticModel, TypeCheckFailReason,
    TypeCheckResult, diagnostic::checker::assign_type_mismatch::check_table_expr, humanize_type,
    semantic::get_func_param_type,
};

use super::super::DiagnosticContext;
use super::call_facts::CallFacts;

pub(super) fn check_param_types(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    facts: &CallFacts,
) -> Option<()> {
    let (arg_types, arg_ranges) = facts.arg_types_and_ranges(semantic_model);

    let mut candidates = facts.param_count_compatible_funcs(semantic_model);
    if candidates.is_empty() {
        // 所有候选的参数数量都不匹配时, 交给参数数量诊断器报错.
        return Some(());
    }

    let source_type = semantic_model.infer_call_receiver_type(&facts.call_expr);
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
    loop {
        let mut has_arg = false;
        let mut next_candidates = Vec::with_capacity(candidates.len());
        let mut failed_param_types = Vec::with_capacity(candidates.len());
        let mut failed_arg = None;

        // 按参数位置逐步收窄候选, 第一处全体失败的位置就是本次诊断的位置.
        for func in &candidates {
            let Some(arg) = get_diagnostic_arg(
                &facts.call_expr,
                func,
                &arg_types,
                &arg_ranges,
                source_type.as_ref(),
                colon_range,
                arg_index,
            ) else {
                next_candidates.push(func.clone());
                continue;
            };
            has_arg = true;

            let Some(param_type) =
                get_diagnostic_param_type(func, &facts.call_expr, source_type.as_ref(), arg_index)
            else {
                if failed_arg.is_none() {
                    failed_arg = Some(arg);
                }
                continue;
            };

            if param_accepts_arg(semantic_model, &param_type, &arg.typ) {
                next_candidates.push(func.clone());
            } else {
                failed_param_types.push(param_type);
                if failed_arg.is_none() {
                    failed_arg = Some(arg);
                }
            }
        }

        if !has_arg {
            break;
        }

        if next_candidates.is_empty() {
            let failed_arg = failed_arg?;
            let param_type = LuaType::from_vec(failed_param_types);
            let result = semantic_model.type_check_detail(&param_type, &failed_arg.typ);
            if result.is_ok() {
                return Some(());
            }

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
                &failed_arg.typ,
                result,
            );
            break;
        }

        candidates = next_candidates;
        arg_index += 1;
    }

    Some(())
}

#[derive(Clone)]
struct DiagnosticArg {
    typ: LuaType,
    range: TextRange,
    expr_index: Option<usize>,
}

fn get_diagnostic_arg(
    call_expr: &LuaCallExpr,
    func: &LuaFunctionType,
    arg_types: &[LuaType],
    arg_ranges: &[TextRange],
    source_type: Option<&LuaType>,
    colon_range: Option<TextRange>,
    arg_index: usize,
) -> Option<DiagnosticArg> {
    // 冒号调用到非冒号定义时, 隐式 receiver 作为第 0 个实参参与类型检查.
    if call_expr.is_colon_call() && !func.is_colon_define() {
        if arg_index == 0 {
            return Some(DiagnosticArg {
                typ: source_type.cloned()?,
                range: colon_range?,
                expr_index: None,
            });
        }

        let index = arg_index - 1;
        return Some(DiagnosticArg {
            typ: arg_types.get(index)?.clone(),
            range: *arg_ranges.get(index)?,
            expr_index: Some(index),
        });
    }

    let typ = arg_types.get(arg_index)?.clone();
    Some(DiagnosticArg {
        typ,
        range: *arg_ranges.get(arg_index)?,
        expr_index: Some(arg_index),
    })
}

fn get_diagnostic_param_type(
    func: &LuaFunctionType,
    call_expr: &LuaCallExpr,
    source_type: Option<&LuaType>,
    arg_index: usize,
) -> Option<LuaType> {
    // 点调用到冒号定义时, self 是第 0 个形参, 后续形参整体右移.
    if !call_expr.is_colon_call() && func.is_colon_define() {
        if arg_index == 0 {
            return source_type.cloned().or(Some(LuaType::SelfInfer));
        }

        return get_func_param_type(func, arg_index - 1);
    }

    get_func_param_type(func, arg_index)
}

fn param_accepts_arg(
    semantic_model: &SemanticModel,
    param_type: &LuaType,
    arg_type: &LuaType,
) -> bool {
    if param_type.is_any()
        || matches!((param_type, arg_type), (LuaType::Integer, LuaType::FloatConst(f)) if f.fract() == 0.0)
    {
        return true;
    }

    semantic_model
        .type_check_detail(param_type, arg_type)
        .is_ok()
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
