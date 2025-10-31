use smol_str::SmolStr;

use crate::{LuaAttributeUse, LuaType};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericParam {
    pub name: SmolStr,
    pub type_constraint: Option<LuaType>,
    pub attributes: Option<Vec<LuaAttributeUse>>,
}

impl GenericParam {
    pub fn new(
        name: SmolStr,
        type_constraint: Option<LuaType>,
        attributes: Option<Vec<LuaAttributeUse>>,
    ) -> Self {
        Self {
            name,
            type_constraint,
            attributes,
        }
    }
}
