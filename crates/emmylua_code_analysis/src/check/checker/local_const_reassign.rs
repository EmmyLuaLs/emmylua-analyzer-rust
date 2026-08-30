//! local_const_reassign: reassignment of `<const>` local variables / for-loop variables.
//!
//! Mirrors the old `diagnostic::checker::local_const_reassign`:
//! `<const>` locals -> `LocalConstReassign`; for-loop variables -> `IterVariableReassign`.

use emmylua_parser::LuaSyntaxKind;

use crate::DiagnosticCode;
use crate::salsa_builder::def::DeclKind;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct LocalConstReassignChecker;

impl Checker for LocalConstReassignChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::LocalConstReassign,
        DiagnosticCode::IterVariableReassign,
    ];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let (Some(decls), Some(uses)) = (semantic_model.decls(), semantic_model.name_uses()) else {
            return;
        };
        let tree = semantic_model.syntax_tree();
        let root = tree.map(|t| t.get_red_root());
        for decl in decls {
            let (is_const, is_iter) = match decl.kind {
                DeclKind::Local { is_const, is_iter } => (is_const, is_iter),
                _ => continue,
            };
            if !is_const && !is_iter {
                continue;
            }
            for use_ in uses {
                if use_.name != decl.name {
                    continue;
                }
                // Write position: name is an assignment target (parent is AssignStat).
                let Some(root) = &root else { continue };
                let Some(node) = use_.syntax.to_node_from_root(root) else {
                    continue;
                };
                let is_write = node
                    .parent()
                    .is_some_and(|parent| parent.kind() == LuaSyntaxKind::AssignStat.into());
                if !is_write {
                    continue;
                }
                // The write position resolves to this const declaration -> reassignment.
                if semantic_model
                    .resolve_name(use_.syntax.get_range().start())
                    .is_some_and(|decl_id| decl_id == decl.id)
                {
                    if is_iter {
                        context.add_diagnostic(
                            DiagnosticCode::IterVariableReassign,
                            use_.syntax.get_range(),
                            t!("Should not reassign to iter variable."),
                        );
                    } else {
                        context.add_diagnostic(
                            DiagnosticCode::LocalConstReassign,
                            use_.syntax.get_range(),
                            t!("Cannot reassign to a constant variable."),
                        );
                    }
                }
            }
        }
    }
}
