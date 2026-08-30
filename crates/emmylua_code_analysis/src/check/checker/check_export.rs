use std::collections::HashSet;
use std::ops::Deref;

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaCallExpr, LuaExpr, LuaIndexExpr, LuaIndexKey, LuaSyntaxKind, LuaVarExpr,
    NumberResult,
};

use crate::DiagnosticCode;
use crate::salsa_builder::def::ModuleExport;
use crate::salsa_builder::facts::FileFacts;
use crate::semantic_model::SemanticModel;
use crate::{FileId, InFiled, LuaMemberKey, LuaType};

use super::{CheckContext, Checker};
use crate::semantic_model::render::humanize_type;

pub struct CheckExportChecker;

impl Checker for CheckExportChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::InjectField, DiagnosticCode::UndefinedField];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        let mut checked_index_exprs = HashSet::new();
        for node in root.descendants().filter_map(LuaAst::cast) {
            match node {
                LuaAst::LuaAssignStat(assign) => {
                    let (vars, _) = assign.get_var_and_expr_list();
                    for var in vars.iter() {
                        if let LuaVarExpr::IndexExpr(index_expr) = var {
                            checked_index_exprs.insert(index_expr.get_syntax_id());
                            check_index_expr(
                                context,
                                semantic_model,
                                index_expr,
                                DiagnosticCode::InjectField,
                            );
                        }
                    }
                }
                LuaAst::LuaIndexExpr(index_expr) => {
                    if checked_index_exprs.contains(&index_expr.get_syntax_id()) {
                        continue;
                    }
                    check_index_expr(
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

fn check_index_expr(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    index_expr: &LuaIndexExpr,
    code: DiagnosticCode,
) {
    let Some(prefix) = index_expr.get_prefix_expr() else {
        return;
    };
    let prefix_ty = semantic_model.type_of_expr(prefix.get_syntax_id());
    // Only handle tables imported via require; local tables in the same file may be freely extended.
    let Some(module_file) = import_module_file(semantic_model, &prefix) else {
        return;
    };
    let Some(module_facts) = semantic_model.file_facts_of(module_file) else {
        return;
    };
    let Some(export_ty) = export_table_type(module_file, module_facts) else {
        return;
    };
    let Some(index_key) = index_expr.get_index_key() else {
        return;
    };
    let Some(key) = lua_member_key(semantic_model, &index_key) else {
        return;
    };
    // If the export surface contains the member → valid.
    if semantic_model.member_info(&export_ty, &key).is_some() {
        return;
    }
    let field_name = index_key.get_path_part();
    let range = index_key
        .get_range()
        .unwrap_or_else(|| index_expr.get_range());
    match code {
        DiagnosticCode::InjectField => {
            context.add_diagnostic(
                DiagnosticCode::InjectField,
                range,
                t!(
                    "Fields cannot be injected into the reference of `%{class}` for `%{field}`. ",
                    class = humanize_type(semantic_model, &prefix_ty),
                    field = field_name
                ),
            );
        }
        DiagnosticCode::UndefinedField => {
            context.add_diagnostic(
                DiagnosticCode::UndefinedField,
                range,
                t!("Undefined field `%{field}`. ", field = field_name),
            );
        }
        _ => {}
    }
}

/// Table type of a module export surface (`return { ... }` / `local M = {}; return M`).
/// Non-table exports (functions / named types) return `None` and are left to undefined_field and other checkers.
fn export_table_type(module_file: FileId, facts: &FileFacts) -> Option<LuaType> {
    let syntax = match &facts.module_export {
        ModuleExport::Expr { value_syntax } => Some(*value_syntax),
        ModuleExport::Decl { decl, .. } => facts
            .decl_by_id(decl)
            .and_then(|decl| decl.value_expr_syntax),
        ModuleExport::Global { .. } | ModuleExport::None => None,
    }?;
    if !matches!(
        syntax.get_kind(),
        LuaSyntaxKind::TableArrayExpr
            | LuaSyntaxKind::TableObjectExpr
            | LuaSyntaxKind::TableEmptyExpr
    ) {
        return None;
    }
    Some(LuaType::TableConst(InFiled::new(
        module_file,
        syntax.get_range(),
    )))
}

/// Prefix is a require import:
/// - directly `require("mod")`;
/// - a name reference bound to a `local m = require("mod")` declaration.
fn import_module_file(semantic_model: &SemanticModel<'_>, prefix: &LuaExpr) -> Option<FileId> {
    let call_expr = match prefix {
        LuaExpr::CallExpr(call_expr) if call_expr.is_require() => call_expr.clone(),
        LuaExpr::NameExpr(name_expr) => {
            let decl_id = semantic_model.resolve_name(name_expr.get_position())?;
            let facts = semantic_model.file_facts()?;
            let decl = facts.decl_by_id(&decl_id)?;
            let value_syntax = decl.value_expr_syntax?;
            let tree = semantic_model.syntax_tree()?;
            let node = value_syntax.to_node_from_root(&tree.get_red_root())?;
            let call_expr = LuaCallExpr::cast(node)?;
            call_expr.is_require().then_some(call_expr)?
        }
        _ => return None,
    };
    let module_name = require_arg_type(semantic_model, &call_expr)?;
    semantic_model.module_file_of(&module_name)
}

/// String module name of require (literal / VM-inferable constant).
fn require_arg_type(semantic_model: &SemanticModel<'_>, call_expr: &LuaCallExpr) -> Option<String> {
    let args_list = call_expr.get_args_list()?;
    let arg_expr = args_list.get_args().next()?;
    match semantic_model.type_of_expr(arg_expr.get_syntax_id()) {
        LuaType::StringConst(s) | LuaType::DocStringConst(s) => Some(s.deref().to_string()),
        _ => None,
    }
}

/// `LuaIndexKey` → domain `LuaMemberKey` (string/integer constant keys).
fn lua_member_key(semantic_model: &SemanticModel<'_>, key: &LuaIndexKey) -> Option<LuaMemberKey> {
    match key {
        LuaIndexKey::Name(name) => Some(LuaMemberKey::Name(name.get_name_text().into())),
        LuaIndexKey::String(string) => Some(LuaMemberKey::Name(string.get_value().into())),
        LuaIndexKey::Integer(integer) => match integer.get_number_value() {
            NumberResult::Int(idx) => Some(LuaMemberKey::Integer(idx)),
            _ => None,
        },
        LuaIndexKey::Idx(idx) => Some(LuaMemberKey::Integer(*idx as i64)),
        LuaIndexKey::Expr(expr) => match semantic_model.type_of_expr(expr.get_syntax_id()) {
            LuaType::StringConst(s) | LuaType::DocStringConst(s) => {
                Some(LuaMemberKey::Name(s.deref().clone()))
            }
            LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => {
                Some(LuaMemberKey::Integer(i))
            }
            _ => None,
        },
    }
}
