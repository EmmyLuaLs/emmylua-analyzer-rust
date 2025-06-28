use emmylua_parser::{LuaVersionCondition, LuaVersionNumber};

use crate::{db_index::LuaType, DbIndex, FileId, LuaExport, LuaSemanticDeclId};

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
    pub property_owner_id: Option<LuaSemanticDeclId>,
    pub is_meta: bool,
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

    pub fn is_export(&self, db: &DbIndex) -> bool {
        let Some(property_owner_id) = &self.property_owner_id else {
            return false;
        };

        db.get_property_index()
            .get_property(property_owner_id)
            .and_then(|property| property.export.as_ref())
            .is_some()
    }

    pub fn get_export<'a>(&self, db: &'a DbIndex) -> Option<&'a LuaExport> {
        let property_owner_id = self.property_owner_id.as_ref()?;
        let export = db
            .get_property_index()
            .get_property(property_owner_id)?
            .export
            .as_ref()?;

        Some(export)
    }
}
