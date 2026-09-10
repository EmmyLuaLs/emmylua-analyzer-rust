//! # FileExportContribution — cross-file export facts layer
//!
//! Corresponds to rust-analyzer's DefMap/public surface above ItemTree:
//! - Only collects this file's own fact identities; declarations/member types are not computed here
//!   (type queries execute lazily by SemanticId, avoiding the cycle
//!   file_exports → decl_type → resolve_type_def → workspace_type_index → export_shard);
//! - Workspace shard indexes merge only this layer, without visiting FileFacts per file.
//!
//! A contribution carries canonical `OwnerId` / `ExportKey` identities plus the
//! member data the incremental workspace-index layer needs (`value_syntax`,
//! `is_method`, `visibility`, source `order`). Overloads are never merged here:
//! every declaration stays a separate `MemberExport`.

use std::sync::Arc;

use emmylua_parser::{LuaSyntaxId, VisibilityKind};
use hashbrown::HashMap;
use smol_str::SmolStr;

use crate::FileId;
use crate::semantic_db::def::{
    DeclKind, ExportKey, LuaMemberKey, ModuleExport, OwnerId, SemanticId, TypeDef, TypeScope,
};

use super::SemanticDatabase;
use super::facts::FileFacts;
use super::query::file_facts;

/// Export contribution visible from a single file.
///
/// This is the unit the incremental workspace-index layer adds/removes when a
/// file is written. It is intentionally immutable once built and shared through
/// `Arc<FileExportContribution>` in `FileCache`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileExportContribution {
    pub file_id: FileId,
    /// Types defined in this file (including private ones; shard index uses scope).
    pub types: Vec<TypeDef>,
    /// Global declaration identities: `name → decl_id`.
    pub globals: Vec<GlobalExport>,
    /// Runtime value identities: `(bare type name, same-name declaration id)` (`local M = {}` implements `@class M`).
    pub runtime_values: Vec<(SmolStr, SemanticId)>,
    /// Member identities: `owner + key → member_id`, in source order.
    pub members: Vec<MemberExport>,
    /// Module export (top-level `return M`).
    pub module: Option<ModuleExport>,
}

/// Backwards-compatible public alias. New code should prefer `FileExportContribution`.
pub type FileExports = FileExportContribution;

impl Default for FileExportContribution {
    fn default() -> Self {
        Self {
            file_id: FileId::VIRTUAL,
            types: Vec::new(),
            globals: Vec::new(),
            runtime_values: Vec::new(),
            members: Vec::new(),
            module: None,
        }
    }
}

impl FileExportContribution {
    /// Canonical module-export owner for this file, if it has a top-level return.
    pub fn module_owner(&self) -> Option<OwnerId> {
        self.module.as_ref().map(|_| OwnerId::Module(self.file_id))
    }

    /// Surface equality used by the incremental update path.
    ///
    /// `value_syntax` is intentionally ignored for member entries: changing the
    /// initializer text (e.g. `M.x = 1` -> `M.x = 100`) does not change the
    /// workspace-index contribution identity. Type/global/module facts still use
    /// full equality because their payload is consumed by workspace indexes.
    pub fn surface_eq(&self, other: &Self) -> bool {
        self.file_id == other.file_id
            && self.types == other.types
            && self.globals == other.globals
            && self.runtime_values == other.runtime_values
            && self.module == other.module
            && self.members.len() == other.members.len()
            && self
                .members
                .iter()
                .zip(other.members.iter())
                .all(|(left, right)| left.surface_eq(right))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalExport {
    pub file_id: FileId,
    pub name: SmolStr,
    pub decl: SemanticId,
}

impl GlobalExport {
    pub fn export_key(&self) -> ExportKey {
        ExportKey::Global(self.name.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberExport {
    pub file_id: FileId,
    /// Raw owner identity as stored in `FileFacts` (kept while queries still use it).
    pub owner: SemanticId,
    /// Canonical owner identity used by the incremental workspace-index layer.
    pub owner_id: OwnerId,
    pub key: LuaMemberKey,
    pub member: SemanticId,
    /// Initializer expression / closure syntax (`None` for pure `@field` docs).
    pub value_syntax: Option<LuaSyntaxId>,
    /// Method definition (`:`).
    pub is_method: bool,
    /// Access visibility (`@private` / `@protected` / ...).
    pub visibility: VisibilityKind,
    pub deprecated: bool,
    /// Source order inside `FileFacts.members`; preserved so overloads stay stable.
    pub order: u32,
}

impl MemberExport {
    pub fn export_key(&self) -> ExportKey {
        ExportKey::Member(self.owner_id.clone(), self.key.clone())
    }

    /// Surface equality for workspace-index updates: ignore `value_syntax` only.
    pub fn surface_eq(&self, other: &Self) -> bool {
        self.file_id == other.file_id
            && self.owner == other.owner
            && self.owner_id == other.owner_id
            && self.key == other.key
            && self.member == other.member
            && self.is_method == other.is_method
            && self.visibility == other.visibility
            && self.deprecated == other.deprecated
            && self.order == other.order
    }
}

impl TypeDef {
    /// Canonical export key for this type definition.
    pub fn export_key(&self) -> ExportKey {
        match &self.id {
            SemanticId::TypeDef(key) => ExportKey::Type(key.scope, key.full_name.clone()),
            _ => ExportKey::Type(TypeScope::File(self.file_id), self.full_name.clone()),
        }
    }
}

/// Convert a raw `SemanticId` owner into a canonical `OwnerId`.
///
/// `Decl` / `Member` are file-local concrete identities; global declarations are
/// normalized to `OwnerId::Global`, and nested table owners to `OwnerId::Table`.
pub(crate) fn owner_id_from_semantic_id(facts: &FileFacts, owner: &SemanticId) -> OwnerId {
    match owner {
        SemanticId::Name(name) => OwnerId::Global(SmolStr::new(name.as_str())),
        SemanticId::TypeDef(key) => OwnerId::Type(key.scope, key.full_name.clone()),
        SemanticId::Decl(key) => match facts.decl_by_id(owner) {
            Some(decl) if matches!(decl.kind, DeclKind::Global) => {
                OwnerId::Global(decl.name.clone())
            }
            _ => OwnerId::Local(key.file_id, key.name_range),
        },
        SemanticId::Member(key) => OwnerId::Table(key.file_id, key.key_range),
        SemanticId::Signature(_) => OwnerId::Concrete(owner.clone()),
    }
}

/// Per-file export contribution (collects identities only, no type precomputation).
pub(crate) fn file_exports(db: &SemanticDatabase, file: FileId) -> &FileExportContribution {
    db.file_exports_of(file)
}

pub(super) fn build_file_exports(
    db: &SemanticDatabase,
    file: FileId,
    file_id: FileId,
) -> FileExportContribution {
    let facts = file_facts(db, file);

    let types = facts.type_defs.clone();

    let globals = facts
        .decls
        .iter()
        .filter(|decl| matches!(decl.kind, DeclKind::Global))
        .map(|decl| GlobalExport {
            file_id,
            name: decl.name.clone(),
            decl: decl.id.clone(),
        })
        .collect();

    let members = facts
        .members
        .iter()
        .enumerate()
        .map(|(order, member)| MemberExport {
            file_id,
            owner: member.owner.clone(),
            owner_id: owner_id_from_semantic_id(facts, &member.owner),
            key: member.key.clone(),
            member: member.id.clone(),
            value_syntax: member.value_syntax,
            is_method: member.is_method,
            visibility: member.visibility,
            deprecated: member.deprecated,
            order: order as u32,
        })
        .collect();

    let runtime_values = facts
        .type_defs
        .iter()
        .filter_map(|def| {
            facts
                .decl_named(def.name.as_str())
                .map(|decl| (def.name.clone(), decl.id.clone()))
        })
        .collect();

    let module =
        Some(facts.module_export.clone()).filter(|export| !matches!(export, ModuleExport::None));

    FileExportContribution {
        file_id,
        types,
        globals,
        runtime_values,
        members,
        module,
    }
}

// ──────────────────────────────────────────────
// Shards
// ──────────────────────────────────────────────

/// Stable shard count: 64 shards; cross-file lookup depends only on the relevant shard's memo.
pub const EXPORT_SHARDS: u8 = 64;

/// file_id → shard (stable: FileId never changes once assigned).
pub fn shard_of(file_id: FileId) -> u8 {
    (file_id.id % EXPORT_SHARDS as u32) as u8
}

/// A shard's export facts (write-time built map lookup).
pub(crate) fn export_shard(db: &SemanticDatabase, shard: u8) -> &ExportShard {
    db.export_shard_of(shard)
}

/// A shard's export contributions: `FileId -> contribution`.
///
/// The shard is a stable partition only. It never aggregates contributions, so
/// updating one file replaces exactly one map entry and never scans other files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportShard {
    pub files: HashMap<FileId, Arc<FileExportContribution>>,
}

pub(super) fn build_export_shard(db: &SemanticDatabase, shard: u8) -> ExportShard {
    let mut files = HashMap::new();
    for &file_id in db.file_ids_in_shard(shard) {
        if let Some(cache) = db.file_cache(file_id) {
            files.insert(file_id, Arc::clone(&cache.exports));
        }
    }
    ExportShard { files }
}
