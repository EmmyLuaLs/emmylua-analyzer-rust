//! Workspace type and deprecated indexes.

use hashbrown::HashMap;
use std::sync::Arc;

use smol_str::SmolStr;

use crate::FileId;
use crate::WorkspaceId;
use crate::semantic_db::SemanticDatabase;
use crate::semantic_db::def::{DeclKind, SemanticId, TypeDef, TypeScope, TypeVisibility};
use crate::semantic_db::exports::FileExports;
use crate::semantic_db::query::{file_facts, file_matches_workspace_id};

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
    #[cfg(test)]
    db.rebuild_metrics
        .full_index_source_scans
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let mut by_scope_name: HashMap<(TypeScope, SmolStr), Vec<TypeDef>> = HashMap::new();
    let mut by_file = HashMap::new();
    for file_id in db.vfs().file_ids() {
        let Some(cache) = db.file_cache(file_id) else {
            continue;
        };
        let exports = cache.exports.as_ref();
        if !file_matches_workspace_id(db, file_id, ws_id) {
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
        by_file.insert(file_id, keys);
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

/// Per-file deprecated facts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DeprecatedFileData {
    pub names: Vec<SmolStr>,
    pub member_keys: Vec<(SemanticId, SmolStr)>,
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
    #[cfg(test)]
    db.rebuild_metrics
        .full_index_source_scans
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let mut index = DeprecatedIndex::default();
    for file_id in db.vfs().file_ids() {
        let Some(cache) = db.file_cache(file_id) else {
            continue;
        };
        if file_matches_workspace_id(db, file_id, ws_id) {
            index.add_file(file_id, cache.deprecated.as_ref());
        }
    }
    index
}
