use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaBlock, LuaClosureExpr, LuaExpr, LuaGeneralToken,
    LuaReturnStat, LuaTokenKind,
};

use crate::{
    DiagnosticCode, LuaSignatureId, LuaType, SemanticModel, SignatureReturnStatus,
    compilation::does_func_body_always_return_or_exit,
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

// 获取(是否doc标注过返回值, 返回值类型)
fn get_function_return_info(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    closure_expr: &LuaClosureExpr,
) -> Option<(bool, LuaType)> {
    let typ = semantic_model
        .infer_bind_value_type(closure_expr.clone().into())
        .unwrap_or(LuaType::Unknown);

    match typ {
        LuaType::DocFunction(func_type) => {
            return Some((true, func_type.get_ret().clone()));
        }
        LuaType::Signature(signature) => {
            let signature = context.db.get_signature_index().get(&signature)?;
            return Some((
                signature.resolve_return == SignatureReturnStatus::DocResolve,
                signature.get_return_type(),
            ));
        }
        _ => {}
    };

    let signature_id = LuaSignatureId::from_closure(semantic_model.get_file_id(), closure_expr);
    let signature = context.db.get_signature_index().get(&signature_id)?;

    Some((
        signature.resolve_return == SignatureReturnStatus::DocResolve,
        signature.get_return_type(),
    ))
}

fn check_missing_return(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    closure_expr: &LuaClosureExpr,
) -> Option<()> {
    let (is_doc_resolve_return, return_type) =
        get_function_return_info(context, semantic_model, closure_expr)?;

    // 如果返回状态不是 DocResolve, 则跳过检查
    if !is_doc_resolve_return {
        return None;
    }

    // 最小返回值数
    let min_expected_return_count = match &return_type {
        LuaType::Variadic(variadic) => {
            let min_len = variadic.get_min_len()?;
            let mut real_min_len = min_len;
            // 逆序检查
            if min_len > 0 {
                for i in (0..min_len).rev() {
                    if let Some(ty) = variadic.get_type(i) {
                        if ty.is_optional() {
                            real_min_len -= 1;
                        } else {
                            break;
                        }
                    }
                }
            }
            real_min_len
        }
        LuaType::Nil | LuaType::Any | LuaType::Unknown => 0,
        _ if return_type.is_nullable() => 0,
        _ => 1,
    };

    for return_stat in get_return_stats(closure_expr) {
        check_return_count(
            context,
            semantic_model,
            &return_stat,
            &return_type,
            min_expected_return_count,
        );
    }

    // 检测缺少返回语句需要处理 if while
    if min_expected_return_count > 0 {
        let range = if let Some(block) = closure_expr.get_block() {
            if does_func_body_always_return_or_exit(block.clone(), &mut |expr: &LuaExpr| {
                Ok(semantic_model
                    .infer_expr(expr.clone())
                    .unwrap_or(LuaType::Unknown))
            })
            .ok()?
            {
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
    return_type: &LuaType,
    min_expected_return_count: usize,
) -> Option<()> {
    let max_expected_return_count = match return_type {
        LuaType::Variadic(variadic) => variadic.get_max_len(),
        LuaType::Any | LuaType::Unknown => Some(1),
        LuaType::Nil => Some(0),
        _ => Some(1),
    };

    // 计算实际返回的表达式数量并记录多余的范围
    let expr_list = return_stat.get_expr_list().collect::<Vec<_>>();
    let mut total_return_count = 0;
    let mut tail_return_nil = false;
    let mut redundant_ranges = Vec::new();

    for (index, expr) in expr_list.iter().enumerate() {
        let expr_type = semantic_model
            .infer_expr(expr.clone())
            .unwrap_or(LuaType::Unknown);
        match expr_type {
            LuaType::Variadic(variadic) => {
                total_return_count += variadic.get_max_len()?;
            }
            LuaType::Nil => {
                if index == expr_list.len() - 1 {
                    tail_return_nil = true;
                }
                total_return_count += 1;
            }
            _ => total_return_count += 1,
        };

        if max_expected_return_count.is_some() && total_return_count > max_expected_return_count? {
            if tail_return_nil && total_return_count - 1 == max_expected_return_count? {
                continue;
            }
            redundant_ranges.push(expr.get_range());
        }
    }

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
    for range in redundant_ranges {
        context.add_diagnostic(
            DiagnosticCode::RedundantReturnValue,
            range,
            t!(
                "Annotations specify that at most %{max} return value(s) are required, found %{rmax} returned here instead.",
                max = max_expected_return_count?,
                rmax = total_return_count
            )
            .to_string(),
            None,
        );
    }

    Some(())
}
