use emmylua_parser::{LuaVersionCondition, LuaVersionNumber, VisibilityKind};
use smol_str::SmolStr;

use crate::{FileId, LuaType, ModuleVisibility, WorkspaceId};

/// Module tree node id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleNodeId {
    pub id: u32,
}

/// Module tree node (equivalent to old `LuaModuleIndex::ModuleNode`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleNode {
    pub children: Vec<(SmolStr, ModuleNodeId)>,
    pub file_ids: Vec<FileId>,
    pub parent: Option<ModuleNodeId>,
}

/// Module info in the salsa layer (equivalent to old `LuaModuleIndex::ModuleInfo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleInfo {
    pub file_id: FileId,
    pub full_module_name: SmolStr,
    pub name: SmolStr,
    pub visible: ModuleVisibility,
    pub workspace_id: WorkspaceId,
    pub is_meta: bool,
    pub version_conds: Vec<LuaVersionCondition>,
    pub export_type: Option<LuaType>,
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
        match self.visible {
            ModuleVisibility::Public => true,
            ModuleVisibility::Internal => {
                // If the current module is not a library module (i.e. it is a built-in module), it can be required.
                (!self.workspace_id.is_library() && !workspace_id.is_library())
                    || self.workspace_id == workspace_id
            }
            ModuleVisibility::Hide => false,
        }
    }

    pub fn has_export_type(&self) -> bool {
        self.export_type.is_some()
    }

    fn matches_version(&self, version_number: &LuaVersionNumber) -> bool {
        self.version_conds.is_empty()
            || self
                .version_conds
                .iter()
                .any(|cond| cond.check(version_number))
    }
}
