use std::sync::{Arc, OnceLock};

use emmylua_parser::{LuaSyntaxId, VisibilityKind};
use rowan::TextRange;
use smol_str::SmolStr;

use crate::FileId;

use super::{LuaMemberKey, OwnerId, SemanticId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemberSource {
    DocField,
    TableField { key_expr: Option<LuaSyntaxId> },
    Runtime { access: LuaSyntaxId },
}

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
    pub source: MemberSource,
    /// Value: `TypeDef` members = doc type nodes; others = expressions.
    pub value_syntax: Option<LuaSyntaxId>,
    /// Inline `---@type` annotation on a table field (`{ ---@type number? vvv = 1 }`).
    pub doc_type_syntax: Option<LuaSyntaxId>,
    /// Module name from `---@module "name"` (resolved as ModuleRef type).
    pub module_path: Option<SmolStr>,
    /// Method definition (`:`).
    pub is_method: bool,
    /// Written through an implicit `self`. Record the syntax id of the function in which it is located.
    pub implicit_self: Option<LuaSyntaxId>,
    /// `---@deprecated` (same comment block as `@field`).
    pub deprecated: bool,
    /// Access visibility annotation (`@public`/`@protected`/`@private`/`@package`/`@internal`; default Public).
    pub visibility: VisibilityKind,
    /// `---@readonly` (annotation owned by this member).
    pub readonly: bool,
    /// `---@field [string] any` index signature.
    pub index_key_syntax: Option<LuaSyntaxId>,
    /// `---@field x? string` nullable field.
    pub is_nullable: bool,
}

impl Member {
    /// Positional table field (`{ value }`): there is no key expression, so its key range
    /// falls back to the whole field. Real keys (`x = v`, `[k] = v`) always have a key expr.
    pub fn is_positional_table_field(&self) -> bool {
        matches!(self.source, MemberSource::TableField { key_expr: None })
    }

    pub fn new(
        file_id: FileId,
        key_range: TextRange,
        key: LuaMemberKey,
        owner: SemanticId,
        source: MemberSource,
    ) -> Self {
        Member {
            id: SemanticId::member(file_id, key_range),
            key,
            owner,
            source,
            value_syntax: None,
            doc_type_syntax: None,
            module_path: None,
            is_method: false,
            implicit_self: None,
            deprecated: false,
            visibility: VisibilityKind::Public,
            readonly: false,
            index_key_syntax: None,
            is_nullable: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccessPath {
    pub root: SemanticId,
    pub keys: Vec<LuaMemberKey>,
}

impl AccessPath {
    pub fn is_exact(&self) -> bool {
        !self.keys.iter().any(LuaMemberKey::is_none)
    }

    pub fn has_direct_owner(&self) -> bool {
        self.keys.is_empty() || matches!(self.root, SemanticId::Name(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberUse {
    pub syntax: LuaSyntaxId,
    pub key: LuaMemberKey,
    pub key_range: TextRange,
    pub key_expr: Option<LuaSyntaxId>,
    pub root_syntax: Option<LuaSyntaxId>,
    pub access_path: Option<AccessPath>,
    pub receiver: Option<AccessPath>,
    pub owner: Option<SemanticId>,
}

impl MemberUse {
    pub fn has_direct_owner(&self) -> bool {
        self.receiver
            .as_ref()
            .is_some_and(AccessPath::has_direct_owner)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemberPathKey {
    pub root: OwnerId,
    pub path: Arc<[LuaMemberKey]>,
    pub key: LuaMemberKey,
}

impl MemberPathKey {
    pub fn new(root: OwnerId, path: &[LuaMemberKey], key: LuaMemberKey) -> Self {
        MemberPathKey {
            root,
            path: if path.is_empty() {
                static EMPTY: OnceLock<Arc<[LuaMemberKey]>> = OnceLock::new();
                Arc::clone(EMPTY.get_or_init(|| Arc::from(Vec::new())))
            } else {
                Arc::from(path)
            },
            key,
        }
    }
}
