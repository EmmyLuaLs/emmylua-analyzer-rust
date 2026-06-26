//! Check assign type mismatch — pure salsa.

use std::ops::Deref;

use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaAstToken, LuaExpr, LuaIndexExpr, LuaLocalStat,
    LuaNameExpr, LuaTableExpr, LuaVarExpr,
};
use rowan::{NodeOrToken, TextRange};

use crate::semantic_model::{DeclPosition, SemanticModel, TypeCheckFailReason, TypeCheckResult};
use crate::{
    DiagnosticCode, LuaDeclId, LuaMemberKey, LuaSemanticDeclId, LuaType, SemanticDeclLevel,
    VariadicType,
};

use super::{DiagnosticContext, humanize_lint_type_salsa};

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    for node in model.get_root().descendants::<LuaAst>() {
        match node {
            LuaAst::LuaAssignStat(assign) => {
                check_assign_stat(context, model, &assign);
            }
            LuaAst::LuaLocalStat(local) => {
                check_local_stat(context, model, &local);
            }
            _ => {}
        }
    }
}

fn check_assign_stat(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    assign: &LuaAssignStat,
) -> Option<()> {
    let (vars, exprs) = assign.get_var_and_expr_list();
    // Use the old infer for now — the new model doesn't have infer_expr_list_types in equivalent form
    let value_types: Vec<(LuaType, TextRange)> = exprs
        .iter()
        .map(|e| {
            let ty = model.infer_expr(e.clone()).unwrap_or(LuaType::Any);
            (ty, e.get_range())
        })
        .collect();

    for (idx, var) in vars.iter().enumerate() {
        match var {
            LuaVarExpr::IndexExpr(index_expr) => {
                check_index_expr(
                    context,
                    model,
                    index_expr,
                    exprs.get(idx).cloned(),
                    value_types.get(idx)?.0.clone(),
                );
            }
            LuaVarExpr::NameExpr(name_expr) => {
                check_name_expr(
                    context,
                    model,
                    name_expr,
                    exprs.get(idx).cloned(),
                    value_types.get(idx)?.0.clone(),
                );
            }
        }
    }
    Some(())
}

fn check_name_expr(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    name_expr: &LuaNameExpr,
    expr: Option<LuaExpr>,
    value_type: LuaType,
) -> Option<()> {
    let semantic_decl =
        model.find_decl_by_node(name_expr.syntax().clone(), SemanticDeclLevel::default())?;
    let source_type = match &semantic_decl {
        LuaSemanticDeclId::LuaDecl(decl_id) => {
            model.get_type_by_decl_position(DeclPosition(decl_id.position))
        }
        _ => None,
    };
    check_assign_type_mismatch(
        context,
        model,
        name_expr.get_range(),
        source_type.as_ref(),
        &value_type,
        false,
    );
    if let Some(expr) = expr {
        check_table_expr(
            context,
            model,
            NodeOrToken::Node(name_expr.syntax().clone()),
            &expr,
            source_type.as_ref(),
        );
    }

    Some(())
}

fn check_index_expr(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    index_expr: &LuaIndexExpr,
    expr: Option<LuaExpr>,
    value_type: LuaType,
) -> Option<()> {
    // Use new model's infer_expr for index expressions
    let source_type = model
        .infer_expr(LuaExpr::IndexExpr(index_expr.clone()))
        .ok();

    check_assign_type_mismatch(
        context,
        model,
        index_expr.get_range(),
        source_type.as_ref(),
        &value_type,
        true,
    );
    if let Some(expr) = expr {
        check_table_expr(
            context,
            model,
            NodeOrToken::Node(index_expr.syntax().clone()),
            &expr,
            source_type.as_ref(),
        );
    }
    Some(())
}

fn check_local_stat(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    local: &LuaLocalStat,
) -> Option<()> {
    let vars = local.get_local_name_list().collect::<Vec<_>>();
    let value_exprs = local.get_value_exprs().collect::<Vec<_>>();
    let value_types: Vec<(LuaType, TextRange)> = value_exprs
        .iter()
        .map(|e| {
            let ty = model.infer_expr(e.clone()).unwrap_or(LuaType::Any);
            (ty, e.get_range())
        })
        .collect();

    for (idx, var) in vars.iter().enumerate() {
        let name_token = var.get_name_token()?;
        let pos = name_token.get_position();
        // Use name_token range as fallback when decl tree isn't available
        let range = model
            .get_decl_range(DeclPosition(pos))
            .unwrap_or_else(|| name_token.get_range());
        let var_type = model.get_type_by_decl_position(DeclPosition(pos));
        let value_type = value_types.get(idx)?.0.clone();
        check_assign_type_mismatch(context, model, range, var_type.as_ref(), &value_type, false);
        if let Some(expr) = value_exprs.get(idx) {
            check_table_expr(
                context,
                model,
                NodeOrToken::Node(var.syntax().clone()),
                expr,
                var_type.as_ref(),
            );
        }
    }
    Some(())
}

/// 检查整个表, 返回`true`表示诊断出异常.
pub fn check_table_expr(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    _decl_node: NodeOrToken<emmylua_parser::LuaSyntaxNode, emmylua_parser::LuaSyntaxToken>,
    table_expr: &LuaExpr,
    table_type: Option<&LuaType>, // 记录的类型
) -> Option<bool> {
    let table_type = table_type?;
    if let Some(table_expr) = LuaTableExpr::cast(table_expr.syntax().clone()) {
        return check_table_expr_content(context, model, table_type, &table_expr);
    }
    Some(false)
}

// 处理 value_expr 是 TableExpr 的情况
fn check_table_expr_content(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    table_type: &LuaType,
    table_expr: &LuaTableExpr,
) -> Option<bool> {
    const MAX_CHECK_COUNT: usize = 250;
    let mut check_count = 0;
    let mut has_diagnostic = false;

    let fields = table_expr.get_fields_with_keys();

    for (idx, (field, field_key)) in fields.iter().enumerate() {
        check_count += 1;
        if check_count > MAX_CHECK_COUNT {
            return Some(has_diagnostic);
        }
        let Some(value_expr) = field.get_value_expr() else {
            continue;
        };

        let expr_type = model.infer_expr(value_expr.clone()).unwrap_or(LuaType::Any);

        // 位于的最后的 TableFieldValue 允许接受函数调用返回的多值
        if field.is_value_field()
            && idx == fields.len() - 1
            && let LuaType::Variadic(variadic) = &expr_type
        {
            if let Some(result) = check_table_last_variadic_type(
                context,
                model,
                table_type,
                idx,
                variadic,
                field.get_range(),
            ) {
                has_diagnostic = has_diagnostic || result;
            }
            continue;
        }

        let Some(member_key) = model.get_member_key(field_key.clone()) else {
            continue;
        };

        let source_type = match model.infer_member_type(table_type, &member_key) {
            Ok(typ) => typ,
            Err(_) => {
                continue;
            }
        };

        if (source_type.is_table() || source_type.is_custom_type())
            && let Some(table_expr) = LuaTableExpr::cast(value_expr.syntax().clone())
        {
            // 检查子表
            if let Some(result) =
                check_table_expr_content(context, model, &source_type, &table_expr)
            {
                has_diagnostic = has_diagnostic || result;
            }
            continue;
        }

        let allow_nil = matches!(table_type, LuaType::Array(_));

        if let Some(result) = check_assign_type_mismatch(
            context,
            model,
            field.get_range(),
            Some(&source_type),
            &expr_type,
            allow_nil,
        ) {
            has_diagnostic = has_diagnostic || result;
        }
    }

    Some(has_diagnostic)
}

fn check_table_last_variadic_type(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    table_type: &LuaType,
    idx: usize,
    value_variadic: &VariadicType,
    range: TextRange,
) -> Option<bool> {
    for offset in idx..(idx + 10) {
        let member_key = LuaMemberKey::Integer((idx + offset) as i64 + 1);
        let source_type = model.infer_member_type(table_type, &member_key).ok()?;
        match source_type {
            LuaType::Variadic(source_variadic) => {
                return Some(source_variadic.deref() != value_variadic);
            }
            _ => {
                let expr_type = value_variadic.get_type(offset)?;

                if let Some(result) = check_assign_type_mismatch(
                    context,
                    model,
                    range,
                    Some(&source_type),
                    expr_type,
                    false,
                ) && result
                {
                    return Some(true);
                }
            }
        }
    }

    Some(false)
}

fn check_assign_type_mismatch(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    range: TextRange,
    source_type: Option<&LuaType>,
    value_type: &LuaType,
    allow_nil: bool,
) -> Option<bool> {
    let source_type = source_type.unwrap_or(&LuaType::Any);
    // 如果一致, 则不进行类型检查
    if source_type == value_type {
        return Some(false);
    }

    // 某些情况下我们应允许可空
    if allow_nil && value_type.is_nullable() {
        return Some(false);
    }

    match (&source_type, &value_type) {
        (LuaType::Def(_), LuaType::Def(_) | LuaType::Ref(_)) => {}
        (LuaType::Def(_), _) => return Some(false),
        (LuaType::Ref(_) | LuaType::Tuple(_) | LuaType::Generic(_), LuaType::TableConst(_)) => {
            return Some(false);
        }
        (LuaType::Nil, _) => return Some(false),
        (LuaType::Ref(_), LuaType::Instance(instance)) => {
            if instance.get_base().is_table() {
                return Some(false);
            }
        }
        _ => {}
    }

    let result = model.type_check_detail(source_type, value_type);
    if result.is_err() {
        add_type_check_diagnostic(context, model, range, source_type, value_type, &result);
        return Some(true);
    }
    Some(false)
}

fn add_type_check_diagnostic(
    context: &mut DiagnosticContext,
    _model: &SemanticModel,
    range: TextRange,
    source_type: &LuaType,
    value_type: &LuaType,
    result: &TypeCheckResult,
) {
    match result {
        Ok(_) => (),
        Err(reason) => {
            let reason_message = match reason {
                TypeCheckFailReason::TypeNotMatchWithReason(reason) => reason.clone(),
                TypeCheckFailReason::TypeRecursion => t!("type recursion").to_string(),
                _ => "".to_string(),
            };

            context.add_diagnostic(
                DiagnosticCode::AssignTypeMismatch,
                range,
                t!(
                    "Cannot assign `%{value}` to `%{source}`. %{reason}",
                    value = humanize_lint_type_salsa(
                        context.get_salsa_db(),
                        context.get_file_id(),
                        value_type
                    ),
                    source = humanize_lint_type_salsa(
                        context.get_salsa_db(),
                        context.get_file_id(),
                        source_type
                    ),
                    reason = reason_message
                )
                .to_string(),
                None,
            );
        }
    }
}
