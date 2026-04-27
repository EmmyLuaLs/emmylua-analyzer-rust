use std::collections::{BTreeSet, HashSet};

use crate::{DiagnosticCode, LuaTypeFlag, SemanticModel};

use super::{Checker, DiagnosticContext};

pub struct InconsistentTypeAccessModifierChecker;

impl Checker for InconsistentTypeAccessModifierChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::InconsistentTypeAccessModifier];

    fn check(context: &mut DiagnosticContext, _: &SemanticModel) {
        let file_id = context.get_file_id();
        let workspace_id = context.type_lookup_workspace_id();
        let type_index = context.db().get_type_index();
        let mut visited_type_names = HashSet::new();
        let mut pending_diagnostics = Vec::new();

        for type_decl in type_index.get_file_type_decls(file_id) {
            let type_name = type_decl.get_full_name();
            if !visited_type_names.insert(type_name.to_string()) {
                continue;
            }
            let visible_type_decls =
                type_index.get_visible_type_decls_by_full_name(file_id, type_name, workspace_id);
            let mut modifiers = BTreeSet::new();
            let mut current_file_ranges = Vec::new();

            for visible_type_decl in visible_type_decls {
                for location in visible_type_decl.get_locations() {
                    modifiers.insert(TypeAccessModifier::from_location_flags(location.flag));

                    if location.file_id == file_id {
                        current_file_ranges.push(location.range);
                    }
                }
            }

            if current_file_ranges.is_empty() || modifiers.len() <= 1 {
                continue;
            }

            let modifiers = modifiers
                .iter()
                .map(TypeAccessModifier::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            let message = t!(
                "Type '%{name}' has inconsistent access modifiers: %{modifiers}.",
                name = type_name,
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
                None,
            );
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TypeAccessModifier {
    Public,
    Internal,
    Private,
}

impl TypeAccessModifier {
    fn from_location_flags(flags: flagset::FlagSet<LuaTypeFlag>) -> Self {
        if flags.contains(LuaTypeFlag::Private) {
            Self::Private
        } else if flags.contains(LuaTypeFlag::Internal) {
            Self::Internal
        } else {
            Self::Public
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Private => "private",
        }
    }
}
