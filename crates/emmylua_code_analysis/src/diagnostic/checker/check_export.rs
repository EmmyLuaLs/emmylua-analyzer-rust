use std::collections::HashSet;

use emmylua_parser::{LuaAst, LuaAstNode, LuaCallExpr, LuaIndexExpr, LuaNameExpr, LuaVarExpr};

use crate::{
    CompilationModuleInfo, DiagnosticCode, LuaSemanticDeclId, LuaType, SemanticDeclLevel,
    SemanticModel,
    module_query::export::{compilation_module_has_export_type, compilation_module_semantic_id},
    module_query::identity::find_compilation_module_by_path,
    parse_require_module_info, parse_require_module_info_by_name_expr,
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
    let db = context.get_compilation().legacy_db();
    let prefix_expr = index_expr.get_prefix_expr()?;
    let imported_export_surface =
        check_require_table_const_with_export_surface(semantic_model, index_expr).is_some();

    let index_key = index_expr.get_index_key()?;
    let index_name = index_key.get_path_part();

    if imported_export_surface {
        let module_info = find_required_export_surface_module(semantic_model, index_expr)?;
        let has_member = semantic_model
            .get_compilation()
            .module_export_has_member(module_info.file_id, index_name.as_str());
        let prefix_typ = semantic_model
            .get_semantic_info(prefix_expr.syntax().clone().into())
            .map(|info| info.typ.clone())
            .unwrap_or(LuaType::Unknown);
        match code {
            DiagnosticCode::InjectField => {
                if !has_member {
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
            }
                DiagnosticCode::UndefinedField => {
                    if !has_member {
                        context.add_diagnostic(
                            DiagnosticCode::UndefinedField,
                            index_key.get_range()?,
                            t!("Undefined field `%{field}`. ", field = index_name,).to_string(),
                            None,
                        );
                    }
                }
            _ => {}
        }

        return Some(());
    }

    let prefix_info = semantic_model.get_semantic_info(prefix_expr.syntax().clone().into())?;
    let prefix_typ = prefix_info.typ.clone();

    // `check_export` 仅需要处理 `TableConst, 其它类型由 `check_field` 负责.
    let LuaType::TableConst(table_const) = &prefix_typ else {
        return Some(());
    };

    // 不是导入表, 且定义位于当前文件中, 则尝试检查本地表.
    if code != DiagnosticCode::UndefinedField && table_const.file_id != semantic_model.get_file_id()
    {
        return Some(());
    }

    let Some(LuaSemanticDeclId::LuaDecl(decl_id)) = prefix_info.semantic_decl else {
        return Some(());
    };
    // 必须为 local 声明
    let decl = semantic_model.get_decl(&decl_id)?;
    if !decl.is_local() {
        return Some(());
    }
    let Some(module_info) = semantic_model.get_module() else {
        return Some(());
    };
    if !compilation_module_has_export_type(semantic_model.get_compilation(), module_info.file_id)
        || compilation_module_semantic_id(semantic_model.get_compilation(), module_info.file_id)
            != Some(LuaSemanticDeclId::LuaDecl(decl_id))
    {
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

fn find_required_export_surface_module<'a>(
    semantic_model: &'a SemanticModel,
    index_expr: &LuaIndexExpr,
) -> Option<&'a CompilationModuleInfo> {
    check_require_table_const_with_export_surface(semantic_model, index_expr)
}

fn check_require_table_const_with_export_surface<'a>(
    semantic_model: &'a SemanticModel,
    index_expr: &LuaIndexExpr,
) -> Option<&'a CompilationModuleInfo> {
    // 获取前缀表达式的语义信息
    let prefix_expr = index_expr.get_prefix_expr()?;
    if let Some(call_expr) = LuaCallExpr::cast(prefix_expr.syntax().clone()) {
        return parse_require_expr_module_info(semantic_model, &call_expr);
    }
    if let Some(name_expr) = LuaNameExpr::cast(prefix_expr.syntax().clone()) {
        if let Some(module_info) = parse_require_module_info_by_name_expr(semantic_model, &name_expr) {
            return Some(module_info);
        }
    }

    let semantic_decl_id = semantic_model
        .get_semantic_info(prefix_expr.syntax().clone().into())
        .and_then(|info| info.semantic_decl)
        .or_else(|| {
            semantic_model.find_decl(prefix_expr.syntax().clone().into(), SemanticDeclLevel::NoTrace)
        })?;
    // 检查是否是声明引用
    let decl_id = match semantic_decl_id {
        LuaSemanticDeclId::LuaDecl(decl_id) => decl_id,
        _ => return None,
    };

    // 获取声明
    let decl = semantic_model.get_decl(&decl_id)?;

    parse_require_module_info(semantic_model, &decl)
}

fn parse_require_expr_module_info<'a>(
    semantic_model: &'a SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<&'a CompilationModuleInfo> {
    let arg_list = call_expr.get_args_list()?;
    let first_arg = arg_list.get_args().next()?;
    let require_path_type = semantic_model.infer_expr(first_arg.clone()).ok()?;
    let module_path: String = match &require_path_type {
        LuaType::StringConst(module_path) => module_path.as_ref().to_string(),
        _ => return None,
    };

    find_compilation_module_by_path(semantic_model.get_compilation(), &module_path)
}
