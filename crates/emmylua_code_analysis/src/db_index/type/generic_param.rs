use smol_str::SmolStr;

use crate::{GenericTplId, LuaAttributeUse, LuaType};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericParam {
    pub name: SmolStr,
    pub tpl_id: Option<GenericTplId>,
    pub type_constraint: Option<LuaType>,
    pub default_type: Option<LuaType>,
    pub attributes: Option<Vec<LuaAttributeUse>>,
}

impl GenericParam {
    pub fn new(
        name: SmolStr,
        type_constraint: Option<LuaType>,
        default_type: Option<LuaType>,
        attributes: Option<Vec<LuaAttributeUse>>,
    ) -> Self {
        Self {
            name,
            tpl_id: None,
            type_constraint,
            default_type,
            attributes,
        }
    }

    pub fn with_tpl_id(mut self, tpl_id: Option<GenericTplId>) -> Self {
        self.tpl_id = tpl_id;
        self
    }
}
