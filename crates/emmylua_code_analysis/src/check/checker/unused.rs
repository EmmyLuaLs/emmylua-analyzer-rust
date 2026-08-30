//! unused: local variables never referenced after declaration.

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct UnusedChecker;

impl Checker for UnusedChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::Unused];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(decls) = semantic_model.decls() else {
            return;
        };
        for decl in decls {
            // Only local variables are checked; `_` prefix is treated as intentional.
            if !decl.kind.is_local() || decl.name.starts_with('_') {
                continue;
            }
            if semantic_model.decl_references(&decl.id).is_empty() {
                context.add_diagnostic(
                    DiagnosticCode::Unused,
                    decl.name_range,
                    t!("%{name} is never used, if this is intentional, prefix it with an underscore: _%{name}", name = decl.name),
                );
            }
        }
    }
}
