use smol_str::SmolStr;

use crate::{LuaAttributeUse, LuaType};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericParam {
    pub name: SmolStr,
    pub type_constraint: Option<LuaType>,
    pub is_variadic: bool,
    pub attributes: Option<Vec<LuaAttributeUse>>,
}

impl GenericParam {
    pub fn new(
        name: SmolStr,
        type_constraint: Option<LuaType>,
        is_variadic: bool,
        attributes: Option<Vec<LuaAttributeUse>>,
    ) -> Self {
        Self {
            name,
            type_constraint,
            is_variadic,
            attributes,
        }
    }
}
