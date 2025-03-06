use emmylua_parser::{LuaVersionCondition, LuaVersionNumber};

use crate::{db_index::LuaType, FileId};

use super::{module_node::ModuleNodeId, workspace::WorkspaceId};

#[derive(Debug)]
pub struct ModuleInfo {
    pub file_id: FileId,
    pub full_module_name: String,
    pub name: String,
    pub module_id: ModuleNodeId,
    pub visible: bool,
    pub export_type: Option<LuaType>,
    pub version_conds: Option<Box<Vec<LuaVersionCondition>>>,
    pub workspace_id: WorkspaceId,
}

impl ModuleInfo {
    pub fn is_visible(&self, version_number: &LuaVersionNumber) -> bool {
        if !self.visible {
            return false;
        }

        if let Some(version_conds) = &self.version_conds {
            for cond in version_conds.iter() {
                if cond.check(version_number) {
                    return true;
                }
            }

            return false;
        }

        true
    }
}
