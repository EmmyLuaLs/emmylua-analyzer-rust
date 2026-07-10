use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaBlock, LuaClosureExpr, LuaExpr, LuaGeneralToken,
    LuaReturnStat, LuaTokenKind,
};

use crate::{
    DiagnosticCode, LuaSignatureId, LuaType, SemanticModel, SignatureReturnStatus,
    compilation::analyze_func_body_missing_return_flags_with,
    db_index::return_row::{return_row_max_len, return_row_min_len},
};

use super::{Checker, DiagnosticContext, get_return_stats};

pub struct CheckReturnCount;

impl Checker for CheckReturnCount {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::RedundantReturnValue,
        DiagnosticCode::MissingReturnValue,
        DiagnosticCode::MissingReturn,
    ];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();

        for closure_expr in root.descendants::<LuaClosureExpr>() {
            check_missing_return(context, semantic_model, &closure_expr);
        }
    }
}

// 获取(是否doc标注过返回值, 返回值行)
fn get_function_return_info(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    closure_expr: &LuaClosureExpr,
) -> Option<(bool, Vec<LuaType>)> {
    let typ = semantic_model
        .infer_bind_value_type(closure_expr.clone().into())
        .unwrap_or(LuaType::Unknown);

    match typ {
        LuaType::DocFunction(func_type) => {
            return Some((true, func_type.get_return_row().to_vec()));
        }
        LuaType::Signature(signature) => {
            let signature = context.db.get_signature_index().get(&signature)?;
            return Some((
                signature.resolve_return == SignatureReturnStatus::DocResolve,
                signature.get_return_row(),
            ));
        }
        _ => {}
    };

    let signature_id = LuaSignatureId::from_closure(semantic_model.get_file_id(), closure_expr);
    let signature = context.db.get_signature_index().get(&signature_id)?;

    Some((
        signature.resolve_return == SignatureReturnStatus::DocResolve,
        signature.get_return_row(),
    ))
}

fn check_missing_return(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    closure_expr: &LuaClosureExpr,
) -> Option<()> {
    let (is_doc_resolve_return, return_row) =
        get_function_return_info(context, semantic_model, closure_expr)?;

    // 如果返回状态不是 DocResolve, 则跳过检查
    if !is_doc_resolve_return {
        return None;
    }

    // 最小返回值数
    let min_expected_return_count = return_row_min_len(&return_row)?;
    let max_expected_return_count = return_row_max_len(&return_row);

    for return_stat in get_return_stats(closure_expr) {
        check_return_count(
            context,
            semantic_model,
            &return_stat,
            min_expected_return_count,
            max_expected_return_count,
        );
    }

    // 检测缺少返回语句需要处理 if while
    if min_expected_return_count > 0 {
        let range = if let Some(block) = closure_expr.get_block() {
            let (can_fall_through, can_break) = analyze_func_body_missing_return_flags_with(
                block.clone(),
                &mut |expr: &LuaExpr| {
                    Ok(semantic_model
                        .infer_expr(expr.clone())
                        .unwrap_or(LuaType::Unknown))
                },
            )
            .ok()?;

            // Non-terminating paths satisfy `MissingReturn`; only paths that
            // can leave the function body without returning should warn.
            if !can_fall_through && !can_break {
                return Some(());
            }

            let token =
                get_block_end_token(&block).unwrap_or(block.tokens::<LuaGeneralToken>().last()?);
            Some(token.get_range())
        } else {
            Some(closure_expr.token_by_kind(LuaTokenKind::TkEnd)?.get_range())
        };
        if let Some(range) = range {
            context.add_diagnostic(
                DiagnosticCode::MissingReturn,
                range,
                t!("Annotations specify that a return value is required here.").to_string(),
                None,
            );
        }
    }

    Some(())
}

fn get_block_end_token(block: &LuaBlock) -> Option<LuaGeneralToken> {
    let token = block
        .token_by_kind(LuaTokenKind::TkEnd)
        .unwrap_or(LuaAst::cast(block.syntax().parent()?)?.token_by_kind(LuaTokenKind::TkEnd)?);
    Some(token)
}

/// 检查返回值数量
fn check_return_count(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    return_stat: &LuaReturnStat,
    min_expected_return_count: usize,
    max_expected_return_count: Option<usize>,
) -> Option<()> {
    // 计算实际返回的表达式数量并记录多余的范围
    let expr_list = return_stat.get_expr_list().collect::<Vec<_>>();
    let tail_expr_type = expr_list
        .last()
        .and_then(|expr| semantic_model.infer_expr(expr.clone()).ok());
    if let Some(LuaType::Variadic(variadic)) = &tail_expr_type
        && variadic.get_max_len().is_none()
    {
        // An unbounded variadic tail such as `string...` is a possible return row,
        // not a proven count, so count diagnostics would be guesswork.
        return None;
    }

    // Count the adjusted return row so a no-return tail call contributes zero
    // slots instead of the scalar `nil` used in single-value expression contexts.
    let return_infos = semantic_model.infer_expr_list_types(&expr_list, None);
    let total_return_count = return_infos.len();

    // 检查缺失的返回值
    if total_return_count < min_expected_return_count {
        context.add_diagnostic(
            DiagnosticCode::MissingReturnValue,
            return_stat.get_range(),
            t!(
                "Annotations specify that at least %{min} return value(s) are required, found %{rmin} returned here instead.",
                min = min_expected_return_count,
                rmin = total_return_count
            )
            .to_string(),
            None,
        );
    }

    // 检查多余的返回值
    if let Some(max_expected_return_count) = max_expected_return_count
        && total_return_count > max_expected_return_count
    {
        let mut last_redundant_range = None;
        for (index, (_, range)) in return_infos.iter().enumerate() {
            if index < max_expected_return_count {
                continue;
            }

            if last_redundant_range == Some(*range) {
                continue;
            }

            context.add_diagnostic(
                DiagnosticCode::RedundantReturnValue,
                *range,
                t!(
                    "Annotations specify that at most %{max} return value(s) are required, found %{rmax} returned here instead.",
                    max = max_expected_return_count,
                    rmax = total_return_count
                )
                .to_string(),
                None,
            );
            last_redundant_range = Some(*range);
        }
    }

    Some(())
}
