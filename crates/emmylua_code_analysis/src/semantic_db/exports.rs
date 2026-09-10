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

use emmylua_parser::{
    LuaAstNode, LuaCallExpr, LuaExpr, LuaLiteralToken, LuaSyntaxId, VisibilityKind,
};
use hashbrown::HashMap;
use smol_str::SmolStr;

use crate::FileId;
use crate::semantic_db::def::{
    DeclKind, ExportKey, LuaMemberKey, ModuleExport, OwnerId, SemanticId, TypeDef, TypeScope,
};

use super::SemanticDatabase;
use super::facts::FileFacts;
use super::query::{self, file_facts};

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
    /// Local `require` aliases (`local M = require("mod")`, `local N = M`).
    ///
    /// The alias target is what makes `function M.extra() end` in another file
    /// attach its member contribution to `OwnerId::Module(mod_file)` instead of
    /// a consumer-local owner.
    pub aliases: Vec<RequireAliasContribution>,
}

/// One local declaration bound to a required module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequireAliasContribution {
    pub file_id: FileId,
    /// Alias declaration (`local M` / `local N = M`).
    pub decl: SemanticId,
    /// Required module file.
    pub module_file: FileId,
}

impl RequireAliasContribution {
    pub fn owner_id(&self) -> OwnerId {
        OwnerId::Module(self.module_file)
    }
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
            aliases: Vec::new(),
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
            && self.aliases == other.aliases
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
    let aliases = build_require_alias_contributions(db, file_id);

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
            owner_id: canonical_member_owner_id(facts, file_id, &member.owner, &aliases),
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
        aliases,
    }
}

/// Canonical `OwnerId` for a member owner.
///
/// - members of the file's module export target become `OwnerId::Module(file_id)`;
/// - members reached through a `require` alias become `OwnerId::Module(target_file)`;
/// - everything else keeps the raw identity mapping.
fn canonical_member_owner_id(
    facts: &FileFacts,
    file_id: FileId,
    owner: &SemanticId,
    aliases: &[RequireAliasContribution],
) -> OwnerId {
    if let Some(module_file) = module_export_owner_file(facts, file_id, owner) {
        return OwnerId::Module(module_file);
    }
    if let Some(alias) = aliases.iter().find(|alias| &alias.decl == owner) {
        return alias.owner_id();
    }
    owner_id_from_semantic_id(facts, owner)
}

/// If `owner` is the raw owner of this file's top-level module export, return the file id.
pub(crate) fn module_export_owner_file(
    facts: &FileFacts,
    file_id: FileId,
    owner: &SemanticId,
) -> Option<FileId> {
    match &facts.module_export {
        ModuleExport::Decl { decl, .. } if decl == owner => Some(file_id),
        ModuleExport::Expr { value_syntax } => match owner {
            SemanticId::Member(key) if key.key_range == value_syntax.get_range() => Some(file_id),
            _ => None,
        },
        _ => None,
    }
}

/// Collect `local M = require("mod")` aliases (including simple local alias chains).
///
/// Uses a fixed-point pass so source order does not matter:
/// `local N = M` resolves once `M` has a known target.
pub(crate) fn build_require_alias_contributions(
    db: &SemanticDatabase,
    file_id: FileId,
) -> Vec<RequireAliasContribution> {
    let Some(config) = db.config_input() else {
        return Vec::new();
    };
    let Some(file) = db.file_data_id(file_id) else {
        return Vec::new();
    };
    let facts = file_facts(db, file);
    if facts.decls.is_empty() {
        return Vec::new();
    }
    let Some(tree) = db.vfs().get_syntax_tree(&file_id) else {
        return Vec::new();
    };
    let root = tree.get_red_root();
    let mut targets: HashMap<SemanticId, FileId> = HashMap::new();

    // Fixed point: a chain `local A = require(...); local B = A` may be
    // visited in any declaration order.
    let mut changed = true;
    let mut rounds = 0;
    while changed && rounds <= facts.decls.len() {
        changed = false;
        rounds += 1;
        for decl in &facts.decls {
            if !decl.kind.is_local() || targets.contains_key(&decl.id) {
                continue;
            }
            let Some(value_syntax) = decl.value_expr_syntax else {
                continue;
            };
            let Some(node) = value_syntax.to_node_from_root(&root) else {
                continue;
            };
            let Some(expr) = LuaExpr::cast(node) else {
                continue;
            };
            if let Some(module_file) =
                require_alias_target(db, config, facts, &root, &expr, &targets)
            {
                targets.insert(decl.id.clone(), module_file);
                changed = true;
            }
        }
    }

    targets
        .into_iter()
        .map(|(decl, module_file)| RequireAliasContribution {
            file_id,
            decl,
            module_file,
        })
        .collect()
}

fn require_alias_target(
    db: &SemanticDatabase,
    config: &crate::Emmyrc,
    facts: &FileFacts,
    root: &emmylua_parser::LuaSyntaxNode,
    expr: &LuaExpr,
    targets: &HashMap<SemanticId, FileId>,
) -> Option<FileId> {
    match expr {
        LuaExpr::CallExpr(call) => require_module_file_from_call(db, config, call),
        LuaExpr::ParenExpr(paren) => {
            require_alias_target(db, config, facts, root, &paren.get_expr()?, targets)
        }
        LuaExpr::NameExpr(name) => {
            let name_text = name.get_name_text()?;
            let decl = facts.find_visible_decl_before_offset(&name_text, name.get_position())?;
            targets.get(&decl.id).copied()
        }
        _ => {
            let _ = (db, config, root);
            None
        }
    }
}

fn require_module_file_from_call(
    db: &SemanticDatabase,
    config: &crate::Emmyrc,
    call: &LuaCallExpr,
) -> Option<FileId> {
    let prefix = call.get_prefix_expr()?;
    let LuaExpr::NameExpr(name) = prefix else {
        return None;
    };
    if name.get_name_text().as_deref() != Some("require") {
        return None;
    }
    let arg = call.get_args_list()?.get_args().next()?;
    let LuaExpr::LiteralExpr(literal) = arg else {
        return None;
    };
    let LuaLiteralToken::String(token) = literal.get_literal()? else {
        return None;
    };
    query::module_file_of(db, config, SmolStr::new(token.get_value()))
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
