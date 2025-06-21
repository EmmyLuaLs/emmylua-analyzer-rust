use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaAstToken, LuaExpr, LuaIndexExpr, LuaLocalStat,
    LuaNameExpr, LuaTableExpr, LuaVarExpr,
};
use rowan::TextRange;

use crate::{
    DiagnosticCode, LuaDeclExtra, LuaDeclId, LuaSemanticDeclId, LuaType, LuaTypeCache,
    SemanticDeclLevel, SemanticModel, TypeCheckFailReason, TypeCheckResult,
};

use super::{humanize_lint_type, Checker, DiagnosticContext};

pub struct AssignTypeMismatchChecker;

impl Checker for AssignTypeMismatchChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::AssignTypeMismatch];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        for node in semantic_model.get_root().descendants::<LuaAst>() {
            match node {
                LuaAst::LuaAssignStat(assign) => {
                    check_assign_stat(context, semantic_model, &assign);
                }
                LuaAst::LuaLocalStat(local) => {
                    check_local_stat(context, semantic_model, &local);
                }
                _ => {}
            }
        }
    }
}

fn check_assign_stat(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    assign: &LuaAssignStat,
) -> Option<()> {
    let (vars, exprs) = assign.get_var_and_expr_list();
    let value_types =
        semantic_model.infer_multi_value_adjusted_expression_types(&exprs, Some(vars.len()));

    for (idx, var) in vars.iter().enumerate() {
        match var {
            LuaVarExpr::IndexExpr(index_expr) => {
                check_index_expr(
                    context,
                    semantic_model,
                    index_expr,
                    exprs.get(idx).map(|expr| expr.clone()),
                    value_types.get(idx)?.0.clone(),
                );
            }
            LuaVarExpr::NameExpr(name_expr) => {
                check_name_expr(
                    context,
                    semantic_model,
                    name_expr,
                    exprs.get(idx).map(|expr| expr.clone()),
                    value_types.get(idx)?.0.clone(),
                );
            }
        }
    }
    Some(())
}

fn check_name_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    name_expr: &LuaNameExpr,
    expr: Option<LuaExpr>,
    value_type: LuaType,
) -> Option<()> {
    let semantic_decl = semantic_model.find_decl(
        rowan::NodeOrToken::Node(name_expr.syntax().clone()),
        SemanticDeclLevel::default(),
    )?;
    let origin_type = match semantic_decl {
        LuaSemanticDeclId::LuaDecl(decl_id) => {
            let decl = semantic_model
                .get_db()
                .get_decl_index()
                .get_decl(&decl_id)?;
            match decl.extra {
                LuaDeclExtra::Param {
                    idx, signature_id, ..
                } => {
                    let signature = semantic_model
                        .get_db()
                        .get_signature_index()
                        .get(&signature_id)?;
                    let param_type = signature.get_param_info_by_id(idx)?;
                    Some(param_type.type_ref.clone())
                }
                _ => semantic_model
                    .get_db()
                    .get_type_index()
                    .get_type_cache(&decl_id.into())
                    .map(|cache| cache.as_type().clone()),
            }
        }
        _ => None,
    };
    check_assign_type_mismatch(
        context,
        semantic_model,
        name_expr.get_range(),
        origin_type.clone(),
        value_type,
        false,
    );
    if let Some(expr) = expr {
        handle_value_is_table_expr(context, semantic_model, origin_type, &expr);
    }

    Some(())
}

fn check_index_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    index_expr: &LuaIndexExpr,
    expr: Option<LuaExpr>,
    value_type: LuaType,
) -> Option<()> {
    let semantic_info =
        semantic_model.get_semantic_info(rowan::NodeOrToken::Node(index_expr.syntax().clone()))?;
    let mut typ = None;
    match semantic_info.semantic_decl {
        // 如果是已显示定义的成员, 我们不能获取其经过类型缩窄后的类型
        Some(LuaSemanticDeclId::Member(member_id)) => {
            let type_cache = semantic_model
                .get_db()
                .get_type_index()
                .get_type_cache(&member_id.into());
            if let Some(type_cache) = type_cache {
                match type_cache {
                    LuaTypeCache::DocType(ty) => {
                        typ = Some(ty.clone());
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    if typ.is_none() {
        typ = Some(semantic_info.typ);
    }

    check_assign_type_mismatch(
        context,
        semantic_model,
        index_expr.get_range(),
        typ.clone(),
        value_type,
        true,
    );
    if let Some(expr) = expr {
        handle_value_is_table_expr(context, semantic_model, typ, &expr);
    }
    Some(())
}

fn check_local_stat(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    local: &LuaLocalStat,
) -> Option<()> {
    let vars = local.get_local_name_list().collect::<Vec<_>>();
    let value_exprs = local.get_value_exprs().collect::<Vec<_>>();
    let value_types =
        semantic_model.infer_multi_value_adjusted_expression_types(&value_exprs, Some(vars.len()));

    for (idx, var) in vars.iter().enumerate() {
        let name_token = var.get_name_token()?;
        let decl_id = LuaDeclId::new(semantic_model.get_file_id(), name_token.get_position());
        let range = semantic_model
            .get_db()
            .get_decl_index()
            .get_decl(&decl_id)?
            .get_range();
        let name_type = semantic_model
            .get_db()
            .get_type_index()
            .get_type_cache(&decl_id.into())
            .map(|cache| cache.as_type().clone())?;
        check_assign_type_mismatch(
            context,
            semantic_model,
            range,
            Some(name_type.clone()),
            value_types.get(idx)?.0.clone(),
            false,
        );
        if let Some(expr) = value_exprs.get(idx).map(|expr| expr) {
            handle_value_is_table_expr(context, semantic_model, Some(name_type), &expr);
        }
    }
    Some(())
}

// 处理 value_expr 是 TableExpr 的情况, 但不会处理 `local a = { x = 1 }, local v = a`
fn handle_value_is_table_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    table_type: Option<LuaType>,
    value_expr: &LuaExpr,
) -> Option<()> {
    let table_type = table_type?;
    let fields = LuaTableExpr::cast(value_expr.syntax().clone())?
        .get_fields()
        .collect::<Vec<_>>();
    if fields.len() > 50 {
        // 如果字段过多, 则不进行类型检查
        return Some(());
    }

    for field in fields {
        // if field.is_value_field() {
        //     continue;
        // }

        let field_key = field.get_field_key();
        if let Some(field_key) = field_key {
            let member_key = semantic_model.get_member_key(&field_key)?;
            let source_type = match semantic_model.infer_member_type(&table_type, &member_key) {
                Ok(typ) => Some(typ),
                Err(_) => {
                    continue;
                }
            };

            let expr = field.get_value_expr();
            if let Some(expr) = expr {
                // Handle nested table expressions recursively
                if LuaTableExpr::cast(expr.syntax().clone()).is_some() {
                    handle_value_is_table_expr(context, semantic_model, source_type.clone(), &expr);
                    // 如果continue，则字段类型为非table，但是提供了table的情况下，会检查不出来
                    // continue;
                } else if field.is_value_field() {
                    continue;
                }

                let expr_type = semantic_model.infer_expr(expr).unwrap_or(LuaType::Any);

                let allow_nil = match table_type {
                    LuaType::Array(_) => true,
                    _ => false,
                };

                check_assign_type_mismatch(
                    context,
                    semantic_model,
                    field.get_range(),
                    source_type,
                    expr_type,
                    allow_nil,
                );
            }
        }
    }

    Some(())
}

fn check_assign_type_mismatch(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    source_type: Option<LuaType>,
    value_type: LuaType,
    allow_nil: bool,
) -> Option<()> {
    let source_type = source_type.unwrap_or(LuaType::Any);
    // 如果一致, 则不进行类型检查
    if source_type == value_type {
        return Some(());
    }

    // 某些情况下我们应允许可空, 例如: boolean[]
    if allow_nil && value_type.is_nullable() {
        return Some(());
    }

    match (&source_type, &value_type) {
        // 如果源类型是定义类型, 则仅在目标类型是定义类型或引用类型时进行类型检查
        (LuaType::Def(_), LuaType::Def(_) | LuaType::Ref(_)) => {}
        (LuaType::Def(_), _) => return Some(()),
        // 此时检查交给 table_field
        (LuaType::Ref(_) | LuaType::Tuple(_), LuaType::TableConst(_)) => return Some(()),
        // 如果源类型是nil, 则不进行类型检查
        (LuaType::Nil, _) => return Some(()),
        // // fix issue #196
        (LuaType::Ref(_), LuaType::Instance(instance)) => {
            if instance.get_base().is_table() {
                return Some(());
            }
        }
        _ => {}
    }

    let result = semantic_model.type_check(&source_type, &value_type);
    if !result.is_ok() {
        add_type_check_diagnostic(
            context,
            semantic_model,
            range,
            &source_type,
            &value_type,
            result,
        );
    }
    Some(())
}

fn add_type_check_diagnostic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    source_type: &LuaType,
    value_type: &LuaType,
    result: TypeCheckResult,
) {
    let db = semantic_model.get_db();
    match result {
        Ok(_) => return,
        Err(reason) => match reason {
            TypeCheckFailReason::TypeNotMatchWithReason(reason) => {
                context.add_diagnostic(
                    DiagnosticCode::AssignTypeMismatch,
                    range,
                    t!(
                        "Cannot assign `%{value}` to `%{source}`. %{reason}",
                        value = humanize_lint_type(db, &value_type),
                        source = humanize_lint_type(db, &source_type),
                        reason = reason
                    )
                    .to_string(),
                    None,
                );
            }
            _ => {
                context.add_diagnostic(
                    DiagnosticCode::AssignTypeMismatch,
                    range,
                    t!(
                        "Cannot assign `%{value}` to `%{source}`. %{reason}",
                        value = humanize_lint_type(db, &value_type),
                        source = humanize_lint_type(db, &source_type),
                        reason = ""
                    )
                    .to_string(),
                    None,
                );
            }
        },
    }
}
