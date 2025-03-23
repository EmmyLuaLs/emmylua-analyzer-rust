use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaBlock, LuaCallExprStat, LuaClosureExpr, LuaGeneralToken,
    LuaIfStat, LuaReturnStat, LuaTokenKind, LuaWhileStat,
};

use crate::{DiagnosticCode, LuaSignatureId, LuaType, SemanticModel, SignatureReturnStatus};

use super::{get_own_return_stats, DiagnosticContext};

pub const CODES: &[DiagnosticCode] = &[
    DiagnosticCode::RedundantReturnValue,
    DiagnosticCode::MissingReturnValue,
    DiagnosticCode::MissingReturn,
];

pub fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) -> Option<()> {
    let root = semantic_model.get_root().clone();

    for closure_expr in root.descendants::<LuaClosureExpr>() {
        check_missing_return(context, semantic_model, &closure_expr);
    }
    Some(())
}

// 获取(是否doc标注过返回值, 返回值类型)
fn get_function_return_info(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    closure_expr: &LuaClosureExpr,
) -> Option<(bool, Vec<LuaType>)> {
    let typ = semantic_model
        .infer_left_value_type_from_right_value(closure_expr.clone().into())
        .unwrap_or(LuaType::Unknown);

    match typ {
        LuaType::DocFunction(func_type) => {
            return Some((true, func_type.get_ret().to_vec()));
        }
        LuaType::Signature(signature) => {
            let signature = context.db.get_signature_index().get(&signature)?;
            return Some((
                signature.resolve_return == SignatureReturnStatus::DocResolve,
                signature.get_return_types(),
            ));
        }
        _ => {}
    };

    let signature_id = LuaSignatureId::from_closure(semantic_model.get_file_id(), closure_expr);
    let signature = context.db.get_signature_index().get(&signature_id)?;

    Some((
        signature.resolve_return == SignatureReturnStatus::DocResolve,
        signature.get_return_types(),
    ))
}

fn check_missing_return(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    closure_expr: &LuaClosureExpr,
) {
    // 如果返回状态不是 DocResolve, 则跳过检查
    let Some((true, return_types)) =
        get_function_return_info(context, semantic_model, closure_expr)
    else {
        return;
    };

    // 如果包含可变参数, 则跳过检查最大值
    let skip_check_max = return_types.iter().any(|ty| ty.is_variadic());

    // 最小返回值数
    let min_expected_return_count = return_types
        .iter()
        .filter(|ty| !ty.is_nullable() && !ty.is_unknown() && !ty.is_any() && !ty.is_variadic())
        .count();

    for return_stat in get_own_return_stats(closure_expr) {
        check_return_count(
            context,
            semantic_model,
            &return_stat,
            &return_types,
            min_expected_return_count,
            skip_check_max,
        );
    }

    // 检测缺少返回语句需要处理 if while
    if min_expected_return_count > 0 {
        let range = if let Some(block) = closure_expr.get_block() {
            let result = check_return_block(context, semantic_model, block);
            match result {
                Ok(_) => return,
                Err(block) => {
                    let token = get_block_end_token(&block)
                        .or_else(|| block.tokens::<LuaGeneralToken>().last());
                    match token {
                        None => return,
                        Some(token) => token.get_range(),
                    }
                }
            }
        } else {
            match closure_expr.token_by_kind(LuaTokenKind::TkEnd) {
                None => return,
                Some(token) => token.get_range(),
            }
        };
        context.add_diagnostic(
            DiagnosticCode::MissingReturn,
            range,
            t!("Annotations specify that a return value is required here.").to_string(),
            None,
        );
    }
}

fn get_block_end_token(block: &LuaBlock) -> Option<LuaGeneralToken> {
    let token = block
        .token_by_kind(LuaTokenKind::TkEnd)
        .unwrap_or(LuaAst::cast(block.syntax().parent()?)?.token_by_kind(LuaTokenKind::TkEnd)?);
    Some(token)
}

fn check_return_block(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    block: LuaBlock,
) -> Result<(), LuaBlock> {
    // 检查是否存在return语句
    if block.children::<LuaReturnStat>().count() > 0 {
        return Ok(());
    }

    // 检查是否 error() 了
    for call_expr_stat in block.children::<LuaCallExprStat>() {
        if let Some(call_expr) = call_expr_stat.get_call_expr() {
            if call_expr.is_error() {
                return Ok(());
            }
        }
    }

    // 检查`if`和`while`语句
    let has_return = check_if_stat(context, semantic_model, &block)?
        | check_while_stat(context, semantic_model, &block)?;

    if has_return {
        Ok(())
    } else {
        Err(block)
    }
}

fn check_if_stat(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    block: &LuaBlock,
) -> Result<bool, LuaBlock> {
    let mut has_return = false;
    for if_stat in block.children::<LuaIfStat>() {
        // 检查`if`的主块
        if let Some(if_block) = if_stat.get_block() {
            check_return_block(context, semantic_model, if_block.clone())?;
        } else {
            return Err(block.clone());
        }

        // 检查所有条件分支
        for clause in if_stat.get_all_clause() {
            if let Some(clause_block) = clause.get_block() {
                check_return_block(context, semantic_model, clause_block.clone())?;
            } else {
                return Err(block.clone());
            }
        }

        // 检查是否存在`else`分支, 如果存在则上面已经检查过
        has_return = has_return || if_stat.get_else_clause().is_some();
    }

    if has_return {
        Ok(has_return)
    } else {
        Err(block.clone())
    }
}

fn check_while_stat(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    block: &LuaBlock,
) -> Result<bool, LuaBlock> {
    let mut has_return = false;
    for while_stat in block.children::<LuaWhileStat>() {
        if let Some(while_block) = while_stat.get_block() {
            check_return_block(context, semantic_model, while_block.clone())?;
        } else {
            return Err(block.clone());
        }

        // 检查`while`条件是否恒真, 如果恒真则代表存在返回语句(上面已经检查过子块)
        if is_while_condition_true(semantic_model, &while_stat) {
            has_return = true;
        }
    }
    Ok(has_return)
}

/// 确定 LuaWhileStat 的条件表达式是否为`true`
fn is_while_condition_true(semantic_model: &SemanticModel, while_stat: &LuaWhileStat) -> bool {
    if let Some(LuaType::BooleanConst(value)) = while_stat
        .get_condition_expr()
        .and_then(|expr| semantic_model.infer_expr(expr.clone()))
    {
        return value;
    }
    false
}

/// 检查返回值数量
fn check_return_count(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    return_stat: &LuaReturnStat,
    return_types: &[LuaType],
    min_expected_return_count: usize,
    skip_check_max: bool,
) {
    let max_return_count = return_types.len();

    // 计算实际返回的表达式数量并记录多余的范围
    let expr_list = return_stat.get_expr_list();
    let mut total_return_count = 0;
    let mut redundant_ranges = Vec::new();

    for expr in expr_list {
        let expr_type = semantic_model
            .infer_expr(expr.clone())
            .unwrap_or(LuaType::Unknown);
        match expr_type {
            LuaType::MuliReturn(types) => {
                total_return_count += types.get_len().unwrap_or(1) as usize;
            }
            _ => total_return_count += 1,
        };

        if total_return_count > max_return_count {
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
    if !skip_check_max {
        for range in redundant_ranges {
            context.add_diagnostic(
            DiagnosticCode::RedundantReturnValue,
            range,
            t!(
                "Annotations specify that at most %{max} return value(s) are required, found %{rmax} returned here instead.",
                max = max_return_count,
                rmax = total_return_count
            )
            .to_string(),
            None,
        );
        }
    }
}
