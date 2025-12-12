use std::collections::HashSet;

use emmylua_parser::{LuaAst, LuaAstNode, LuaCallExpr, LuaIndexExpr, LuaVarExpr};

use crate::{
    DiagnosticCode, LuaSemanticDeclId, LuaType, ModuleInfo, SemanticDeclLevel, SemanticModel,
    parse_require_module_info,
};

use super::{Checker, DiagnosticContext, check_field, humanize_lint_type};

pub struct CheckExportChecker;

impl Checker for CheckExportChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::InjectField, DiagnosticCode::UndefinedField];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        let mut checked_index_expr = HashSet::new();
        for node in root.descendants::<LuaAst>() {
            match node {
                LuaAst::LuaAssignStat(assign) => {
                    let (vars, _) = assign.get_var_and_expr_list();
                    for var in vars.iter() {
                        if let LuaVarExpr::IndexExpr(index_expr) = var {
                            checked_index_expr.insert(index_expr.syntax().clone());
                            check_export_index_expr(
                                context,
                                semantic_model,
                                index_expr,
                                DiagnosticCode::InjectField,
                            );
                        }
                    }
                }
                LuaAst::LuaIndexExpr(index_expr) => {
                    if checked_index_expr.contains(index_expr.syntax()) {
                        continue;
                    }
                    check_export_index_expr(
                        context,
                        semantic_model,
                        &index_expr,
                        DiagnosticCode::UndefinedField,
                    );
                }
                _ => {}
            }
        }
    }
}

fn check_export_index_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    index_expr: &LuaIndexExpr,
    code: DiagnosticCode,
) -> Option<()> {
    let db = context.db;
    let prefix_expr = index_expr.get_prefix_expr()?;
    let prefix_info = semantic_model.get_semantic_info(prefix_expr.syntax().clone().into())?;
    let prefix_typ = prefix_info.typ.clone();

    // `check_export` 仅需要处理 `TableConst, 其它类型由 `check_field` 负责.
    let LuaType::TableConst(table_const) = &prefix_typ else {
        return Some(());
    };

    let index_key = index_expr.get_index_key()?;

    // 检查该表是否为导入的表.
    if let Some(module_info) = check_require_table_const_with_export(semantic_model, index_expr) {
        if code == DiagnosticCode::InjectField {
            // 检查字段定义是否来自导入的表.
            if let Some(info) = semantic_model.get_semantic_info(index_expr.syntax().clone().into())
                && is_cross_file_member_from_imported_export_table_const(
                    module_info,
                    info.semantic_decl,
                )
            {
                let index_name = index_key.get_path_part();
                context.add_diagnostic(
                    DiagnosticCode::InjectField,
                    index_key.get_range()?,
                    t!(
                        "Fields cannot be injected into the reference of `%{class}` for `%{field}`. ",
                        class = humanize_lint_type(db, &prefix_typ),
                        field = index_name,
                    )
                    .to_string(),
                    None,
                );
                return Some(());
            }
        }

        if check_field::is_valid_member(semantic_model, &prefix_typ, index_expr, &index_key, code)
            .is_some()
        {
            return Some(());
        }

        let index_name = index_key.get_path_part();
        match code {
            DiagnosticCode::InjectField => {
                context.add_diagnostic(
                    DiagnosticCode::InjectField,
                    index_key.get_range()?,
                    t!(
                        "Fields cannot be injected into the reference of `%{class}` for `%{field}`. ",
                        class = humanize_lint_type(db, &prefix_typ),
                        field = index_name,
                    )
                    .to_string(),
                    None,
                );
            }
            DiagnosticCode::UndefinedField => {
                context.add_diagnostic(
                    DiagnosticCode::UndefinedField,
                    index_key.get_range()?,
                    t!("Undefined field `%{field}`. ", field = index_name,).to_string(),
                    None,
                );
            }
            _ => {}
        }

        return Some(());
    }

    // 不是导入表, 且定义位于当前文件中, 则尝试检查本地表.
    if code != DiagnosticCode::UndefinedField && table_const.file_id != semantic_model.get_file_id()
    {
        return Some(());
    }

    let Some(LuaSemanticDeclId::LuaDecl(decl_id)) = prefix_info.semantic_decl else {
        return Some(());
    };
    // 必须为 local 声明
    let decl = semantic_model
        .get_db()
        .get_decl_index()
        .get_decl(&decl_id)?;
    if !decl.is_local() {
        return Some(());
    }
    // 且该声明标记了 `export`
    let property = semantic_model
        .get_db()
        .get_property_index()
        .get_property(&decl_id.into())?;
    if property.export().is_none() {
        return Some(());
    }

    if check_field::is_valid_member(semantic_model, &prefix_typ, index_expr, &index_key, code)
        .is_some()
    {
        return Some(());
    }

    let index_name = index_key.get_path_part();
    context.add_diagnostic(
        DiagnosticCode::UndefinedField,
        index_key.get_range()?,
        t!("Undefined field `%{field}`. ", field = index_name,).to_string(),
        None,
    );

    Some(())
}

fn check_require_table_const_with_export<'a>(
    semantic_model: &'a SemanticModel,
    index_expr: &LuaIndexExpr,
) -> Option<&'a ModuleInfo> {
    // 获取前缀表达式的语义信息
    let prefix_expr = index_expr.get_prefix_expr()?;
    if let Some(call_expr) = LuaCallExpr::cast(prefix_expr.syntax().clone()) {
        let module_info = parse_require_expr_module_info(semantic_model, &call_expr)?;
        if module_info.is_export(semantic_model.get_db()) {
            return Some(module_info);
        }
    }

    let semantic_decl_id = semantic_model.find_decl(
        prefix_expr.syntax().clone().into(),
        SemanticDeclLevel::NoTrace,
    )?;
    // 检查是否是声明引用
    let decl_id = match semantic_decl_id {
        LuaSemanticDeclId::LuaDecl(decl_id) => decl_id,
        _ => return None,
    };

    // 获取声明
    let decl = semantic_model
        .get_db()
        .get_decl_index()
        .get_decl(&decl_id)?;

    let module_info = parse_require_module_info(semantic_model, &decl)?;
    if module_info.is_export(semantic_model.get_db()) {
        return Some(module_info);
    }
    None
}

fn parse_require_expr_module_info<'a>(
    semantic_model: &'a SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<&'a ModuleInfo> {
    let arg_list = call_expr.get_args_list()?;
    let first_arg = arg_list.get_args().next()?;
    let require_path_type = semantic_model.infer_expr(first_arg.clone()).ok()?;
    let module_path: String = match &require_path_type {
        LuaType::StringConst(module_path) => module_path.as_ref().to_string(),
        _ => return None,
    };

    semantic_model
        .get_db()
        .get_module_index()
        .find_module(&module_path)
}

fn is_cross_file_member_from_imported_export_table_const(
    module_info: &ModuleInfo,
    semantic_decl: Option<LuaSemanticDeclId>,
) -> bool {
    if let Some(LuaSemanticDeclId::Member(member_id)) = semantic_decl
        && module_info.file_id != member_id.file_id
    {
        return true;
    }

    false
}
