use rowan::TextRange;

use crate::{GlobalId, InFiled, LuaDeclId, LuaTypeDeclId};

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum LuaMemberOwner {
    Type(LuaTypeDeclId),
    Element(InFiled<TextRange>),
    DeclId(LuaDeclId),
    Global(GlobalId),
}

impl From<LuaTypeDeclId> for LuaMemberOwner {
    fn from(decl_id: LuaTypeDeclId) -> Self {
        Self::Type(decl_id)
    }
}

impl From<InFiled<TextRange>> for LuaMemberOwner {
    fn from(range: InFiled<TextRange>) -> Self {
        Self::Element(range)
    }
}

impl LuaMemberOwner {
    pub fn get_type_id(&self) -> Option<&LuaTypeDeclId> {
        match self {
            LuaMemberOwner::Type(id) => Some(id),
            _ => None,
        }
    }

    pub fn get_element_range(&self) -> Option<&InFiled<TextRange>> {
        match self {
            LuaMemberOwner::Element(range) => Some(range),
            _ => None,
        }
    }
}
