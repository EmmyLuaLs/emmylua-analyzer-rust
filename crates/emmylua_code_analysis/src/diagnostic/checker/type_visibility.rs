use std::collections::HashMap;

use crate::{
    DbIndex, DiagnosticCode, LuaDeclLocation, LuaTypeFlag, SemanticModel,
    db_index::{LuaTypeIdentifier, WorkspaceId},
};
use rowan::TextRange;

use super::{Checker, DiagnosticContext};

pub struct InconsistentTypeVisibilityChecker;

impl Checker for InconsistentTypeVisibilityChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::InconsistentTypeVisibility];

    fn check(context: &mut DiagnosticContext, _: &SemanticModel) {
        let file_id = context.get_file_id();
        let current_workspace_id = context.get_db().resolve_workspace_id(file_id);
        let type_index = context.get_db().get_type_index();
        let mut groups: HashMap<String, VisibilityGroup> = HashMap::new();
        for type_decl in type_index.get_all_types() {
            let Some(resolved_visibility) =
                get_resolved_visibility(type_decl.get_id().get_id(), current_workspace_id)
            else {
                continue;
            };

            let group = groups
                .entry(type_decl.get_full_name().to_string())
                .or_default();
            group.merge_resolved_visibility(resolved_visibility);
            for location in type_decl.get_locations() {
                let Some(explicit_visibility) = get_explicit_visibility(context.get_db(), location)
                else {
                    continue;
                };
                group.explicit_state = Some(match group.explicit_state {
                    Some(state) => state.merge(explicit_visibility),
                    None => ExplicitVisibilityState::from_visibility(explicit_visibility),
                });

                if location.file_id == file_id {
                    group.explicit_ranges.push(location.range);
                }
            }
        }

        let mut pending_diagnostics = Vec::new();
        for (name, group) in groups {
            let Some(explicit_state) = group.explicit_state else {
                continue;
            };
            let Some(resolved_visibility) = group.resolved_visibility else {
                continue;
            };
            if group.explicit_ranges.is_empty() || explicit_state.matches(resolved_visibility) {
                continue;
            }
            let visibility = match resolved_visibility {
                ResolvedTypeVisibility::Public => "public".to_string(),
                ResolvedTypeVisibility::Internal(workspace_id) => {
                    format!("internal({})", workspace_id)
                }
            };

            let message = t!(
                "Type '%{name}' has inconsistent explicit visibility declarations; resolved visibility is %{visibility}.",
                name = name, visibility = visibility
            )
            .to_string();
            for range in group.explicit_ranges {
                pending_diagnostics.push((range, message.clone()));
            }
        }

        for (range, message) in pending_diagnostics {
            context.add_diagnostic(
                DiagnosticCode::InconsistentTypeVisibility,
                range,
                message,
                None,
            );
        }
    }
}

#[derive(Default)]
struct VisibilityGroup {
    resolved_visibility: Option<ResolvedTypeVisibility>,
    explicit_state: Option<ExplicitVisibilityState>,
    explicit_ranges: Vec<TextRange>,
}

impl VisibilityGroup {
    fn merge_resolved_visibility(&mut self, visibility: ResolvedTypeVisibility) {
        self.resolved_visibility = Some(match self.resolved_visibility {
            Some(existing_visibility) => merge_group_visibility(existing_visibility, visibility),
            None => visibility,
        });
    }
}

#[derive(Clone, Copy)]
enum ResolvedTypeVisibility {
    Public,
    Internal(WorkspaceId),
}

#[derive(Clone, Copy)]
enum ExplicitVisibilityState {
    Public,
    Internal(WorkspaceId),
    Mixed,
}

impl ExplicitVisibilityState {
    fn from_visibility(visibility: ResolvedTypeVisibility) -> Self {
        match visibility {
            ResolvedTypeVisibility::Public => Self::Public,
            ResolvedTypeVisibility::Internal(workspace_id) => Self::Internal(workspace_id),
        }
    }

    fn merge(self, visibility: ResolvedTypeVisibility) -> Self {
        match (self, visibility) {
            (Self::Mixed, _) => Self::Mixed,
            (Self::Public, ResolvedTypeVisibility::Public) => Self::Public,
            (Self::Public, ResolvedTypeVisibility::Internal(_)) => Self::Mixed,
            (Self::Internal(_), ResolvedTypeVisibility::Public) => Self::Mixed,
            (Self::Internal(left), ResolvedTypeVisibility::Internal(right)) => {
                if left == right {
                    Self::Internal(left)
                } else {
                    Self::Mixed
                }
            }
        }
    }

    fn matches(self, visibility: ResolvedTypeVisibility) -> bool {
        match (self, visibility) {
            (Self::Public, ResolvedTypeVisibility::Public) => true,
            (Self::Internal(left), ResolvedTypeVisibility::Internal(right)) => left == right,
            _ => false,
        }
    }
}

fn get_explicit_visibility(
    db: &DbIndex,
    location: &LuaDeclLocation,
) -> Option<ResolvedTypeVisibility> {
    if location.flag.contains(LuaTypeFlag::Public) {
        Some(ResolvedTypeVisibility::Public)
    } else if location.flag.contains(LuaTypeFlag::Internal) {
        Some(ResolvedTypeVisibility::Internal(
            db.resolve_workspace_id(location.file_id)?,
        ))
    } else {
        None
    }
}

fn get_resolved_visibility(
    type_identifier: &LuaTypeIdentifier,
    current_workspace_id: Option<WorkspaceId>,
) -> Option<ResolvedTypeVisibility> {
    match type_identifier {
        LuaTypeIdentifier::Global(_) => Some(ResolvedTypeVisibility::Public),
        LuaTypeIdentifier::Internal(workspace_id, _) => {
            if current_workspace_id == Some(*workspace_id) {
                Some(ResolvedTypeVisibility::Internal(*workspace_id))
            } else {
                None
            }
        }
        LuaTypeIdentifier::Local(_, _) => None,
    }
}

fn merge_group_visibility(
    left: ResolvedTypeVisibility,
    right: ResolvedTypeVisibility,
) -> ResolvedTypeVisibility {
    match (left, right) {
        (ResolvedTypeVisibility::Public, _) | (_, ResolvedTypeVisibility::Public) => {
            ResolvedTypeVisibility::Public
        }
        (
            ResolvedTypeVisibility::Internal(left_workspace),
            ResolvedTypeVisibility::Internal(right_workspace),
        ) => {
            if left_workspace == right_workspace {
                ResolvedTypeVisibility::Internal(left_workspace)
            } else {
                ResolvedTypeVisibility::Public
            }
        }
    }
}
