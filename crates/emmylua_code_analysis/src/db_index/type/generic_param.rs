use smol_str::SmolStr;

use crate::{LuaAttributeUse, LuaType};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericParam {
    pub name: SmolStr,
    pub constraint: Option<LuaType>,
    pub default: Option<LuaType>,
    pub attributes: Option<Vec<LuaAttributeUse>>,
}

impl GenericParam {
    pub fn new(
        name: SmolStr,
        constraint: Option<LuaType>,
        default: Option<LuaType>,
        attributes: Option<Vec<LuaAttributeUse>>,
    ) -> Self {
        Self {
            name,
            constraint,
            default,
            attributes,
        }
    }
}
