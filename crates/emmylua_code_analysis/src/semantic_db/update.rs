//! Write-side mutation entry points.
use super::FileCache;
use super::SemanticDatabase;
use super::def::{
    ChangedKeys, DependencyKey, FileDependencies, SemanticId, TypeDef, TypeScope, TypeVisibility,
};
use super::exports::{FileExports, build_file_exports};
use super::flow::build_flow_tree;
use super::query::aggregate_member_bucket;
use super::query::file_workspace_id_for_path;
use super::query::{
    DeprecatedFileData, FileReferences, ModuleEntry, WorkspaceIndexCache, all_workspace_ids,
    build_deprecated_file, build_file_facts, build_file_references, build_global_type_aggregate,
    build_member_aggregates, build_module_entry, build_workspace_decl_index,
    build_workspace_deprecated_index, build_workspace_member_index, build_workspace_module_index,
    build_workspace_reference_index, build_workspace_type_index, changed_keys, file_workspace_id,
};
use crate::{FileId, WorkspaceId, file_path_to_uri};
use hashbrown::HashMap;
use hashbrown::HashSet;
use smol_str::SmolStr;
use std::sync::Arc;

/// One file change applied to workspace indexes.
pub(crate) struct WorkspaceFileUpdate<'a> {
    pub(crate) file_id: FileId,
    pub(crate) old_workspace: Option<WorkspaceId>,
    pub(crate) old_exports: Option<&'a FileExports>,
    pub(crate) new_exports: Option<&'a FileExports>,
    pub(crate) new_references: Option<Arc<FileReferences>>,
    pub(crate) module_entry: Option<ModuleEntry>,
    pub(crate) new_deprecated: Option<&'a DeprecatedFileData>,
}

pub(crate) fn apply_file_to_workspace_indexes(
    db: &mut SemanticDatabase,
    update: WorkspaceFileUpdate<'_>,
) {
    let WorkspaceFileUpdate {
        file_id,
        old_workspace,
        old_exports,
        new_exports,
        new_references,
        module_entry,
        new_deprecated,
    } = update;
    let mut changed_global_type_names: HashSet<SmolStr> = HashSet::new();
    let mut changed_member_owners: HashSet<SemanticId> = HashSet::new();
    let mut changed_member_names: HashSet<(SemanticId, SmolStr)> = HashSet::new();
    for exports in [old_exports, new_exports].into_iter().flatten() {
        for def in &exports.types {
            if def.visibility == TypeVisibility::Public {
                changed_global_type_names.insert(def.full_name.clone());
            }
        }
        for member in &exports.members {
            changed_member_owners.insert(member.owner.clone());
            changed_member_names.insert((member.owner.clone(), member.key.to_path().into()));
        }
    }
    // This function mutates `db.workspace_index`; clone the tiny cached list to
    // avoid holding a borrow of `db` across the mutation loop.
    let ws_ids = all_workspace_ids(db).to_vec();
    let target_ws = file_workspace_id(db, file_id).unwrap_or(WorkspaceId::REMOTE);

    for ws_id in &ws_ids {
        if let Some(index) = db.workspace_index.types.get_mut(ws_id) {
            index.remove_file(file_id);
        }
        if let Some(index) = db.workspace_index.members.get_mut(ws_id) {
            index.remove_file(file_id);
        }
        if let Some(index) = db.workspace_index.decls.get_mut(ws_id) {
            index.remove_file(file_id);
        }
        if let Some(index) = db.workspace_index.deprecated.get_mut(ws_id) {
            index.remove_file(file_id);
        }
        if let Some(index) = db.workspace_index.references.get_mut(ws_id) {
            index.remove_file(file_id);
        }
    }

    // Module index: replace the entry in place when the workspace is unchanged,
    // otherwise remove from the old workspace and add to the new one. Each
    // apply_file_change is a no-op when the module entry did not change.
    let mut module_derived_rebuilds = 0u32;
    match old_workspace {
        Some(old_workspace) if old_workspace != target_ws => {
            if let Some(index) = db.workspace_index.modules.get_mut(&old_workspace) {
                module_derived_rebuilds += u32::from(index.apply_file_change(file_id, None));
            }
            if let Some(index) = db.workspace_index.modules.get_mut(&target_ws) {
                module_derived_rebuilds +=
                    u32::from(index.apply_file_change(file_id, module_entry));
            }
        }
        _ => {
            if let Some(index) = db.workspace_index.modules.get_mut(&target_ws) {
                module_derived_rebuilds +=
                    u32::from(index.apply_file_change(file_id, module_entry));
            }
        }
    }
    let _ = module_derived_rebuilds;
    #[cfg(test)]
    if module_derived_rebuilds > 0 {
        db.rebuild_metrics.module_derived_rebuilds.fetch_add(
            module_derived_rebuilds,
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    if let Some(exports) = new_exports {
        if let Some(index) = db.workspace_index.types.get_mut(&target_ws) {
            index.add_file(target_ws, file_id, exports);
        }
        if let Some(index) = db.workspace_index.members.get_mut(&target_ws) {
            index.add_file(file_id, exports);
        }
        if let Some(index) = db.workspace_index.decls.get_mut(&target_ws) {
            index.add_file(file_id, exports);
        }
    }

    refresh_owner_member_keys(db, changed_member_owners, changed_member_names);
    refresh_global_type_keys(db, changed_global_type_names);

    if let Some(deprecated) = new_deprecated
        && let Some(index) = db.workspace_index.deprecated.get_mut(&target_ws)
    {
        index.add_file(file_id, deprecated);
    }

    if let Some(references) = new_references
        && let Some(index) = db.workspace_index.references.get_mut(&target_ws)
    {
        index.add_file(file_id, references);
    }
}
pub(crate) fn rebuild_file_after_write(
    db: &mut SemanticDatabase,
    file_id: FileId,
    old_workspace: Option<WorkspaceId>,
    metadata_changed: bool,
    old_module_entry: Option<ModuleEntry>,
) {
    if db.config_input().is_none() {
        return;
    }
    let is_new_file = db.file_facts_of(file_id).is_none();
    if is_new_file {
        db.refresh_module_fallback_root();
    }
    let Some(data) = db.file_data(file_id) else {
        return;
    };
    let text = data.text.clone();
    let path = data.path.clone();

    let old_cache = db.files.remove(&file_id);
    let old_exports = old_cache.as_ref().map(|cache| Arc::clone(&cache.exports));
    let old_references = old_cache
        .as_ref()
        .map(|cache| Arc::clone(&cache.references));

    let facts = build_file_facts(db, file_id, file_id, &text);
    let workspace_id = file_workspace_id_for_path(db.workspace_roots().as_ref(), path.as_deref());

    // Facts must be visible before flow/export builders run.
    db.files.insert(
        file_id,
        FileCache {
            facts,
            flow: Default::default(),
            exports: Arc::new(Default::default()),
            references: old_references.clone().unwrap_or_default(),
            module_entry: None,
            deprecated: Arc::new(Default::default()),
            workspace_id,
        },
    );

    let flow = build_flow_tree(db, file_id);
    let exports = build_file_exports(db, file_id, file_id);
    let new_exports = Arc::new(exports);
    {
        let cache = db
            .files
            .get_mut(&file_id)
            .expect("file cache must exist after facts insert");
        cache.flow = flow;
        cache.exports = Arc::clone(&new_exports);
    }

    let deprecated_data = build_deprecated_file(db, file_id);
    let module_entry = build_module_entry(db, file_id);
    if let Some(cache) = db.files.get_mut(&file_id) {
        cache.module_entry = module_entry.clone();
        cache.deprecated = Arc::new(deprecated_data.clone());
    }

    let module_name_changes =
        module_name_changed_keys(old_module_entry.as_ref(), module_entry.as_ref());

    // Update export/type/member/decl/module indexes first, so the new file's
    // declarations are visible while its own references are resolved below.
    apply_file_to_workspace_indexes(
        db,
        WorkspaceFileUpdate {
            file_id,
            old_workspace,
            old_exports: old_exports.as_deref(),
            new_exports: Some(new_exports.as_ref()),
            new_references: None,
            module_entry,
            new_deprecated: Some(&deprecated_data),
        },
    );

    let references = Arc::new(build_file_references(db, file_id));
    db.files
        .get_mut(&file_id)
        .expect("file cache must exist after write")
        .references = Arc::clone(&references);
    apply_file_reference_index(db, file_id, Arc::clone(&references));

    // With no registered roots and no explicit main root, module names are derived
    // from the common path root, so a path change can affect other files' entries.
    let fallback_may_change = db.workspace_roots().is_empty()
        && db.main_root().is_none()
        && (metadata_changed || old_exports.is_none());
    if fallback_may_change {
        db.refresh_module_fallback_root();
        rebuild_all_module_entries(db);
    }

    refresh_dependent_references(
        db,
        file_id,
        old_exports.as_deref(),
        Some(new_exports.as_ref()),
        old_references.as_deref(),
        Some(references.as_ref()),
        module_name_changes,
        metadata_changed || fallback_may_change,
        fallback_may_change,
    );
}

pub(crate) fn rebuild_file_after_remove(
    db: &mut SemanticDatabase,
    file_id: FileId,
    old_workspace: Option<WorkspaceId>,
    old_exports: Option<Arc<FileExports>>,
    old_references: Option<Arc<FileReferences>>,
    old_module_entry: Option<ModuleEntry>,
) {
    db.files.remove(&file_id);
    db.refresh_module_fallback_root();

    if db.config_input().is_none() {
        return;
    }

    apply_file_to_workspace_indexes(
        db,
        WorkspaceFileUpdate {
            file_id,
            old_workspace,
            old_exports: old_exports.as_deref(),
            new_exports: None,
            new_references: None,
            module_entry: None,
            new_deprecated: None,
        },
    );

    if db.workspace_roots().is_empty() && db.main_root().is_none() {
        db.refresh_module_fallback_root();
        rebuild_all_module_entries(db);
    }

    let module_name_changes = module_name_changed_keys(old_module_entry.as_ref(), None);

    refresh_dependent_references(
        db,
        file_id,
        old_exports.as_deref(),
        None,
        old_references.as_deref(),
        None,
        module_name_changes,
        false,
        false,
    );
}
pub(crate) fn rebuild_all_caches(db: &mut SemanticDatabase) {
    #[cfg(test)]
    db.rebuild_metrics
        .full_rebuilds
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    db.refresh_module_fallback_root();
    if db.config_input().is_none() {
        db.files.clear();
        db.workspace_index = WorkspaceIndexCache::new();
        return;
    }
    // Clear the previous generation before building the new one: facts builders
    // call `file_workspace_id()`, which must observe the current roots instead of
    // a stale cached `FileCache.workspace_id`.
    db.files.clear();
    let file_ids = db.vfs.file_ids();

    let mut files = HashMap::with_capacity(file_ids.len());
    for file_id in file_ids.iter().copied() {
        let Some(data) = db.file_data(file_id) else {
            continue;
        };
        let facts = build_file_facts(db, file_id, file_id, &data.text);
        let workspace_id =
            file_workspace_id_for_path(db.workspace_roots().as_ref(), data.path.as_deref());
        files.insert(
            file_id,
            FileCache {
                facts,
                flow: Default::default(),
                exports: Arc::new(Default::default()),
                references: Arc::new(Default::default()),
                module_entry: None,
                deprecated: Arc::new(Default::default()),
                workspace_id,
            },
        );
    }
    db.files = files;
    for file_id in file_ids.iter().copied() {
        if db.file_data(file_id).is_none() {
            continue;
        }
        let flow = build_flow_tree(db, file_id);
        let cache = db
            .files
            .get_mut(&file_id)
            .expect("file cache must exist after facts build");
        cache.flow = flow;
    }

    for file_id in file_ids.iter().copied() {
        if db.file_data(file_id).is_none() {
            continue;
        }
        let module_entry = build_module_entry(db, file_id);
        let deprecated = Arc::new(build_deprecated_file(db, file_id));
        if let Some(cache) = db.files.get_mut(&file_id) {
            cache.module_entry = module_entry;
            cache.deprecated = deprecated;
        }
    }

    // Build module entries/indexes *before* export contributions: resolving a
    // `local M = require("mod")` alias needs `module_file_of()`, and the module
    // index only depends on facts + workspace roots.
    rebuild_module_indexes(db);

    for file_id in file_ids.iter().copied() {
        if db.file_data(file_id).is_none() {
            continue;
        }
        let exports = build_file_exports(db, file_id, file_id);
        let cache = db
            .files
            .get_mut(&file_id)
            .expect("file cache must exist after facts build");
        cache.exports = Arc::new(exports);
    }

    rebuild_workspace_indexes(db);

    for file_id in file_ids.iter().copied() {
        if let Some(file) = db.file_data_id(file_id) {
            let references = build_file_references(db, file);
            db.files
                .get_mut(&file_id)
                .expect("file cache must exist before references build")
                .references = Arc::new(references);
        }
    }

    rebuild_workspace_reference_indexes(db);
    rebuild_dependency_index(db);
}

/// Recompute every file module entry after the fallback root changed.
///
/// Workspace module indexes are built from FileCache.module_entry, so all
/// entries must be refreshed before rebuild_module_indexes.
pub(crate) fn rebuild_all_module_entries(db: &mut SemanticDatabase) {
    for file_id in db.file_ids() {
        if db.file_data_id(file_id).is_none() {
            continue;
        }
        let entry = build_module_entry(db, file_id);
        if let Some(cache) = db.files.get_mut(&file_id) {
            cache.module_entry = entry;
        }
    }
    rebuild_module_indexes(db);
}

/// One file's contribution update for the workspace indexes.
///
/// Bundles the parameters that used to be passed as a long positional list
/// (`file_id`, workspace, exports, references, module entry, deprecated data).
pub(crate) fn apply_file_reference_index(
    db: &mut SemanticDatabase,
    file_id: FileId,
    references: Arc<FileReferences>,
) {
    let ws_id = file_workspace_id(db, file_id).unwrap_or(WorkspaceId::REMOTE);
    let index = db.workspace_index.references.entry(ws_id).or_default();
    index.remove_file(file_id);
    index.add_file(file_id, references);
}

/// Rebuild only the per-workspace module indexes.
pub(crate) fn rebuild_module_indexes(db: &mut SemanticDatabase) {
    let ws_ids = all_workspace_ids(db);
    let modules = ws_ids
        .iter()
        .map(|&ws_id| (ws_id, build_workspace_module_index(db, ws_id)))
        .collect::<HashMap<_, _>>();
    db.workspace_index.modules = modules;
}

fn rebuild_workspace_indexes(db: &mut SemanticDatabase) {
    #[cfg(test)]
    db.rebuild_metrics
        .workspace_index_rebuilds
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let ws_ids = all_workspace_ids(db);
    let workspace_types = ws_ids
        .iter()
        .map(|&ws_id| (ws_id, build_workspace_type_index(db, ws_id)))
        .collect::<HashMap<_, _>>();
    let workspace_members = ws_ids
        .iter()
        .map(|&ws_id| (ws_id, build_workspace_member_index(db, ws_id)))
        .collect::<HashMap<_, _>>();
    let (owner_members, owner_members_named) = build_member_aggregates(ws_ids, &workspace_members);
    let workspace_decls = ws_ids
        .iter()
        .map(|&ws_id| (ws_id, build_workspace_decl_index(db, ws_id)))
        .collect::<HashMap<_, _>>();
    let workspace_modules = ws_ids
        .iter()
        .map(|&ws_id| (ws_id, build_workspace_module_index(db, ws_id)))
        .collect::<HashMap<_, _>>();
    let workspace_deprecated = ws_ids
        .iter()
        .map(|&ws_id| (ws_id, build_workspace_deprecated_index(db, ws_id)))
        .collect::<HashMap<_, _>>();
    let global_types = build_global_type_aggregate(ws_ids, &workspace_types);
    db.workspace_index = WorkspaceIndexCache {
        types: workspace_types,
        global_types,
        members: workspace_members,
        owner_members,
        owner_members_named,
        decls: workspace_decls,
        modules: workspace_modules,
        deprecated: workspace_deprecated,
        references: HashMap::new(),
    };
}

/// P7: canonical export-surface delta.
///
/// Pure value edits (`M.x = 1` -> `M.x = 2`) produce no key because the member
/// identity is unchanged; consumers read the new value by id.
fn rebuild_workspace_reference_indexes(db: &mut SemanticDatabase) {
    let ws_ids = all_workspace_ids(db);
    let workspace_references = ws_ids
        .iter()
        .map(|&ws_id| (ws_id, build_workspace_reference_index(db, ws_id)))
        .collect::<HashMap<_, _>>();
    db.workspace_index.references = workspace_references;
}
pub(crate) fn rebuild_dependency_index(db: &mut SemanticDatabase) {
    db.dependency_index.clear();
    let entries: Vec<(FileId, Arc<FileReferences>)> = db
        .files
        .iter()
        .map(|(file_id, cache)| (*file_id, Arc::clone(&cache.references)))
        .collect();
    for (file_id, references) in entries {
        add_file_dependencies(db, file_id, &references.deps);
    }
}

fn add_file_dependencies(db: &mut SemanticDatabase, file_id: FileId, deps: &FileDependencies) {
    for key in deps.iter() {
        db.dependency_index
            .entry(key.clone())
            .or_default()
            .insert(file_id);
    }
}

fn remove_file_dependencies(db: &mut SemanticDatabase, file_id: FileId, deps: &FileDependencies) {
    for key in deps.iter() {
        if let Some(files) = db.dependency_index.get_mut(key) {
            files.remove(&file_id);
            if files.is_empty() {
                db.dependency_index.remove(key);
            }
        }
    }
}

/// P7: update the changed file's contribution, then refresh only files whose
/// canonical dependency keys intersect `changed_keys`.
pub(crate) fn refresh_dependent_references(
    db: &mut SemanticDatabase,
    changed_file_id: FileId,
    old_exports: Option<&FileExports>,
    new_exports: Option<&FileExports>,
    old_references: Option<&FileReferences>,
    new_references: Option<&FileReferences>,
    extra_changed: ChangedKeys,
    force_module_changed: bool,
    force_module_contributions: bool,
) {
    let mut changed = changed_keys(old_exports, new_exports);
    changed.extend(extra_changed);
    if force_module_changed {
        changed.insert(DependencyKey::Module(changed_file_id));
    }

    if let Some(references) = old_references {
        remove_file_dependencies(db, changed_file_id, &references.deps);
    }
    if let Some(references) = new_references {
        add_file_dependencies(db, changed_file_id, &references.deps);
    }

    if changed.is_empty() {
        return;
    }

    let mut processed_references: HashSet<FileId> = HashSet::new();
    let mut processed_contributions: HashSet<FileId> = HashSet::new();
    let mut pending: Vec<DependencyKey> = changed.into_iter().collect();

    while let Some(key) = pending.pop() {
        let needs_contribution = match &key {
            DependencyKey::ModuleName(_) => true,
            DependencyKey::Module(_) => force_module_contributions,
            _ => false,
        };
        let Some(targets) = db
            .dependency_index
            .get(&key)
            .map(|files| files.iter().copied().collect::<Vec<_>>())
        else {
            continue;
        };
        for file_id in targets {
            if file_id == changed_file_id {
                continue;
            }
            if needs_contribution {
                if processed_contributions.insert(file_id) {
                    processed_references.insert(file_id);
                    let nested = refresh_file_contribution_and_references(db, file_id);
                    if !nested.is_empty() {
                        pending.extend(nested);
                    }
                }
            } else if processed_references.insert(file_id) {
                refresh_file_reference_index(db, file_id);
            }
        }
    }
}

fn refresh_file_reference_index(db: &mut SemanticDatabase, file_id: FileId) {
    if db.file_data_id(file_id).is_none() {
        return;
    }
    #[cfg(test)]
    db.rebuild_metrics
        .dependent_reference_refreshes
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let old_references = db
        .files
        .get(&file_id)
        .map(|cache| Arc::clone(&cache.references));
    let references = Arc::new(build_file_references(db, file_id));
    if let Some(old_references) = &old_references {
        remove_file_dependencies(db, file_id, &old_references.deps);
    }
    add_file_dependencies(db, file_id, &references.deps);

    if let Some(cache) = db.files.get_mut(&file_id) {
        cache.references = Arc::clone(&references);
    }
    let ws_id = file_workspace_id(db, file_id).unwrap_or(WorkspaceId::REMOTE);
    let index = db.workspace_index.references.entry(ws_id).or_default();
    index.remove_file(file_id);
    index.add_file(file_id, references);
}

/// Rebuild one file export contribution and references when a module
/// dependency changed such that its own require aliases or canonical owner
/// mapping may now resolve differently. Facts and flow are unchanged.
fn refresh_file_contribution_and_references(
    db: &mut SemanticDatabase,
    file_id: FileId,
) -> ChangedKeys {
    if db.file_data_id(file_id).is_none() {
        return ChangedKeys::default();
    }
    let Some(old_exports) = db
        .files
        .get(&file_id)
        .map(|cache| Arc::clone(&cache.exports))
    else {
        return ChangedKeys::default();
    };
    let Some(old_references) = db
        .files
        .get(&file_id)
        .map(|cache| Arc::clone(&cache.references))
    else {
        return ChangedKeys::default();
    };
    let old_workspace = db.files.get(&file_id).and_then(|cache| cache.workspace_id);

    #[cfg(test)]
    db.rebuild_metrics
        .dependent_contribution_refreshes
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let module_entry = build_module_entry(db, file_id);
    let deprecated = build_deprecated_file(db, file_id);
    let exports = Arc::new(build_file_exports(db, file_id, file_id));
    apply_file_to_workspace_indexes(
        db,
        WorkspaceFileUpdate {
            file_id,
            old_workspace,
            old_exports: Some(old_exports.as_ref()),
            new_exports: Some(exports.as_ref()),
            new_references: None,
            module_entry,
            new_deprecated: Some(&deprecated),
        },
    );
    if let Some(cache) = db.files.get_mut(&file_id) {
        cache.exports = Arc::clone(&exports);
    }
    let references = Arc::new(build_file_references(db, file_id));
    if let Some(cache) = db.files.get_mut(&file_id) {
        cache.references = Arc::clone(&references);
    }
    apply_file_reference_index(db, file_id, Arc::clone(&references));

    remove_file_dependencies(db, file_id, &old_references.deps);
    add_file_dependencies(db, file_id, &references.deps);

    changed_keys(Some(old_exports.as_ref()), Some(exports.as_ref()))
}
/// Canonical ModuleName keys published when a file module entry changes.
///
/// Both the full module name and the last path segment are published: the
/// latter is what a consumer fuzzy require dependency records.
fn module_name_changed_keys(old: Option<&ModuleEntry>, new: Option<&ModuleEntry>) -> ChangedKeys {
    let mut keys = ChangedKeys::default();
    if old == new {
        return keys;
    }
    let mut names: HashSet<SmolStr> = HashSet::new();
    for entry in [old, new].into_iter().flatten() {
        names.insert(entry.full_module_name.clone());
        if entry.name != entry.full_module_name {
            names.insert(entry.name.clone());
        }
    }
    for name in names {
        keys.insert(DependencyKey::ModuleName(name));
    }
    keys
}

pub(crate) fn refresh_global_type_keys(db: &mut SemanticDatabase, names: HashSet<SmolStr>) {
    if names.is_empty() {
        return;
    }
    let ws_ids = db.all_workspace_ids_arc();
    let types = &db.workspace_index.types;
    let global_types = &mut db.workspace_index.global_types;

    for name in names {
        let mut single: Option<Arc<[TypeDef]>> = None;
        let mut merged: Option<Vec<TypeDef>> = None;
        for &ws_id in ws_ids.iter() {
            let Some(index) = types.get(&ws_id) else {
                continue;
            };
            let bucket = index.find_all(TypeScope::Global, &name);
            if bucket.is_empty() {
                continue;
            }
            match &mut merged {
                Some(out) => out.extend(bucket.iter().cloned()),
                None => {
                    if let Some(first) = single.take() {
                        let mut out = Vec::with_capacity(first.len() + bucket.len());
                        out.extend(first.iter().cloned());
                        out.extend(bucket.iter().cloned());
                        merged = Some(out);
                    } else {
                        single = Some(bucket);
                    }
                }
            }
        }
        match merged {
            Some(out) => {
                global_types.insert(name, Arc::from(out));
            }
            None => match single {
                Some(bucket) => {
                    global_types.insert(name, bucket);
                }
                None => {
                    global_types.remove(&name);
                }
            },
        }
    }
}
pub(crate) fn refresh_owner_member_keys(
    db: &mut SemanticDatabase,
    owners: HashSet<SemanticId>,
    names: HashSet<(SemanticId, SmolStr)>,
) {
    if owners.is_empty() && names.is_empty() {
        return;
    }
    let ws_ids = db.all_workspace_ids_arc();
    let members = &db.workspace_index.members;
    {
        let owner_members = &mut db.workspace_index.owner_members;
        for owner in owners {
            match aggregate_member_bucket(&ws_ids, members, &owner, None) {
                Some(bucket) => {
                    owner_members.insert(owner, bucket);
                }
                None => {
                    owner_members.remove(&owner);
                }
            }
        }
    }
    {
        let owner_members_named = &mut db.workspace_index.owner_members_named;
        for key in names {
            match aggregate_member_bucket(&ws_ids, members, &key.0, Some(key.1.as_str())) {
                Some(bucket) => {
                    owner_members_named.insert(key, bucket);
                }
                None => {
                    owner_members_named.remove(&key);
                }
            }
        }
    }
}

/// Explicit single-file mutation.
#[derive(Debug, Clone)]
pub enum FileChange {
    Set {
        file_id: FileId,
        path: Option<std::path::PathBuf>,
        uri: Option<lsp_types::Uri>,
        text: String,
    },
    Remove {
        file_id: FileId,
    },
}

impl FileChange {
    pub fn set_file(file_id: FileId, path: Option<std::path::PathBuf>, text: String) -> Self {
        let uri = path.as_ref().and_then(file_path_to_uri);
        FileChange::Set {
            file_id,
            path,
            uri,
            text,
        }
    }

    pub fn remove(file_id: FileId) -> Self {
        FileChange::Remove { file_id }
    }
}

/// Batch update using URI + optional text (`None` removes the file).
#[derive(Debug, Clone, Default)]
pub struct BatchChange {
    pub files: Vec<(lsp_types::Uri, Option<String>)>,
}

/// Result summary of one mutation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UpdateSummary {
    pub updated: usize,
    pub removed: usize,
    pub full_rebuild: bool,
}

impl SemanticDatabase {
    pub fn apply_file_change(&mut self, change: FileChange) -> UpdateSummary {
        match change {
            FileChange::Set {
                file_id,
                path,
                uri,
                text,
            } => {
                self.set_file_inner(file_id, path, uri, text);
                UpdateSummary {
                    updated: 1,
                    ..UpdateSummary::default()
                }
            }
            FileChange::Remove { file_id } => {
                self.remove_file_inner(file_id);
                UpdateSummary {
                    removed: 1,
                    ..UpdateSummary::default()
                }
            }
        }
    }

    pub fn apply_batch(&mut self, batch: BatchChange) -> UpdateSummary {
        self.apply_batch_with_ids(batch).0
    }

    /// Apply a batch and return the affected file ids in input order.
    pub fn apply_batch_with_ids(&mut self, batch: BatchChange) -> (UpdateSummary, Vec<FileId>) {
        let updated = batch.files.len();
        let file_ids = self.set_files(batch.files);
        (
            UpdateSummary {
                updated,
                removed: 0,
                full_rebuild: true,
            },
            file_ids,
        )
    }
}
