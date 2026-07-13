//! Inconsistent type access modifier — pure salsa.

use std::collections::HashSet;

use crate::DiagnosticCode;
use crate::compilation::SalsaDocVisibilityKindSummary;
use crate::semantic_model::SemanticModel;

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let type_names = model.file_type_names();
    let mut visited = HashSet::new();

    for name in &type_names {
        if !visited.insert(name.clone()) {
            continue;
        }
        let Some(entries) = model.type_def_entries(name) else {
            continue;
        };

        let mut visibilities = HashSet::new();
        for e in &entries {
            let vis_str = match e.visibility {
                SalsaDocVisibilityKindSummary::Public => "public",
                SalsaDocVisibilityKindSummary::Private => "private",
                SalsaDocVisibilityKindSummary::Protected => "protected",
                SalsaDocVisibilityKindSummary::Package => "package",
                SalsaDocVisibilityKindSummary::Internal => "internal",
            };
            visibilities.insert(vis_str);
        }

        if visibilities.len() > 1 {
            let modifiers: Vec<&str> = visibilities.into_iter().collect();
            let msg = t!(
                "Type '%{name}' has inconsistent access modifiers: %{modifiers}.",
                name = name,
                modifiers = modifiers.join(", ")
            )
            .to_string();
            // Report on current file's entries
            for e in &entries {
                if e.file_id == model.get_file_id() {
                    let range = rowan::TextRange::new(e.syntax_offset, e.syntax_offset);
                    context.add_diagnostic(
                        DiagnosticCode::InconsistentTypeAccessModifier,
                        range,
                        msg.clone(),
                        None,
                    );
                }
            }
        }
    }
}
