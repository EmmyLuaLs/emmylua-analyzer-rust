//! # Value alias origin tracing
//!
//! For simple aliases like `local b = a` / `local f = t.func`, hover and goto need
//! to trace back to the real declaration (function definition / member definition), so the original signature and comments can be shown.

use std::collections::HashSet;

use emmylua_code_analysis::{ModuleExport, SalsaSemanticModel, SemanticId};
use emmylua_parser::{LuaAstNode, LuaAstToken, LuaExpr, LuaIndexExpr};

/// Follow the value initialization chain of a local variable / member to its final declared identity.
///
/// - `local a = b` → the declaration of `b`
/// - `local f = t.func` → the member declaration of `t.func`
/// - `function M:f() end` / `local function f() end` return themselves directly
pub fn resolve_alias_origin(
    model: &SalsaSemanticModel<'_>,
    decl: &SemanticId,
) -> Option<SemanticId> {
    let mut visited = HashSet::new();
    let mut current = decl.clone();
    loop {
        if !visited.insert(current.clone()) {
            return None;
        }
        match &current {
            SemanticId::Decl(key) => {
                let file_model = model.model_for(key.file_id)?;
                let facts = file_model.file_facts()?;
                let decl_info = facts.decl_by_id(&current)?;
                let Some(value_syntax) = decl_info.value_expr_syntax else {
                    // Parameters/uninitialized locals have no traceable value expression: the declaration itself is the final origin.
                    return Some(current);
                };
                let tree = file_model.syntax_tree()?;
                let node = value_syntax.to_node_from_root(&tree.get_red_root())?;
                let expr = LuaExpr::cast(node)?;
                match expr {
                    LuaExpr::ClosureExpr(_) => return Some(current),
                    LuaExpr::NameExpr(name_expr) => {
                        current = file_model.resolve_name(name_expr.get_position())?;
                    }
                    LuaExpr::IndexExpr(index_expr) => {
                        current = file_model
                            .resolve_member(&index_expr)
                            .and_then(|resolved| resolved.member_id)
                            .or_else(|| resolve_require_index_member(&file_model, &index_expr))?;
                    }
                    LuaExpr::CallExpr(call) if call.is_require() => {
                        let arg = call.get_args_list()?.get_args().next()?;
                        let module_name = string_literal_of(&arg)?;
                        let module_file = file_model.module_file_of(&module_name)?;
                        let module_facts = file_model.file_facts_of(module_file)?;
                        match &module_facts.module_export {
                            ModuleExport::Decl { decl, .. } => {
                                current = decl.clone();
                            }
                            ModuleExport::Global { name } => {
                                current = file_model
                                    .global_decl(name.as_str())
                                    .unwrap_or_else(|| SemanticId::name(name.clone()));
                            }
                            _ => return None,
                        }
                    }
                    _ => return None,
                }
            }
            SemanticId::Member(key) => {
                let file_model = model.model_for(key.file_id)?;
                let facts = file_model.file_facts()?;
                let member = facts.member_by_id(&current)?;
                // Method definitions/function-valued fields are themselves the final definition.
                if member.is_method {
                    return Some(current);
                }
                let Some(value_syntax) = member.value_syntax else {
                    return Some(current);
                };
                let tree = file_model.syntax_tree()?;
                let node = value_syntax.to_node_from_root(&tree.get_red_root())?;
                let Some(expr) = LuaExpr::cast(node) else {
                    return Some(current);
                };
                match expr {
                    LuaExpr::ClosureExpr(_) => return Some(current),
                    LuaExpr::NameExpr(name_expr) => {
                        current = file_model.resolve_name(name_expr.get_position())?;
                    }
                    LuaExpr::IndexExpr(index_expr) => {
                        current = file_model
                            .resolve_member(&index_expr)
                            .and_then(|resolved| resolved.member_id)
                            .or_else(|| resolve_require_index_member(&file_model, &index_expr))?;
                    }
                    _ => return Some(current),
                }
            }
            other => return Some(other.clone()),
        }
    }
}

/// Resolves a direct `require("mod").field` member expression. When `resolve_member`
/// does not recognize a require call prefix, this looks up the member via the module export owner.
fn resolve_require_index_member(
    model: &SalsaSemanticModel<'_>,
    index_expr: &LuaIndexExpr,
) -> Option<SemanticId> {
    let LuaExpr::CallExpr(call) = index_expr.get_prefix_expr()? else {
        return None;
    };
    if !call.is_require() {
        return None;
    }
    let arg = call.get_args_list()?.get_args().next()?;
    let module_name = string_literal_of(&arg)?;
    let module_file = model.module_file_of(&module_name)?;
    let module_facts = model.file_facts_of(module_file)?;
    let owner = match &module_facts.module_export {
        ModuleExport::Decl { decl, .. } => decl.clone(),
        ModuleExport::Expr { value_syntax } => {
            SemanticId::member(module_file, value_syntax.get_range())
        }
        _ => return None,
    };
    let key = index_expr.get_index_key()?.get_path_part().to_string();
    let module_model = model.model_for(module_file)?;
    module_model
        .members_of_owner(&owner)
        .into_iter()
        .find(|member| member.name.as_str() == key)
        .map(|member| member.id.clone())
}

/// File containing the semantic id (returns `None` when a TypeDef Global/Internal scope has no single file).
fn string_literal_of(expr: &LuaExpr) -> Option<String> {
    let token = expr
        .syntax()
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
        .find_map(emmylua_parser::LuaLiteralToken::cast)?;
    match token {
        emmylua_parser::LuaLiteralToken::String(string) => Some(string.get_value()),
        _ => None,
    }
}

pub fn semantic_id_file(id: &SemanticId) -> Option<emmylua_code_analysis::FileId> {
    match id {
        SemanticId::Decl(key) => Some(key.file_id),
        SemanticId::Member(key) => Some(key.file_id),
        SemanticId::TypeDef(key) => match key.scope {
            emmylua_code_analysis::TypeScope::File(file_id) => Some(file_id),
            _ => None,
        },
        _ => None,
    }
}
