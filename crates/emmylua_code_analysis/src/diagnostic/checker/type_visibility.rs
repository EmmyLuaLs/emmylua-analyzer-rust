use emmylua_parser::{LuaAstNode, LuaAstToken, LuaDocNameType};

use crate::{
    DbIndex, DiagnosticCode, LuaDeclLocation, LuaTypeFlag, SemanticModel, TypeVisibility,
    db_index::WorkspaceId, is_type_decl_visible,
};

use super::{Checker, DiagnosticContext};

pub struct InvisibleTypeReferenceChecker;

impl Checker for InvisibleTypeReferenceChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::InvisibleTypeReference];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let file_id = semantic_model.get_file_id();
        let db = semantic_model.get_db();
        let root = semantic_model.get_root().clone();
        for name_type in root.descendants::<LuaDocNameType>() {
            let Some(name) = name_type.get_name_text() else {
                continue;
            };
            if name_type.get_generic_param().is_some() {
                continue;
            }

            let Some(type_decl) = db.get_type_index().find_type_decl(file_id, &name) else {
                continue;
            };
            if is_type_decl_visible(db, file_id, &type_decl.get_id()).unwrap_or(true) {
                continue;
            }

            let Some(name_token) = name_type.get_name_token() else {
                continue;
            };
            let message = match type_decl.get_visibility() {
                TypeVisibility::Public => continue,
                TypeVisibility::Internal(workspace_id) => t!(
                    "Type '%{name}' is internal to workspace '%{workspace}' and cannot be used here.",
                    name = name,
                    workspace = workspace_id
                )
                .to_string(),
            };
            context.add_diagnostic(
                DiagnosticCode::InvisibleTypeReference,
                name_token.get_range(),
                message,
                None,
            );
        }
    }
}

pub struct InconsistentTypeVisibilityChecker;

impl Checker for InconsistentTypeVisibilityChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::InconsistentTypeVisibility];

    fn check(context: &mut DiagnosticContext, _: &SemanticModel) {
        let file_id = context.get_file_id();
        let type_index = context.get_db().get_type_index();
        let mut pending_diagnostics = Vec::new();
        for type_decl in type_index.get_all_types() {
            if type_decl.get_id().is_local() {
                continue;
            }

            let mut explicit_state: Option<ExplicitVisibilityState> = None;
            let mut explicit_ranges = Vec::new();
            for location in type_decl.get_locations() {
                let Some(explicit_visibility) = get_explicit_visibility(context.get_db(), location)
                else {
                    continue;
                };
                explicit_state = Some(match explicit_state {
                    Some(state) => state.merge(explicit_visibility),
                    None => ExplicitVisibilityState::from_visibility(explicit_visibility),
                });

                if location.file_id == file_id {
                    explicit_ranges.push(location.range);
                }
            }

            let Some(explicit_state) = explicit_state else {
                continue;
            };
            if explicit_ranges.is_empty() || explicit_state.matches(type_decl.get_visibility()) {
                continue;
            }
            let visibility = match type_decl.get_visibility() {
                TypeVisibility::Public => "public".to_string(),
                TypeVisibility::Internal(workspace_id) => format!("internal({})", workspace_id),
            };

            let message = t!(
                "Type '%{name}' has inconsistent explicit visibility declarations; resolved visibility is %{visibility}.",
                name = type_decl.get_full_name(), visibility = visibility
            )
            .to_string();
            for range in explicit_ranges {
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

#[derive(Clone, Copy)]
enum ExplicitVisibilityState {
    Public,
    Internal(WorkspaceId),
    Mixed,
}

impl ExplicitVisibilityState {
    fn from_visibility(visibility: TypeVisibility) -> Self {
        match visibility {
            TypeVisibility::Public => Self::Public,
            TypeVisibility::Internal(workspace_id) => Self::Internal(workspace_id),
        }
    }

    fn merge(self, visibility: TypeVisibility) -> Self {
        match (self, visibility) {
            (Self::Mixed, _) => Self::Mixed,
            (Self::Public, TypeVisibility::Public) => Self::Public,
            (Self::Public, TypeVisibility::Internal(_)) => Self::Mixed,
            (Self::Internal(_), TypeVisibility::Public) => Self::Mixed,
            (Self::Internal(left), TypeVisibility::Internal(right)) => {
                if left == right {
                    Self::Internal(left)
                } else {
                    Self::Mixed
                }
            }
        }
    }

    fn matches(self, visibility: TypeVisibility) -> bool {
        match (self, visibility) {
            (Self::Public, TypeVisibility::Public) => true,
            (Self::Internal(left), TypeVisibility::Internal(right)) => left == right,
            _ => false,
        }
    }
}

fn get_explicit_visibility(db: &DbIndex, location: &LuaDeclLocation) -> Option<TypeVisibility> {
    if location.flag.contains(LuaTypeFlag::Public) {
        Some(TypeVisibility::Public)
    } else if location.flag.contains(LuaTypeFlag::Internal) {
        Some(TypeVisibility::Internal(
            db.get_module_index()
                .get_workspace_id(location.file_id)
                .or_else(|| {
                    if db.get_vfs().is_remote_file(&location.file_id) {
                        Some(WorkspaceId::REMOTE)
                    } else {
                        None
                    }
                })?,
        ))
    } else {
        None
    }
}
