use smol_str::SmolStr;

use crate::LuaType;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericParam {
    pub name: SmolStr,
    pub type_constraint: Option<LuaType>,
}

impl GenericParam {
    pub fn new(name: SmolStr, type_constraint: Option<LuaType>) -> Self {
        Self {
            name,
            type_constraint,
        }
    }
}
