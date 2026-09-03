//! # FileExports — cross-file export facts layer
//!
//! Corresponds to rust-analyzer's DefMap/public surface above ItemTree:
//! - Only collects this file's own fact identities; declarations/member types are not computed here
//!   (type queries execute lazily by SemanticId, avoiding the cycle
//!   file_exports → decl_type → resolve_type_def → workspace_type_index → export_shard);
//! - Workspace shard indexes merge only this layer, without visiting FileFacts per file.
//!
//! Currently holds: all TypeDefs, global declaration identities, member identities, module export identities.

use smol_str::SmolStr;

use crate::FileId;
use crate::salsa_builder::def::{LuaMemberKey, ModuleExport, SemanticId, TypeDef};

use super::SalsaDb;
use super::inputs::{ConfigInput, SourceFileInput};
use super::query::file_facts;

/// Export identities visible from a single file.
#[derive(Debug, Clone, PartialEq, Eq, salsa::SalsaValue)]
pub struct FileExports {
    pub file_id: FileId,
    /// Types defined in this file (including private ones; shard index uses scope).
    pub types: Vec<TypeDef>,
    /// Global declaration identities: `name → decl_id`.
    pub globals: Vec<GlobalExport>,
    /// Runtime value identities: `(bare type name, same-name declaration id)` (`local M = {}` implements `@class M`).
    pub runtime_values: Vec<(SmolStr, SemanticId)>,
    /// Member identities: `owner + key → member_id`.
    pub members: Vec<MemberExport>,
    /// Module export (top-level `return M`).
    pub module: Option<ModuleExport>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::SalsaValue)]
pub struct GlobalExport {
    pub file_id: FileId,
    pub name: SmolStr,
    pub decl: SemanticId,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::SalsaValue)]
pub struct MemberExport {
    pub file_id: FileId,
    pub owner: SemanticId,
    pub key: LuaMemberKey,
    pub member: SemanticId,
    pub deprecated: bool,
}

/// Per-file export facts (collects identities only, no type precomputation; invalidates precisely on text changes).
#[salsa::tracked(returns(ref), lru = 512)]
pub(crate) fn file_exports(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    config: ConfigInput,
) -> FileExports {
    let file_id = file.file_id(db);
    let facts = file_facts(db, file, config);

    let types = facts.type_defs.clone();

    let globals = facts
        .decls
        .iter()
        .filter(|decl| matches!(decl.kind, crate::salsa_builder::def::DeclKind::Global))
        .map(|decl| GlobalExport {
            file_id,
            name: decl.name.clone(),
            decl: decl.id.clone(),
        })
        .collect();

    let members = facts
        .members
        .iter()
        .map(|member| MemberExport {
            file_id,
            owner: member.owner.clone(),
            key: member.key.clone(),
            member: member.id.clone(),
            deprecated: member.deprecated,
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

    FileExports {
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

/// A shard's export facts (salsa tracked: depends only on `file_exports` of files in this shard).
#[salsa::tracked(returns(ref))]
pub(crate) fn export_shard(
    db: &dyn SalsaDb,
    workspace: super::inputs::WorkspaceInput,
    config: ConfigInput,
    shard: u8,
) -> ExportShard {
    let mut types = Vec::new();
    let mut globals = Vec::new();
    let mut runtime_values = Vec::new();
    let mut members = Vec::new();
    let mut modules = Vec::new();
    for file_id in workspace.file_ids(db).iter().copied() {
        if shard_of(file_id) != shard {
            continue;
        }
        let Some(file) = db.file_input(file_id) else {
            continue;
        };
        let exports = file_exports(db, file, config);
        types.extend(exports.types.iter().cloned());
        globals.extend(exports.globals.iter().cloned());
        runtime_values.extend(
            exports
                .runtime_values
                .iter()
                .map(|(name, decl)| (exports.file_id, name.clone(), decl.clone())),
        );
        members.extend(exports.members.iter().cloned());
        if let Some(module) = &exports.module {
            modules.push((file_id, module.clone()));
        }
    }
    ExportShard {
        types,
        globals,
        runtime_values,
        members,
        modules,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::SalsaValue)]
pub struct ExportShard {
    pub types: Vec<TypeDef>,
    pub globals: Vec<GlobalExport>,
    pub runtime_values: Vec<(FileId, SmolStr, SemanticId)>,
    pub members: Vec<MemberExport>,
    pub modules: Vec<(FileId, ModuleExport)>,
}
