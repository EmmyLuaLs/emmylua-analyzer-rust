use emmylua_parser::{LuaVersionCondition, LuaVersionNumber, VisibilityKind};

use crate::{FileId, LuaSemanticDeclId, db_index::LuaType};

use super::{module_node::ModuleNodeId, workspace::WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModuleVisibility {
    /// Public
    #[default]
    Default,
    Public,
    Internal,
    Hide,
}

impl ModuleVisibility {
    pub fn from_visibility_kind(visibility: VisibilityKind) -> Option<Self> {
        match visibility {
            VisibilityKind::Public => Some(Self::Public),
            VisibilityKind::Internal => Some(Self::Internal),
            _ => None,
        }
    }

    pub fn merge(self, visibility: Self) -> Self {
        match (self, visibility) {
            (Self::Hide, _) | (_, Self::Hide) => Self::Hide,
            (Self::Default, next) => next,
            (current, Self::Default) => current,
            (_, next) => next,
        }
    }

    pub(self) fn resolve(self) -> Self {
        match self {
            Self::Default => Self::Public,
            visibility => visibility,
        }
    }

    pub fn is_hidden(self) -> bool {
        self == Self::Hide
    }
}

#[derive(Debug)]
pub struct ModuleInfo {
    pub file_id: FileId,
    pub full_module_name: String,
    pub name: String,
    pub module_id: ModuleNodeId,
    pub visible: ModuleVisibility,
    pub export_type: Option<LuaType>,
    pub version_conds: Option<Box<Vec<LuaVersionCondition>>>,
    pub workspace_id: WorkspaceId,
    pub semantic_id: Option<LuaSemanticDeclId>,
    pub is_meta: bool,
}

impl ModuleInfo {
    pub fn is_visible(&self, version_number: &LuaVersionNumber) -> bool {
        !self.visible.is_hidden() && self.matches_version(version_number)
    }

    pub fn merge_visibility(&mut self, visibility: VisibilityKind) {
        if let Some(visibility) = ModuleVisibility::from_visibility_kind(visibility) {
            self.set_visibility(self.visible.merge(visibility));
        }
    }

    pub fn set_visibility(&mut self, visibility: ModuleVisibility) {
        self.visible = visibility;
    }

    pub fn is_requireable_from(&self, workspace_id: WorkspaceId) -> bool {
        match self.visible.resolve() {
            ModuleVisibility::Public => true,
            ModuleVisibility::Internal => {
                // 如果当前模块并非库模块(即为内置模块), 则允许被 require
                (!self.workspace_id.is_library() && !workspace_id.is_library())
                    || self.workspace_id == workspace_id
            }
            ModuleVisibility::Default => true,
            ModuleVisibility::Hide => false,
        }
    }

    pub fn has_export_type(&self) -> bool {
        self.export_type.is_some()
    }

    fn matches_version(&self, version_number: &LuaVersionNumber) -> bool {
        self.version_conds
            .as_ref()
            .is_none_or(|version_conds| version_conds.iter().any(|cond| cond.check(version_number)))
    }
}
