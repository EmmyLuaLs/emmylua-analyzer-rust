use std::sync::Arc;

use internment::ArcIntern;
use rowan::TextRange;
use smol_str::SmolStr;

use crate::{GlobalId, InFiled, LuaGenericType, LuaTypeDeclId};

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum LuaMemberOwner {
    LocalUnresolve,
    Type(LuaTypeDeclId),
    Element(InFiled<TextRange>),
    GlobalPath(GlobalId),
    Generic(Arc<LuaGenericType>),
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

impl From<SmolStr> for LuaMemberOwner {
    fn from(path: SmolStr) -> Self {
        Self::GlobalPath(GlobalId::new(&path))
    }
}

impl From<ArcIntern<SmolStr>> for LuaMemberOwner {
    fn from(path: ArcIntern<SmolStr>) -> Self {
        Self::GlobalPath(GlobalId(path.clone()))
    }
}

impl From<GlobalId> for LuaMemberOwner {
    fn from(global_id: GlobalId) -> Self {
        Self::GlobalPath(global_id)
    }
}

impl From<LuaGenericType> for LuaMemberOwner {
    fn from(generic: LuaGenericType) -> Self {
        Self::Generic(Arc::new(generic))
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

    pub fn get_path(&self) -> Option<&GlobalId> {
        match self {
            LuaMemberOwner::GlobalPath(path) => Some(path),
            _ => None,
        }
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self, LuaMemberOwner::LocalUnresolve)
    }
}
