use serde::{Deserialize, Serialize};

use crate::{FileId, LuaDeclId, LuaMemberId, LuaSignatureId, LuaTypeDeclId};

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum LuaSemanticDeclId {
    TypeDecl(LuaTypeDeclId),
    Member(LuaMemberId),
    LuaDecl(LuaDeclId),
    Signature(LuaSignatureId),
}

impl From<LuaDeclId> for LuaSemanticDeclId {
    fn from(id: LuaDeclId) -> Self {
        LuaSemanticDeclId::LuaDecl(id)
    }
}

impl From<LuaTypeDeclId> for LuaSemanticDeclId {
    fn from(id: LuaTypeDeclId) -> Self {
        LuaSemanticDeclId::TypeDecl(id)
    }
}

impl From<LuaMemberId> for LuaSemanticDeclId {
    fn from(id: LuaMemberId) -> Self {
        LuaSemanticDeclId::Member(id)
    }
}

impl From<LuaSignatureId> for LuaSemanticDeclId {
    fn from(id: LuaSignatureId) -> Self {
        LuaSemanticDeclId::Signature(id)
    }
}

impl LuaSemanticDeclId {
    pub fn get_file_id(&self) -> Option<FileId> {
        match self {
            LuaSemanticDeclId::TypeDecl(_) => None,
            LuaSemanticDeclId::Member(id) => Some(id.file_id),
            LuaSemanticDeclId::LuaDecl(id) => Some(id.file_id),
            LuaSemanticDeclId::Signature(id) => Some(id.get_file_id()),
        }
    }
}
