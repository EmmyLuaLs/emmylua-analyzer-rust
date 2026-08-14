use std::ops::Deref;

use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaAstToken, LuaExpr, LuaIndexExpr, LuaLocalStat,
    LuaNameExpr, LuaTableExpr, LuaTableField, LuaVarExpr,
};
use rowan::{NodeOrToken, TextRange};

use crate::{
    AssignabilityResult, DbIndex, DiagnosticCode, LuaDeclExtra, LuaDeclId, LuaMemberId,
    LuaMemberKey, LuaSemanticDeclId, LuaType, LuaTypeCache, SemanticDeclLevel, SemanticModel,
    TypeMismatch, VariadicType, get_real_type, infer_index_expr, render_type_mismatch,
};

use super::{Checker, DiagnosticContext, humanize_lint_type};

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
    let value_types = semantic_model.infer_expr_list_types(&exprs, Some(vars.len()));

    for (idx, var) in vars.iter().enumerate() {
        match var {
            LuaVarExpr::IndexExpr(index_expr) => {
                check_index_expr(
                    context,
                    semantic_model,
                    index_expr,
                    exprs.get(idx).cloned(),
                    value_types.get(idx)?.0.clone(),
                );
            }
            LuaVarExpr::NameExpr(name_expr) => {
                check_name_expr(
                    context,
                    semantic_model,
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
    semantic_model: &SemanticModel,
    name_expr: &LuaNameExpr,
    expr: Option<LuaExpr>,
    value_type: LuaType,
) -> Option<()> {
    let semantic_decl = semantic_model.find_decl(
        NodeOrToken::Node(name_expr.syntax().clone()),
        SemanticDeclLevel::default(),
    )?;
    let source_type = match semantic_decl.clone() {
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
    let source_type = source_type.map(|source_type| {
        semantic_model
            .apply_assignment_target_casts(LuaExpr::NameExpr(name_expr.clone()), source_type)
    });
    let table_has_diagnostic = expr
        .as_ref()
        .and_then(|expr| check_table_expr(context, semantic_model, expr, source_type.as_ref()))
        .unwrap_or(false);
    if !table_has_diagnostic {
        check_assign_type_mismatch(
            context,
            semantic_model,
            name_expr.get_range(),
            source_type.as_ref(),
            &value_type,
            false,
        );
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
    let source_type = infer_index_expr(
        semantic_model.get_db(),
        &mut semantic_model.get_cache().borrow_mut(),
        index_expr.clone(),
        false,
    )
    .ok();
    let source_type = source_type.map(|source_type| {
        semantic_model
            .apply_assignment_target_casts(LuaExpr::IndexExpr(index_expr.clone()), source_type)
    });

    let table_has_diagnostic = expr
        .as_ref()
        .and_then(|expr| check_table_expr(context, semantic_model, expr, source_type.as_ref()))
        .unwrap_or(false);
    if !table_has_diagnostic {
        check_assign_type_mismatch(
            context,
            semantic_model,
            index_expr.get_range(),
            source_type.as_ref(),
            &value_type,
            true,
        );
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
    let value_types = semantic_model.infer_expr_list_types(&value_exprs, Some(vars.len()));

    for (idx, var) in vars.iter().enumerate() {
        let name_token = var.get_name_token()?;
        let decl_id = LuaDeclId::new(semantic_model.get_file_id(), name_token.get_position());
        let range = semantic_model
            .get_db()
            .get_decl_index()
            .get_decl(&decl_id)?
            .get_range();
        let var_type = semantic_model
            .get_db()
            .get_type_index()
            .get_type_cache(&decl_id.into())
            .map(|cache| cache.as_type().clone())?;
        let value_type = value_types.get(idx)?.0.clone();
        let table_has_diagnostic = value_exprs
            .get(idx)
            .and_then(|expr| check_table_expr(context, semantic_model, expr, Some(&var_type)))
            .unwrap_or(false);
        if !table_has_diagnostic {
            check_assign_type_mismatch(
                context,
                semantic_model,
                range,
                Some(&var_type),
                &value_type,
                false,
            );
        }
    }
    Some(())
}

/// 检查整个表, 返回`true`表示诊断出异常.
pub fn check_table_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    table_expr: &LuaExpr,
    table_type: Option<&LuaType>, // 记录的类型
) -> Option<bool> {
    let table_type = table_type?;
    let Some(table_expr) = LuaTableExpr::cast(table_expr.syntax().clone()) else {
        return Some(false);
    };

    check_table_expr_content(context, semantic_model, table_type, &table_expr)
}

// 处理 value_expr 是 TableExpr 的情况, 但不会处理 `local a = { x = 1 }, local v = a`
fn check_table_expr_content(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    table_type: &LuaType,
    table_expr: &LuaTableExpr,
) -> Option<bool> {
    const MAX_CHECK_COUNT: usize = 250;
    let mut check_count = 0;
    let mut has_diagnostic = false;

    let fields = table_expr.get_fields_with_keys().collect::<Vec<_>>();

    for (idx, (field, field_key)) in fields.iter().enumerate() {
        check_count += 1;
        if check_count > MAX_CHECK_COUNT {
            return Some(has_diagnostic);
        }
        let Some(value_expr) = field.get_value_expr() else {
            continue;
        };

        let expr_type = semantic_model
            .infer_expr(value_expr.clone())
            .unwrap_or(LuaType::Any);

        if let Some(annotated_type) = get_table_field_doc_type(semantic_model, field) {
            if !expr_type.is_unknown() && !expr_type.is_any() {
                if let AssignabilityResult::NotAssignable(_) =
                    semantic_model.check_assignable(&expr_type, annotated_type)
                {
                    has_diagnostic = true;
                    continue;
                }
            }
        }

        // 位于的最后的 TableFieldValue 允许接受函数调用返回的多值, 而且返回的值必然会从下标 1 开始覆盖掉所有索引字段.
        if field.is_value_field()
            && idx == fields.len() - 1
            && let LuaType::Variadic(variadic) = &expr_type
        {
            if let Some(result) = check_table_last_variadic_type(
                context,
                semantic_model,
                table_type,
                idx,
                variadic,
                field.get_range(),
            ) {
                has_diagnostic = has_diagnostic || result;
            }
            continue;
        }

        let Some(member_key) = semantic_model.get_member_key(field_key) else {
            continue;
        };

        let source_type = match semantic_model.infer_member_type(table_type, &member_key) {
            Ok(typ) => typ,
            Err(_) => {
                continue;
            }
        };

        let real_source_type = get_real_type_or_self(semantic_model.get_db(), &source_type);
        if (real_source_type.is_table() || real_source_type.is_custom_type())
            && let Some(table_expr) = LuaTableExpr::cast(value_expr.syntax().clone())
        {
            // 检查子表
            if let Some(result) =
                check_table_expr_content(context, semantic_model, &source_type, &table_expr)
            {
                has_diagnostic = has_diagnostic || result;
            }
            continue;
        }

        let allow_nil = matches!(table_type, LuaType::Array(_));

        if let Some(result) = check_assign_type_mismatch(
            context,
            semantic_model,
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
    semantic_model: &SemanticModel,
    table_type: &LuaType,
    idx: usize,
    value_variadic: &VariadicType,
    range: TextRange,
) -> Option<bool> {
    // test max 10
    for offset in idx..(idx + 10) {
        let member_key = LuaMemberKey::Integer((idx + offset) as i64 + 1);
        let source_type = semantic_model
            .infer_member_type(table_type, &member_key)
            .ok()?;
        match source_type {
            LuaType::Variadic(source_variadic) => {
                return Some(source_variadic.deref() != value_variadic);
            }
            _ => {
                let expr_type = value_variadic.get_type(offset)?;

                if let Some(result) = check_assign_type_mismatch(
                    context,
                    semantic_model,
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
    semantic_model: &SemanticModel,
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

    // 某些情况下我们应允许可空, 例如: boolean[]
    if allow_nil && value_type.is_nullable() {
        return Some(false);
    }

    let real_source_type = get_real_type_or_self(semantic_model.get_db(), source_type);
    match (real_source_type, value_type) {
        // 如果源类型是定义类型, 则仅在目标类型是定义类型或引用类型时进行类型检查
        (LuaType::Def(_), LuaType::Def(_) | LuaType::Ref(_)) => {}
        (LuaType::Def(_), _) => return Some(false),
        // 此时检查交给 table_field
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

    if let AssignabilityResult::NotAssignable(mismatch) =
        semantic_model.check_assignable(value_type, source_type)
    {
        add_type_check_diagnostic(
            context,
            semantic_model,
            range,
            source_type,
            value_type,
            &mismatch,
        );
        return Some(true);
    }
    Some(false)
}

fn add_type_check_diagnostic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    source_type: &LuaType,
    value_type: &LuaType,
    mismatch: &TypeMismatch,
) {
    let db = semantic_model.get_db();
    context.add_diagnostic(
        DiagnosticCode::AssignTypeMismatch,
        range,
        t!(
            "Cannot assign `%{value}` to `%{source}`. %{reason}",
            value = humanize_lint_type(db, value_type),
            source = humanize_lint_type(db, source_type),
            reason = render_type_mismatch(db, mismatch)
        )
        .to_string(),
        None,
    );
}

fn get_real_type_or_self<'a>(db: &'a DbIndex, ty: &'a LuaType) -> &'a LuaType {
    get_real_type(db, ty).unwrap_or(ty)
}

fn get_table_field_doc_type<'a>(
    semantic_model: &'a SemanticModel,
    field: &LuaTableField,
) -> Option<&'a LuaType> {
    let member_id = LuaMemberId::new(field.get_syntax_id(), semantic_model.get_file_id());
    let type_cache = semantic_model
        .get_db()
        .get_type_index()
        .get_type_cache(&member_id.into())?;
    match type_cache {
        LuaTypeCache::DocType(annotated_type) => Some(annotated_type),
        _ => None,
    }
}
