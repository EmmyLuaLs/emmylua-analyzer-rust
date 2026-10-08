//! Global semantic identity `SemanticId` (<=16 bytes: 8-byte `ArcIntern` pointer + discriminant).
//!
//! Each variant is the globally unique id of the corresponding definition:
//! - `Decl`: file + name token range (declaration location).
//! - `Member`: file + member-key token range (**declaration location** - identity, file-dependent; the lookup key is the file-independent `LuaMemberKey`).
//! - `TypeDef`: scope + full name (scope distinguishes same-named types in different namespaces).
//! - `Signature`: file + closure syntax node of the function body.
//! - `Name`: unresolved global name reference ("MyMod" / "a.b"); phase 2 `resolve_owner` associates it with the real definition.

use emmylua_parser::LuaSyntaxId;
use internment::ArcIntern;
use rowan::TextRange;
use smol_str::SmolStr;

use crate::FileId;

use super::TypeScope;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeclKey {
    pub file_id: FileId,
    pub name_range: TextRange,
}

/// Member declaration identity (file + member-key position). See `LuaMemberKey` for the file-independent lookup key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemberDeclKey {
    pub file_id: FileId,
    pub key_range: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeDefKey {
    pub scope: TypeScope,
    pub full_name: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SignatureKey {
    pub file_id: FileId,
    pub closure_syntax: LuaSyntaxId,
}

/// Global semantic identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SemanticId {
    Decl(ArcIntern<DeclKey>),
    Member(ArcIntern<MemberDeclKey>),
    TypeDef(ArcIntern<TypeDefKey>),
    Signature(ArcIntern<SignatureKey>),
    /// Unresolved global name reference (phase 2 associates it with the real definition).
    Name(ArcIntern<SmolStr>),
}

impl SemanticId {
    pub fn decl(file_id: FileId, name_range: TextRange) -> Self {
        SemanticId::Decl(ArcIntern::new(DeclKey {
            file_id,
            name_range,
        }))
    }

    /// Member declaration identity (file + member-key position). The lookup key is the file-independent `LuaMemberKey`.
    pub fn member(file_id: FileId, key_range: TextRange) -> Self {
        SemanticId::Member(ArcIntern::new(MemberDeclKey { file_id, key_range }))
    }

    pub fn type_def(scope: TypeScope, full_name: SmolStr) -> Self {
        SemanticId::TypeDef(ArcIntern::new(TypeDefKey { scope, full_name }))
    }

    pub fn signature(file_id: FileId, closure_syntax: LuaSyntaxId) -> Self {
        SemanticId::Signature(ArcIntern::new(SignatureKey {
            file_id,
            closure_syntax,
        }))
    }

    /// Unresolved global name reference.
    pub fn name(name: SmolStr) -> Self {
        SemanticId::Name(ArcIntern::new(name))
    }

    /// `Member` identity's declaration position (member-key range); returns `None` for non-members.
    pub fn member_key_range(&self) -> Option<TextRange> {
        match self {
            SemanticId::Member(key) => Some(key.key_range),
            _ => None,
        }
    }
}
