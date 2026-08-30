//! type_access_modifier: inconsistent access modifiers for same-named types across scopes (restored from the M2 disabled list).
//!
//! Mirrors the old `diagnostic::checker::type_access_modifier`:
//! for each type definition in the current file (deduplicated by full_name), collect visible same-named definitions
//! in the File -> Internal -> Global scope buckets; if the modifier set has more than one value and a
//! definition exists in the current file, report once per location.

use std::collections::{BTreeSet, HashSet};

use crate::DiagnosticCode;
use crate::WorkspaceId;
use crate::salsa_builder::def::{TypeScope, TypeVisibility};
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct InconsistentTypeAccessModifierChecker;

impl Checker for InconsistentTypeAccessModifierChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::InconsistentTypeAccessModifier];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(facts) = semantic_model.file_facts() else {
            return;
        };
        let file_id = semantic_model.file_id();
        let mut visited_type_names = HashSet::new();
        let mut pending_diagnostics = Vec::new();

        for def in &facts.type_defs {
            if !visited_type_names.insert(def.full_name.clone()) {
                continue;
            }
            let mut modifiers = BTreeSet::new();
            let mut current_file_ranges = Vec::new();
            for scope in [
                TypeScope::Global,
                TypeScope::Internal(WorkspaceId::MAIN),
                TypeScope::File(file_id),
            ] {
                for visible in semantic_model.type_defs_in_scope(scope, &def.full_name) {
                    modifiers.insert(AccessModifier::from_visibility(visible.visibility));
                    if visible.file_id == file_id {
                        current_file_ranges.push(visible.name_range);
                    }
                }
            }

            if current_file_ranges.is_empty() || modifiers.len() <= 1 {
                continue;
            }

            let modifiers = modifiers
                .iter()
                .map(AccessModifier::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            let message = t!(
                "Type '%{name}' has inconsistent access modifiers: %{modifiers}.",
                name = def.full_name,
                modifiers = modifiers
            )
            .to_string();
            for range in current_file_ranges {
                pending_diagnostics.push((range, message.clone()));
            }
        }

        for (range, message) in pending_diagnostics {
            context.add_diagnostic(
                DiagnosticCode::InconsistentTypeAccessModifier,
                range,
                message,
            );
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum AccessModifier {
    Public,
    Internal,
    File,
}

impl AccessModifier {
    fn from_visibility(visibility: TypeVisibility) -> Self {
        match visibility {
            TypeVisibility::Public => Self::Public,
            TypeVisibility::Internal => Self::Internal,
            TypeVisibility::Private => Self::File,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::File => "file",
        }
    }
}
