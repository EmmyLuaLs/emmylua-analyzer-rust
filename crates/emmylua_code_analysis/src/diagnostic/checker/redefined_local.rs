use hashbrown::{HashMap, HashSet};

use emmylua_parser::LuaSyntaxKind;

use crate::{
    DiagnosticCode, LuaDecl, LuaDeclId, LuaDeclarationTree, LuaScope, LuaScopeKind, ScopeOrDeclId,
    SemanticModel,
};

use super::{Checker, DiagnosticContext};

pub struct RedefinedLocalChecker;

impl Checker for RedefinedLocalChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::RedefinedLocal];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let file_id = semantic_model.get_file_id();
        let Some(decl_tree) = semantic_model
            .get_compilation()
            .legacy_db()
            .get_decl_index()
            .get_decl_tree(&file_id)
        else {
            return;
        };

        let Some(root_scope) = decl_tree.get_root_scope() else {
            return;
        };
        let mut diagnostics = HashSet::new();
        let mut root_locals = HashMap::new();

        check_scope_for_redefined_locals(decl_tree, root_scope, &mut root_locals, &mut diagnostics);

        // 添加诊断信息
        for decl_id in diagnostics {
            if let Some(decl) = decl_tree.get_decl(&decl_id) {
                context.add_diagnostic(
                    DiagnosticCode::RedefinedLocal,
                    decl.get_range(),
                    t!("Redefined local variable `%{name}`", name = decl.get_name()).to_string(),
                    None,
                );
            }
        }
    }
}

fn check_scope_for_redefined_locals(
    decl_tree: &LuaDeclarationTree,
    scope: &LuaScope,
    parent_locals: &mut HashMap<String, LuaDeclId>,
    diagnostics: &mut HashSet<LuaDeclId>,
) {
    let should_add_to_parent = should_add_to_parent_scope(scope);

    let mut current_locals = parent_locals.clone();

    // 检查当前作用域中的声明
    for child in scope.get_children() {
        if let ScopeOrDeclId::Decl(decl_id) = child
            && let Some(decl) = decl_tree.get_decl(decl_id)
        {
            let name = decl.get_name().to_string();
            if decl.is_local() && name != "..." && !name.starts_with("_") {
                if current_locals.contains_key(&name) {
                    let old_decl = current_locals
                        .get(&name)
                        .and_then(|id| decl_tree.get_decl(id));
                    if var_name_not_conflicts_with_function_param_name(decl, old_decl).is_some() {
                        continue;
                    }

                    // 发现重定义，记录诊断
                    diagnostics.insert(*decl_id);
                }
                // 将当前声明加入映射
                current_locals.insert(name.clone(), *decl_id);
            }
        }
    }

    // 检查子作用域
    for child in scope.get_children() {
        if let ScopeOrDeclId::Scope(scope_id) = child
            && let Some(child_scope) = decl_tree.get_scope(scope_id)
        {
            check_scope_for_redefined_locals(
                decl_tree,
                child_scope,
                &mut current_locals,
                diagnostics,
            );
        }
    }

    // 更新到父作用域
    if should_add_to_parent {
        for (name, decl_id) in current_locals {
            parent_locals.insert(name, decl_id);
        }
    }
}

/// 处理 a = function(a)
fn var_name_not_conflicts_with_function_param_name(
    current_decl: &LuaDecl,
    old_decl: Option<&LuaDecl>,
) -> Option<()> {
    let old_decl = old_decl?;
    if old_decl.is_param() || !current_decl.is_param() {
        return None;
    }
    if let Some(value_syntax_id) = old_decl.get_value_syntax_id() {
        if value_syntax_id.get_kind() != LuaSyntaxKind::ClosureExpr {
            return None;
        }
        if let crate::LuaDeclExtra::Param { signature_id, .. } = current_decl.extra
            && value_syntax_id.get_range().start() == signature_id.get_position()
        {
            return Some(()); // 不冲突
        }
    }

    None
}

/// 检查是否需要加入到父作用域
fn should_add_to_parent_scope(scope: &LuaScope) -> bool {
    scope.get_kind() == LuaScopeKind::FuncStat
        || scope.get_kind() == LuaScopeKind::LocalOrAssignStat
        || scope.get_kind() == LuaScopeKind::Repeat
        || scope.get_kind() == LuaScopeKind::MethodStat
}
