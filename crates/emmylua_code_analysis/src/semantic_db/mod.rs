pub(crate) mod def;
pub(crate) mod exports;
pub(crate) mod facade;
pub(crate) mod facts;

#[cfg(test)]
mod phase_tests;

pub(crate) mod flow;
pub(crate) mod index;
pub(crate) mod inputs;

pub(crate) mod query;
#[cfg(test)]
mod tests;
pub(crate) mod types;
pub(crate) mod update;

use hashbrown::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use emmylua_parser::{LineIndex, LuaVersionNumber};
use lsp_types::Uri;

use crate::vfs::file_path_to_uri;
use crate::vfs::{LuaDocument, Vfs};
use crate::{
    Emmyrc, FileData, FileId, SemanticModel, WorkspaceFolder, WorkspaceImport, uri_to_file_path,
};
pub use def::*;
use inputs::{WorkspaceRoot, language_level_to_version};

pub(crate) use facade::{AnalysisView, FileView};
pub use facade::{MemberList, TypeDefList};
pub use update::{BatchChange, FileChange, UpdateSummary};

pub(crate) struct FileCache {
    facts: facts::FileFacts,
    flow: flow::FlowTree,
    exports: Arc<exports::FileExportContribution>,
    references: Arc<query::FileReferences>,
    /// Per-file module entry, replacing ModuleShard lookup during indexing.
    pub(crate) module_entry: Option<query::ModuleEntry>,
    /// Per-file deprecated facts, replacing DeprecatedShard lookup.
    pub(crate) deprecated: Arc<query::DeprecatedFileData>,
    /// Cached file -> owning workspace, so shard/index scans do not strip paths
    /// against all roots on every lookup.
    pub(crate) workspace_id: Option<WorkspaceId>,
}

/// Test-only counters used by the P0/P1 incremental-index baseline tests.
///
/// They let tests assert that a single-file edit does not fall back to a full
/// workspace rebuild. Production builds do not contain this struct.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct RebuildMetrics {
    pub(crate) full_rebuilds: std::sync::atomic::AtomicU32,
    pub(crate) workspace_index_rebuilds: std::sync::atomic::AtomicU32,
    /// Number of full workspace-index source scans. Each workspace index
    /// builder increments it once; the incremental update path must keep it at 0.
    pub(crate) full_index_source_scans: std::sync::atomic::AtomicU32,
    /// Number of affected files whose reference index was refreshed by a
    /// canonical changed-key intersection (P7).
    pub(crate) dependent_reference_refreshes: std::sync::atomic::AtomicU32,
    /// Number of ModuleIndex::rebuild_derived invocations on the incremental write path.
    /// Must stay 0 for edits that do not change a file module entry (for example value-only edits).
    pub(crate) module_derived_rebuilds: std::sync::atomic::AtomicU32,
    /// Number of dependent files whose export contribution and references
    /// were rebuilt because a module dependency changed.
    pub(crate) dependent_contribution_refreshes: std::sync::atomic::AtomicU32,
}

#[cfg(test)]
impl RebuildMetrics {
    pub(crate) fn reset(&self) {
        use std::sync::atomic::Ordering;
        self.full_rebuilds.store(0, Ordering::Relaxed);
        self.workspace_index_rebuilds.store(0, Ordering::Relaxed);
        self.full_index_source_scans.store(0, Ordering::Relaxed);
        self.dependent_reference_refreshes
            .store(0, Ordering::Relaxed);
        self.module_derived_rebuilds.store(0, Ordering::Relaxed);
        self.dependent_contribution_refreshes
            .store(0, Ordering::Relaxed);
    }

    pub(crate) fn full_rebuilds(&self) -> u32 {
        self.full_rebuilds
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn workspace_index_rebuilds(&self) -> u32 {
        self.workspace_index_rebuilds
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn full_index_source_scans(&self) -> u32 {
        self.full_index_source_scans
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn dependent_reference_refreshes(&self) -> u32 {
        self.dependent_reference_refreshes
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn module_derived_rebuilds(&self) -> u32 {
        self.module_derived_rebuilds
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn dependent_contribution_refreshes(&self) -> u32 {
        self.dependent_contribution_refreshes
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

pub struct SemanticDatabase {
    // ── Plain config/data ──
    config: Option<Arc<Emmyrc>>,
    /// Main workspace root (used for require module name fallback).
    main_root: Option<PathBuf>,
    /// Registered workspace roots.
    workspace_roots: Arc<[WorkspaceRoot]>,

    /// Cached workspace ids derived from `workspace_roots` (always includes
    /// `REMOTE`, or `MAIN` when no roots are registered).
    ///
    /// Refreshed only when roots change, so hot query paths do not allocate a
    /// fresh `Vec`/sort on every call.
    all_workspace_ids: Arc<[WorkspaceId]>,
    /// Cached lookup order derived from `all_workspace_ids`
    /// (main -> library -> std -> remote; library registration order stays stable).
    workspace_lookup_order: Arc<[WorkspaceId]>,

    /// Plain VFS state independent of workspace/config inputs.
    vfs: Vfs,

    /// Per-file caches, kept together so file invalidation is one entry update.
    files: HashMap<FileId, FileCache>,

    /// Cached fallback root for module-name derivation when no workspace roots
    /// are registered. Recomputed only on full rebuilds.
    module_fallback_root: Option<PathBuf>,

    /// Plain merged workspace indexes (type/member/decl/module/reference).
    workspace_index: query::WorkspaceIndexCache,

    /// Reverse identity-level dependency index (P7): key -> files that read it.
    dependency_index: HashMap<DependencyKey, HashSet<FileId>>,

    #[cfg(test)]
    pub(crate) rebuild_metrics: RebuildMetrics,
}

impl Default for SemanticDatabase {
    fn default() -> Self {
        Self {
            config: None,
            main_root: None,
            workspace_roots: Arc::from(Vec::<WorkspaceRoot>::new()),
            all_workspace_ids: Arc::from(vec![WorkspaceId::MAIN, WorkspaceId::REMOTE]),
            workspace_lookup_order: Arc::from(vec![WorkspaceId::MAIN, WorkspaceId::REMOTE]),
            vfs: Vfs::new(),
            files: HashMap::new(),
            module_fallback_root: None,
            workspace_index: query::WorkspaceIndexCache::new(),
            dependency_index: HashMap::new(),
            #[cfg(test)]
            rebuild_metrics: RebuildMetrics::default(),
        }
    }
}

impl SemanticDatabase {
    pub(crate) fn file_data_id(&self, file_id: FileId) -> Option<FileId> {
        self.vfs.file(file_id).map(|_| file_id)
    }

    pub(crate) fn file_data(&self, file_id: FileId) -> Option<&FileData> {
        self.vfs.file(file_id)
    }

    pub(crate) fn workspace_roots(&self) -> &Arc<[WorkspaceRoot]> {
        &self.workspace_roots
    }

    /// Recompute the module-name fallback root after a file add/remove.
    ///
    /// With registered workspace roots the fallback is unused. Otherwise the
    /// fallback is either the explicit `main_root` or the common parent of all
    /// files. File add/remove is rare compared to file edits; edits never call
    /// this because the file id is already registered.
    fn refresh_module_fallback_root(&mut self) {
        if !self.workspace_roots.is_empty() {
            self.module_fallback_root = None;
            return;
        }
        if let Some(root) = &self.main_root {
            self.module_fallback_root = Some(root.clone());
            return;
        }
        let paths: Vec<PathBuf> = self
            .vfs
            .file_ids()
            .into_iter()
            .filter_map(|file_id| self.vfs.file(file_id))
            .filter_map(|file| file.path.clone())
            .collect();
        self.module_fallback_root = query::common_path_root(&paths);
    }

    pub(crate) fn module_fallback_root(&self) -> Option<PathBuf> {
        self.module_fallback_root.clone()
    }

    pub(crate) fn file_cache(&self, file_id: FileId) -> Option<&FileCache> {
        self.files.get(&file_id)
    }

    pub(crate) fn file_facts_of(&self, file_id: FileId) -> Option<&facts::FileFacts> {
        self.file_cache(file_id).map(|cache| &cache.facts)
    }
    pub(crate) fn flow_tree_of(&self, file_id: FileId) -> &flow::FlowTree {
        &self
            .file_cache(file_id)
            .expect("flow tree must be built before read")
            .flow
    }

    pub(crate) fn file_exports_of(&self, file_id: FileId) -> &exports::FileExportContribution {
        self.file_cache(file_id)
            .expect("file exports must be built before read")
            .exports
            .as_ref()
    }

    #[cfg(test)]
    pub(crate) fn file_module_entry_of(&self, file_id: FileId) -> Option<&query::ModuleEntry> {
        self.file_cache(file_id)
            .and_then(|cache| cache.module_entry.as_ref())
    }

    #[cfg(test)]
    pub(crate) fn file_deprecated_of(&self, file_id: FileId) -> Option<&query::DeprecatedFileData> {
        self.file_cache(file_id)
            .map(|cache| cache.deprecated.as_ref())
    }

    pub(crate) fn workspace_index_cache(&self) -> &query::WorkspaceIndexCache {
        &self.workspace_index
    }
}
fn compute_workspace_ids(roots: &[WorkspaceRoot]) -> Vec<WorkspaceId> {
    let mut ids: Vec<WorkspaceId> = if roots.is_empty() {
        vec![WorkspaceId::MAIN]
    } else {
        roots.iter().map(|root| root.id).collect()
    };
    if !ids.contains(&WorkspaceId::REMOTE) {
        ids.push(WorkspaceId::REMOTE);
    }
    ids
}

fn compute_workspace_lookup_order(ids: &[WorkspaceId]) -> Vec<WorkspaceId> {
    let mut ordered = ids.to_vec();
    ordered.sort_by_key(|ws_id| {
        if ws_id.is_main() {
            0
        } else if ws_id.is_library() {
            1
        } else if ws_id.is_std() {
            2
        } else {
            3
        }
    });
    ordered
}

impl fmt::Debug for SemanticDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SemanticDatabase")
            .field("file_count", &self.vfs.len())
            .field("has_config", &self.config.is_some())
            .finish()
    }
}

impl SemanticDatabase {
    pub fn new() -> Self {
        Self::default()
    }

    fn set_workspace_roots(&mut self, roots: Arc<[WorkspaceRoot]>) {
        self.workspace_roots = roots;
        self.refresh_workspace_id_caches();
    }

    /// Recompute the cached workspace id list / lookup order.
    fn refresh_workspace_id_caches(&mut self) {
        let ids = compute_workspace_ids(&self.workspace_roots);
        self.workspace_lookup_order = Arc::from(compute_workspace_lookup_order(&ids));
        self.all_workspace_ids = Arc::from(ids);
    }

    pub(crate) fn all_workspace_ids(&self) -> &[WorkspaceId] {
        &self.all_workspace_ids
    }

    /// Shared handle to the cached workspace id list, used by mutation paths that
    /// need to read the list while mutating another field of the database.
    pub(crate) fn all_workspace_ids_arc(&self) -> Arc<[WorkspaceId]> {
        Arc::clone(&self.all_workspace_ids)
    }

    pub(crate) fn workspace_lookup_order(&self) -> &[WorkspaceId] {
        &self.workspace_lookup_order
    }

    /// Rebuild every per-file cache and shard cache from the current inputs.
    ///
    /// All reads are pure map lookups after this; writes are the only place where
    /// caches are populated.
    fn rebuild_all_caches(&mut self) {
        update::rebuild_all_caches(self);
    }

    fn rebuild_file_after_write(
        &mut self,
        file_id: FileId,
        old_workspace: Option<WorkspaceId>,
        metadata_changed: bool,
        old_module_entry: Option<query::ModuleEntry>,
    ) {
        update::rebuild_file_after_write(
            self,
            file_id,
            old_workspace,
            metadata_changed,
            old_module_entry,
        );
    }

    fn rebuild_file_after_remove(
        &mut self,
        file_id: FileId,
        old_workspace: Option<WorkspaceId>,
        old_exports: Option<Arc<exports::FileExportContribution>>,
        old_references: Option<Arc<query::FileReferences>>,
        old_module_entry: Option<query::ModuleEntry>,
    ) {
        update::rebuild_file_after_remove(
            self,
            file_id,
            old_workspace,
            old_exports,
            old_references,
            old_module_entry,
        );
    }

    fn reset_file_facts_cache(&mut self) {
        self.rebuild_all_caches();
    }

    /// Remove a file from the workspace file list and VFS snapshot.
    fn workspace_remove_file(&mut self, file_id: FileId) {
        let old_workspace = self.workspace_id_of(file_id);
        let old_module_entry = if self.file_facts_of(file_id).is_some() {
            query::build_module_entry(self, file_id)
        } else {
            None
        };
        let (old_exports, old_references) = match self.files.remove(&file_id) {
            Some(cache) => (Some(cache.exports), Some(cache.references)),
            None => (None, None),
        };
        self.vfs.remove(file_id);
        self.rebuild_file_after_remove(
            file_id,
            old_workspace,
            old_exports,
            old_references,
            old_module_entry,
        );
    }

    // ---- Config ----

    pub fn update_config(&mut self, emmyrc: Arc<Emmyrc>) {
        self.vfs.update_config(emmyrc.clone());
        self.config = Some(emmyrc);
        self.reset_file_facts_cache();
    }

    /// Main workspace root (used for require module name derivation).
    pub fn update_main_root(&mut self, root: PathBuf) {
        self.main_root = Some(root);
        // The fallback root participates in module-entry derivation, so rebuild
        // module shards/indexes instead of leaving cached entries stale.
        self.reset_file_facts_cache();
    }

    pub fn main_root(&self) -> Option<PathBuf> {
        self.main_root.clone()
    }

    pub(crate) fn strict_array_index(&self) -> bool {
        self.config
            .as_ref()
            .map(|config| config.strict.array_index)
            .unwrap_or(true)
    }

    /// Run `f` for every workspace file on scoped worker threads.
    ///
    /// Each worker creates its own `SemanticModel` view over the shared read-only
    /// `SemanticDatabase`; `f` must be `Sync` because it is invoked concurrently.
    pub fn parallel_for_each_file<F>(&self, f: F)
    where
        F: Fn(FileId, &SemanticModel<'_>) + Sync,
    {
        let file_ids: Vec<FileId> = self.file_ids().to_vec();
        std::thread::scope(|scope| {
            for file_id in file_ids {
                let f = &f;
                scope.spawn(move || {
                    let model = SemanticModel::new(self, file_id);
                    f(file_id, &model);
                });
            }
        });
    }

    /// Register the built-in std workspace root.
    pub fn add_std_workspace(&mut self, root: PathBuf) {
        let mut roots = self.workspace_roots.to_vec();
        roots.retain(|root_entry| !root_entry.id.is_std());
        roots.push(WorkspaceRoot {
            id: WorkspaceId::STD,
            root,
            import: WorkspaceImport::All,
        });
        self.set_workspace_roots(Arc::from(roots));
        self.reset_file_facts_cache();
    }

    /// Register or replace the main workspace root.
    pub fn add_main_workspace(&mut self, root: PathBuf) {
        self.main_root = Some(root.clone());
        let mut roots = self.workspace_roots.to_vec();
        roots.retain(|root_entry| !root_entry.id.is_main());
        roots.push(WorkspaceRoot {
            id: WorkspaceId::MAIN,
            root,
            import: WorkspaceImport::All,
        });
        self.set_workspace_roots(Arc::from(roots));
        self.reset_file_facts_cache();
    }

    /// Register a library workspace (allocates a new `WorkspaceId`).
    pub fn add_library_workspace(&mut self, workspace: &WorkspaceFolder) {
        let mut roots = self.workspace_roots.to_vec();
        let id = WorkspaceId {
            id: self.next_library_workspace_id(&roots),
        };
        roots.push(WorkspaceRoot {
            id,
            root: workspace.root.clone(),
            import: workspace.import.clone(),
        });
        self.set_workspace_roots(Arc::from(roots));
        self.reset_file_facts_cache();
    }

    /// Keep only the std workspace (clear main/library before reload).
    pub fn clear_non_std_workspaces(&mut self) {
        let roots: Vec<WorkspaceRoot> = self
            .workspace_roots
            .iter()
            .filter(|root_entry| root_entry.id.is_std())
            .cloned()
            .collect();
        self.set_workspace_roots(Arc::from(roots));
        self.reset_file_facts_cache();
    }

    fn next_library_workspace_id(&self, roots: &[WorkspaceRoot]) -> u32 {
        let used: HashSet<u32> = roots.iter().map(|root_entry| root_entry.id.id).collect();
        let mut candidate = WorkspaceId::LIBRARY_START.id;
        while used.contains(&candidate) {
            candidate += 1;
        }
        candidate
    }

    /// Add paths that workspace reload must preserve (e.g. bundled std lib).
    pub fn add_protected_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        let mut set = self.vfs.protected_paths().clone();
        set.extend(paths);
        self.vfs.set_protected_paths(set);
    }

    /// Currently protected paths (loaded std lib etc.).
    pub fn protected_paths(&self) -> &HashSet<PathBuf> {
        self.vfs.protected_paths()
    }

    /// Current configured runtime version (used for `---@version` visibility).
    pub fn lua_version(&self) -> Option<LuaVersionNumber> {
        self.config
            .as_ref()
            .map(|config| language_level_to_version(config.get_language_level()))
    }

    // ---- URI / FileId mapping ----

    pub(crate) fn allocate_file_id(&mut self) -> FileId {
        self.vfs.allocate_file_id()
    }

    pub fn lookup_file_id(&self, uri: &Uri) -> Option<FileId> {
        if let Some(path) = uri_to_file_path(uri)
            && let Some(id) = self.vfs.lookup_by_path(&path)
        {
            return Some(id);
        }
        self.vfs.lookup_by_uri(uri)
    }

    pub fn file_uri(&self, file_id: FileId) -> Option<Uri> {
        self.vfs.file(file_id).and_then(|file| file.uri.clone())
    }

    pub fn file_path(&self, file_id: FileId) -> Option<PathBuf> {
        self.vfs.file(file_id).and_then(|file| file.path.clone())
    }

    // ---- File management ----

    /// Update many files and rebuild derived caches once. Used for workspace
    /// loading / batch watcher updates to avoid O(N^2) incremental rebuilds.
    pub(crate) fn set_files(&mut self, files: Vec<(Uri, Option<String>)>) -> Vec<FileId> {
        let mut file_ids = Vec::with_capacity(files.len());
        for (uri, text) in files {
            let file_id = self
                .lookup_file_id(&uri)
                .unwrap_or_else(|| self.vfs.allocate_file_id());
            if let Some(text) = text {
                let path = uri_to_file_path(&uri);
                self.vfs.insert_at(file_id, Some(uri), path, text);
            } else {
                self.vfs.remove(file_id);
            }
            file_ids.push(file_id);
        }
        if !file_ids.is_empty() {
            self.rebuild_all_caches();
        }
        file_ids
    }

    pub fn set_file_content(&mut self, uri: &Uri, text: Option<String>) -> FileId {
        let fid = self
            .lookup_file_id(uri)
            .unwrap_or_else(|| self.vfs.allocate_file_id());
        if let Some(text) = text {
            let path = uri_to_file_path(uri);
            self.set_file_inner(fid, path, Some(uri.clone()), text);
        } else {
            self.remove_file_inner(fid);
        }
        fid
    }

    pub fn set_file(&mut self, file_id: FileId, path: Option<PathBuf>, text: String) {
        let uri = path.as_ref().and_then(file_path_to_uri);
        self.set_file_inner(file_id, path, uri, text);
    }

    fn set_file_inner(
        &mut self,
        file_id: FileId,
        path: Option<PathBuf>,
        uri: Option<Uri>,
        text: String,
    ) {
        let old_path = self.vfs.file(file_id).and_then(|file| file.path.clone());
        let old_uri = self.vfs.file(file_id).and_then(|file| file.uri.clone());
        let old_workspace = self.workspace_id_of(file_id);
        let old_module_entry = if self.file_facts_of(file_id).is_some() {
            query::build_module_entry(self, file_id)
        } else {
            None
        };
        let metadata_changed = old_path != path || old_uri != uri;
        self.vfs.insert_at(file_id, uri, path, text);
        self.rebuild_file_after_write(file_id, old_workspace, metadata_changed, old_module_entry);
    }

    /// Reload the workspace file set while preserving existing FileIds and protected files.
    ///
    /// This updates VFS in place and rebuilds derived caches once, instead of cloning
    /// the whole `FileData` table and rebuilding VFS from scratch.
    pub(crate) fn reload_workspace_files(
        &mut self,
        files: Vec<(PathBuf, Option<String>)>,
        open_files: Vec<(Uri, String)>,
    ) -> Vec<Uri> {
        let open_paths: HashSet<PathBuf> = open_files
            .iter()
            .filter_map(|(uri, _)| uri_to_file_path(uri))
            .collect();
        let mut kept_paths = open_paths.clone();
        kept_paths.extend(files.iter().map(|(path, _)| path.clone()));
        kept_paths.extend(self.vfs.protected_paths().iter().cloned());

        let old_entries: Vec<(FileId, Option<PathBuf>, Option<Uri>)> = self
            .vfs
            .file_ids()
            .into_iter()
            .filter_map(|file_id| {
                self.vfs
                    .file(file_id)
                    .map(|file| (file_id, file.path.clone(), file.uri.clone()))
            })
            .collect();

        let stale_uris: Vec<Uri> = old_entries
            .iter()
            .filter_map(|(_, path, _)| {
                let path = path.as_ref()?;
                if kept_paths.contains(path) {
                    None
                } else {
                    file_path_to_uri(path)
                }
            })
            .collect();

        let mut removals: Vec<FileId> = old_entries
            .iter()
            .filter_map(|(file_id, path, _)| {
                let path = path.as_ref()?;
                (!kept_paths.contains(path)).then_some(*file_id)
            })
            .collect();

        let mut path_to_id: HashMap<PathBuf, FileId> = old_entries
            .iter()
            .filter_map(|(file_id, path, _)| path.clone().map(|path| (path, *file_id)))
            .collect();

        let mut inserts: Vec<(FileId, Option<PathBuf>, Option<Uri>, String)> = Vec::new();
        for (path, text) in files
            .into_iter()
            .filter(|(path, _)| !open_paths.contains(path))
        {
            let uri = file_path_to_uri(&path);
            let file_id = path_to_id
                .get(&path)
                .copied()
                .unwrap_or_else(|| self.vfs.allocate_file_id());
            if let Some(text) = text {
                path_to_id.insert(path.clone(), file_id);
                inserts.push((file_id, Some(path), uri, text));
            } else {
                removals.push(file_id);
                path_to_id.remove(&path);
            }
        }

        for (uri, text) in open_files {
            let path = uri_to_file_path(&uri);
            let file_id = self
                .lookup_file_id(&uri)
                .or_else(|| path.as_ref().and_then(|path| path_to_id.get(path).copied()))
                .unwrap_or_else(|| self.vfs.allocate_file_id());
            if let Some(path) = &path {
                path_to_id.insert(path.clone(), file_id);
            }
            inserts.push((file_id, path, Some(uri), text));
        }

        let changed = !removals.is_empty() || !inserts.is_empty();
        for file_id in removals {
            self.vfs.remove(file_id);
        }
        for (file_id, path, uri, text) in inserts {
            self.vfs.insert_at(file_id, uri, path, text);
        }
        if changed {
            self.rebuild_all_caches();
        }

        stale_uris
    }

    pub fn remove_file(&mut self, file_id: FileId) {
        self.remove_file_inner(file_id);
    }

    fn remove_file_inner(&mut self, file_id: FileId) {
        self.workspace_remove_file(file_id);
    }

    pub fn clear(&mut self) {
        self.workspace_roots = Arc::from(Vec::<WorkspaceRoot>::new());
        self.all_workspace_ids = Arc::from(vec![WorkspaceId::MAIN, WorkspaceId::REMOTE]);
        self.workspace_lookup_order = Arc::from(vec![WorkspaceId::MAIN, WorkspaceId::REMOTE]);
        self.main_root = None;
        self.vfs = Vfs::new();
        self.files = HashMap::new();

        self.module_fallback_root = None;
        self.workspace_index = query::WorkspaceIndexCache::new();
        self.dependency_index = HashMap::new();
        self.rebuild_all_caches();
    }

    /// Current VFS snapshot (immutable, shareable across threads).
    pub(crate) fn vfs(&self) -> &Vfs {
        &self.vfs
    }

    pub fn file_ids(&self) -> Vec<FileId> {
        self.vfs.file_ids()
    }

    /// Main workspace file list.
    pub fn main_workspace_file_ids(&self) -> Vec<FileId> {
        let has_roots = !self.workspace_roots().is_empty();
        if !has_roots {
            // Keep old behavior when no roots are registered: treat all files as main.
            return self.vfs.file_ids();
        }
        self.vfs
            .file_ids()
            .iter()
            .copied()
            .filter(|&file_id| self.workspace_id_of(file_id).is_some_and(|id| id.is_main()))
            .collect()
    }

    pub fn std_workspace_file_ids(&self) -> Vec<FileId> {
        self.vfs
            .file_ids()
            .iter()
            .copied()
            .filter(|&file_id| self.workspace_id_of(file_id).is_some_and(|id| id.is_std()))
            .collect()
    }

    pub fn library_workspace_file_ids(&self) -> Vec<FileId> {
        self.vfs
            .file_ids()
            .iter()
            .copied()
            .filter(|&file_id| {
                self.workspace_id_of(file_id)
                    .is_some_and(|id| id.is_library())
            })
            .collect()
    }

    pub fn get_file_text(&self, file_id: FileId) -> Option<&str> {
        self.vfs.file(file_id).map(|file| file.text.as_ref())
    }

    /// Per-file line index from the VFS.
    pub fn line_index(&self, file_id: FileId) -> Option<&LineIndex> {
        self.vfs.line_index(file_id)
    }

    /// Document view borrowed from the VFS.
    pub fn document(&self, file_id: FileId) -> Option<LuaDocument<'_>> {
        self.vfs.get_document(&file_id)
    }

    // ── Input accessors (for tracked layer / facade) ──

    pub(crate) fn config_input(&self) -> Option<&Emmyrc> {
        self.config.as_deref()
    }

    // ── Facade ──

    /// Module name → module file (for require resolution and handlers such as document_link).
    pub fn module_file_of(&self, module_name: &str) -> Option<FileId> {
        self.analysis().module_file_of(module_name)
    }

    // ── Reference index ──

    /// All use sites of a declaration (Decl) (cross-file, aggregated through sharded reference index).
    pub fn decl_reference_ranges(&self, decl: &SemanticId) -> Vec<(FileId, rowan::TextRange)> {
        let Some(_config) = self.config_input() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for &ws_id in query::all_workspace_ids(self) {
            let index = query::workspace_reference_index_for(self, ws_id);
            if let Some(ranges) = index.decl_refs.get(decl) {
                out.extend(ranges.iter().copied());
            }
        }
        out
    }

    /// All use sites of a member (Member) (cross-file, aggregated through sharded reference index).
    pub fn member_reference_ranges(&self, member: &SemanticId) -> Vec<(FileId, rowan::TextRange)> {
        let Some(_config) = self.config_input() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for &ws_id in query::all_workspace_ids(self) {
            let index = query::workspace_reference_index_for(self, ws_id);
            if let Some(ranges) = index.member_refs.get(member) {
                out.extend(ranges.iter().copied());
            }
        }
        out
    }

    /// All definition sites of a member (Member) (cross-file, aggregated through sharded reference index).
    pub fn member_definition_ranges(&self, member: &SemanticId) -> Vec<(FileId, rowan::TextRange)> {
        let Some(_config) = self.config_input() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for &ws_id in query::all_workspace_ids(self) {
            let index = query::workspace_reference_index_for(self, ws_id);
            if let Some(ranges) = index.member_defs.get(member) {
                out.extend(ranges.iter().copied());
            }
        }
        out
    }

    /// File → owning workspace id.
    pub fn workspace_id_of(&self, file_id: FileId) -> Option<WorkspaceId> {
        query::file_workspace_id(self, file_id)
    }

    pub fn is_std_file(&self, file_id: FileId) -> bool {
        self.workspace_id_of(file_id).is_some_and(|id| id.is_std())
    }

    pub fn is_main_file(&self, file_id: FileId) -> bool {
        self.workspace_id_of(file_id).is_some_and(|id| id.is_main())
    }

    pub fn is_library_file(&self, file_id: FileId) -> bool {
        self.workspace_id_of(file_id)
            .is_some_and(|id| id.is_library())
    }

    /// File → semantic module info (equivalent to ModuleIndex).
    pub fn module_info_of(&self, file_id: FileId) -> Option<ModuleInfo> {
        let _config = self.config_input()?;
        let ws_id = query::file_workspace_id(self, file_id).unwrap_or(WorkspaceId::REMOTE);
        let index = query::workspace_module_index_for(self, ws_id);
        let mut info = index.module_info(file_id)?;
        if let Some(shell) = self.analysis().module_export_type(file_id) {
            info.export_type = Some(self.analysis().type_shell_lua(file_id, &shell));
        }
        Some(info)
    }

    /// Module path → module tree node id (empty path returns the root node).
    pub fn module_node(&self, module_path: &str) -> Option<ModuleNodeId> {
        let _config = self.config_input()?;
        for &ws_id in query::all_workspace_ids(self) {
            let index = query::workspace_module_index_for(self, ws_id);
            if let Some(node_id) = index.find_module_node(module_path) {
                return Some(node_id);
            }
        }
        None
    }

    /// Module tree node details.
    pub fn module_node_info(&self, node_id: ModuleNodeId) -> Option<ModuleNode> {
        let _config = self.config_input()?;
        let index = query::workspace_module_index_for(self, node_id.workspace_id);
        index.module_node(node_id).cloned()
    }

    /// File id list under a module tree node.
    pub fn module_node_file_ids(&self, node_id: ModuleNodeId) -> Vec<FileId> {
        let Some(_config) = self.config_input() else {
            return Vec::new();
        };
        let index = query::workspace_module_index_for(self, node_id.workspace_id);
        index
            .module_file_ids(node_id)
            .map(|ids| ids.to_vec())
            .unwrap_or_default()
    }

    /// File → module name (relative to owning workspace root, for auto-require).
    pub fn module_name_of(&self, file_id: FileId) -> Option<String> {
        let path = self.file_path(file_id)?;
        self.module_name_from_path(&path)
    }

    /// Path → module name (relative to owning workspace root).
    pub fn module_name_from_path(&self, path: &std::path::Path) -> Option<String> {
        let roots = self.workspace_roots();
        if let Some((_, root)) = query::find_workspace_root(roots.as_ref(), path)
            && let Some(name) = query::module_name_from_path(path, Some(&root))
        {
            return Some(name.to_string());
        }
        let root = self.main_root.as_deref();
        query::module_name_from_path(path, root).map(|name| name.to_string())
    }

    /// Query facade (crate-internal: used by semantic_model and tests).
    pub(crate) fn analysis(&self) -> AnalysisView<'_> {
        AnalysisView::new(self)
    }
}
