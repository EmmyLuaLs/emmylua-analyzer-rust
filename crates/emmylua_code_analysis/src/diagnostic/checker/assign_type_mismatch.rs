use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaAstToken, LuaExpr, LuaIndexExpr, LuaLocalStat,
    LuaNameExpr, LuaVarExpr,
};
use rowan::{NodeOrToken, TextRange};

use crate::{
    AssignabilityResult, DiagnosticCode, LuaDeclExtra, LuaDeclId, LuaSemanticDeclId, LuaType,
    LuaUnionType, SemanticDeclLevel, SemanticModel, infer_index_expr,
};

use super::{
    Checker, DiagnosticContext, DiagnosticMessage, humanize_lint_type, render_diagnostic_detail,
    table::check_table_assignment_diagnostics,
};

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
) {
    let (vars, exprs) = assign.get_var_and_expr_list();
    let source_types = semantic_model.infer_expr_list_types(&exprs, Some(vars.len()));

    for (idx, var) in vars.iter().enumerate() {
        let Some(source_type) = source_types.get(idx).map(|it| &it.0) else {
            continue;
        };
        match var {
            LuaVarExpr::IndexExpr(index_expr) => {
                check_index_expr(
                    context,
                    semantic_model,
                    index_expr,
                    exprs.get(idx),
                    source_type,
                );
            }
            LuaVarExpr::NameExpr(name_expr) => {
                check_name_expr(
                    context,
                    semantic_model,
                    name_expr,
                    exprs.get(idx),
                    source_type,
                );
            }
        }
    }
}

fn check_name_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    name_expr: &LuaNameExpr,
    expr: Option<&LuaExpr>,
    source_type: &LuaType,
) -> Option<()> {
    let semantic_decl = semantic_model.find_decl(
        NodeOrToken::Node(name_expr.syntax().clone()),
        SemanticDeclLevel::default(),
    )?;
    let (target_type, is_doc_type) = match semantic_decl {
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
                    (param_type.type_ref.clone(), true)
                }
                _ => {
                    let type_cache = semantic_model
                        .get_db()
                        .get_type_index()
                        .get_type_cache(&decl_id.into())?;
                    (type_cache.as_type().clone(), type_cache.is_doc())
                }
            }
        }
        _ => return Some(()),
    };

    // 显式 cast 会改变赋值目标类型, 必须在判断推断 nil 前应用.
    let target_type = semantic_model
        .apply_assignment_target_casts(LuaExpr::NameExpr(name_expr.clone()), target_type);
    if !is_doc_type && target_type.is_nil() {
        return Some(());
    }

    let table_handled = expr.is_some_and(|expr| {
        check_table_assignment_diagnostics(context, semantic_model, expr, source_type, &target_type)
            .is_handled()
    });
    if !table_handled && !is_allowed_def_assignment(semantic_model, source_type, &target_type, expr)
    {
        check_assign_type_mismatch(
            context,
            semantic_model,
            name_expr.get_range(),
            source_type,
            &target_type,
        );
    }

    Some(())
}

fn check_index_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    index_expr: &LuaIndexExpr,
    expr: Option<&LuaExpr>,
    source_type: &LuaType,
) -> Option<()> {
    let target_type = infer_index_expr(
        semantic_model.get_db(),
        &mut semantic_model.get_cache().borrow_mut(),
        index_expr.clone(),
        false,
    )
    .ok()?;
    let target_type = semantic_model
        .apply_assignment_target_casts(LuaExpr::IndexExpr(index_expr.clone()), target_type);

    let table_handled = expr.is_some_and(|expr| {
        check_table_assignment_diagnostics(context, semantic_model, expr, source_type, &target_type)
            .is_handled()
    });
    if !table_handled && !source_type.is_nil() && !target_type.is_nil() {
        let nullable_target = (source_type.is_nullable() && !target_type.is_nullable())
            .then(|| LuaUnionType::Nullable(target_type.clone()).into());
        let target_type = nullable_target.as_ref().unwrap_or(&target_type);
        check_assign_type_mismatch(
            context,
            semantic_model,
            index_expr.get_range(),
            source_type,
            target_type,
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
    let source_types = semantic_model.infer_expr_list_types(&value_exprs, Some(vars.len()));

    for (idx, var) in vars.iter().enumerate() {
        let name_token = var.get_name_token()?;
        let decl_id = LuaDeclId::new(semantic_model.get_file_id(), name_token.get_position());
        let range = semantic_model
            .get_db()
            .get_decl_index()
            .get_decl(&decl_id)?
            .get_range();
        let Some(type_cache) = semantic_model
            .get_db()
            .get_type_index()
            .get_type_cache(&decl_id.into())
        else {
            continue;
        };
        let target_type = type_cache.as_type();
        if type_cache.is_infer() && target_type.is_nil() {
            continue;
        }
        let Some(source_type) = source_types.get(idx).map(|it| &it.0) else {
            continue;
        };
        let value_expr = value_exprs.get(idx);
        let table_handled = value_expr.is_some_and(|expr| {
            check_table_assignment_diagnostics(
                context,
                semantic_model,
                expr,
                source_type,
                target_type,
            )
            .is_handled()
        });
        if !table_handled
            && !is_allowed_def_assignment(semantic_model, source_type, target_type, value_expr)
        {
            check_assign_type_mismatch(context, semantic_model, range, source_type, target_type);
        }
    }
    Some(())
}

// 处理声明类型定义时无法由普通类型关系表达的初始化形式.
fn is_allowed_def_assignment(
    semantic_model: &SemanticModel,
    source_type: &LuaType,
    target_type: &LuaType,
    value_expr: Option<&LuaExpr>,
) -> bool {
    let LuaType::Def(type_decl_id) = target_type else {
        return false;
    };
    if matches!(source_type, LuaType::Global) || matches!(value_expr, Some(LuaExpr::TableExpr(_))) {
        return true;
    }
    matches!(value_expr, Some(LuaExpr::CallExpr(call_expr)) if call_expr.is_setmetatable())
        && semantic_model
            .get_db()
            .get_type_index()
            .get_type_decl(type_decl_id)
            .is_some_and(|type_decl| type_decl.is_class())
}

fn check_assign_type_mismatch(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    source_type: &LuaType,
    target_type: &LuaType,
) {
    let AssignabilityResult::NotAssignable(mismatch) =
        semantic_model.check_assignable(source_type, target_type)
    else {
        return;
    };
    let db = semantic_model.get_db();
    context.add_diagnostic(
        DiagnosticCode::AssignTypeMismatch,
        range,
        DiagnosticMessage::with_detail(
            t!(
                "Cannot assign `%{value}` to `%{source}`.",
                value = humanize_lint_type(db, source_type),
                source = humanize_lint_type(db, target_type),
            )
            .to_string(),
            render_diagnostic_detail(db, &mismatch, source_type, target_type),
        ),
        None,
    );
}
