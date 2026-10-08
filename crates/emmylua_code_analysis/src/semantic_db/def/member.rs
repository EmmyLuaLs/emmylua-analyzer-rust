use emmylua_parser::{LuaSyntaxId, VisibilityKind};
use rowan::TextRange;
use smol_str::SmolStr;

use crate::FileId;

use super::{LuaMemberKey, SemanticId};

/// Cross-file member reference: a member of an owner, declared in `file_id`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemberRef {
    pub file_id: FileId,
    /// Member declaration identity (`SemanticId::Member`).
    pub id: SemanticId,
    /// Member name (the `x` in `T.x`).
    pub name: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Member {
    /// Globally unique identity = declaration location (file + member-key token range).
    pub id: SemanticId,
    /// File-independent lookup key: `Name("Field")` / `Integer(1)`.
    pub key: LuaMemberKey,
    /// Owner (resolved association): `Decl(local)` / `TypeDef(@field)` / `Name(global, resolved in phase 2)` / `Member(nested a.b.c)`.
    pub owner: SemanticId,
    /// Value: `TypeDef` members = doc type nodes; others = expressions.
    pub value_syntax: Option<LuaSyntaxId>,
    /// Inline `---@type` annotation on a table field (`{ ---@type number? vvv = 1 }`).
    pub doc_type_syntax: Option<LuaSyntaxId>,
    /// Module name from `---@module "name"` (resolved as ModuleRef type).
    pub module_path: Option<SmolStr>,
    /// Method definition (`:`).
    pub is_method: bool,
    /// `---@deprecated` (same comment block as `@field`).
    pub deprecated: bool,
    /// Access visibility annotation (`@public`/`@protected`/`@private`/`@package`/`@internal`; default Public).
    pub visibility: VisibilityKind,
    /// `---@readonly` (annotation owned by this member).
    pub readonly: bool,
    /// `---@field [string] any` index signature.
    pub is_index_signature: bool,
    /// `---@field x? string` nullable field.
    pub is_nullable: bool,
}

impl Member {
    pub fn new(
        file_id: FileId,
        key_range: TextRange,
        key: LuaMemberKey,
        owner: SemanticId,
    ) -> Self {
        Member {
            id: SemanticId::member(file_id, key_range),
            key,
            owner,
            value_syntax: None,
            doc_type_syntax: None,
            module_path: None,
            is_method: false,
            deprecated: false,
            visibility: VisibilityKind::Public,
            readonly: false,
            is_index_signature: false,
            is_nullable: false,
        }
    }
}
