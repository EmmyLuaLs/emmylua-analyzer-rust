//! # Node-keyed derived query layer
//!
//! Node-keyed derived queries plus plain workspace indexes. Recursive cycles converge via the native `cycle_fn`.

use hashbrown::{HashMap, HashSet};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::SemanticDatabase;
use super::def::{
    ChangedKeys, ConstructorAttribute, DeclKind, DependencyKey, DocGenericParam, ExportKey,
    FileDependencies, LuaMemberKey, MemberRef, ModuleExport, ModuleInfo, ModuleNode, ModuleNodeId,
    ModuleVisibility, OwnerId, SemanticId, TypeDef, TypeDefKind,
};
use super::exports::{
    EXPORT_SHARDS, FileExports, export_shard, module_export_owner_file, owner_id_from_semantic_id,
};
use super::facts::{FactsBuilder, FileFacts};
use super::types::{LiteralShell, PrimitiveType, TableId, TypeCandidate, TypeShell};
use crate::Emmyrc;
use crate::FileId;
use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaCallExpr, LuaClosureExpr, LuaDocType, LuaExpr, LuaIndexExpr,
    LuaLiteralExpr, LuaLiteralToken, LuaReturnStat, LuaSyntaxId, LuaSyntaxTree,
    LuaTypeBinaryOperator, LuaVersionCondition, UnaryOperator,
};
use rowan::{TextRange, TextSize};

/// Per-file minimum fact arena (declarations + scopes + type definitions).
///
/// Results are cached in the plain `SemanticDatabase` file cache and invalidated
/// by file/config/workspace writes. It reads the same input fields so callers
/// are correctly invalidated when the underlying text/config/roots change.
pub(crate) fn build_file_facts(
    db: &SemanticDatabase,
    _file: FileId,
    file_id: FileId,
    text: &str,
) -> FileFacts {
    let workspace_id = file_workspace_id(db, file_id).unwrap_or(WorkspaceId::MAIN);
    let tree = db
        .vfs()
        .get_syntax_tree(&file_id)
        .expect("syntax tree must be built before read");
    let chunk = tree.get_chunk_node();
    FactsBuilder::new(file_id, workspace_id).build(&chunk, text)
}

pub(crate) fn syntax_tree(db: &SemanticDatabase, file: FileId) -> &LuaSyntaxTree {
    db.vfs()
        .get_syntax_tree(&file)
        .expect("syntax tree must be built before read")
}

pub(crate) fn file_facts(db: &SemanticDatabase, file: FileId) -> &FileFacts {
    db.file_facts_of(file)
        .expect("file facts must be built before read")
}

/// Rebuild all write-time caches from the current inputs.
///
/// This is the only place where the per-file and shard caches are populated.
/// Read-side query functions perform pure map lookups and never call `get_or_init`.
pub(crate) fn changed_keys(old: Option<&FileExports>, new: Option<&FileExports>) -> ChangedKeys {
    let empty = FileExports::default();
    let old = old.unwrap_or(&empty);
    let new = new.unwrap_or(&empty);
    let mut changed = ChangedKeys::default();

    // Globals: same-name declaration identity/payload changes.
    let old_globals: HashMap<&str, &super::exports::GlobalExport> = old
        .globals
        .iter()
        .map(|global| (global.name.as_str(), global))
        .collect();
    let new_globals: HashMap<&str, &super::exports::GlobalExport> = new
        .globals
        .iter()
        .map(|global| (global.name.as_str(), global))
        .collect();
    for (name, old_global) in &old_globals {
        if new_globals.get(name) != Some(old_global) {
            changed.insert(DependencyKey::Global(SmolStr::new(*name)));
        }
    }
    for name in new_globals.keys() {
        if !old_globals.contains_key(name) {
            changed.insert(DependencyKey::Global(SmolStr::new(*name)));
        }
    }

    // Named types: canonical `(scope, full_name)` key.
    fn type_map(exports: &FileExports) -> HashMap<ExportKey, Vec<&TypeDef>> {
        let mut map: HashMap<ExportKey, Vec<&TypeDef>> = HashMap::new();
        for def in &exports.types {
            map.entry(def.export_key()).or_default().push(def);
        }
        map
    }
    let old_types = type_map(old);
    let new_types = type_map(new);
    for (key, old_defs) in &old_types {
        if new_types.get(key) != Some(old_defs)
            && let ExportKey::Type(scope, name) = key
        {
            changed.insert(DependencyKey::Type(*scope, name.clone()));
        }
    }
    for key in new_types.keys() {
        if !old_types.contains_key(key)
            && let ExportKey::Type(scope, name) = key
        {
            changed.insert(DependencyKey::Type(*scope, name.clone()));
        }
    }

    // Runtime values: `local M = {}` implementing `@class M`.
    let runtime_map = |exports: &FileExports| {
        exports
            .runtime_values
            .iter()
            .map(|(name, decl)| (name.clone(), decl.clone()))
            .collect::<HashMap<SmolStr, SemanticId>>()
    };
    let old_runtime = runtime_map(old);
    let new_runtime = runtime_map(new);
    for (name, old_decl) in &old_runtime {
        if new_runtime.get(name) != Some(old_decl) {
            changed.insert(DependencyKey::RuntimeValue(name.clone()));
            changed.insert(DependencyKey::Global(name.clone()));
        }
    }
    for name in new_runtime.keys() {
        if !old_runtime.contains_key(name) {
            changed.insert(DependencyKey::RuntimeValue(name.clone()));
            changed.insert(DependencyKey::Global(name.clone()));
        }
    }

    // Members: canonical `(OwnerId, LuaMemberKey)`.
    fn member_map(
        exports: &FileExports,
    ) -> HashMap<DependencyKey, Vec<&super::exports::MemberExport>> {
        let mut map: HashMap<DependencyKey, Vec<&super::exports::MemberExport>> = HashMap::new();
        for member in &exports.members {
            let ExportKey::Member(owner_id, key) = member.export_key() else {
                continue;
            };
            map.entry(DependencyKey::Member(owner_id, key))
                .or_default()
                .push(member);
        }
        map
    }
    let old_members = member_map(old);
    let new_members = member_map(new);
    for (key, old_entries) in &old_members {
        let same = new_members.get(key).is_some_and(|new_entries| {
            new_entries.len() == old_entries.len()
                && new_entries
                    .iter()
                    .zip(old_entries.iter())
                    .all(|(new_member, old_member)| new_member.surface_eq(old_member))
        });
        if !same {
            changed.insert(key.clone());
        }
    }
    for key in new_members.keys() {
        if !old_members.contains_key(key) {
            changed.insert(key.clone());
        }
    }

    // Module export identity.
    if old.module != new.module {
        changed.insert(DependencyKey::Module(old.file_id));
        changed.insert(DependencyKey::Module(new.file_id));
    }

    // Require aliases: consumers of the aliased module must refresh.
    if old.aliases != new.aliases {
        for alias in old.aliases.iter().chain(new.aliases.iter()) {
            changed.insert(DependencyKey::Module(alias.module_file));
        }
    }

    changed
}

/// Rebuild the reverse dependency map from all per-file references (full rebuild).
// ──────────────────────────────────────────────
// Plain workspace type index (scope + full_name → definitions)
// ──────────────────────────────────────────────
use super::def::TypeScope;
use super::def::TypeVisibility;
use crate::WorkspaceId;
use smol_str::SmolStr;

/// Workspace type index: plain `(scope, full_name)` -> all type definitions.
///
/// Maintains a per-file key list so a file update removes/adds exactly its own
/// definitions instead of rebuilding the workspace index.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceTypeIndex {
    /// Bucket contents are stored as a shared `Arc<[TypeDef]>`; repeated queries
    /// clone the `Arc` instead of collecting a fresh `Vec` every call.
    by_scope_name: HashMap<(TypeScope, SmolStr), Arc<[TypeDef]>>,
    by_file: HashMap<FileId, Vec<((TypeScope, SmolStr), SemanticId)>>,
}

impl WorkspaceTypeIndex {
    fn type_key(def: &TypeDef, ws_id: WorkspaceId) -> (TypeScope, SmolStr) {
        let scope = match def.visibility {
            TypeVisibility::Public => TypeScope::Global,
            TypeVisibility::Internal => TypeScope::Internal(ws_id),
            TypeVisibility::Private => TypeScope::File(def.file_id),
        };
        (scope, def.full_name.clone())
    }

    /// **All** definitions in the bucket (same-name definitions in multiple places, used by duplicate-type checks).
    pub(crate) fn find_all(&self, scope: TypeScope, full_name: &str) -> Arc<[TypeDef]> {
        self.by_scope_name
            .get(&(scope, SmolStr::new(full_name)))
            .cloned()
            .unwrap_or_default()
    }

    /// Iterate the global-scope buckets (`full_name` -> shared definitions).
    fn global_type_buckets(&self) -> impl Iterator<Item = (&SmolStr, &Arc<[TypeDef]>)> {
        self.by_scope_name
            .iter()
            .filter_map(|((scope, name), defs)| {
                (*scope == TypeScope::Global).then_some((name, defs))
            })
    }

    pub(crate) fn remove_file(&mut self, file_id: FileId) {
        let Some(keys) = self.by_file.remove(&file_id) else {
            return;
        };
        for (key, type_id) in keys {
            let Some(defs) = self.by_scope_name.get(&key) else {
                continue;
            };
            let new_defs: Vec<TypeDef> = defs
                .iter()
                .filter(|def| def.id != type_id)
                .cloned()
                .collect();
            if new_defs.is_empty() {
                self.by_scope_name.remove(&key);
            } else {
                self.by_scope_name.insert(key, Arc::from(new_defs));
            }
        }
    }

    pub(crate) fn add_file(&mut self, ws_id: WorkspaceId, file_id: FileId, exports: &FileExports) {
        let mut keys = Vec::new();
        for def in &exports.types {
            let key = Self::type_key(def, ws_id);
            let mut new_defs = self
                .by_scope_name
                .get(&key)
                .map(|defs| defs.to_vec())
                .unwrap_or_default();
            new_defs.push(def.clone());
            self.by_scope_name.insert(key.clone(), Arc::from(new_defs));
            keys.push((key, def.id.clone()));
        }
        self.by_file.insert(file_id, keys);
    }
}

pub(crate) fn build_workspace_type_index(
    db: &SemanticDatabase,
    ws_id: WorkspaceId,
) -> WorkspaceTypeIndex {
    let mut by_scope_name: HashMap<(TypeScope, SmolStr), Vec<TypeDef>> = HashMap::new();
    let mut by_file = HashMap::new();
    for shard in 0..EXPORT_SHARDS {
        let shard = export_shard(db, shard);
        for (file_id, exports) in &shard.files {
            if !file_matches_workspace_id(db, *file_id, ws_id) {
                continue;
            }
            let mut keys = Vec::new();
            for def in &exports.types {
                let key = WorkspaceTypeIndex::type_key(def, ws_id);
                by_scope_name
                    .entry(key.clone())
                    .or_default()
                    .push(def.clone());
                keys.push((key, def.id.clone()));
            }
            by_file.insert(*file_id, keys);
        }
    }
    WorkspaceTypeIndex {
        by_scope_name: by_scope_name
            .into_iter()
            .map(|(key, defs)| (key, Arc::from(defs)))
            .collect(),
        by_file,
    }
}

/// Build the cross-workspace global type aggregate in deterministic workspace
/// lookup order (`all_workspace_ids` order). One file edit refreshes only the
/// affected names via [`refresh_global_type_keys`].
pub(crate) fn build_global_type_aggregate(
    ws_ids: &[WorkspaceId],
    types: &HashMap<WorkspaceId, WorkspaceTypeIndex>,
) -> HashMap<SmolStr, Arc<[TypeDef]>> {
    let mut aggregate: HashMap<SmolStr, Vec<TypeDef>> = HashMap::new();
    for &ws_id in ws_ids {
        let Some(index) = types.get(&ws_id) else {
            continue;
        };
        for (name, defs) in index.global_type_buckets() {
            aggregate
                .entry(name.clone())
                .or_default()
                .extend(defs.iter().cloned());
        }
    }
    aggregate
        .into_iter()
        .map(|(name, defs)| (name, Arc::from(defs)))
        .collect()
}

pub(crate) fn workspace_type_index_for(
    db: &SemanticDatabase,
    ws_id: WorkspaceId,
) -> &WorkspaceTypeIndex {
    db.workspace_index_cache()
        .types
        .get(&ws_id)
        .expect("workspace type index must be built before read")
}

/// Per-file deprecated facts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DeprecatedFileData {
    pub names: Vec<SmolStr>,
    pub member_keys: Vec<(SemanticId, SmolStr)>,
}

/// A shard's deprecated facts: `FileId -> per-file data`.
///
/// Empty files are not stored; a file update replaces or removes exactly one entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DeprecatedShard {
    pub(crate) files: HashMap<FileId, DeprecatedFileData>,
}

pub(crate) fn deprecated_shard(db: &SemanticDatabase, shard: u8) -> &DeprecatedShard {
    db.deprecated_shard_of(shard)
}

pub(crate) fn build_deprecated_file(db: &SemanticDatabase, file_id: FileId) -> DeprecatedFileData {
    let mut data = DeprecatedFileData::default();
    let Some(file) = db.file_data_id(file_id) else {
        return data;
    };
    let facts = file_facts(db, file);
    for decl in &facts.decls {
        if matches!(decl.kind, DeclKind::Global) && decl.deprecated {
            data.names.push(decl.name.clone());
        }
    }
    for member in &facts.members {
        if member.deprecated {
            let key: SmolStr = member.key.to_path().into();
            data.member_keys.push((member.owner.clone(), key));
        }
    }
    data
}

pub(crate) fn build_deprecated_shard(db: &SemanticDatabase, shard: u8) -> DeprecatedShard {
    #[cfg(test)]
    db.rebuild_metrics
        .shard_scan_builds
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut files = HashMap::new();
    for &file_id in db.file_ids_in_shard(shard) {
        let data = build_deprecated_file(db, file_id);
        if !data.names.is_empty() || !data.member_keys.is_empty() {
            files.insert(file_id, data);
        }
    }
    DeprecatedShard { files }
}

/// Incrementally maintained deprecated-name index for one workspace.
///
/// The old `deprecated_*_names_for()` API built a fresh `Arc<HashSet>` on every
/// call. This index is updated with the same file add/remove operations as the
/// other workspace indexes, so queries are pure hash lookups with no allocation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DeprecatedIndex {
    global_counts: HashMap<SmolStr, u32>,
    member_counts: HashMap<SmolStr, u32>,
    by_file: HashMap<FileId, DeprecatedFileData>,
}

impl DeprecatedIndex {
    pub(crate) fn add_file(&mut self, file_id: FileId, data: &DeprecatedFileData) {
        if self.by_file.contains_key(&file_id) {
            self.remove_file(file_id);
        }
        for name in &data.names {
            *self.global_counts.entry(name.clone()).or_default() += 1;
        }
        for (_, key) in &data.member_keys {
            *self.member_counts.entry(key.clone()).or_default() += 1;
        }
        if !data.names.is_empty() || !data.member_keys.is_empty() {
            self.by_file.insert(file_id, data.clone());
        }
    }

    pub(crate) fn remove_file(&mut self, file_id: FileId) {
        let Some(data) = self.by_file.remove(&file_id) else {
            return;
        };
        for name in &data.names {
            if let Some(count) = self.global_counts.get_mut(name) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.global_counts.remove(name);
                }
            }
        }
        for (_, key) in &data.member_keys {
            if let Some(count) = self.member_counts.get_mut(key) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.member_counts.remove(key);
                }
            }
        }
    }

    pub(crate) fn is_global_deprecated(&self, name: &str) -> bool {
        self.global_counts.contains_key(name)
    }

    pub(crate) fn is_member_name_deprecated(&self, name: &str) -> bool {
        self.member_counts.contains_key(name)
    }
}

pub(crate) fn build_workspace_deprecated_index(
    db: &SemanticDatabase,
    ws_id: WorkspaceId,
) -> DeprecatedIndex {
    let mut index = DeprecatedIndex::default();
    for shard in 0..EXPORT_SHARDS {
        let shard = deprecated_shard(db, shard);
        for (file_id, data) in &shard.files {
            if file_matches_workspace_id(db, *file_id, ws_id) {
                index.add_file(*file_id, data);
            }
        }
    }
    index
}

pub(crate) fn workspace_deprecated_index_for(
    db: &SemanticDatabase,
    ws_id: WorkspaceId,
) -> &DeprecatedIndex {
    db.workspace_index_cache()
        .deprecated
        .get(&ws_id)
        .expect("workspace deprecated index must be built before read")
}

/// Cached workspace ids (roots + `REMOTE`; `MAIN` when no roots are registered).
///
/// The list is refreshed only when workspace roots change, so hot query paths do
/// not allocate a fresh `Vec` on every call.
pub(crate) fn all_workspace_ids(db: &SemanticDatabase) -> &[WorkspaceId] {
    db.all_workspace_ids()
}

/// Deterministic workspace priority used by cross-workspace lookups.
///
/// Main workspace wins over libraries, libraries over std; remote stays last.
/// Library order keeps registration order (stable sort). Cached in the database.
pub(crate) fn workspace_lookup_order(db: &SemanticDatabase) -> &[WorkspaceId] {
    db.workspace_lookup_order()
}

fn file_matches_workspace_id(db: &SemanticDatabase, file_id: FileId, ws_id: WorkspaceId) -> bool {
    let file_ws = file_workspace_id(db, file_id);
    if ws_id == WorkspaceId::REMOTE {
        file_ws.is_none()
    } else {
        file_ws == Some(ws_id)
    }
}

fn find_global_types(db: &SemanticDatabase, full_name: &str) -> Arc<[TypeDef]> {
    // The cross-workspace aggregate is maintained by `rebuild_workspace_indexes`
    // and `refresh_global_type_keys`; repeated queries only clone an `Arc`.
    db.workspace_index_cache()
        .global_types
        .get(full_name)
        .cloned()
        .unwrap_or_default()
}

fn find_internal_types(db: &SemanticDatabase, ws: WorkspaceId, full_name: &str) -> Arc<[TypeDef]> {
    workspace_type_index_for(db, ws).find_all(TypeScope::Internal(ws), full_name)
}

fn find_file_types(db: &SemanticDatabase, file_id: FileId, full_name: &str) -> Arc<[TypeDef]> {
    let ws = file_workspace_id(db, file_id).unwrap_or(WorkspaceId::MAIN);
    workspace_type_index_for(db, ws).find_all(TypeScope::File(file_id), full_name)
}

/// Resolve **all definition locations** of a named type in the current file scope
/// (mirrors `resolve_type_def` resolution order, but returns every same-name definition in the bucket; for duplicate-type checks).
pub(crate) fn resolve_type_def_locations(
    db: &SemanticDatabase,
    file: FileId,
    bare_name: SmolStr,
) -> Arc<[TypeDef]> {
    let file_id = file;
    let facts = file_facts(db, file);
    let ws = file_workspace_id(db, file_id).unwrap_or(WorkspaceId::MAIN);

    if let Some(ns) = &facts.namespace {
        let full = SmolStr::new(format!("{}.{}", ns, bare_name));
        let defs = find_internal_types(db, ws, &full);
        if !defs.is_empty() {
            return defs;
        }
        let defs = find_global_types(db, &full);
        if !defs.is_empty() {
            return defs;
        }
    }
    for us in &facts.usings {
        let full = SmolStr::new(format!("{}.{}", us, bare_name));
        let defs = find_internal_types(db, ws, &full);
        if !defs.is_empty() {
            return defs;
        }
        let defs = find_global_types(db, &full);
        if !defs.is_empty() {
            return defs;
        }
    }

    let defs = find_file_types(db, file_id, &bare_name);
    if !defs.is_empty() {
        return defs;
    }
    let defs = find_internal_types(db, ws, &bare_name);
    if !defs.is_empty() {
        return defs;
    }
    find_global_types(db, &bare_name)
}

/// Resolve a named type in the current file scope (mirrors the old `find_type_decl` order):
/// 1. file namespace qualification (Internal -> Global); 2. `@using` qualification (Internal -> Global);
/// 3. bare name (**same-file Private** -> Internal -> Global).
pub(crate) fn resolve_type_def(
    db: &SemanticDatabase,
    file: FileId,
    bare_name: SmolStr,
) -> Option<TypeDef> {
    resolve_type_def_locations(db, file, bare_name)
        .first()
        .cloned()
}

/// Constructor attribute associated with a type definition.
///
/// Class tables are usually created by factory functions like `meta("ClassName")` with `---@[constructor("init")]`;
/// the attribute belongs to the factory function signature. Here we trace back from the runtime value declaration
/// bound to the type definition to that factory call, so class tables required across files keep constructor-call semantics.
pub(crate) fn constructor_attribute_of_type(
    db: &SemanticDatabase,
    type_def: SemanticId,
) -> Option<ConstructorAttribute> {
    for resolved in resolve_owner_set(db, type_def.clone()) {
        if matches!(resolved, SemanticId::Decl(_)) {
            if let Some(attribute) = constructor_attribute_of_decl(db, resolved.clone()) {
                return Some(attribute);
            }
        }
    }
    None
}

fn constructor_attribute_of_decl(
    db: &SemanticDatabase,
    decl: SemanticId,
) -> Option<ConstructorAttribute> {
    let key = match &decl {
        SemanticId::Decl(key) => key,
        _ => return None,
    };
    let file = db.file_data_id(key.file_id)?;
    let facts = file_facts(db, file);
    let decl = facts.decl_by_id(&decl)?;
    let value_syntax = decl.value_expr_syntax?;
    let tree = syntax_tree(db, file);
    let node = value_syntax.to_node_from_root(&tree.get_red_root())?;
    let call = LuaCallExpr::cast(node)?;
    let prefix = call.get_prefix_expr()?;
    let LuaExpr::NameExpr(name_expr) = &prefix else {
        return None;
    };
    let callee_decl = resolve_name(db, file, name_expr.get_position())?;
    let callee_file = match &callee_decl {
        SemanticId::Decl(key) => key.file_id,
        _ => return None,
    };
    let callee_input = db.file_data_id(callee_file)?;
    let callee_facts = file_facts(db, callee_input);
    let callee = callee_facts.decl_by_id(&callee_decl)?;
    let callee_closure = callee.value_expr_syntax?;
    let signature = callee_facts.signature_by_closure(callee_closure)?;
    let docs = signature.docs.as_ref()?;
    docs.constructor_params
        .first()
        .map(|(_, attribute)| attribute.clone())
}

// ──────────────────────────────────────────────
// Phase 2: workspace member association (after full analysis)
// ──────────────────────────────────────────────

/// Workspace-level member index: owner -> member references.
///
/// Each owner bucket keeps the declarations in source order, so overloads stay
/// stable, and a per-file key list makes remove/add O(file contribution).
///
/// Aggregate lists (`all` / `by_name`) are stored as `Arc<[MemberRef]>` and
/// rebuilt once per owner after a file add/remove batch; queries clone the
/// `Arc` instead of re-collecting the bucket on every call.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OwnerMembers {
    by_id: HashMap<SemanticId, MemberRef>,
    by_name_ids: HashMap<SmolStr, Vec<SemanticId>>,
    order: Vec<SemanticId>,
    all: Arc<[MemberRef]>,
    by_name: HashMap<SmolStr, Arc<[MemberRef]>>,
}

impl OwnerMembers {
    fn insert(&mut self, member_id: SemanticId, member: MemberRef, name: SmolStr) -> bool {
        if self.by_id.contains_key(&member_id) {
            return false;
        }
        self.by_id.insert(member_id.clone(), member);
        self.order.push(member_id.clone());
        self.by_name_ids.entry(name).or_default().push(member_id);
        true
    }

    fn remove(&mut self, member_id: &SemanticId) -> bool {
        if self.by_id.remove(member_id).is_none() {
            return false;
        }
        self.order.retain(|id| id != member_id);
        for member_ids in self.by_name_ids.values_mut() {
            member_ids.retain(|id| id != member_id);
        }
        self.by_name_ids
            .retain(|_, member_ids| !member_ids.is_empty());
        true
    }

    /// Rebuild the shared aggregate lists after this bucket changed.
    fn rebuild_caches(&mut self) {
        let all: Vec<MemberRef> = self
            .order
            .iter()
            .filter_map(|member_id| self.by_id.get(member_id).cloned())
            .collect();
        self.all = Arc::from(all);

        let mut by_name = HashMap::with_capacity(self.by_name_ids.len());
        for (name, member_ids) in &self.by_name_ids {
            let members: Vec<MemberRef> = member_ids
                .iter()
                .filter_map(|member_id| self.by_id.get(member_id).cloned())
                .collect();
            by_name.insert(name.clone(), Arc::from(members));
        }
        self.by_name = by_name;
    }

    fn all(&self) -> Arc<[MemberRef]> {
        Arc::clone(&self.all)
    }

    fn named(&self, name: &str) -> Option<Arc<[MemberRef]>> {
        self.by_name.get(name).cloned()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceMemberIndex {
    owners: HashMap<SemanticId, OwnerMembers>,
    /// Canonical `OwnerId` buckets. Module export / require alias members attach
    /// here so `require("mod").extra` can see members contributed by other files.
    owners_by_id: HashMap<OwnerId, OwnerMembers>,
    by_file: HashMap<FileId, Vec<(SemanticId, SemanticId)>>,
    by_file_owner_id: HashMap<FileId, Vec<(OwnerId, SemanticId)>>,
}

impl WorkspaceMemberIndex {
    fn rebuild_owner_caches<T: Eq + std::hash::Hash>(
        owners: &mut HashMap<T, OwnerMembers>,
        touched: HashSet<T>,
    ) {
        for owner in touched {
            let is_empty = owners
                .get(&owner)
                .is_none_or(|bucket| bucket.by_id.is_empty());
            if is_empty {
                owners.remove(&owner);
            } else if let Some(bucket) = owners.get_mut(&owner) {
                bucket.rebuild_caches();
            }
        }
    }

    pub(crate) fn members_of_owner(&self, owner: &SemanticId) -> Option<Arc<[MemberRef]>> {
        self.owners.get(owner).map(OwnerMembers::all)
    }

    pub(crate) fn members_of_owner_named(
        &self,
        owner: &SemanticId,
        name: &str,
    ) -> Option<Arc<[MemberRef]>> {
        self.owners.get(owner).and_then(|bucket| bucket.named(name))
    }

    pub(crate) fn members_of_owner_id(&self, owner: &OwnerId) -> Option<Arc<[MemberRef]>> {
        self.owners_by_id.get(owner).map(OwnerMembers::all)
    }

    pub(crate) fn members_of_owner_id_named(
        &self,
        owner: &OwnerId,
        name: &str,
    ) -> Option<Arc<[MemberRef]>> {
        self.owners_by_id
            .get(owner)
            .and_then(|bucket| bucket.named(name))
    }

    pub(crate) fn remove_file(&mut self, file_id: FileId) {
        let mut touched_raw: HashSet<SemanticId> = HashSet::new();
        if let Some(keys) = self.by_file.remove(&file_id) {
            for (owner, member_id) in keys {
                if let Some(bucket) = self.owners.get_mut(&owner)
                    && bucket.remove(&member_id)
                {
                    touched_raw.insert(owner);
                }
            }
        }
        Self::rebuild_owner_caches(&mut self.owners, touched_raw);

        let mut touched_canonical: HashSet<OwnerId> = HashSet::new();
        if let Some(keys) = self.by_file_owner_id.remove(&file_id) {
            for (owner, member_id) in keys {
                if let Some(bucket) = self.owners_by_id.get_mut(&owner)
                    && bucket.remove(&member_id)
                {
                    touched_canonical.insert(owner);
                }
            }
        }
        Self::rebuild_owner_caches(&mut self.owners_by_id, touched_canonical);
    }

    /// Insert one file's member contributions without rebuilding aggregate caches.
    ///
    /// Returns the `by_file` key lists; callers that batch many files (full
    /// workspace build) call [`Self::rebuild_touched_caches`] once at the end.
    fn add_file_inner(
        &mut self,
        exports: &FileExports,
        touched_raw: &mut HashSet<SemanticId>,
        touched_canonical: &mut HashSet<OwnerId>,
    ) -> (Vec<(SemanticId, SemanticId)>, Vec<(OwnerId, SemanticId)>) {
        let mut keys = Vec::new();
        let mut canonical_keys = Vec::new();
        for member in &exports.members {
            let owner = member.owner.clone();
            let member_id = member.member.clone();
            let name: SmolStr = member.key.to_path().into();
            let member_ref = MemberRef {
                file_id: member.file_id,
                id: member_id.clone(),
                name: name.clone(),
            };

            let bucket = self.owners.entry(owner.clone()).or_default();
            if bucket.insert(member_id.clone(), member_ref.clone(), name.clone()) {
                keys.push((owner.clone(), member_id.clone()));
                touched_raw.insert(owner);
            }

            let canonical_owner = member.owner_id.clone();
            let canonical_bucket = self
                .owners_by_id
                .entry(canonical_owner.clone())
                .or_default();
            if canonical_bucket.insert(member_id.clone(), member_ref, name) {
                canonical_keys.push((canonical_owner.clone(), member_id));
                touched_canonical.insert(canonical_owner);
            }
        }
        (keys, canonical_keys)
    }

    fn rebuild_touched_caches(
        &mut self,
        touched_raw: HashSet<SemanticId>,
        touched_canonical: HashSet<OwnerId>,
    ) {
        Self::rebuild_owner_caches(&mut self.owners, touched_raw);
        Self::rebuild_owner_caches(&mut self.owners_by_id, touched_canonical);
    }

    /// Raw owner identities present in this workspace bucket.
    fn owner_ids(&self) -> impl Iterator<Item = SemanticId> + '_ {
        self.owners.keys().cloned()
    }

    /// Raw `(owner, name)` keys present in this workspace bucket.
    fn owner_name_keys(&self) -> impl Iterator<Item = (SemanticId, SmolStr)> + '_ {
        self.owners.iter().flat_map(|(owner, bucket)| {
            bucket
                .by_name_ids
                .keys()
                .map(move |name| (owner.clone(), name.clone()))
        })
    }

    pub(crate) fn add_file(&mut self, file_id: FileId, exports: &FileExports) {
        let mut touched_raw: HashSet<SemanticId> = HashSet::new();
        let mut touched_canonical: HashSet<OwnerId> = HashSet::new();
        let (keys, canonical_keys) =
            self.add_file_inner(exports, &mut touched_raw, &mut touched_canonical);
        self.rebuild_touched_caches(touched_raw, touched_canonical);
        self.by_file.insert(file_id, keys);
        self.by_file_owner_id.insert(file_id, canonical_keys);
    }
}

/// Member index scoped to a single workspace.
pub(crate) fn build_workspace_member_index(
    db: &SemanticDatabase,
    ws_id: WorkspaceId,
) -> WorkspaceMemberIndex {
    let mut index = WorkspaceMemberIndex::default();
    let mut touched_raw: HashSet<SemanticId> = HashSet::new();
    let mut touched_canonical: HashSet<OwnerId> = HashSet::new();
    for shard in 0..EXPORT_SHARDS {
        let shard = export_shard(db, shard);
        for (file_id, exports) in &shard.files {
            if !file_matches_workspace_id(db, *file_id, ws_id) {
                continue;
            }
            let (keys, canonical_keys) =
                index.add_file_inner(exports, &mut touched_raw, &mut touched_canonical);
            index.by_file.insert(*file_id, keys);
            index.by_file_owner_id.insert(*file_id, canonical_keys);
        }
    }
    index.rebuild_touched_caches(touched_raw, touched_canonical);
    index
}

/// Merge one raw owner bucket across workspaces, returning the single workspace
/// bucket `Arc` when possible so repeated queries can clone it.
pub(crate) fn aggregate_member_bucket(
    ws_ids: &[WorkspaceId],
    members: &HashMap<WorkspaceId, WorkspaceMemberIndex>,
    owner: &SemanticId,
    named: Option<&str>,
) -> Option<Arc<[MemberRef]>> {
    let mut single: Option<Arc<[MemberRef]>> = None;
    let mut extra: Vec<MemberRef> = Vec::new();
    for &ws_id in ws_ids {
        let Some(index) = members.get(&ws_id) else {
            continue;
        };
        let bucket = match named {
            Some(name) => index.members_of_owner_named(owner, name),
            None => index.members_of_owner(owner),
        };
        let Some(bucket) = bucket else {
            continue;
        };
        if bucket.is_empty() {
            continue;
        }
        if single.is_none() && extra.is_empty() {
            single = Some(bucket);
        } else {
            if let Some(first) = single.take() {
                extra.extend(first.iter().cloned());
            }
            extra.extend(bucket.iter().cloned());
        }
    }
    match single {
        Some(bucket) => Some(bucket),
        None => (!extra.is_empty()).then(|| Arc::from(extra)),
    }
}

/// Build cross-workspace raw owner / named owner aggregate caches.
pub(crate) fn build_member_aggregates(
    ws_ids: &[WorkspaceId],
    members: &HashMap<WorkspaceId, WorkspaceMemberIndex>,
) -> (
    HashMap<SemanticId, Arc<[MemberRef]>>,
    HashMap<(SemanticId, SmolStr), Arc<[MemberRef]>>,
) {
    let mut owner_keys: HashSet<SemanticId> = HashSet::new();
    let mut name_keys: HashSet<(SemanticId, SmolStr)> = HashSet::new();
    for &ws_id in ws_ids {
        let Some(index) = members.get(&ws_id) else {
            continue;
        };
        owner_keys.extend(index.owner_ids());
        name_keys.extend(index.owner_name_keys());
    }

    let owner_members = owner_keys
        .into_iter()
        .filter_map(|owner| {
            aggregate_member_bucket(ws_ids, members, &owner, None).map(|bucket| (owner, bucket))
        })
        .collect();

    let owner_members_named = name_keys
        .into_iter()
        .filter_map(|key| {
            aggregate_member_bucket(ws_ids, members, &key.0, Some(key.1.as_str()))
                .map(|bucket| (key, bucket))
        })
        .collect();

    (owner_members, owner_members_named)
}

/// Recompute the aggregate buckets for owners / names changed by a file update.
pub(crate) fn workspace_member_index_for(
    db: &SemanticDatabase,
    ws_id: WorkspaceId,
) -> &WorkspaceMemberIndex {
    db.workspace_index_cache()
        .members
        .get(&ws_id)
        .expect("workspace member index must be built before read")
}

/// Per-file reference index: only collects reference points in this file that can resolve to cross-file identities.
///
/// This is the L1 layer of the reference index: each file computes independently and is memoized; editing one file recomputes one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileReferences {
    pub decl_refs: HashMap<SemanticId, Vec<TextRange>>,
    pub member_refs: HashMap<SemanticId, Vec<TextRange>>,
    /// Member definition sites (`T.x = v` / `@field x` / table field keys / method names).
    pub member_defs: HashMap<SemanticId, Vec<TextRange>>,
    /// Canonical identity-level dependencies of this file (P7).
    pub deps: FileDependencies,
}

/// Per-file reference index. Pure lookup in the write-time built cache.
pub(crate) fn build_file_references(db: &SemanticDatabase, file: FileId) -> FileReferences {
    let facts = file_facts(db, file);
    let tree = syntax_tree(db, file);
    let mut out = FileReferences::default();

    // Name use sites -> declarations (and identity-level dependencies).
    for name_use in &facts.name_uses {
        if let Some(decl) = resolve_name(db, file, name_use.syntax.get_range().start()) {
            push_target_dependencies(db, &decl, &mut out.deps);
            out.decl_refs
                .entry(decl)
                .or_default()
                .push(name_use.syntax.get_range());
        } else {
            // Unresolved global may be defined by a later file write.
            out.deps
                .insert(DependencyKey::Global(name_use.name.clone()));
        }
    }

    // Member definition sites (so the workspace reference index can give declaration ranges directly without re-scanning members per file).
    for member in &facts.members {
        if let Some(range) = member.id.member_key_range() {
            out.member_defs
                .entry(member.id.clone())
                .or_default()
                .push(range);
        }
    }

    // Index expression use sites -> members.
    for &syntax in &facts.member_uses {
        let Some(node) = syntax.to_node_from_root(&tree.get_red_root()) else {
            continue;
        };
        let Some(index_expr) = LuaIndexExpr::cast(node) else {
            continue;
        };
        let owner_and_name = member_ref_from_index_expr(&facts, &index_expr);
        if let Some((owner, name)) = &owner_and_name
            && let Some(owner_id) = canonical_owner_id(db, owner)
        {
            out.deps.insert(DependencyKey::Member(
                owner_id,
                LuaMemberKey::Name(name.clone()),
            ));
        }
        if let Some(member_id) = resolve_member_id(db, &facts, &index_expr) {
            push_target_dependencies(db, &member_id, &mut out.deps);
            let Some(key) = index_expr.get_index_key() else {
                continue;
            };
            let Some(range) = key.get_range() else {
                continue;
            };
            out.member_refs.entry(member_id).or_default().push(range);
        }
    }

    // require("mod") -> module identity dependency.
    if let Some(config) = db.config_input() {
        for call in tree
            .get_red_root()
            .descendants()
            .filter_map(LuaCallExpr::cast)
        {
            if !call.is_require() {
                continue;
            }
            let Some(module_name) = require_module_name_from_call(&call) else {
                continue;
            };
            if let Some(module_file) = module_file_of(db, config, module_name) {
                out.deps.insert(DependencyKey::Module(module_file));
            }
        }
    }

    out
}

/// `require("mod")` literal argument -> module name.
fn require_module_name_from_call(call: &LuaCallExpr) -> Option<SmolStr> {
    let arg = call.get_args_list()?.get_args().next()?;
    let LuaExpr::LiteralExpr(literal) = arg else {
        return None;
    };
    let LuaLiteralToken::String(token) = literal.get_literal()? else {
        return None;
    };
    Some(SmolStr::new(token.get_value()))
}

/// Record canonical dependencies for a resolved semantic target.
fn push_target_dependencies(
    db: &SemanticDatabase,
    target: &SemanticId,
    deps: &mut FileDependencies,
) {
    match target {
        SemanticId::Name(name) => deps.insert(DependencyKey::Global(SmolStr::new(name.as_str()))),
        SemanticId::TypeDef(key) => {
            deps.insert(DependencyKey::Type(key.scope, key.full_name.clone()));
            deps.insert(DependencyKey::RuntimeValue(key.full_name.clone()));
        }
        SemanticId::Decl(key) => {
            if let Some(facts) = db.file_facts_of(key.file_id)
                && let Some(decl) = facts.decl_by_id(target)
                && matches!(decl.kind, DeclKind::Global)
            {
                deps.insert(DependencyKey::Global(decl.name.clone()));
            }
        }
        SemanticId::Member(key) => {
            if let Some(facts) = db.file_facts_of(key.file_id)
                && let Some(member) = facts.member_by_id(target)
                && let Some(owner_id) = canonical_owner_id(db, &member.owner)
            {
                deps.insert(DependencyKey::Member(owner_id, member.key.clone()));
            }
        }
        SemanticId::Signature(_) => {}
    }
}

/// A shard's reference index: `FileId -> per-file references`.
///
/// A file update replaces exactly one map entry; no aggregation over all files is
/// required inside a shard.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReferenceShard {
    pub(crate) files: HashMap<FileId, Arc<FileReferences>>,
}

pub(crate) fn reference_shard(db: &SemanticDatabase, shard: u8) -> &ReferenceShard {
    db.reference_shard_of(shard)
}

pub(crate) fn build_reference_shard(db: &SemanticDatabase, shard: u8) -> ReferenceShard {
    #[cfg(test)]
    db.rebuild_metrics
        .shard_scan_builds
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut files = HashMap::new();
    for &file_id in db.file_ids_in_shard(shard) {
        if let Some(cache) = db.file_cache(file_id) {
            files.insert(file_id, Arc::clone(&cache.references));
        }
    }
    ReferenceShard { files }
}

/// Workspace-level reference index: aggregates per-file references and keeps a
/// `FileId -> Arc<FileReferences>` map for incremental remove/add.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceReferenceIndex {
    pub decl_refs: HashMap<SemanticId, Vec<(FileId, TextRange)>>,
    pub member_refs: HashMap<SemanticId, Vec<(FileId, TextRange)>>,
    pub member_defs: HashMap<SemanticId, Vec<(FileId, TextRange)>>,
    by_file: HashMap<FileId, Arc<FileReferences>>,
}

impl WorkspaceReferenceIndex {
    pub(crate) fn remove_file(&mut self, file_id: FileId) {
        let Some(refs) = self.by_file.remove(&file_id) else {
            return;
        };
        retain_file_ranges(&mut self.decl_refs, file_id, &refs.decl_refs);
        retain_file_ranges(&mut self.member_refs, file_id, &refs.member_refs);
        retain_file_ranges(&mut self.member_defs, file_id, &refs.member_defs);
    }

    pub(crate) fn add_file(&mut self, file_id: FileId, refs: Arc<FileReferences>) {
        extend_file_ranges(&mut self.decl_refs, file_id, &refs.decl_refs);
        extend_file_ranges(&mut self.member_refs, file_id, &refs.member_refs);
        extend_file_ranges(&mut self.member_defs, file_id, &refs.member_defs);
        self.by_file.insert(file_id, refs);
    }
}

fn extend_file_ranges(
    aggregate: &mut HashMap<SemanticId, Vec<(FileId, TextRange)>>,
    file_id: FileId,
    ranges: &HashMap<SemanticId, Vec<TextRange>>,
) {
    for (target, ranges) in ranges {
        aggregate
            .entry(target.clone())
            .or_default()
            .extend(ranges.iter().map(|range| (file_id, *range)));
    }
}

fn retain_file_ranges(
    aggregate: &mut HashMap<SemanticId, Vec<(FileId, TextRange)>>,
    file_id: FileId,
    ranges: &HashMap<SemanticId, Vec<TextRange>>,
) {
    for target in ranges.keys() {
        if let Some(entries) = aggregate.get_mut(target) {
            entries.retain(|(entry_file, _)| *entry_file != file_id);
            if entries.is_empty() {
                aggregate.remove(target);
            }
        }
    }
}

/// Reference index scoped to a single workspace.
pub(crate) fn build_workspace_reference_index(
    db: &SemanticDatabase,
    ws_id: WorkspaceId,
) -> WorkspaceReferenceIndex {
    let mut index = WorkspaceReferenceIndex::default();
    for shard in 0..EXPORT_SHARDS {
        let shard = reference_shard(db, shard);
        for (file_id, refs) in &shard.files {
            if !file_matches_workspace_id(db, *file_id, ws_id) {
                continue;
            }
            index.add_file(*file_id, Arc::clone(refs));
        }
    }
    index
}

pub(crate) fn workspace_reference_index_for(
    db: &SemanticDatabase,
    ws_id: WorkspaceId,
) -> &WorkspaceReferenceIndex {
    db.workspace_index_cache()
        .references
        .get(&ws_id)
        .expect("workspace reference index must be built before read")
}

/// Whether `resolve_member_id` can be trusted directly for this owner/name pair.
///
/// Name-rooted owners are unambiguous. Local declarations are trusted only when they
/// cannot carry an annotated class type / same-owner `@class` shadow / same-name doc member;
/// otherwise the full resolver must decide between runtime member and `@field`.
/// Query-level member resolution: owner/name -> concrete member id.
fn resolve_member_id(
    db: &SemanticDatabase,
    facts: &FileFacts,
    index_expr: &LuaIndexExpr,
) -> Option<SemanticId> {
    let (owner, name) = member_ref_from_index_expr(facts, index_expr)?;
    for resolved in resolve_owner_set(db, owner) {
        for member in members_of_owner(db, resolved).iter().cloned() {
            if member.name == name {
                return Some(member.id);
            }
        }
    }
    None
}

/// Canonical module owner for a raw file-local owner identity.
///
/// Covers both this file's own module export target (`return M`) and local
/// declarations bound to another module through `require`.
fn logical_module_owner_id(db: &SemanticDatabase, owner: &SemanticId) -> Option<OwnerId> {
    let file_id = match owner {
        SemanticId::Decl(key) => key.file_id,
        SemanticId::Member(key) => key.file_id,
        _ => return None,
    };
    let facts = db.file_facts_of(file_id)?;
    if let Some(module_file) = module_export_owner_file(facts, file_id, owner) {
        return Some(OwnerId::Module(module_file));
    }
    let exports = db.file_cache(file_id)?.exports.as_ref();
    exports
        .aliases
        .iter()
        .find(|alias| &alias.decl == owner)
        .map(|alias| alias.owner_id())
}

/// Merge canonical `OwnerId` bucket members into a raw member list.
///
/// Returns the raw `Arc` unchanged when there is nothing to add, so the common
/// raw-owner query does not allocate on every call.
fn merge_canonical_members(
    db: &SemanticDatabase,
    owner: &SemanticId,
    raw: Arc<[MemberRef]>,
    named: Option<&str>,
) -> Arc<[MemberRef]> {
    let Some(owner_id) = logical_module_owner_id(db, owner) else {
        return raw;
    };
    let mut extra: Vec<MemberRef> = Vec::new();
    for &ws_id in all_workspace_ids(db) {
        let index = workspace_member_index_for(db, ws_id);
        let members = match named {
            Some(name) => index.members_of_owner_id_named(&owner_id, name),
            None => index.members_of_owner_id(&owner_id),
        };
        let Some(members) = members else {
            continue;
        };
        for member in members.iter() {
            let already_known = raw
                .iter()
                .any(|existing| existing.id == member.id && existing.file_id == member.file_id)
                || extra
                    .iter()
                    .any(|existing| existing.id == member.id && existing.file_id == member.file_id);
            if !already_known {
                extra.push(member.clone());
            }
        }
    }
    if extra.is_empty() {
        return raw;
    }
    let mut out = Vec::with_capacity(raw.len() + extra.len());
    out.extend(raw.iter().cloned());
    out.extend(extra);
    Arc::from(out)
}
/// Read the raw (non-canonical) member bucket for an owner.
///
/// Prefers the workspace member index, whose buckets are shared `Arc` values;
/// falls back to the owner's `FileFacts` when the index has no contribution
/// (e.g. an unconfigured database or a facts-only test setup).
fn raw_owner_members(db: &SemanticDatabase, owner: &SemanticId) -> Arc<[MemberRef]> {
    let from_index = if db.config_input().is_some() {
        members_from_workspaces(db, owner, None)
    } else {
        Arc::from(Vec::new())
    };
    if !from_index.is_empty() {
        return from_index;
    }
    let owner_file = match owner {
        SemanticId::Decl(key) => Some(key.file_id),
        SemanticId::Member(key) => Some(key.file_id),
        _ => None,
    };
    if let Some(owner_file_id) = owner_file
        && let Some(file_id) = db.file_data_id(owner_file_id)
    {
        let facts = file_facts(db, file_id);
        return Arc::from(
            facts
                .members_of_owner(owner)
                .map(|member| MemberRef {
                    file_id: owner_file_id,
                    id: member.id.clone(),
                    name: member.key.to_path().into(),
                })
                .collect::<Vec<_>>(),
        );
    }
    from_index
}

fn raw_owner_members_named(
    db: &SemanticDatabase,
    owner: &SemanticId,
    name: &str,
) -> Arc<[MemberRef]> {
    let from_index = if db.config_input().is_some() {
        members_from_workspaces(db, owner, Some(name))
    } else {
        Arc::from(Vec::new())
    };
    if !from_index.is_empty() {
        return from_index;
    }
    let owner_file = match owner {
        SemanticId::Decl(key) => Some(key.file_id),
        SemanticId::Member(key) => Some(key.file_id),
        _ => None,
    };
    if let Some(owner_file_id) = owner_file
        && let Some(file_id) = db.file_data_id(owner_file_id)
    {
        let facts = file_facts(db, file_id);
        return Arc::from(
            facts
                .members_of_owner_named(owner, name)
                .map(|member| MemberRef {
                    file_id: owner_file_id,
                    id: member.id.clone(),
                    name: member.key.to_path().into(),
                })
                .collect::<Vec<_>>(),
        );
    }
    from_index
}

/// Collect one raw owner bucket from the workspace member indexes.
///
/// When only one workspace has a non-empty bucket (the common case), its cached
/// `Arc` is returned directly instead of cloning every member.
fn members_from_workspaces(
    db: &SemanticDatabase,
    owner: &SemanticId,
    named: Option<&str>,
) -> Arc<[MemberRef]> {
    let cache = db.workspace_index_cache();
    if let Some(name) = named {
        if let Some(bucket) = cache
            .owner_members_named
            .get(&(owner.clone(), SmolStr::new(name)))
        {
            return Arc::clone(bucket);
        }
    } else if let Some(bucket) = cache.owner_members.get(owner) {
        return Arc::clone(bucket);
    }

    // Fallback for buckets not present in the aggregate cache (facts-only test
    // setups / owner without contributions): merge in workspace order.
    let mut single: Option<Arc<[MemberRef]>> = None;
    let mut extra: Vec<MemberRef> = Vec::new();
    for &ws_id in all_workspace_ids(db) {
        let index = workspace_member_index_for(db, ws_id);
        let members = match named {
            Some(name) => index.members_of_owner_named(owner, name),
            None => index.members_of_owner(owner),
        };
        let Some(members) = members else {
            continue;
        };
        if members.is_empty() {
            continue;
        }
        if single.is_none() && extra.is_empty() {
            single = Some(members);
        } else {
            if let Some(first) = single.take() {
                extra.extend(first.iter().cloned());
            }
            extra.extend(members.iter().cloned());
        }
    }
    match single {
        Some(members) => members,
        None => Arc::from(extra),
    }
}

/// Members of an owner `SemanticId` (cross-file; reads cached workspace buckets,
/// falling back to the declaring file's `FileFacts`).
pub(crate) fn members_of_owner(db: &SemanticDatabase, owner: SemanticId) -> Arc<[MemberRef]> {
    let raw = raw_owner_members(db, &owner);
    merge_canonical_members(db, &owner, raw, None)
}

/// Members of an owner with a specific name.
///
/// Uses the workspace member index's `by_name` cache when available; file-local
/// owners fall back to their own `FileFacts` (which already indexes by name).
pub(crate) fn members_of_owner_named(
    db: &SemanticDatabase,
    owner: SemanticId,
    name: SmolStr,
) -> Arc<[MemberRef]> {
    let raw = raw_owner_members_named(db, &owner, name.as_str());
    merge_canonical_members(db, &owner, raw, Some(name.as_str()))
}

/// Member keys of an owner `SemanticId` (cross-file, completion candidates).
/// Union: owner key (runtime `M.x`) + resolved concrete id key (`@field` etc.).
pub(crate) fn member_keys_of_owner(db: &SemanticDatabase, owner: SemanticId) -> Vec<SmolStr> {
    let mut keys: Vec<SmolStr> = Vec::new();
    // Union of dual identities: same-name type (@field) + runtime value (member declaration).
    for resolved in resolve_owner_set(db, owner.clone()) {
        keys.extend(
            members_of_owner(db, resolved)
                .iter()
                .cloned()
                .map(|member| member.name),
        );
    }
    keys.sort();
    keys.dedup();
    keys
}

/// All type definitions for a given scope + full name (cross-file, reuses the workspace type index).
pub(crate) fn type_defs_in_scope(
    db: &SemanticDatabase,
    scope: TypeScope,
    full_name: SmolStr,
) -> Arc<[TypeDef]> {
    match scope {
        TypeScope::Global => find_global_types(db, &full_name),
        TypeScope::Internal(ws) => find_internal_types(db, ws, &full_name),
        TypeScope::File(file_id) => find_file_types(db, file_id, &full_name),
    }
}

/// Look up a global type (`@class` etc.) by full name (cross-file, reuses the workspace type index).
pub(crate) fn global_type_by_name(db: &SemanticDatabase, full_name: SmolStr) -> Option<SemanticId> {
    find_global_types(db, &full_name)
        .first()
        .map(|def| def.id.clone())
}

/// Per-file declaration contributions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FileDeclContribution {
    globals: Vec<(SmolStr, SemanticId)>,
    types: Vec<(SemanticId, SmolStr)>,
    runtime_values: Vec<(SmolStr, SemanticId)>,
}

/// Workspace declaration index: global declarations + type runtime values + type
/// definition locations, with per-file contributions for incremental updates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceDeclIndex {
    /// Global-name -> declarations, in file contribution order.
    global_by_name: HashMap<SmolStr, Vec<SemanticId>>,
    /// Type runtime values by bare name: `local M = {}` implementing `@class M`.
    runtime_by_name: HashMap<SmolStr, Vec<SemanticId>>,
    /// Type runtime values by `(file_id, bare_name)`.
    runtime_by_file_name: HashMap<(FileId, SmolStr), SemanticId>,
    /// Type definition -> `(file_id, bare name)`.
    type_def_by_id: HashMap<SemanticId, (FileId, SmolStr)>,
    by_file: HashMap<FileId, FileDeclContribution>,
}

impl WorkspaceDeclIndex {
    pub(crate) fn global_decl_named(&self, name: &SmolStr) -> Option<SemanticId> {
        self.global_by_name
            .get(name)
            .and_then(|decls| decls.last().cloned())
    }

    /// All global declarations with this name (cross-file overload candidates).
    pub(crate) fn global_decls_named(&self, name: &SmolStr) -> &[SemanticId] {
        self.global_by_name
            .get(name)
            .map(|decls| decls.as_slice())
            .unwrap_or(&[])
    }

    pub(crate) fn runtime_value_in(
        &self,
        file_id: FileId,
        bare_name: &SmolStr,
    ) -> Option<SemanticId> {
        self.runtime_by_file_name
            .get(&(file_id, bare_name.clone()))
            .cloned()
    }

    pub(crate) fn runtime_decls_named(&self, bare_name: &SmolStr) -> &[SemanticId] {
        self.runtime_by_name
            .get(bare_name)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub(crate) fn type_def_location(&self, id: &SemanticId) -> Option<(FileId, SmolStr)> {
        self.type_def_by_id.get(id).cloned()
    }

    pub(crate) fn remove_file(&mut self, file_id: FileId) {
        let Some(contribution) = self.by_file.remove(&file_id) else {
            return;
        };
        for (name, decl) in contribution.globals {
            if let Some(decls) = self.global_by_name.get_mut(&name) {
                decls.retain(|existing| existing != &decl);
                if decls.is_empty() {
                    self.global_by_name.remove(&name);
                }
            }
        }
        for (type_id, _) in contribution.types {
            self.type_def_by_id.remove(&type_id);
        }
        for (name, decl) in contribution.runtime_values {
            if let Some(decls) = self.runtime_by_name.get_mut(&name) {
                decls.retain(|existing| existing != &decl);
                if decls.is_empty() {
                    self.runtime_by_name.remove(&name);
                }
            }
            self.runtime_by_file_name.remove(&(file_id, name));
        }
    }

    pub(crate) fn add_file(&mut self, file_id: FileId, exports: &FileExports) {
        let mut contribution = FileDeclContribution::default();
        for global in &exports.globals {
            contribution
                .globals
                .push((global.name.clone(), global.decl.clone()));
            self.global_by_name
                .entry(global.name.clone())
                .or_default()
                .push(global.decl.clone());
        }
        for def in &exports.types {
            contribution.types.push((def.id.clone(), def.name.clone()));
            self.type_def_by_id
                .entry(def.id.clone())
                .or_insert((def.file_id, def.name.clone()));
        }
        for (name, decl) in &exports.runtime_values {
            contribution
                .runtime_values
                .push((name.clone(), decl.clone()));
            self.runtime_by_name
                .entry(name.clone())
                .or_default()
                .push(decl.clone());
            self.runtime_by_file_name
                .entry((file_id, name.clone()))
                .or_insert_with(|| decl.clone());
        }
        self.by_file.insert(file_id, contribution);
    }
}

pub(crate) fn build_workspace_decl_index(
    db: &SemanticDatabase,
    ws_id: WorkspaceId,
) -> WorkspaceDeclIndex {
    let mut index = WorkspaceDeclIndex::default();
    for shard in 0..EXPORT_SHARDS {
        let shard = export_shard(db, shard);
        for (file_id, exports) in &shard.files {
            if file_matches_workspace_id(db, *file_id, ws_id) {
                index.add_file(*file_id, exports);
            }
        }
    }
    index
}

pub(crate) fn workspace_decl_index_for(
    db: &SemanticDatabase,
    ws_id: WorkspaceId,
) -> &WorkspaceDeclIndex {
    db.workspace_index_cache()
        .decls
        .get(&ws_id)
        .expect("workspace declaration index must be built before read")
}

/// Look up a global variable/function declaration by name (cross-file, reuses the workspace declaration index).
pub(crate) fn global_decl_by_name(db: &SemanticDatabase, name: SmolStr) -> Option<SemanticId> {
    let roots = db.workspace_roots();
    if roots.is_empty() {
        return workspace_decl_index_for(db, WorkspaceId::MAIN).global_decl_named(&name);
    }
    for root in roots.iter() {
        if let Some(decl) = workspace_decl_index_for(db, root.id).global_decl_named(&name) {
            return Some(decl);
        }
    }
    None
}

/// All global declarations with this name in one workspace.
pub(crate) fn global_decls_named_in_workspace(
    db: &SemanticDatabase,
    ws_id: WorkspaceId,
    name: &str,
) -> Vec<SemanticId> {
    let name = SmolStr::new(name);
    workspace_decl_index_for(db, ws_id)
        .global_decls_named(&name)
        .to_vec()
}

/// Global overload candidates for a declaration's owning workspace.
///
/// Cross-file overloads are a same-workspace concept; a main workspace `f` must not
/// merge with a std/library `f` (otherwise shadowing changes diagnostics).
pub(crate) fn global_decls_named_for_file(
    db: &SemanticDatabase,
    file_id: FileId,
    name: &str,
) -> Vec<SemanticId> {
    if let Some(ws_id) = file_workspace_id(db, file_id) {
        let decls = global_decls_named_in_workspace(db, ws_id, name);
        if !decls.is_empty() {
            return decls;
        }
    }
    global_decls_named_primary(db, name)
}

/// First non-empty workspace in explicit priority order (main > library > std).
pub(crate) fn global_decls_named_primary(db: &SemanticDatabase, name: &str) -> Vec<SemanticId> {
    for &ws_id in workspace_lookup_order(db) {
        let decls = global_decls_named_in_workspace(db, ws_id, name);
        if !decls.is_empty() {
            return decls;
        }
    }
    Vec::new()
}

// ──────────────────────────────────────────────
// require / module resolution (M0: path-derived module name + suffix matching)
// ──────────────────────────────────────────────

/// Per-file module information (equivalent to a semantic ModuleIndex entry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModuleEntry {
    pub file_id: FileId,
    pub path: PathBuf,
    pub full_module_name: SmolStr,
    pub name: SmolStr,
    pub workspace_id: WorkspaceId,
    pub visible: ModuleVisibility,
    pub is_meta: bool,
    pub version_conds: Vec<LuaVersionCondition>,
}

/// A shard's module entries: `FileId -> ModuleEntry`.
///
/// Each file contributes one entry using its owning workspace's root, so a file
/// update replaces exactly one entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ModuleShard {
    pub(crate) files: HashMap<FileId, ModuleEntry>,
}

/// Module shard query. Each file contributes a `ModuleEntry` using its owning workspace's root,
/// so the per-workspace index can merge shards without scanning every file again.
pub(crate) fn module_shard(db: &SemanticDatabase, shard: u8) -> &ModuleShard {
    db.module_shard_of(shard)
}

/// Build one file's module entry using the current workspace roots.
pub(crate) fn build_module_entry(db: &SemanticDatabase, file_id: FileId) -> Option<ModuleEntry> {
    let roots = db.workspace_roots();
    let fallback_root = db.module_fallback_root();
    let file = db.file_data_id(file_id)?;
    let path = db.file_path(file)?;
    let file_ws = file_workspace_id(db, file_id);
    let root_path = match &file_ws {
        Some(ws) if !roots.is_empty() => roots
            .iter()
            .find(|root| root.id == *ws)
            .map(|root| root.root.clone()),
        _ if roots.is_empty() => fallback_root,
        _ => None,
    }?;
    let full_module_name = module_name_from_path(&path, Some(&root_path))?;
    let facts = file_facts(db, file);
    let name = SmolStr::new(
        full_module_name
            .rsplit('.')
            .next()
            .unwrap_or(&full_module_name),
    );
    Some(ModuleEntry {
        file_id,
        path,
        full_module_name,
        name,
        workspace_id: file_ws.unwrap_or(WorkspaceId::REMOTE),
        visible: facts.module_visibility,
        is_meta: facts.is_meta,
        version_conds: facts.version_conds.clone(),
    })
}

pub(crate) fn build_module_shard(db: &SemanticDatabase, shard: u8) -> ModuleShard {
    #[cfg(test)]
    db.rebuild_metrics
        .shard_scan_builds
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut files: HashMap<FileId, ModuleEntry> = HashMap::new();
    for &file_id in db.file_ids_in_shard(shard) {
        if let Some(entry) = build_module_entry(db, file_id) {
            files.insert(file_id, entry);
        }
    }
    ModuleShard { files }
}

/// Workspace module index: module name (relative to workspace root) -> file.
/// Module index scoped to a single workspace.
pub(crate) fn build_workspace_module_index(
    db: &SemanticDatabase,
    ws_id: WorkspaceId,
) -> ModuleIndex {
    let roots = db.workspace_roots();
    let ws_roots: Vec<PathBuf> = roots
        .iter()
        .filter(|root| root.id == ws_id)
        .map(|root| root.root.clone())
        .collect();

    let mut entries: Vec<ModuleEntry> = Vec::new();
    let mut by_path: HashMap<PathBuf, FileId> = HashMap::new();
    let mut module_name_to_file_ids: HashMap<SmolStr, Vec<FileId>> = HashMap::new();

    for shard in 0..EXPORT_SHARDS {
        let shard = module_shard(db, shard);
        for entry in shard.files.values() {
            if entry.workspace_id != ws_id {
                continue;
            }
            by_path.insert(normalize_path(&entry.path), entry.file_id);
            module_name_to_file_ids
                .entry(entry.name.clone())
                .or_default()
                .push(entry.file_id);
            entries.push(entry.clone());
        }
    }

    entries.sort_by(|a, b| {
        a.full_module_name
            .as_str()
            .cmp(b.full_module_name.as_str())
            .then_with(|| a.workspace_id.id.cmp(&b.workspace_id.id))
            .then_with(|| a.file_id.id.cmp(&b.file_id.id))
    });
    for file_ids in module_name_to_file_ids.values_mut() {
        file_ids.sort_unstable();
        file_ids.dedup();
    }
    let entry_by_file_id = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.file_id, index))
        .collect();

    let (nodes, root) = build_module_tree(&entries, ws_id);

    ModuleIndex {
        workspace_id: ws_id,
        entries,
        entry_by_file_id,
        by_path,
        module_name_to_file_ids,
        roots: ws_roots,
        nodes,
        root,
    }
}

pub(crate) fn workspace_module_index_for(
    db: &SemanticDatabase,
    ws_id: WorkspaceId,
) -> &ModuleIndex {
    db.workspace_index_cache()
        .modules
        .get(&ws_id)
        .expect("workspace module index must be built before read")
}

fn build_module_tree(
    entries: &[ModuleEntry],
    ws_id: WorkspaceId,
) -> (HashMap<ModuleNodeId, ModuleNode>, ModuleNodeId) {
    let root = ModuleNodeId {
        id: 0,
        workspace_id: ws_id,
    };
    let mut nodes = HashMap::new();
    nodes.insert(
        root,
        ModuleNode {
            children: Vec::new(),
            file_ids: Vec::new(),
            parent: None,
        },
    );
    let mut next_id = 1u32;

    for entry in entries {
        let parts: Vec<&str> = entry.full_module_name.split('.').collect();
        let mut current = root;
        for (index, part) in parts.iter().enumerate() {
            let is_last = index + 1 == parts.len();
            let child_id = {
                let node = nodes.get_mut(&current).expect("module node exists");
                if let Some(position) = node
                    .children
                    .iter()
                    .position(|(name, _)| name.as_str() == *part)
                {
                    node.children[position].1
                } else {
                    let id = ModuleNodeId {
                        id: next_id,
                        workspace_id: ws_id,
                    };
                    next_id += 1;
                    node.children.push((SmolStr::new(*part), id));
                    id
                }
            };
            nodes.entry(child_id).or_insert_with(|| ModuleNode {
                children: Vec::new(),
                file_ids: Vec::new(),
                parent: Some(current),
            });
            if is_last {
                nodes
                    .get_mut(&child_id)
                    .expect("module node just inserted")
                    .file_ids
                    .push(entry.file_id);
            }
            current = child_id;
        }
    }

    (nodes, root)
}

/// Select the most specific workspace root containing `path`.
///
/// Prefer the shortest relative path; on a tie keep the old LuaModuleIndex
/// semantics (main root wins over non-main).
fn best_workspace_root(
    roots: &[crate::semantic_db::inputs::WorkspaceRoot],
    path: &Path,
) -> Option<(usize, WorkspaceId)> {
    let mut best: Option<(usize, WorkspaceId)> = None;
    for root in roots {
        let Ok(rel) = path.strip_prefix(&root.root) else {
            continue;
        };
        if !root.import.includes_path(rel) {
            continue;
        }
        let rel_len = rel.components().count();
        let replace = match &best {
            None => true,
            Some((best_len, best_id)) => {
                rel_len < *best_len
                    || (rel_len == *best_len && root.id.is_main() && !best_id.is_main())
            }
        };
        if replace {
            best = Some((rel_len, root.id));
        }
    }
    best
}

/// Choose the workspace root that contains `path`.
///
/// Returns `(workspace id, root path)`. Callers that only need the id should use
/// [`find_workspace_id`] to avoid cloning the root path.
pub(crate) fn find_workspace_root(
    roots: &[crate::semantic_db::inputs::WorkspaceRoot],
    path: &Path,
) -> Option<(WorkspaceId, PathBuf)> {
    let (_, id) = best_workspace_root(roots, path)?;
    let root = roots
        .iter()
        .find(|root| root.id == id)
        .map(|root| root.root.clone())?;
    Some((id, root))
}

/// Root lookup without the `PathBuf` clone used by the legacy signature.
pub(crate) fn find_workspace_id(
    roots: &[crate::semantic_db::inputs::WorkspaceRoot],
    path: &Path,
) -> Option<WorkspaceId> {
    best_workspace_root(roots, path).map(|(_, id)| id)
}

/// File path -> owning workspace, without touching the VFS or file cache.
pub(crate) fn file_workspace_id_for_path(
    roots: &[crate::semantic_db::inputs::WorkspaceRoot],
    path: Option<&Path>,
) -> Option<WorkspaceId> {
    let path = path?;
    if roots.is_empty() {
        return Some(WorkspaceId::MAIN);
    }
    find_workspace_id(roots, path)
}

/// File -> its workspace.
pub(crate) fn file_workspace_id(db: &SemanticDatabase, file_id: FileId) -> Option<WorkspaceId> {
    if let Some(cache) = db.file_cache(file_id) {
        return cache.workspace_id;
    }
    let path = db.file_path(file_id)?;
    file_workspace_id_for_path(db.workspace_roots().as_ref(), Some(&path))
}

/// Module name -> file. Resolution order: module_map rewrite -> exact match -> require pattern (`?.lua`/`?/init.lua`).
pub(crate) fn module_file_of(
    db: &SemanticDatabase,
    config: &Emmyrc,
    module_name: SmolStr,
) -> Option<FileId> {
    let mut name = module_name.replace('\\', ".");
    // module_map rewrite rules (config order; every matching rule is applied, consistent with the old replace_module_path).
    for (pattern, replace) in config.module_replace() {
        if let Ok(regex) = regex::Regex::new(pattern.as_str())
            && regex.is_match(&name)
        {
            name = regex.replace(&name, replace.as_str()).into_owned();
        }
    }
    let patterns = config.module_patterns().to_vec();
    for &ws_id in workspace_lookup_order(db) {
        let index = workspace_module_index_for(db, ws_id);
        // Paths still containing `/` after module_map rewriting (`signalstrings/signalstrings.lua`)
        // first try exact literal relative-path matching, then `?` pattern resolution.
        if name.contains('/') {
            if let Some(file_id) = index.resolve_literal_path(&name) {
                return Some(file_id);
            }
        }
        let normalized = name.replace('/', ".");
        if let Some(file_id) = index.exact(&normalized) {
            return Some(file_id);
        }
        // `require("some.prefix.mod")` may target a module registered as
        // `mod` (or `prefix.mod`): try progressively shorter suffixes before
        // the broader fuzzy (short require -> longer module) fallback.
        if let Some(file_id) = index.fuzzy_prefix(&normalized) {
            return Some(file_id);
        }
        if let Some(file_id) = index.fuzzy(&normalized) {
            return Some(file_id);
        }
        if let Some(file_id) = index.resolve_by_pattern(&normalized, &patterns) {
            return Some(file_id);
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleIndex {
    /// Workspace this index belongs to (needed when rebuilding the module tree).
    workspace_id: WorkspaceId,
    /// Module entries sorted by `(full_module_name, workspace_id, file_id)`.
    entries: Vec<ModuleEntry>,
    /// File -> entry index (module queries by file are O(1)).
    entry_by_file_id: HashMap<FileId, usize>,
    /// Normalized path -> file (for pattern / literal path resolution).
    by_path: HashMap<PathBuf, FileId>,
    /// Module last segment -> files (for fuzzy search).
    module_name_to_file_ids: HashMap<SmolStr, Vec<FileId>>,
    /// All workspace root paths (for pattern resolution).
    roots: Vec<PathBuf>,
    /// Module tree nodes.
    nodes: HashMap<ModuleNodeId, ModuleNode>,
    /// Module tree root node.
    root: ModuleNodeId,
}

impl ModuleIndex {
    /// Replace one file's module entry and rebuild only this workspace's derived maps.
    ///
    /// Module path/visibility changes are rare, so this is intentionally
    /// workspace-local rather than global.
    pub(crate) fn apply_file_change(&mut self, file_id: FileId, new_entry: Option<ModuleEntry>) {
        if let Some(index) = self.entry_by_file_id.get(&file_id).copied()
            && index < self.entries.len()
        {
            self.entries.remove(index);
        }
        if let Some(entry) = new_entry {
            self.entries.push(entry);
        }
        self.rebuild_derived();
    }

    fn rebuild_derived(&mut self) {
        self.entries.sort_by(|a, b| {
            a.full_module_name
                .as_str()
                .cmp(b.full_module_name.as_str())
                .then_with(|| a.workspace_id.id.cmp(&b.workspace_id.id))
                .then_with(|| a.file_id.id.cmp(&b.file_id.id))
        });
        self.entry_by_file_id = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.file_id, index))
            .collect();
        self.by_path = self
            .entries
            .iter()
            .map(|entry| (normalize_path(&entry.path), entry.file_id))
            .collect();
        self.module_name_to_file_ids.clear();
        for entry in &self.entries {
            self.module_name_to_file_ids
                .entry(entry.name.clone())
                .or_default()
                .push(entry.file_id);
        }
        for file_ids in self.module_name_to_file_ids.values_mut() {
            file_ids.sort_unstable();
            file_ids.dedup();
        }
        let (nodes, root) = build_module_tree(&self.entries, self.workspace_id);
        self.nodes = nodes;
        self.root = root;
    }

    fn exact(&self, name: &str) -> Option<FileId> {
        // `entries` is sorted by `full_module_name`; avoid scanning unrelated modules.
        let start = self
            .entries
            .partition_point(|entry| entry.full_module_name.as_str() < name);
        let mut first = None;
        for entry in &self.entries[start..] {
            if entry.full_module_name.as_str() != name {
                break;
            }
            if first.is_none() {
                first = Some(entry.file_id);
            }
            if !entry.visible.is_hidden() {
                return Some(entry.file_id);
            }
        }
        first
    }

    /// Match a literal relative path containing `/` (`signalstrings/signalstrings.lua`).
    fn resolve_literal_path(&self, name: &str) -> Option<FileId> {
        let rel = name.replace('\\', "/");
        for root in &self.roots {
            let candidate = root.join(&rel);
            if let Some(file_id) = self.by_path.get(&normalize_path(&candidate)) {
                return Some(*file_id);
            }
        }
        None
    }

    /// Prefix-stripping exact matching: `some.prefix.mod` also tries `prefix.mod`, `mod`.
    ///
    /// Longest remaining suffix wins, so an exact longer module name is always
    /// preferred over a shorter prefix match.
    fn fuzzy_prefix(&self, name: &str) -> Option<FileId> {
        let mut parts: Vec<&str> = name.split('.').collect();
        while parts.len() > 1 {
            parts.remove(0);
            let candidate = parts.join(".");
            if let Some(file_id) = self.exact(&candidate) {
                return Some(file_id);
            }
        }
        None
    }

    /// Suffix fuzzy matching (old `fuzzy_find_module`): `event` matches `lua.cmp.utils.event`;
    /// prefer the fewest leading segments, then take one stably in module-name lexicographic order.
    fn fuzzy(&self, name: &str) -> Option<FileId> {
        let last_name = name.rsplit('.').next().unwrap_or(name);
        let file_ids = self.module_name_to_file_ids.get(last_name)?;
        let suffix = format!(".{name}");
        file_ids
            .iter()
            .filter_map(|&file_id| {
                let entry = self
                    .entry_by_file_id
                    .get(&file_id)
                    .and_then(|&index| self.entries.get(index))?;
                let full_module_name = entry.full_module_name.as_str();
                let leading_segment_count = if full_module_name == name {
                    Some(0)
                } else {
                    full_module_name
                        .strip_suffix(&suffix)
                        .map(|prefix| prefix.split('.').count())
                }?;
                Some((leading_segment_count, entry))
            })
            .min_by(|(left_count, left), (right_count, right)| {
                left_count
                    .cmp(right_count)
                    .then_with(|| left.full_module_name.cmp(&right.full_module_name))
            })
            .map(|(_, entry)| entry.file_id)
    }

    /// Exact-match candidate paths under each root using require patterns (`?` -> `a/b`).
    fn resolve_by_pattern(&self, name: &str, patterns: &[SmolStr]) -> Option<FileId> {
        let rel = name.replace('.', "/");
        for root in &self.roots {
            for pattern in patterns {
                let candidate = pattern.replace('?', &rel);
                let candidate = root.join(candidate);
                if let Some(file_id) = self.by_path.get(&normalize_path(&candidate)) {
                    return Some(*file_id);
                }
            }
        }
        None
    }

    pub(crate) fn find_module_node(&self, module_path: &str) -> Option<ModuleNodeId> {
        if module_path.is_empty() {
            return Some(self.root);
        }
        let mut current = self.root;
        for part in module_path.replace(['\\', '/'], ".").split('.') {
            let node = self.nodes.get(&current)?;
            let child_id = node
                .children
                .iter()
                .find(|(name, _)| name.as_str() == part)
                .map(|(_, id)| *id)?;
            current = child_id;
        }
        Some(current)
    }

    pub(crate) fn module_node(&self, id: ModuleNodeId) -> Option<&ModuleNode> {
        self.nodes.get(&id)
    }

    pub(crate) fn module_file_ids(&self, id: ModuleNodeId) -> Option<&[FileId]> {
        self.nodes.get(&id).map(|node| node.file_ids.as_slice())
    }

    pub(crate) fn module_info(&self, file_id: FileId) -> Option<ModuleInfo> {
        let entry = self
            .entry_by_file_id
            .get(&file_id)
            .and_then(|&index| self.entries.get(index))?;
        Some(ModuleInfo {
            file_id: entry.file_id,
            full_module_name: entry.full_module_name.clone(),
            name: entry.name.clone(),
            visible: entry.visible,
            workspace_id: entry.workspace_id,
            is_meta: entry.is_meta,
            version_conds: entry.version_conds.clone(),
            export_type: None,
        })
    }
}

/// Plain merged workspace indexes.
///
/// This is a write-time built cache. It is never mutated by read-side queries;
/// `rebuild_all_caches` replaces it wholesale whenever inputs change.
#[derive(Debug, Default)]
pub(crate) struct WorkspaceIndexCache {
    pub(crate) types: HashMap<WorkspaceId, WorkspaceTypeIndex>,
    /// Cross-workspace global type aggregate (`full_name` -> definitions in
    /// workspace lookup order), so repeated global type queries clone an `Arc`.
    pub(crate) global_types: HashMap<SmolStr, Arc<[TypeDef]>>,
    pub(crate) members: HashMap<WorkspaceId, WorkspaceMemberIndex>,
    /// Cross-workspace raw owner buckets (`owner -> members` in workspace order).
    pub(crate) owner_members: HashMap<SemanticId, Arc<[MemberRef]>>,
    /// Cross-workspace named owner buckets (`(owner, name) -> members`).
    pub(crate) owner_members_named: HashMap<(SemanticId, SmolStr), Arc<[MemberRef]>>,
    pub(crate) decls: HashMap<WorkspaceId, WorkspaceDeclIndex>,
    pub(crate) modules: HashMap<WorkspaceId, ModuleIndex>,
    pub(crate) deprecated: HashMap<WorkspaceId, DeprecatedIndex>,
    pub(crate) references: HashMap<WorkspaceId, WorkspaceReferenceIndex>,
}

impl WorkspaceIndexCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

/// File path -> module name: relative to workspace root, strip `.lua`, `init.lua` -> parent dir, `/` -> `.`.
pub(crate) fn module_name_from_path(path: &Path, root: Option<&Path>) -> Option<SmolStr> {
    let rel = match root {
        Some(root) => path.strip_prefix(root).unwrap_or(path),
        None => path,
    };
    let text = rel.to_string_lossy().replace('\\', "/");
    let text = text.strip_suffix(".lua").unwrap_or(&text);
    let text = text.strip_suffix("/init").unwrap_or(&text);
    if text.is_empty() {
        return None;
    }
    Some(SmolStr::new(text.replace('/', ".")))
}

/// Common prefix of all files' parent directories as the fallback workspace root.
pub(crate) fn common_path_root(paths: &[PathBuf]) -> Option<PathBuf> {
    let parents: Vec<PathBuf> = paths
        .iter()
        .filter_map(|p| p.parent().map(Path::to_path_buf))
        .collect();
    let first = parents.first()?;
    let mut common = first.clone();
    for p in &parents[1..] {
        let a: Vec<_> = common.components().collect();
        let b: Vec<_> = p.components().collect();
        let n = a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
        let mut next = PathBuf::new();
        for comp in &a[..n] {
            next.push(comp.as_os_str());
        }
        common = next;
        if common.as_os_str().is_empty() {
            break;
        }
    }
    (!common.as_os_str().is_empty()).then_some(common)
}

/// Normalize a path (strip trailing separators, unify slashes) for exact by_path matching.
fn normalize_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        // Windows filesystems are case-insensitive: normalize module-name index to lowercase so require("Module") can match module.lua.
        let lowered = path.to_string_lossy().to_lowercase();
        Path::new(&lowered).components().collect()
    }
    #[cfg(not(windows))]
    {
        path.components().collect()
    }
}

/// Phase 2 association: resolve `Name("a.b")` to a real definition (type/variable/member chain).
/// `Decl`/`TypeDef`/`Member` are already concrete and returned as-is.
pub(crate) fn resolve_owner(db: &SemanticDatabase, owner: SemanticId) -> Option<SemanticId> {
    let SemanticId::Name(name) = &owner else {
        return Some(owner.clone());
    };
    if let Some(dot) = name.rfind('.') {
        // "a.b" -> resolve "a" first, then take its member "b".
        let head = SmolStr::new(&name[..dot]);
        let tail = &name[dot + 1..];
        let resolved = resolve_owner(db, SemanticId::name(head.clone()))?;
        // Members are looked up by union: declared under the head's name key (same-file `M.N = {}`) or under a concrete id key (`@field` etc.).
        let mut members = members_of_owner(db, SemanticId::name(head.clone())).to_vec();
        members.extend(members_of_owner(db, resolved).iter().cloned());
        members
            .into_iter()
            .find(|member| member.name.as_str() == tail)
            .map(|member| member.id)
    } else {
        global_type_by_name(db, SmolStr::new(name.as_str()))
            .or_else(|| global_decl_by_name(db, SmolStr::new(name.as_str())))
    }
}

/// Canonical `OwnerId` for a raw semantic identity.
///
/// Module export targets and require aliases are normalized to `OwnerId::Module`;
/// all other identities map through the file contribution mapping.
pub(crate) fn canonical_owner_id(db: &SemanticDatabase, owner: &SemanticId) -> Option<OwnerId> {
    match owner {
        SemanticId::Name(name) => Some(OwnerId::Global(SmolStr::new(name.as_str()))),
        SemanticId::TypeDef(key) => Some(OwnerId::Type(key.scope, key.full_name.clone())),
        SemanticId::Signature(_) => Some(OwnerId::Concrete(owner.clone())),
        SemanticId::Decl(key) => {
            if let Some(module_owner) = logical_module_owner_id(db, owner) {
                return Some(module_owner);
            }
            let facts = db.file_facts_of(key.file_id)?;
            Some(owner_id_from_semantic_id(facts, owner))
        }
        SemanticId::Member(key) => {
            if let Some(module_owner) = logical_module_owner_id(db, owner) {
                return Some(module_owner);
            }
            let facts = db.file_facts_of(key.file_id)?;
            Some(owner_id_from_semantic_id(facts, owner))
        }
    }
}

/// Convert a canonical owner back to a raw definition identity.
///
/// `OwnerId::Module` returns the module export target; `OwnerId::Global` prefers
/// a real declaration over the unresolved `Name`.
pub(crate) fn owner_id_to_semantic_id(
    db: &SemanticDatabase,
    owner_id: &OwnerId,
) -> Option<SemanticId> {
    match owner_id {
        OwnerId::Type(scope, name) => type_defs_in_scope(db, *scope, name.clone())
            .first()
            .map(|def| def.id.clone()),
        OwnerId::Global(name) => {
            global_decl_by_name(db, name.clone()).or_else(|| Some(SemanticId::name(name.clone())))
        }
        OwnerId::Local(file_id, name_range) => Some(SemanticId::decl(*file_id, *name_range)),
        OwnerId::Table(file_id, key_range) => Some(SemanticId::member(*file_id, *key_range)),
        OwnerId::Module(file_id) => module_export_owner_raw(db, *file_id),
        OwnerId::Concrete(id) => Some(id.clone()),
    }
}

/// Raw owner of a file's module export (`return M` / `return { ... }`).
pub(crate) fn module_export_owner_raw(
    db: &SemanticDatabase,
    file_id: FileId,
) -> Option<SemanticId> {
    let facts = db.file_facts_of(file_id)?;
    match &facts.module_export {
        ModuleExport::Decl { decl, .. } => Some(decl.clone()),
        ModuleExport::Expr { value_syntax } => {
            Some(SemanticId::member(file_id, value_syntax.get_range()))
        }
        ModuleExport::Global { .. } | ModuleExport::None => None,
    }
}

/// Direct canonical-owner member lookup across workspaces (deterministic priority).
pub(crate) fn members_of_owner_id_named_all(
    db: &SemanticDatabase,
    owner_id: &OwnerId,
    name: &str,
) -> Vec<MemberRef> {
    let mut out: Vec<MemberRef> = Vec::new();
    for &ws_id in workspace_lookup_order(db) {
        let index = workspace_member_index_for(db, ws_id);
        let Some(members) = index.members_of_owner_id_named(owner_id, name) else {
            continue;
        };
        for member in members.iter() {
            if !out
                .iter()
                .any(|existing| existing.id == member.id && existing.file_id == member.file_id)
            {
                out.push(member.clone());
            }
        }
    }
    out
}

fn push_unique_owner_id(out: &mut Vec<OwnerId>, owner_id: OwnerId) {
    if !out.contains(&owner_id) {
        out.push(owner_id);
    }
}

/// Add the runtime value declaration(s) associated with a named type.
///
/// `---@class M` + `local M = {}` (or a global meta table) is a dual identity:
/// members declared through the runtime table must be visible when resolving
/// the type/global name.
fn push_type_associated_owner_ids(
    db: &SemanticDatabase,
    type_id: &SemanticId,
    out: &mut Vec<OwnerId>,
) {
    for &ws_id in workspace_lookup_order(db) {
        let index = workspace_decl_index_for(db, ws_id);
        let Some((file_id, bare_name)) = index.type_def_location(type_id) else {
            continue;
        };
        if let Some(decl_id) = index.runtime_value_in(file_id, &bare_name) {
            if let Some(decl_owner) = canonical_owner_id(db, &decl_id) {
                push_unique_owner_id(out, decl_owner);
            }
            // Main-workspace `---@meta` API files use a global runtime table
            // (`_KLuaPlayer = {}`) as the type surface, while its methods are
            // declared under `Name("_KLuaPlayer")`. Include that owner so
            // `@class _KLuaPlayer` + `function _KLuaPlayer.Msg` resolves.
            if let SemanticId::Decl(key) = &decl_id
                && let Some(decl_facts) = db.file_facts_of(key.file_id)
                && let Some(decl) = decl_facts.decl_by_id(&decl_id)
                && matches!(decl.kind, DeclKind::Global)
                && decl_facts.is_meta
                && file_workspace_id(db, key.file_id).is_some_and(|ws| ws.is_main())
            {
                push_unique_owner_id(out, OwnerId::Concrete(SemanticId::name(decl.name.clone())));
            }
        }
        if let Some(facts) = db.file_facts_of(file_id)
            && let Some(def) = facts.type_def_by_id(type_id)
            && let Some(owner_syntax) = def.owner_syntax
        {
            for decl in facts.decls_by_owner_syntax(owner_syntax) {
                if let Some(decl_owner) = canonical_owner_id(db, &decl.id) {
                    push_unique_owner_id(out, decl_owner);
                }
            }
        }
        break;
    }
}

/// Deterministic identity-level owner resolution.
///
/// This is the P5 canonical replacement for the old heuristic
/// `resolve_owner_set()` + score path. Order is stable and explicit:
/// 1. canonical identity of the input;
/// 2. runtime value / `owner_syntax` associations for named types;
/// 3. global named type for a global name;
/// 4. dotted-name member chain (`M.sub`).
pub(crate) fn resolve_owner_ids(db: &SemanticDatabase, owner: &SemanticId) -> Vec<OwnerId> {
    let mut out: Vec<OwnerId> = Vec::new();
    if let Some(owner_id) = canonical_owner_id(db, owner) {
        push_unique_owner_id(&mut out, owner_id);
    }

    match owner {
        SemanticId::Name(name) => {
            let name_str = SmolStr::new(name.as_str());
            // Named type + its runtime value (`---@class M` + `local M = {}`).
            for def in type_defs_in_scope(db, TypeScope::Global, name_str.clone()).iter() {
                if let ExportKey::Type(scope, full_name) = def.export_key() {
                    push_unique_owner_id(&mut out, OwnerId::Type(scope, full_name));
                }
                push_type_associated_owner_ids(db, &def.id, &mut out);
            }
            // Global runtime declaration (`_G[name]`, std globals, ...).
            if let Some(decl) = global_decl_by_name(db, name_str.clone())
                && let Some(owner_id) = canonical_owner_id(db, &decl)
            {
                push_unique_owner_id(&mut out, owner_id);
            }
            // Runtime declarations named like the type, plus their `owner_syntax`
            // associated classes (differently-named runtime tables).
            for &ws_id in workspace_lookup_order(db) {
                for decl_id in workspace_decl_index_for(db, ws_id).runtime_decls_named(&name_str) {
                    if let Some(owner_id) = canonical_owner_id(db, decl_id) {
                        push_unique_owner_id(&mut out, owner_id);
                    }
                    if let SemanticId::Decl(key) = decl_id
                        && let Some(facts) = db.file_facts_of(key.file_id)
                        && let Some(decl) = facts.decl_by_id(decl_id)
                        && let Some(owner_syntax) = decl.owner_syntax
                    {
                        for def in facts.type_defs_by_owner_syntax(owner_syntax).filter(|def| {
                            matches!(def.kind, TypeDefKind::Class | TypeDefKind::Enum)
                        }) {
                            if let ExportKey::Type(scope, full_name) = def.export_key() {
                                push_unique_owner_id(&mut out, OwnerId::Type(scope, full_name));
                            }
                        }
                    }
                }
            }
            if let Some(dot) = name.rfind('.') {
                let head = SmolStr::new(&name[..dot]);
                let tail = SmolStr::new(&name[dot + 1..]);
                for head_owner in resolve_owner_ids(db, &SemanticId::name(head)) {
                    for member in members_of_owner_id_named_all(db, &head_owner, &tail) {
                        if let Some(member_owner) = canonical_owner_id(db, &member.id) {
                            push_unique_owner_id(&mut out, member_owner);
                        }
                    }
                }
            }
        }
        SemanticId::TypeDef(key) => {
            push_unique_owner_id(&mut out, OwnerId::Type(key.scope, key.full_name.clone()));
            push_type_associated_owner_ids(db, owner, &mut out);
        }
        SemanticId::Decl(key) => {
            if let Some(facts) = db.file_facts_of(key.file_id)
                && let Some(decl) = facts.decl_by_id(owner)
                && let Some(owner_syntax) = decl.owner_syntax
            {
                for def in facts
                    .type_defs_by_owner_syntax(owner_syntax)
                    .filter(|def| matches!(def.kind, TypeDefKind::Class | TypeDefKind::Enum))
                {
                    if let ExportKey::Type(scope, full_name) = def.export_key() {
                        push_unique_owner_id(&mut out, OwnerId::Type(scope, full_name));
                    }
                }
            }
        }
        _ => {}
    }

    out
}

/// Phase 2 association: resolve an owner to an **identity set** (dual identity: same-name type + runtime value).
/// `Name("M")` -> `{TypeDef(M), Decl(M)}`; member lookup uses the union across sets.
/// For name chains (`a.b`), recursively take members along each head identity.
pub(crate) fn resolve_owner_set(db: &SemanticDatabase, owner: SemanticId) -> Vec<SemanticId> {
    // Deterministic P5/P7 wrapper built on `resolve_owner_ids()`.
    // The raw owner stays first for member keys stored directly under
    // `Name` / `Decl` / `Member` identities.
    let mut out: Vec<SemanticId> = Vec::new();
    push_unique(&mut out, owner.clone());
    for owner_id in resolve_owner_ids(db, &owner) {
        if let Some(raw) = owner_id_to_semantic_id(db, &owner_id) {
            push_unique(&mut out, raw);
        }
    }
    out
}

fn push_unique(out: &mut Vec<SemanticId>, id: SemanticId) {
    if !out.contains(&id) {
        out.push(id);
    }
}

// ──────────────────────────────────────────────
// L3 semantics: declaration types (with cycle convergence)
// ──────────────────────────────────────────────

/// Type of a declaration. Recursive dependencies (mutual references) converge via semantic's native fixed point.
/// Priority: `---@type` annotation -> initializer expression.
/// Keyed by file so an initializer can still resolve cross-file members.
pub(crate) fn decl_type(
    db: &SemanticDatabase,
    file: FileId,
    config: &Emmyrc,
    decl: SemanticId,
) -> TypeShell {
    let facts = file_facts(db, file);
    let Some(decl) = facts.decl_by_id(&decl) else {
        return TypeShell::unknown();
    };

    if let Some(type_syntax) = decl.doc_type_syntax {
        let shell = lower_doc_type(db, file, type_syntax, &[]);
        if !shell.is_unknown() {
            return shell;
        }
    }

    // `---@module "name"`: project directly to a module reference.
    if let Some(module_path) = &decl.module_path
        && let Some(module_file) = module_file_of(db, config, module_path.clone())
    {
        return TypeShell::from_module_ref(module_file);
    }

    // Parameter declaration: `---@param` annotation (belongs to the closure signature, matched by name).
    if matches!(decl.kind, DeclKind::Param)
        && let Some((sig, _)) = facts.signature_and_param_index_of_decl(decl)
        && let Some(docs) = &sig.docs
        && let Some((_, type_syntax)) = docs.param_types.iter().find(|(name, _)| name == &decl.name)
    {
        let generics = docs.generic_params.as_slice();
        let mut shell = lower_doc_type(db, file, *type_syntax, generics);
        if !shell.is_unknown() {
            // `---@param name? T`: the parameter type must include nil.
            if docs.nullable_params.iter().any(|n| n == &decl.name) {
                shell.merge(&TypeShell::from_primitive(PrimitiveType::Nil));
            }
            return shell;
        }
    }

    // `for k, v in pairs(x)`: take types from the iterator function's return slots (owner is ForRangeStat).
    if matches!(decl.kind, DeclKind::Local { is_iter: true, .. })
        && let Some(shell) = iter_slot_type(db, &facts, file, config, &decl)
        && !shell.is_unknown()
    {
        return shell;
    }

    if let Some(value_expr_syntax) = decl.value_expr_syntax {
        let shell = expr_type_of(db, file, config, value_expr_syntax);
        if !shell.is_unknown() {
            // Globals cannot carry generic parameters not instantiated inside the function body (the `a` in `function f(x) a = x end` is unknown outside).
            if matches!(decl.kind, DeclKind::Global) {
                let generic_names: HashSet<&str> = facts
                    .signatures
                    .iter()
                    .filter_map(|sig| sig.docs.as_ref())
                    .flat_map(|docs| docs.generic_params.iter().map(|g| g.name.as_str()))
                    .collect();
                if shell.candidates.iter().any(|candidate| {
                    matches!(
                        candidate,
                        TypeCandidate::Generic(name) if generic_names.contains(name.as_str())
                    )
                }) {
                    return TypeShell::unknown();
                }
            }
            return shell;
        }
    }

    TypeShell::unknown()
}

/// Iterator slot types for `for k, v in pairs(x)`.
///
/// Only recognizes `pairs/ipairs/next(x)` where x is a named type / generic instance: reads the returned list from
/// the `__pairs`/`__ipairs` function signature's `---@return fun(): K, V` and projects it by slot (preserving order,
/// not lost through TypeShell's candidate set).
fn iter_slot_type(
    db: &SemanticDatabase,
    facts: &FileFacts,
    file: FileId,
    config: &Emmyrc,
    decl: &crate::semantic_db::def::Decl,
) -> Option<TypeShell> {
    let key = (file, decl.id.clone());
    if ITER_SLOT_IN_PROGRESS.with(|stack| stack.borrow().contains(&key)) {
        return None;
    }
    let _guard = IterSlotGuard::enter(key);
    let owner = decl.owner_syntax?;
    let tree = syntax_tree(db, file);
    let node = owner.to_node_from_root(&tree.get_red_root())?;
    let stat = emmylua_parser::LuaForRangeStat::cast(node)?;
    let vars = stat.get_var_name_list().collect::<Vec<_>>();
    let index = vars
        .iter()
        .position(|var| var.get_name_text() == decl.name.as_str())?;
    let iter_expr = stat.get_expr_list().next()?;
    let LuaExpr::CallExpr(call) = iter_expr else {
        return None;
    };
    let member_name = match call.get_prefix_expr()? {
        LuaExpr::NameExpr(name) => match name.get_name_text()?.as_str() {
            "pairs" | "next" => "__pairs",
            "ipairs" => "__ipairs",
            _ => return None,
        },
        _ => return None,
    };
    let arg = call.get_args_list()?.get_args().next()?;
    let arg_shell = expr_type(db, facts, file, config, arg);

    for candidate in &arg_shell.candidates {
        let (def, generic_args) = match candidate {
            TypeCandidate::Named(name) => {
                let def = resolve_type_def(db, file, SmolStr::new(name.as_str()))?;
                (def, Vec::new())
            }
            TypeCandidate::GenericInstance(ins) => {
                let def = resolve_type_def(db, file, SmolStr::new(ins.name.as_str()))?;
                (def, ins.args.clone())
            }
            _ => continue,
        };
        let mut member_refs = members_of_owner(db, def.id.clone()).to_vec();
        for resolved in resolve_owner_set(db, def.id.clone()) {
            member_refs.extend(members_of_owner(db, resolved).iter().cloned());
        }
        for member_ref in member_refs
            .iter()
            .filter(|member| member.name == member_name)
        {
            let Some(member_file) = db.file_data_id(member_ref.file_id) else {
                continue;
            };
            let member_facts = file_facts(db, member_file);
            let Some(member) = member_facts.member_by_id(&member_ref.id) else {
                continue;
            };
            let Some(closure_syntax) = member.value_syntax else {
                continue;
            };
            let Some(signature) = member_facts.signature_by_closure(closure_syntax) else {
                continue;
            };
            let Some(docs) = &signature.docs else {
                continue;
            };
            // Generator signature: `---@return fun(): integer, T` (returns stores the fun type node).
            let Some(return_syntax) = docs.returns.first() else {
                continue;
            };
            let member_tree = syntax_tree(db, member_file);
            let Some(return_node) = return_syntax.to_node_from_root(&member_tree.get_red_root())
            else {
                continue;
            };
            let Some(LuaDocType::Func(func)) = LuaDocType::cast(return_node) else {
                continue;
            };
            let Some(return_list) = func.get_return_type_list() else {
                continue;
            };
            let mut slots: Vec<TypeShell> = Vec::new();
            for ret in return_list.get_return_type_list() {
                if let (_, Some(ret_type)) = ret.get_name_and_type() {
                    slots.push(lower_doc_type_node(
                        db,
                        member_file,
                        &def.generic_params,
                        &ret_type,
                    ));
                }
            }
            if let Some(slot) = slots.get(index) {
                let substituted = substitute_generics(slot, &def.generic_params, &generic_args);
                if !substituted.is_unknown() {
                    return Some(substituted);
                }
            }
        }
    }
    None
}

/// Lower a doc type node to `TypeShell`.
/// `generics` = generic params in the current scope (`T` -> `Generic(T)`); named types resolve to TypeDef (cross-file).
pub(crate) fn lower_doc_type(
    db: &SemanticDatabase,
    file: FileId,
    type_syntax: LuaSyntaxId,
    generics: &[DocGenericParam],
) -> TypeShell {
    let tree = syntax_tree(db, file);
    let root = tree.get_red_root();
    let Some(node) = type_syntax.to_node_from_root(&root) else {
        return TypeShell::unknown();
    };
    let Some(doc_type) = LuaDocType::cast(node) else {
        return TypeShell::unknown();
    };
    lower_doc_type_node(db, file, generics, &doc_type)
}

fn lower_doc_type_node(
    db: &SemanticDatabase,
    file: FileId,
    generics: &[DocGenericParam],
    doc_type: &LuaDocType,
) -> TypeShell {
    match doc_type {
        LuaDocType::Name(name_type) => match name_type.get_name_text() {
            Some(name) => {
                // Generic parameters take precedence (shadow same-name types).
                if generics.iter().any(|g| g.name == name) {
                    TypeShell::from_generic(&name)
                } else if let Some(primitive) = primitive_from_name(&name) {
                    primitive
                } else if let Some(def) = resolve_type_def(db, file, SmolStr::new(&name)) {
                    TypeShell::from_name(def.full_name.as_str())
                } else {
                    TypeShell::from_name(&name)
                }
            }
            None => TypeShell::unknown(),
        },
        LuaDocType::Literal(literal) => match literal.get_literal() {
            Some(LuaLiteralToken::String(str)) => {
                TypeShell::from_literal(LiteralShell::String(SmolStr::new(str.get_value())))
            }
            Some(LuaLiteralToken::Number(number)) => match number.get_number_value() {
                emmylua_parser::NumberResult::Int(i) => {
                    TypeShell::from_literal(LiteralShell::Integer(i))
                }
                emmylua_parser::NumberResult::Uint(u) => {
                    TypeShell::from_literal(LiteralShell::Integer(u as i64))
                }
                // Float constants preserve bit patterns (f64 has no `Ord`; `LiteralShell::Float(u64)`).
                emmylua_parser::NumberResult::Float(f) => {
                    TypeShell::from_literal(LiteralShell::Float(f.to_bits()))
                }
                emmylua_parser::NumberResult::Number => {
                    TypeShell::from_primitive(PrimitiveType::Number)
                }
            },
            Some(LuaLiteralToken::Bool(bool_token)) => {
                TypeShell::from_literal(LiteralShell::Boolean(bool_token.is_true()))
            }
            Some(LuaLiteralToken::Nil(_)) => TypeShell::from_literal(LiteralShell::Nil),
            _ => TypeShell::unknown(),
        },
        LuaDocType::Array(array) => array
            .get_type()
            .map(|base| TypeShell::from_array(lower_doc_type_node(db, file, generics, &base)))
            .unwrap_or_else(|| TypeShell::from_primitive(PrimitiveType::Table)),
        LuaDocType::Variadic(variadic) => variadic
            .get_type()
            .map(|inner| TypeShell::from_variadic(lower_doc_type_node(db, file, generics, &inner)))
            .unwrap_or_else(TypeShell::unknown),
        LuaDocType::Tuple(tuple) => {
            let types = tuple
                .get_types()
                .map(|item| lower_doc_type_node(db, file, generics, &item))
                .collect();
            TypeShell::from_tuple(types)
        }
        LuaDocType::Object(object) => {
            // An empty object literal `{}` is a structural type and must not be lowered to broad `table`;
            // otherwise flow analysis would narrow `myenum|{}`'s table branch to `Table` and lose `{}`.
            if object.get_fields().next().is_none() {
                TypeShell::from_primitive(PrimitiveType::EmptyObject)
            } else {
                TypeShell::from_primitive(PrimitiveType::Table)
            }
        }
        LuaDocType::Generic(generic_type) => {
            // `Box<number>`: base type name + arguments (generic instantiation).
            if let Some(name) = generic_type.get_name_type().and_then(|n| n.get_name_text()) {
                let mut args = Vec::new();
                if let Some(list) = generic_type.get_generic_types() {
                    for arg in list.get_types() {
                        args.push(lower_doc_type_node(db, file, generics, &arg));
                    }
                }
                TypeShell::from_generic_instance(&name, args)
            } else {
                TypeShell::unknown()
            }
        }
        LuaDocType::StrTpl(str_tpl) => {
            // `` `T` ``: string argument replaces the placeholder name (`xxx.`T`` -> prefix "xxx.", `` `T`.xxx `` -> suffix ".xxx").
            let (prefix, name, suffix) = str_tpl.get_name();
            let tpl_index = name
                .as_deref()
                .and_then(|n| generics.iter().position(|g| g.name == n))
                .map(|idx| idx as u32);
            TypeShell::from_str_tpl(
                &prefix.unwrap_or_default(),
                &name.unwrap_or_default(),
                tpl_index,
                &suffix.unwrap_or_default(),
            )
        }
        LuaDocType::Func(func_type) => {
            // `fun<T, U>(...)`: merge generic declarations into scope; `T` in params/returns -> `Generic("T")`.
            let mut local_generics: Vec<DocGenericParam> = generics.to_vec();
            let mut fun_generics: Vec<SmolStr> = Vec::new();
            if let Some(decl_list) = func_type.get_generic_decl_list() {
                for decl in decl_list.get_generic_decl() {
                    if let Some(token) = decl.get_name_token() {
                        let name = token.get_name_text().to_string();
                        local_generics.push(DocGenericParam::new(
                            SmolStr::new(&name),
                            None,
                            None,
                            false,
                            false,
                        ));
                        fun_generics.push(SmolStr::new(&name));
                    }
                }
            }
            let mut params = Vec::new();
            let mut param_names = Vec::new();
            let mut is_variadic = false;
            for param in func_type.get_params() {
                if param.is_dots() {
                    is_variadic = true;
                }
                param_names.push(
                    param
                        .get_name_token()
                        .map(|token| SmolStr::new(token.get_name_text()))
                        .or_else(|| param.is_dots().then(|| SmolStr::new("...")))
                        .unwrap_or_default(),
                );
                if let Some(param_type) = param.get_type() {
                    let mut shell = lower_doc_type_node(db, file, &local_generics, &param_type);
                    // `fun(i?: integer)` params are nullable: the nullable marker is not on the type node.
                    if param.is_nullable() {
                        shell.merge(&TypeShell::from_primitive(PrimitiveType::Nil));
                    }
                    params.push(shell);
                } else {
                    params.push(TypeShell::unknown());
                }
            }
            let mut returns_multi = Vec::new();
            let mut returns = TypeShell::unknown();
            if let Some(list) = func_type.get_return_type_list() {
                for ret in list.get_return_type_list() {
                    if let (_, Some(ret_type)) = ret.get_name_and_type() {
                        let shell = lower_doc_type_node(db, file, &local_generics, &ret_type);
                        returns.merge(&shell);
                        returns_multi.push(shell);
                    }
                }
            }
            let async_state = if func_type.is_async() {
                1
            } else if func_type.is_sync() {
                2
            } else {
                0
            };
            let is_variadic = is_variadic;
            TypeShell::from_function(
                params,
                param_names,
                returns,
                returns_multi,
                fun_generics,
                async_state,
                false,
                is_variadic,
            )
        }
        LuaDocType::Binary(binary) => {
            let op = binary.get_op_token().map(|token| token.get_op());
            if op == Some(LuaTypeBinaryOperator::Union) {
                if let Some((left, right)) = binary.get_types() {
                    let mut shell = lower_doc_type_node(db, file, generics, &left);
                    shell.merge(&lower_doc_type_node(db, file, generics, &right));
                    return shell;
                }
            }
            TypeShell::unknown()
        }
        LuaDocType::Nullable(nullable) => nullable
            .get_type()
            .map(|inner| {
                // `T?` = T | nil.
                let mut shell = lower_doc_type_node(db, file, generics, &inner);
                shell.merge(&TypeShell::from_primitive(PrimitiveType::Nil));
                shell
            })
            .unwrap_or_else(TypeShell::unknown),
        LuaDocType::MultiLineUnion(multi) => {
            let mut shell = TypeShell::unknown();
            for field in multi.get_fields() {
                if let Some(item) = field.get_type() {
                    shell.merge(&lower_doc_type_node(db, file, generics, &item));
                }
            }
            shell
        }
        _ => TypeShell::unknown(),
    }
}

/// Base type name -> `PrimitiveType`.
pub(crate) fn primitive_from_name(name: &str) -> Option<TypeShell> {
    let primitive = match name {
        "string" => PrimitiveType::String,
        "number" => PrimitiveType::Number,
        "integer" | "int" => PrimitiveType::Integer,
        "boolean" | "bool" => PrimitiveType::Boolean,
        "nil" | "void" => PrimitiveType::Nil,
        "table" => PrimitiveType::Table,
        "function" => PrimitiveType::Function,
        _ => return None,
    };
    Some(TypeShell::from_primitive(primitive))
}

// ──────────────────────────────────────────────
// L3 semantics: member types (with cycle convergence)
// ──────────────────────────────────────────────

/// Declared type of a member. Members can be mutually recursive (`T.foo = T.bar`), also converged by semantic's native fixed point.
pub(crate) fn member_type(
    db: &SemanticDatabase,
    file: FileId,
    config: &Emmyrc,
    member: SemanticId,
) -> TypeShell {
    let facts = file_facts(db, file);
    let Some(member) = facts.member_by_id(&member) else {
        return TypeShell::unknown();
    };

    // `---@module "name"`: project directly to a module reference.
    if let Some(module_path) = &member.module_path
        && let Some(module_file) = module_file_of(db, config, module_path.clone())
    {
        return TypeShell::from_module_ref(module_file);
    }

    // Value: owner is `TypeDef` (@field) -> doc type node; otherwise -> expression.
    if let Some(value_syntax) = member.value_syntax {
        match &member.owner {
            SemanticId::TypeDef(_) => {
                let generics = facts
                    .type_def_by_id(&member.owner)
                    .map(|def| def.generic_params.as_slice())
                    .unwrap_or(&[]);
                let shell = lower_doc_type(db, file, value_syntax, generics);
                if !shell.is_unknown() {
                    return shell;
                }
            }
            _ => {
                let shell = expr_type_of(db, file, config, value_syntax);
                if !shell.is_unknown() {
                    return shell;
                }
            }
        }
    }

    if member.is_method {
        return TypeShell::from_primitive(PrimitiveType::Function);
    }

    TypeShell::unknown()
}

/// Direct member names of a local declaration (completion candidates).
pub(crate) fn member_keys_of_decl(
    db: &SemanticDatabase,
    file: FileId,
    decl: SemanticId,
) -> Vec<SmolStr> {
    let facts = file_facts(db, file);
    let mut keys = facts
        .members_of_owner(&decl)
        .map(|member| member.key.to_path().into())
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    keys
}

/// Member keys of a named type (including parent types, completion candidates). `type_def` is a global type id.
pub(crate) fn member_keys_of_type(
    db: &SemanticDatabase,
    file: FileId,
    type_def: SemanticId,
) -> Vec<SmolStr> {
    let facts = file_facts(db, file);
    let Some(def) = facts.type_def_by_id(&type_def) else {
        return Vec::new();
    };
    let mut keys = Vec::new();
    let mut visited = Vec::new();
    collect_type_keys(&facts, def.id.clone(), &mut visited, &mut keys);
    keys.sort();
    keys.dedup();
    keys
}

/// Type of a named type's member (including parent types). Pure function; caller must be in a tracked context.
pub(crate) fn type_member(
    db: &SemanticDatabase,
    facts: &FileFacts,
    file: FileId,
    type_def: SemanticId,
    name: &str,
    visited: &mut Vec<SemanticId>,
) -> Option<TypeShell> {
    if visited.contains(&type_def) {
        return None;
    }
    visited.push(type_def.clone());

    if let Some(member) = facts.field_members_of_type(&type_def, name) {
        if let Some(value_syntax) = member.value_syntax {
            let generics = facts
                .type_def_by_id(&type_def)
                .map(|def| def.generic_params.as_slice())
                .unwrap_or(&[]);
            let shell = lower_doc_type(db, file, value_syntax, generics);
            if !shell.is_unknown() {
                return Some(shell);
            }
        }
    }

    let def = facts.type_def_by_id(&type_def)?;
    for super_name in &def.super_names {
        if let Some(super_def) = facts.type_def_by_full_name(super_name) {
            if let Some(shell) = type_member(db, facts, file, super_def.id.clone(), name, visited) {
                return Some(shell);
            }
        }
    }
    None
}

fn collect_type_keys(
    facts: &FileFacts,
    type_def: SemanticId,
    visited: &mut Vec<SemanticId>,
    out: &mut Vec<SmolStr>,
) {
    if visited.contains(&type_def) {
        return;
    }
    visited.push(type_def.clone());
    for member in facts
        .members
        .iter()
        .filter(|member| member.owner == type_def)
    {
        out.push(member.key.to_path().into());
    }
    if let Some(def) = facts.type_def_by_id(&type_def) {
        for super_name in &def.super_names {
            if let Some(super_def) = facts.type_def_by_full_name(super_name) {
                collect_type_keys(facts, super_def.id.clone(), visited, out);
            }
        }
    }
}

// ──────────────────────────────────────────────
// L3 semantics: name resolution and references
// ──────────────────────────────────────────────

/// Name use site -> global id of declaration (scope-aware; falls back to a workspace global declaration when local lookup misses).
pub(crate) fn resolve_name(
    db: &SemanticDatabase,
    file: FileId,
    offset: TextSize,
) -> Option<SemanticId> {
    let facts = file_facts(db, file);
    let name_use = facts.name_use_at_offset(offset)?;
    facts
        .find_visible_decl_before_offset(&name_use.name, offset)
        .map(|decl| decl.id.clone())
        .or_else(|| global_decl_by_name(db, name_use.name.clone()))
}

/// All references to a declaration (name use sites).
pub(crate) fn decl_references(
    db: &SemanticDatabase,
    file: FileId,
    decl: SemanticId,
) -> Vec<LuaSyntaxId> {
    let facts = file_facts(db, file);
    let Some(decl) = facts.decl_by_id(&decl) else {
        return Vec::new();
    };
    facts
        .name_uses
        .iter()
        .filter(|use_| use_.name == decl.name)
        .filter(|use_| {
            let offset = use_.syntax.get_range().start();
            facts
                .find_visible_decl_before_offset(&use_.name, offset)
                .is_some_and(|candidate| candidate.id == decl.id)
        })
        .map(|use_| use_.syntax)
        .collect()
}

// ──────────────────────────────────────────────
// L3 semantics: signature returns (with cycle convergence)
// ──────────────────────────────────────────────

/// Per-slot function return types. Doc annotations take priority (one slot per `---@return`);
/// otherwise scan the function body's `return` statements and merge by slot. Mutual recursion converges via semantic fixed point.
pub(crate) fn signature_returns(
    db: &SemanticDatabase,
    file: FileId,
    config: &Emmyrc,
    closure_syntax: LuaSyntaxId,
) -> Vec<TypeShell> {
    let facts = file_facts(db, file);
    let Some(sig) = facts.signature_by_closure(closure_syntax) else {
        return Vec::new();
    };

    if let Some(docs) = &sig.docs
        && !docs.returns.is_empty()
    {
        let generics = docs.generic_params.as_slice();
        let mut slots = Vec::with_capacity(docs.returns.len());
        for &type_syntax in &docs.returns {
            slots.push(lower_doc_type(db, file, type_syntax, generics));
        }
        if slots.iter().any(|slot| !slot.is_unknown()) {
            return slots;
        }
    }

    // `---@return_overload`: expand rows into multi-return slots, union across rows, fill missing slots with nil.
    if let Some(docs) = &sig.docs
        && !docs.return_overload_rows.is_empty()
    {
        let generics = docs.generic_params.as_slice();
        let mut rows: Vec<Vec<LuaSyntaxId>> = Vec::new();
        let mut index = 0;
        for &len in &docs.return_overload_rows {
            let end = (index + len).min(docs.return_overloads.len());
            rows.push(
                docs.return_overloads[index..end]
                    .iter()
                    .map(|(_, syntax)| *syntax)
                    .collect(),
            );
            index = end;
        }
        let max_len = rows.iter().map(|row| row.len()).max().unwrap_or(0);
        let mut slots = Vec::with_capacity(max_len);
        for slot in 0..max_len {
            let mut shell = TypeShell::unknown();
            for row in &rows {
                if let Some(&type_syntax) = row.get(slot) {
                    shell.merge(&lower_doc_type(db, file, type_syntax, generics));
                } else {
                    shell.merge(&TypeShell::from_primitive(PrimitiveType::Nil));
                }
            }
            slots.push(shell);
        }
        if slots.iter().any(|slot| !slot.is_unknown()) {
            return slots;
        }
    }

    let tree = syntax_tree(db, file);
    let root = tree.get_red_root();
    let Some(node) = closure_syntax.to_node_from_root(&root) else {
        return Vec::new();
    };
    let Some(closure) = LuaClosureExpr::cast(node) else {
        return Vec::new();
    };
    let multi_path_returns = closure.descendants::<LuaReturnStat>().count() > 1;
    let mut slots: Vec<TypeShell> = Vec::new();
    for ret in closure.descendants::<LuaReturnStat>() {
        for (index, expr) in ret.get_expr_list().enumerate() {
            let shell = match &expr {
                LuaExpr::NameExpr(name) if name.get_name_text().as_deref() == Some("self") => {
                    method_self_return_shell(&facts, closure_syntax)
                }
                // Preserve integer literal precision when merging multi-path returns (a `return 1` branch should not be widened to number).
                LuaExpr::LiteralExpr(literal)
                    if multi_path_returns
                        && let Some(LuaLiteralToken::Number(number)) = literal.get_literal() =>
                {
                    match number.get_number_value() {
                        emmylua_parser::NumberResult::Int(_)
                        | emmylua_parser::NumberResult::Uint(_) => {
                            Some(TypeShell::from_primitive(PrimitiveType::Integer))
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
            .unwrap_or_else(|| expr_type_of(db, file, config, expr.get_syntax_id()));
            if let Some(slot) = slots.get_mut(index) {
                slot.merge(&shell);
            } else {
                slots.push(shell);
            }
        }
    }
    // When there is no explicit return annotation and the body cannot infer a type, keep member returns from class-field declarations.
    if slots.is_empty() || slots.iter().all(TypeShell::is_unknown) {
        if let Some(expected) = member_expected_returns(db, &facts, file, config, closure_syntax) {
            return expected;
        }
    }
    slots
}

/// When a member implementation function (`function Test.e()`) has no `---@return`, use the return type
/// of the same-named `---@field e fun(): ...` as this implementation's signature return.
fn member_expected_returns(
    db: &SemanticDatabase,
    facts: &FileFacts,
    file: FileId,
    config: &Emmyrc,
    closure_syntax: LuaSyntaxId,
) -> Option<Vec<TypeShell>> {
    let member = facts
        .members
        .iter()
        .find(|member| member.value_syntax == Some(closure_syntax))?;
    for field in facts
        .members
        .iter()
        .filter(|field| field.key == member.key && field.owner != member.owner)
        .filter(|field| matches!(&field.owner, SemanticId::TypeDef(_)))
    {
        let shell = member_type(db, file, config, field.id.clone());
        for candidate in shell.candidates {
            if let TypeCandidate::Function(fun) = candidate {
                if fun.returns_multi.len() > 1 {
                    return Some(fun.returns_multi.clone());
                } else {
                    return Some(vec![fun.returns.clone()]);
                }
            }
        }
    }
    None
}

/// Self return type for `function T:method() return self end`:
/// first find the type definition associated with the method (`---@class T` comment's owner statement); fall back to the owner table identity if no type is found.
fn method_self_return_shell(facts: &FileFacts, closure_syntax: LuaSyntaxId) -> Option<TypeShell> {
    let member = facts
        .member_by_value_syntax(closure_syntax)
        .filter(|member| member.is_method)?;
    let type_def = match &member.owner {
        SemanticId::TypeDef(type_def) => {
            facts.type_def_by_id(&SemanticId::TypeDef(type_def.clone()))
        }
        SemanticId::Decl(owner_decl) => {
            let owner_decl = facts.decl_by_id(&SemanticId::Decl(owner_decl.clone()))?;
            facts.type_def_by_owner_syntax(owner_decl.owner_syntax?)
        }
        _ => None,
    };
    if let Some(def) = type_def {
        return Some(TypeShell::from_name(def.full_name.as_str()));
    }

    // No type definition: fall back to the owner declaration's initializer identity (usually a table literal).
    let owner_decl = match &member.owner {
        SemanticId::Decl(owner_decl) => facts.decl_by_id(&SemanticId::Decl(owner_decl.clone()))?,
        _ => return None,
    };
    let value_syntax = owner_decl.value_expr_syntax?;
    Some(TypeShell::from_table(TableId::from_range(
        facts.file_id,
        value_syntax.get_range(),
    )))
}

/// Function return type (merged view, compatible with old consumers). Doc annotations take priority; otherwise scan the function body's `return` statements.
/// Mutual recursion (`foo`->`bar`->`foo`) converges via semantic's native fixed point.
pub(crate) fn signature_return(
    db: &SemanticDatabase,
    file: FileId,
    config: &Emmyrc,
    closure_syntax: LuaSyntaxId,
) -> TypeShell {
    let mut shell = TypeShell::unknown();
    for slot in signature_returns(db, file, config, closure_syntax) {
        shell.merge(&slot);
    }
    shell
}

/// Type of the function's `param_index`-th parameter (`---@param` annotation + generic binding).
pub(crate) fn param_type(
    db: &SemanticDatabase,
    file: FileId,
    closure_syntax: LuaSyntaxId,
    param_index: usize,
) -> TypeShell {
    let facts = file_facts(db, file);
    let Some(sig) = facts.signature_by_closure(closure_syntax) else {
        return TypeShell::unknown();
    };
    let Some(docs) = &sig.docs else {
        return TypeShell::unknown();
    };
    let Some(param_name) = sig.param_names.get(param_index) else {
        return TypeShell::unknown();
    };
    let Some((_, type_syntax)) = docs.param_types.iter().find(|(name, _)| name == param_name)
    else {
        return TypeShell::unknown();
    };
    lower_doc_type(db, file, *type_syntax, &docs.generic_params)
}

/// Call target -> its function body (closure) `LuaSyntaxId`.
fn callee_closure_syntax(facts: &FileFacts, callee: LuaExpr) -> Option<LuaSyntaxId> {
    match callee {
        LuaExpr::NameExpr(name_expr) => {
            let name = name_expr.get_name_text()?;
            let offset = name_expr.get_position();
            let decl = facts.find_visible_decl_before_offset(&name, offset)?;
            decl.value_expr_syntax
        }
        LuaExpr::IndexExpr(index_expr) => {
            let (owner, name) = member_ref_from_index_expr(facts, &index_expr)?;
            let member = facts
                .members_of_owner(&owner)
                .find(|m| m.key.name() == Some(name.as_str()))?;
            member.value_syntax
        }
        LuaExpr::ClosureExpr(closure) => Some(closure.get_syntax_id()),
        _ => None,
    }
}

// ──────────────────────────────────────────────
// L3 semantics: module exports
// ──────────────────────────────────────────────

/// Value type exported by a module (type of `return M` / table literal, etc.).
pub(crate) fn module_export_type(
    db: &SemanticDatabase,
    file: FileId,
    config: &Emmyrc,
) -> TypeShell {
    let facts = file_facts(db, file);
    match &facts.module_export {
        // `return M`: declaration identity table (TableConst members reachable) + name identity (Named -> decl owner reachable).
        ModuleExport::Decl { decl, name } => {
            let mut shell = decl_type(db, file, config, decl.clone());
            shell.merge(&TypeShell::from_name(name.as_str()));
            shell
        }
        ModuleExport::Global { name } => {
            // For global exports, resolve by workspace declaration identity first, then fall back to the name.
            let decl = global_decl_by_name(db, SmolStr::new(name.as_str()));
            if let Some(decl) = decl {
                let SemanticId::Decl(decl_key) = &decl else {
                    return TypeShell::from_name(name.as_str());
                };
                if let Some(decl_file) = db.file_data_id(decl_key.file_id) {
                    let shell = decl_type(db, decl_file, config, decl);
                    if !shell.is_unknown() {
                        return shell;
                    }
                }
            }
            TypeShell::from_name(name.as_str())
        }
        ModuleExport::Expr { value_syntax } => {
            let tree = syntax_tree(db, file);
            let Some(expr) = find_expr_by_syntax_id(&tree, value_syntax) else {
                return TypeShell::unknown();
            };
            expr_type(db, &facts, file, config, expr)
        }
        ModuleExport::None => TypeShell::unknown(),
    }
}

// ──────────────────────────────────────────────
// Expression types
// ──────────────────────────────────────────────

/// Locate an expression in the syntax tree by `LuaSyntaxId` (kind+range, unique).
pub(crate) fn find_expr_by_syntax_id(
    tree: &LuaSyntaxTree,
    syntax_id: &LuaSyntaxId,
) -> Option<LuaExpr> {
    let root = tree.get_red_root();
    let node = syntax_id.to_node_from_root(&root)?;
    LuaExpr::cast(node)
}

thread_local! {
    static EXPR_TYPE_IN_PROGRESS: RefCell<Vec<(FileId, LuaSyntaxId)>> =
        const { RefCell::new(Vec::new()) };
}

struct ExprTypeGuard {
    key: (FileId, LuaSyntaxId),
}

impl ExprTypeGuard {
    fn enter(key: (FileId, LuaSyntaxId)) -> Self {
        EXPR_TYPE_IN_PROGRESS.with(|stack| stack.borrow_mut().push(key.clone()));
        Self { key }
    }
}

thread_local! {
    static ITER_SLOT_IN_PROGRESS: RefCell<Vec<(FileId, SemanticId)>> =
        const { RefCell::new(Vec::new()) };
}

struct IterSlotGuard {
    key: (FileId, SemanticId),
}

impl IterSlotGuard {
    fn enter(key: (FileId, SemanticId)) -> Self {
        ITER_SLOT_IN_PROGRESS.with(|stack| stack.borrow_mut().push(key.clone()));
        Self { key }
    }
}

impl Drop for IterSlotGuard {
    fn drop(&mut self) {
        ITER_SLOT_IN_PROGRESS.with(|stack| {
            let mut stack = stack.borrow_mut();
            if let Some(pos) = stack.iter().rposition(|key| key == &self.key) {
                stack.remove(pos);
            }
        });
    }
}

impl Drop for ExprTypeGuard {
    fn drop(&mut self) {
        EXPR_TYPE_IN_PROGRESS.with(|stack| {
            let mut stack = stack.borrow_mut();
            if let Some(pos) = stack.iter().rposition(|key| key == &self.key) {
                stack.remove(pos);
            }
        });
    }
}

/// Type of an expression (by syntax position, node-keyed). Entry point for the semantic/infer layer.
///
/// Semantic's tracked memo/cycle handling is replaced by a per-thread in-progress guard:
/// re-entering the same expression while it is still being inferred returns `Unknown`,
/// which matches the previous Semantic `cycle_initial` behavior.
pub(crate) fn expr_type_of(
    db: &SemanticDatabase,
    file: FileId,
    config: &Emmyrc,
    expr_syntax: LuaSyntaxId,
) -> TypeShell {
    let key = (file, expr_syntax);
    if EXPR_TYPE_IN_PROGRESS.with(|stack| stack.borrow().contains(&key)) {
        return TypeShell::unknown();
    }
    let _guard = ExprTypeGuard::enter(key);
    let facts = file_facts(db, file);
    let tree = syntax_tree(db, file);
    let Some(expr) = find_expr_by_syntax_id(&tree, &expr_syntax) else {
        return TypeShell::unknown();
    };
    expr_type(db, &facts, file, config, expr)
}

fn expr_type(
    db: &SemanticDatabase,
    facts: &FileFacts,
    file: FileId,
    config: &Emmyrc,
    expr: LuaExpr,
) -> TypeShell {
    // Deep member/call chains (1500+ levels) use an explicit task stack: avoids exhausting the native stack by recursive prefix evaluation.
    if let Some(shell) = expr_type_chain(db, facts, file, config, expr.clone()) {
        return shell;
    }
    expr_type_node(db, facts, file, config, expr)
}

#[derive(Clone)]
enum ChainFrame {
    Call(LuaCallExpr, LuaExpr),
    Index(LuaIndexExpr),
    Paren,
}

fn expr_type_chain(
    db: &SemanticDatabase,
    facts: &FileFacts,
    file: FileId,
    config: &Emmyrc,
    expr: LuaExpr,
) -> Option<TypeShell> {
    let mut current = expr;
    let mut frames: Vec<ChainFrame> = Vec::new();
    loop {
        match current {
            LuaExpr::CallExpr(call) => {
                let prefix = call.get_prefix_expr()?;
                frames.push(ChainFrame::Call(call, prefix.clone()));
                current = prefix;
            }
            LuaExpr::IndexExpr(index) => {
                let prefix = index.get_prefix_expr()?;
                frames.push(ChainFrame::Index(index));
                current = prefix;
            }
            LuaExpr::ParenExpr(paren) => {
                let inner = paren.get_expr()?;
                frames.push(ChainFrame::Paren);
                current = inner;
            }
            _ => break,
        }
    }
    let mut shell = expr_type_node(db, facts, file, config, current);
    for frame in frames.iter().rev() {
        match frame {
            ChainFrame::Call(call, prefix) => {
                shell =
                    expr_type_call(db, facts, file, config, call.clone(), prefix.clone(), shell);
            }
            ChainFrame::Index(index) => {
                shell = expr_type_index(db, facts, file, config, index.clone(), shell);
            }
            ChainFrame::Paren => {}
        }
    }
    Some(shell)
}

fn expr_type_node(
    db: &SemanticDatabase,
    facts: &FileFacts,
    file: FileId,
    config: &Emmyrc,
    expr: LuaExpr,
) -> TypeShell {
    match expr {
        LuaExpr::LiteralExpr(literal) => literal_type(&literal),
        LuaExpr::TableExpr(table) => {
            TypeShell::from_table(TableId::from_range(file, table.get_range()))
        }
        LuaExpr::ClosureExpr(_) => TypeShell::from_primitive(PrimitiveType::Function),
        LuaExpr::NameExpr(name_expr) => {
            let Some(name) = name_expr.get_name_text() else {
                return TypeShell::unknown();
            };
            if name == "nil" {
                return TypeShell::from_primitive(PrimitiveType::Nil);
            }

            let offset = name_expr.get_position();
            if let Some(decl) = facts.find_visible_decl_before_offset(&name, offset) {
                return decl_type(db, file, config, decl.id.clone());
            }
            // Cross-file global fallback: global variable -> global type name.
            let global_name = SmolStr::new(name.as_str());
            if let Some(decl) = global_decl_by_name(db, global_name.clone()) {
                if let SemanticId::Decl(key) = &decl
                    && let Some(decl_file) = db.file_data_id(key.file_id)
                {
                    let shell = decl_type(db, decl_file, config, decl);
                    if !shell.is_unknown() {
                        return shell;
                    }
                }
            }
            if let Some(type_def) = global_type_by_name(db, global_name) {
                if let SemanticId::TypeDef(key) = &type_def {
                    return TypeShell::from_name(key.full_name.as_str());
                }
            }
            TypeShell::unknown()
        }
        LuaExpr::IndexExpr(index_expr) => {
            let Some(prefix) = index_expr.get_prefix_expr() else {
                return TypeShell::unknown();
            };
            let prefix_shell = expr_type(db, facts, file, config, prefix);
            expr_type_index(db, facts, file, config, index_expr, prefix_shell)
        }
        LuaExpr::CallExpr(call_expr) => {
            let Some(prefix) = call_expr.get_prefix_expr() else {
                return TypeShell::unknown();
            };
            let prefix_shell = expr_type(db, facts, file, config, prefix.clone());
            expr_type_call(db, facts, file, config, call_expr, prefix, prefix_shell)
        }
        LuaExpr::BinaryExpr(binary) => {
            let op = binary.get_op_token().map(|token| token.get_op());
            match op {
                Some(BinaryOperator::OpOr) | Some(BinaryOperator::OpAnd) => {
                    if let Some((left, right)) = binary.get_exprs() {
                        let mut shell = expr_type(db, facts, file, config, left);
                        shell.merge(&expr_type(db, facts, file, config, right));
                        return shell;
                    }
                    TypeShell::unknown()
                }
                Some(BinaryOperator::OpConcat) => TypeShell::from_primitive(PrimitiveType::String),
                Some(
                    BinaryOperator::OpLt
                    | BinaryOperator::OpLe
                    | BinaryOperator::OpGt
                    | BinaryOperator::OpGe
                    | BinaryOperator::OpEq
                    | BinaryOperator::OpNe,
                ) => TypeShell::from_primitive(PrimitiveType::Boolean),
                Some(
                    BinaryOperator::OpAdd
                    | BinaryOperator::OpSub
                    | BinaryOperator::OpMul
                    | BinaryOperator::OpDiv
                    | BinaryOperator::OpIDiv
                    | BinaryOperator::OpMod
                    | BinaryOperator::OpPow,
                ) => TypeShell::from_primitive(PrimitiveType::Number),
                _ => TypeShell::unknown(),
            }
        }
        LuaExpr::UnaryExpr(unary) => {
            let op = unary.get_op_token().map(|token| token.get_op());
            if op == Some(UnaryOperator::OpNot) {
                TypeShell::from_primitive(PrimitiveType::Boolean)
            } else {
                unary
                    .get_expr()
                    .map(|expr| expr_type(db, facts, file, config, expr))
                    .unwrap_or_else(TypeShell::unknown)
            }
        }
        LuaExpr::ParenExpr(paren) => paren
            .get_expr()
            .map(|expr| expr_type(db, facts, file, config, expr))
            .unwrap_or_else(TypeShell::unknown),
        _ => TypeShell::unknown(),
    }
}

fn expr_type_index(
    db: &SemanticDatabase,
    facts: &FileFacts,
    file: FileId,
    config: &Emmyrc,
    index_expr: LuaIndexExpr,
    prefix_shell: TypeShell,
) -> TypeShell {
    let Some((owner, name)) = member_ref_from_index_expr(facts, &index_expr) else {
        return TypeShell::unknown();
    };
    // 1. In-file members (owner key).
    if let Some(member) = facts
        .members_of_owner(&owner)
        .find(|m| m.key.name() == Some(name.as_str()))
    {
        return member_type(db, file, config, member.id.clone());
    }
    // 2/3. Phase 2: cross-file merged lookup + type-member fallback (requires workspace).
    // 2. Cross-file members by owner key + resolved concrete id key.
    if let Some(shell) = member_type_via_owner(db, config, &owner, &name) {
        return shell;
    }
    // 3. If the prefix type is a named class/export name -> cross-file owner members (@field + runtime), then inherited @fields.
    for candidate in &prefix_shell.candidates {
        // Generic instantiation `Box<number>`: member types contain `Generic(T)`, substitute with actual args.
        if let TypeCandidate::GenericInstance(ins) = candidate {
            if let Some(def) = resolve_type_def(db, file, SmolStr::new(ins.name.as_str())) {
                if let Some(shell) = member_type_via_owner(db, config, &def.id, &name) {
                    let substituted = substitute_generics(&shell, &def.generic_params, &ins.args);
                    if !substituted.is_unknown() {
                        return substituted;
                    }
                }
            }
        }
        // Anonymous table literal: members are collected under a synthetic owner; look them up by that owner.
        if let TypeCandidate::Table(table_id) = candidate {
            let owner = SemanticId::member(
                FileId::new(table_id.file_id),
                TextRange::new(TextSize::from(table_id.start), TextSize::from(table_id.end)),
            );
            if let Some(shell) = member_type_via_owner(db, config, &owner, &name) {
                return shell;
            }
        }
        if let TypeCandidate::Named(type_name) = candidate {
            // Cross-file: runtime members under Name(type_name) key + resolved @fields.
            // Direct class-table writes (`Foo.extra`) can see dot assignments;
            // instance access through a named type (`other.extra`) only inherits `:` methods,
            // not arbitrary members from the global class table as instance fields.
            let prefix_is_class_table = index_expr.get_prefix_expr().is_some_and(|prefix| {
                matches!(
                    &prefix,
                    LuaExpr::NameExpr(name)
                        if name.get_name_text().as_deref() == Some(type_name.as_str())
                )
            });
            let named_owner = SemanticId::name(type_name.clone());
            let is_class_instance = resolve_type_def(db, file, type_name.clone())
                .is_some_and(|def| def.kind == TypeDefKind::Class);
            if prefix_is_class_table || !is_class_instance {
                if let Some(shell) = member_type_via_owner(db, config, &named_owner, &name) {
                    return shell;
                }
            } else if let Some(shell) =
                member_type_via_owner_method(db, config, &named_owner, &name)
            {
                return shell;
            }
            // Inherited @fields (in-file class definitions).
            let def = facts
                .type_def_by_name(type_name.as_str())
                .or_else(|| facts.type_def_by_full_name(type_name.as_str()));
            if let Some(def) = def {
                let mut visited = Vec::new();
                if let Some(shell) =
                    type_member(db, facts, file, def.id.clone(), &name, &mut visited)
                {
                    return shell;
                }
            }
        }
    }
    TypeShell::unknown()
}

fn expr_type_call(
    db: &SemanticDatabase,
    facts: &FileFacts,
    file: FileId,
    config: &Emmyrc,
    call_expr: LuaCallExpr,
    prefix: LuaExpr,
    prefix_shell: TypeShell,
) -> TypeShell {
    // require special case: module name -> module file -> module export type.
    if call_expr.is_require()
        && let Some(module_name) = require_module_name(&call_expr)
        && let Some(module_file) = module_file_of(db, config, SmolStr::new(&module_name))
        && let Some(module_input) = db.file_data_id(module_file)
    {
        let shell = module_export_type(db, module_input, config);
        if !shell.is_unknown() {
            return shell;
        }
    }
    match callee_closure_syntax(facts, prefix) {
        Some(closure_syntax) => signature_return(db, file, config, closure_syntax),
        None => {
            // Function-value call: if callee type is fun(...), take its return type (including generic substitution).
            for candidate in &prefix_shell.candidates {
                if let TypeCandidate::Function(fun) = candidate {
                    return fun.returns.clone();
                }
            }
            TypeShell::unknown()
        }
    }
}

/// Phase 2 member type: union of members by owner key + resolved concrete id key (cross-file).
/// Each member's type is resolved in its declaring file (`member_type` keyed by file input, so invalidation is file-precise).
fn member_type_via_owner(
    db: &SemanticDatabase,
    config: &Emmyrc,
    owner: &SemanticId,
    name: &str,
) -> Option<TypeShell> {
    // Union of dual identities: same-name type (@field) + runtime value (member declaration).
    for owner in resolve_owner_set(db, owner.clone()) {
        for member in members_of_owner(db, owner).iter().cloned() {
            if member.name != name {
                continue;
            }
            let Some(member_file_data_id) = db.file_data_id(member.file_id) else {
                continue;
            };
            let shell = member_type(db, member_file_data_id, config, member.id);
            if !shell.is_unknown() {
                return Some(shell);
            }
        }
    }
    None
}

/// Same as `member_type_via_owner`, but only accepts `:` method members.
/// Instance access inherits methods from the class table, not arbitrary dot-assignments on it.
fn member_type_via_owner_method(
    db: &SemanticDatabase,
    config: &Emmyrc,
    owner: &SemanticId,
    name: &str,
) -> Option<TypeShell> {
    for resolved in resolve_owner_set(db, owner.clone()) {
        for member in members_of_owner(db, resolved).iter().cloned() {
            if member.name != name {
                continue;
            }
            let Some(member_file_data_id) = db.file_data_id(member.file_id) else {
                continue;
            };
            let member_facts = file_facts(db, member_file_data_id);
            let Some(member_def) = member_facts.member_by_id(&member.id) else {
                continue;
            };
            if !member_def.is_method {
                continue;
            }
            let shell = member_type(db, member_file_data_id, config, member.id);
            if !shell.is_unknown() {
                return Some(shell);
            }
        }
    }
    None
}

/// Generic substitution: replace `Generic(param)` candidates in a shell with argument types (recursing into function types).
fn substitute_generics(
    shell: &TypeShell,
    params: &[DocGenericParam],
    args: &[TypeShell],
) -> TypeShell {
    let mut out = TypeShell::unknown();
    for candidate in &shell.candidates {
        match candidate {
            TypeCandidate::Generic(gname) => {
                let index = params.iter().position(|p| &p.name == gname);
                if let Some(index) = index
                    && let Some(arg) = args.get(index)
                {
                    out.merge(arg);
                } else {
                    out.merge(&TypeShell {
                        candidates: vec![candidate.clone()],
                    });
                }
            }
            TypeCandidate::Array(base) => {
                out.merge(&TypeShell::from_array(substitute_generics(
                    base, params, args,
                )));
            }
            TypeCandidate::Variadic(base) => {
                out.merge(&TypeShell::from_variadic(substitute_generics(
                    base, params, args,
                )));
            }
            TypeCandidate::Tuple(types) => {
                out.merge(&TypeShell::from_tuple(
                    types
                        .iter()
                        .map(|ty| substitute_generics(ty, params, args))
                        .collect(),
                ));
            }
            TypeCandidate::Function(fun) => {
                let new_params = fun
                    .params
                    .iter()
                    .map(|p| substitute_generics(p, params, args))
                    .collect();
                let new_returns = substitute_generics(&fun.returns, params, args);
                let new_returns_multi = fun
                    .returns_multi
                    .iter()
                    .map(|r| substitute_generics(r, params, args))
                    .collect();
                out.merge(&TypeShell::from_function(
                    new_params,
                    fun.param_names.clone(),
                    new_returns,
                    new_returns_multi,
                    fun.generic_params.clone(),
                    fun.async_state,
                    fun.is_colon_define,
                    fun.is_variadic,
                ));
            }
            _ => out.merge(&TypeShell {
                candidates: vec![candidate.clone()],
            }),
        }
    }
    out
}

fn literal_type(literal: &LuaLiteralExpr) -> TypeShell {
    match literal.get_literal() {
        Some(LuaLiteralToken::String(_)) => TypeShell::from_primitive(PrimitiveType::String),
        Some(LuaLiteralToken::Number(number)) => match number.get_number_value() {
            emmylua_parser::NumberResult::Int(_) | emmylua_parser::NumberResult::Uint(_) => {
                TypeShell::from_primitive(PrimitiveType::Number)
            }
            emmylua_parser::NumberResult::Float(f) => {
                TypeShell::from_literal(LiteralShell::Float(f.to_bits()))
            }
            emmylua_parser::NumberResult::Number => {
                TypeShell::from_primitive(PrimitiveType::Number)
            }
        },
        Some(LuaLiteralToken::Bool(_)) => TypeShell::from_primitive(PrimitiveType::Boolean),
        Some(LuaLiteralToken::Nil(_)) => TypeShell::from_primitive(PrimitiveType::Nil),
        Some(LuaLiteralToken::Dots(_)) => TypeShell::unknown(),
        _ => TypeShell::unknown(),
    }
}

/// Module name for a `require` call (the first argument must be a string literal; dynamic arguments are left to the infer layer).
fn require_module_name(call_expr: &LuaCallExpr) -> Option<String> {
    let arg_list = call_expr.get_args_list()?;
    let first = arg_list.get_args().next()?;
    match first {
        LuaExpr::LiteralExpr(literal) => match literal.get_literal()? {
            LuaLiteralToken::String(token) => Some(token.get_value()),
            _ => None,
        },
        _ => None,
    }
}

/// file_id → (file input, config input).
pub(crate) fn file_and_config(db: &SemanticDatabase, file_id: FileId) -> Option<(FileId, &Emmyrc)> {
    Some((db.file_data_id(file_id)?, db.config_input()?))
}

/// Index expression -> `(owner, name)` (member reference resolution).
pub(crate) fn member_ref_from_index_expr(
    facts: &FileFacts,
    index_expr: &LuaIndexExpr,
) -> Option<(SemanticId, SmolStr)> {
    let name = SmolStr::new(index_expr.get_index_key()?.get_path_part());
    let prefix = index_expr.get_prefix_expr()?;
    let mut segments = Vec::new();
    let owner = resolve_expr_root(facts, prefix, &mut segments)?;
    Some((owner, name))
}

fn resolve_expr_root(
    facts: &FileFacts,
    expr: LuaExpr,
    _segments: &mut Vec<SmolStr>,
) -> Option<SemanticId> {
    // Deep member chains: expand IndexExpr with an explicit task stack to avoid native stack recursion.
    let mut current = expr;
    let mut segments = Vec::new();
    loop {
        match current {
            LuaExpr::ParenExpr(paren) => {
                current = paren.get_expr()?;
            }
            LuaExpr::IndexExpr(parent) => {
                segments.push(SmolStr::new(parent.get_index_key()?.get_path_part()));
                current = parent.get_prefix_expr()?;
            }
            _ => break,
        }
    }
    let owner = match current {
        LuaExpr::NameExpr(name_expr) => {
            let name = name_expr.get_name_text()?;
            if name == "_ENV" || name == "_G" {
                SemanticId::name(SmolStr::new(name))
            } else {
                let offset = name_expr.get_position();
                if let Some(decl) = facts.find_visible_decl_before_offset(&name, offset)
                    && !matches!(decl.kind, DeclKind::Global)
                {
                    decl.id.clone()
                } else {
                    SemanticId::name(SmolStr::new(name))
                }
            }
        }
        _ => return None,
    };
    if let SemanticId::Name(root) = &owner {
        let mut path = root.as_str().to_string();
        for s in segments.iter().rev() {
            path.push('.');
            path.push_str(s);
        }
        return Some(SemanticId::name(SmolStr::new(path)));
    }
    Some(owner)
}
