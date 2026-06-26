//! Duplicate field checker — pure salsa.

use hashbrown::HashMap;

use crate::DiagnosticCode;
use crate::compilation::{
    SalsaPropertyKeySummary, SalsaPropertySourceSummary, WorkspacePropertyEntry,
};
use crate::semantic_model::SemanticModel;

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let Some(decl_tree) = model.decl_tree() else {
        return;
    };
    let mut visited_types = std::collections::HashSet::new();

    for decl in &decl_tree.decls {
        let name = decl.name.to_string();
        // Only process types (Ref/Def refer to type decls)
        if !visited_types.insert(name.clone()) {
            continue;
        }
        let Some(members) = model.get_property_entries(&name) else {
            continue;
        };

        let mut by_key: HashMap<&SalsaPropertyKeySummary, Vec<&WorkspacePropertyEntry>> =
            HashMap::new();
        for m in &members {
            by_key.entry(&m.key).or_default().push(m);
        }

        for (key, entries) in &by_key {
            if entries.len() < 2 {
                continue;
            }

            // Check for duplicate decls (DocField)
            let doc_fields: Vec<_> = entries
                .iter()
                .filter(|e| matches!(e.source, SalsaPropertySourceSummary::DocField))
                .collect();
            if doc_fields.len() > 1 {
                for e in &doc_fields {
                    if e.file_id == model.get_file_id() {
                        let key_name = key_to_string(key);
                        context.add_diagnostic(
                            DiagnosticCode::DuplicateDocField,
                            rowan::TextRange::new(
                                rowan::TextSize::from(0u32),
                                rowan::TextSize::from(0u32),
                            ),
                            t!("Duplicate field `%{name}`.", name = key_name).to_string(),
                            None,
                        );
                    }
                }
            }

            // Check for duplicate set fields (TableField → set via assignment)
            let set_fields: Vec<_> = entries
                .iter()
                .filter(|e| matches!(e.source, SalsaPropertySourceSummary::TableField))
                .collect();
            if set_fields.len() > 1 {
                for e in &set_fields {
                    if e.file_id == model.get_file_id() {
                        context.add_diagnostic(
                            DiagnosticCode::DuplicateSetField,
                            rowan::TextRange::new(
                                rowan::TextSize::from(0u32),
                                rowan::TextSize::from(0u32),
                            ),
                            t!("Duplicate field `%{name}`.", name = key_to_string(key)).to_string(),
                            None,
                        );
                    }
                }
            }
        }
    }
}

fn key_to_string(key: &SalsaPropertyKeySummary) -> String {
    match key {
        SalsaPropertyKeySummary::Name(n) => n.to_string(),
        SalsaPropertyKeySummary::Integer(i) => i.to_string(),
        _ => "?".to_string(),
    }
}
