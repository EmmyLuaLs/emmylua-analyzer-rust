//! undefined_global: references names that are neither local, global, nor built-in.

use emmylua_parser::LuaSyntaxKind;

use crate::DiagnosticCode;
use crate::salsa_builder::def::SemanticId;
use crate::semantic_model::SemanticModel;

use super::super::builtin::is_builtin_global;
use super::{CheckContext, Checker};
pub struct UndefinedGlobal;

impl Checker for UndefinedGlobal {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::UndefinedGlobal];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(uses) = semantic_model.name_uses() else {
            return;
        };
        let tree = semantic_model.syntax_tree();
        let root = tree.map(|t| t.get_red_root());
        for use_ in uses {
            // A definition position (assignment target, parent is AssignStat) is not a reference; skip it.
            if let Some(root) = &root
                && let Some(node) = use_.syntax.to_node_from_root(root)
                && node
                    .parent()
                    .is_some_and(|parent| parent.kind() == LuaSyntaxKind::AssignStat.into())
            {
                continue;
            }
            // Already resolved to a local declaration.
            if semantic_model
                .resolve_name(use_.syntax.get_range().start())
                .is_some()
            {
                continue;
            }
            // Lua built-in global.
            if is_builtin_global(use_.name.as_str()) {
                continue;
            }
            // `emmyrc.diagnostics.globals` / `globals_regex` whitelist.
            if context.is_global_disabled(use_.name.as_str()) {
                continue;
            }
            // Global variable/type in the workspace.
            let owner = SemanticId::name(use_.name.clone());
            if semantic_model.resolve_owner(&owner).is_some() {
                continue;
            }
            context.add_diagnostic(
                DiagnosticCode::UndefinedGlobal,
                use_.syntax.get_range(),
                t!("undefined global variable: `%{name}`", name = use_.name),
            );
        }
    }
}
