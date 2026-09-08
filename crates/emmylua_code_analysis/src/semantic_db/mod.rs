pub(crate) mod def;
pub(crate) mod exports;
pub(crate) mod facade;
pub(crate) mod facts;
pub(crate) mod flow;
pub(crate) mod index;
pub(crate) mod inputs;
pub(crate) mod query;
#[cfg(test)]
mod tests;
pub(crate) mod types;

use hashbrown::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use emmylua_parser::{LineIndex, LuaVersionNumber};
use lsp_types::Uri;

use crate::vfs::file_path_to_uri;
use crate::vfs::{LuaDocument, Vfs};
use crate::{Emmyrc, FileData, FileId, WorkspaceFolder, WorkspaceImport, uri_to_file_path};
pub use def::*;
use inputs::{ConfigInputData, WorkspaceRoot, language_level_to_version};

pub(crate) use facade::SemanticQueries;
pub use facade::{MemberList, TypeDefList};
pub struct SemanticDatabase {
    // ── Plain config/data ──
    config: Option<ConfigInputData>,
    /// Registered workspace roots.
    workspace_roots: Arc<[WorkspaceRoot]>,

    /// Plain VFS state independent of workspace/config inputs.
    vfs: Vfs,

    /// Plain per-file facts cache, built eagerly on every write.
    file_facts: HashMap<FileId, facts::FileFacts>,

    /// Plain per-file control-flow graph cache (same invalidation as `file_facts`).
    flow_trees: HashMap<FileId, flow::FlowTree>,

    /// Plain per-file exports / shard caches.
    file_exports: HashMap<FileId, exports::FileExports>,
    export_shards: HashMap<u8, exports::ExportShard>,

    /// Plain per-file references and remaining shard caches.
    file_references: HashMap<FileId, query::FileReferences>,
    deprecated_shards: HashMap<u8, query::DeprecatedShard>,
    module_shards: HashMap<u8, query::ModuleShard>,
    reference_shards: HashMap<u8, query::ReferenceShard>,

    /// Plain merged workspace indexes (type/member/decl/module/reference).
    workspace_index: query::WorkspaceIndexCache,
}

impl Default for SemanticDatabase {
    fn default() -> Self {
        Self {
            config: None,
            workspace_roots: Arc::from(Vec::<WorkspaceRoot>::new()),
            vfs: Vfs::new(),
            file_facts: HashMap::new(),
            flow_trees: HashMap::new(),
            file_exports: HashMap::new(),
            export_shards: HashMap::new(),
            file_references: HashMap::new(),
            deprecated_shards: HashMap::new(),
            module_shards: HashMap::new(),
            reference_shards: HashMap::new(),
            workspace_index: query::WorkspaceIndexCache::new(),
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

    pub(crate) fn file_facts_map(&self) -> &HashMap<FileId, facts::FileFacts> {
        &self.file_facts
    }

    pub(crate) fn flow_tree_of(&self, file_id: FileId) -> &flow::FlowTree {
        self.flow_trees
            .get(&file_id)
            .expect("flow tree must be built before read")
    }

    pub(crate) fn file_exports_of(&self, file_id: FileId) -> &exports::FileExports {
        self.file_exports
            .get(&file_id)
            .expect("file exports must be built before read")
    }

    pub(crate) fn export_shard_of(&self, shard: u8) -> &exports::ExportShard {
        self.export_shards
            .get(&shard)
            .expect("export shard must be built before read")
    }

    pub(crate) fn file_references_of(&self, file_id: FileId) -> &query::FileReferences {
        self.file_references
            .get(&file_id)
            .expect("file references must be built before read")
    }

    pub(crate) fn deprecated_shard_of(&self, shard: u8) -> &query::DeprecatedShard {
        self.deprecated_shards
            .get(&shard)
            .expect("deprecated shard must be built before read")
    }

    pub(crate) fn module_shard_of(&self, shard: u8) -> &query::ModuleShard {
        self.module_shards
            .get(&shard)
            .expect("module shard must be built before read")
    }

    pub(crate) fn reference_shard_of(&self, shard: u8) -> &query::ReferenceShard {
        self.reference_shards
            .get(&shard)
            .expect("reference shard must be built before read")
    }

    pub(crate) fn workspace_index_cache(&self) -> &query::WorkspaceIndexCache {
        &self.workspace_index
    }
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
    }

    /// Rebuild every per-file cache and shard cache from the current inputs.
    ///
    /// All reads are pure map lookups after this; writes are the only place where
    /// caches are populated.
    fn rebuild_all_caches(&mut self) {
        query::rebuild_all_caches(self);
    }

    fn rebuild_file_after_write(&mut self, file_id: FileId, metadata_changed: bool) {
        query::rebuild_file_after_write(self, file_id, metadata_changed);
    }

    fn rebuild_file_after_remove(&mut self, file_id: FileId) {
        query::rebuild_file_after_remove(self, file_id);
    }

    fn reset_file_facts_cache(&mut self) {
        self.rebuild_all_caches();
    }

    /// Remove a file from the workspace file list and VFS snapshot.
    fn workspace_remove_file(&mut self, file_id: FileId) {
        self.vfs.remove(file_id);
        self.file_facts.remove(&file_id);
        self.flow_trees.remove(&file_id);
        self.file_exports.remove(&file_id);
        self.file_references.remove(&file_id);
        self.rebuild_file_after_remove(file_id);
    }

    // ---- Config ----

    pub fn update_config(&mut self, emmyrc: Arc<Emmyrc>) {
        self.vfs.update_config(emmyrc.clone());
        let (
            language_level,
            special_like,
            non_std_symbols,
            module_patterns,
            module_replace,
            known_doc_tags,
            strict_array_index,
        ) = ConfigInputData::parts_from_emmyrc(&emmyrc);
        self.config = Some(ConfigInputData::new(
            language_level,
            special_like,
            non_std_symbols,
            module_patterns,
            module_replace,
            known_doc_tags,
            strict_array_index,
            None,
        ));
        self.reset_file_facts_cache();
    }

    /// Main workspace root (used for require module name derivation).
    pub fn update_main_root(&mut self, root: PathBuf) {
        if let Some(config) = &self.config {
            let main_root = Some(root);
            self.config = Some(ConfigInputData::new(
                config.language_level,
                config.special_like.clone(),
                config.non_std_symbols.clone(),
                config.module_patterns.clone(),
                config.module_replace.clone(),
                config.known_doc_tags.clone(),
                config.strict_array_index,
                main_root,
            ));
        }
    }

    pub fn main_root(&self) -> Option<PathBuf> {
        self.config
            .as_ref()
            .and_then(|config| config.main_root.clone())
    }

    pub(crate) fn strict_array_index(&self) -> bool {
        self.config
            .as_ref()
            .map(|config| config.strict_array_index)
            .unwrap_or(true)
    }

    /// Run `f` for every workspace file on scoped worker threads.
    ///
    /// Each worker owns its own `SemanticDatabase` clone, sharing the same semantic memo
    /// and the shared high-level semantic cache. `f` must be `Sync` because it is
    /// invoked concurrently from multiple scoped threads.
    pub fn parallel_for_each_file<F>(&self, f: F)
    where
        F: Fn(FileId, &crate::SemanticModel<'_>) + Sync,
    {
        let file_ids: Vec<FileId> = self.file_ids().to_vec();
        std::thread::scope(|scope| {
            for file_id in file_ids {
                let f = &f;
                scope.spawn(move || {
                    if let Some(model) = crate::SemanticModel::new(self, file_id) {
                        f(file_id, &model);
                    }
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
        self.update_main_root(root.clone());
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
            .map(|config| language_level_to_version(config.language_level))
    }

    // ---- URI / FileId mapping ----

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
        let metadata_changed = old_path != path || old_uri != uri;
        self.vfs.insert_at(file_id, uri, path, text);
        self.rebuild_file_after_write(file_id, metadata_changed);
    }

    /// Replace the whole workspace file set in one write.
    pub(crate) fn replace_files(&mut self, file_data_ids: HashMap<FileId, FileData>) {
        let protected_paths = self
            .vfs
            .protected_paths()
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut vfs = Vfs::new();
        if let Some(emmyrc) = self.vfs.emmyrc() {
            vfs.update_config(emmyrc);
        }
        vfs.set_protected_paths(protected_paths);
        for (file_id, input) in file_data_ids {
            let text = input.text.to_string();
            let path = input.path.clone();
            let uri = input.uri.clone();
            vfs.insert_at(file_id, uri, path, text);
        }

        self.vfs = vfs;
        self.rebuild_all_caches();
    }

    /// Current workspace file map (FileId -> source data).
    pub(crate) fn file_data_map(&self) -> HashMap<FileId, FileData> {
        self.vfs
            .files()
            .into_iter()
            .map(|file| (file.file_id, file))
            .collect()
    }

    /// Allocate a fresh FileId.
    pub(crate) fn allocate_file_id(&mut self) -> FileId {
        self.vfs.allocate_file_id()
    }

    pub fn remove_file(&mut self, file_id: FileId) {
        self.remove_file_inner(file_id);
    }

    fn remove_file_inner(&mut self, file_id: FileId) {
        self.workspace_remove_file(file_id);
    }

    pub fn clear(&mut self) {
        self.workspace_roots = Arc::from(Vec::<WorkspaceRoot>::new());
        self.vfs = Vfs::new();
        self.file_facts = HashMap::new();
        self.flow_trees = HashMap::new();
        self.file_exports = HashMap::new();
        self.export_shards = HashMap::new();
        self.file_references = HashMap::new();
        self.deprecated_shards = HashMap::new();
        self.module_shards = HashMap::new();
        self.reference_shards = HashMap::new();
        self.workspace_index = query::WorkspaceIndexCache::new();
        self.rebuild_all_caches();
    }

    /// Current VFS snapshot (immutable, shareable across threads).
    #[allow(dead_code)]
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

    pub(crate) fn config_input(&self) -> Option<&ConfigInputData> {
        self.config.as_ref()
    }

    // ── Facade ──

    /// Module name → module file (for require resolution and handlers such as document_link).
    pub fn module_file_of(&self, module_name: &str) -> Option<FileId> {
        self.q().module_file_of(module_name)
    }

    // ── Reference index ──

    /// All use sites of a declaration (Decl) (cross-file, aggregated through sharded reference index).
    pub fn decl_reference_ranges(&self, decl: &SemanticId) -> Vec<(FileId, rowan::TextRange)> {
        let Some(_config) = self.config_input() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for ws_id in query::all_workspace_ids(self) {
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
        for ws_id in query::all_workspace_ids(self) {
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
        for ws_id in query::all_workspace_ids(self) {
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
        if let Some(shell) = self.q().module_export_type(file_id) {
            info.export_type = Some(self.q().type_shell_lua(file_id, &shell));
        }
        Some(info)
    }

    /// Module path → module tree node id (empty path returns the root node).
    pub fn module_node(&self, module_path: &str) -> Option<ModuleNodeId> {
        let _config = self.config_input()?;
        for ws_id in query::all_workspace_ids(self) {
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
        let roots = self.workspace_roots().to_vec();
        if let Some((_, root)) = query::find_workspace_root(&roots, path)
            && let Some(name) = query::module_name_from_path(path, Some(&root))
        {
            return Some(name.to_string());
        }
        let root = self.config_input()?.main_root().clone();
        query::module_name_from_path(path, root.as_deref()).map(|name| name.to_string())
    }

    /// Query facade (crate-internal: used by semantic_model and tests).
    #[allow(dead_code)]
    pub(crate) fn q(&self) -> SemanticQueries<'_> {
        SemanticQueries::new(self)
    }
}
