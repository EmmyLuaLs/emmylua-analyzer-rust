use emmylua_parser::LuaSyntaxId;
use rowan::TextSize;

use crate::{FileId, InFiled, LuaDeclId, LuaMemberId};

use super::LuaType;

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub enum LuaTypeOwner {
    Decl(LuaDeclId),
    Member(LuaMemberId),
    SyntaxId(InFiled<LuaSyntaxId>),
}

impl From<LuaDeclId> for LuaTypeOwner {
    fn from(decl_id: LuaDeclId) -> Self {
        Self::Decl(decl_id)
    }
}

impl From<LuaMemberId> for LuaTypeOwner {
    fn from(member_id: LuaMemberId) -> Self {
        Self::Member(member_id)
    }
}

impl From<InFiled<LuaSyntaxId>> for LuaTypeOwner {
    fn from(syntax_id: InFiled<LuaSyntaxId>) -> Self {
        Self::SyntaxId(syntax_id)
    }
}

impl LuaTypeOwner {
    pub fn get_file_id(&self) -> FileId {
        match self {
            LuaTypeOwner::Decl(id) => id.file_id,
            LuaTypeOwner::Member(id) => id.file_id,
            LuaTypeOwner::SyntaxId(id) => id.file_id,
        }
    }

    pub fn get_position(&self) -> TextSize {
        match self {
            LuaTypeOwner::Decl(id) => id.position,
            LuaTypeOwner::Member(id) => id.get_position(),
            LuaTypeOwner::SyntaxId(id) => id.value.get_range().start(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum LuaTypeCache {
    DocType(LuaType),
    InferType(LuaType),
}

impl LuaTypeCache {
    pub fn as_type(&self) -> &LuaType {
        match self {
            LuaTypeCache::DocType(ty) => ty,
            LuaTypeCache::InferType(ty) => ty,
        }
    }

    pub fn is_infer(&self) -> bool {
        matches!(self, LuaTypeCache::InferType(_))
    }

    pub fn is_doc(&self) -> bool {
        matches!(self, LuaTypeCache::DocType(_))
    }
}

impl std::ops::Deref for LuaTypeCache {
    type Target = LuaType;

    fn deref(&self) -> &Self::Target {
        self.as_type()
    }
}
