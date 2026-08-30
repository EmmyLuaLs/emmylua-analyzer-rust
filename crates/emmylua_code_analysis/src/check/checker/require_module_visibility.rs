//! require_module_visibility: the required module is not visible (restored from the M2 disabled list; UnresolvedRequire
//! is handled by the `unresolved_require` checker).
//!
//! Module visibility facts (`FileFacts.module_visibility`): `---@meta no-require`/`---@meta _` -> Hide;
//! the export target tag of the first top-level return (NameExpr uses the declaration tag; anonymous table uses the return statement tag)
//! `---@internal` -> Internal; default Public.
//! M0: salsa has no multi-workspace partitioning, so Internal is always treated as not externally visible.

use emmylua_parser::{LuaAstNode, LuaCallExpr};

use crate::DiagnosticCode;
use crate::LuaType;
use crate::salsa_builder::def::ModuleVisibility;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct RequireModuleVisibilityChecker;

impl Checker for RequireModuleVisibilityChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::RequireModuleNotVisible];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for call_expr in root.descendants().filter_map(LuaCallExpr::cast) {
            if !call_expr.is_require() {
                continue;
            }
            check_require(context, semantic_model, &call_expr);
        }
    }
}

fn check_require(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    call_expr: &LuaCallExpr,
) {
    let Some(args_list) = call_expr.get_args_list() else {
        return;
    };
    let Some(arg_expr) = args_list.get_args().next() else {
        return;
    };
    let ty = semantic_model.type_of_expr(arg_expr.get_syntax_id());
    let module_path = match &ty {
        LuaType::StringConst(s) | LuaType::DocStringConst(s) => s.as_ref().to_string(),
        _ => return,
    };
    let Some(module_file) = semantic_model.module_file_of(&module_path) else {
        return; // UnresolvedRequire is reported by the unresolved_require checker
    };
    let Some(facts) = semantic_model.file_facts_of(module_file) else {
        return;
    };
    if facts.module_visibility != ModuleVisibility::Public {
        context.add_diagnostic(
            DiagnosticCode::RequireModuleNotVisible,
            arg_expr.get_range(),
            t!(
                "module '%{module}' visibility is not `public`",
                module = module_path
            ),
        );
    }
}
