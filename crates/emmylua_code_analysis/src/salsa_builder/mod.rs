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
pub(crate) mod vfs_snapshot;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use emmylua_parser::LineIndex;
use lsp_types::Uri;

use crate::semantic_model::cache::SemanticCache;
use crate::vfs::file_path_to_uri;
use crate::{Emmyrc, FileId, WorkspaceFolder, WorkspaceImport};
pub use def::*;
use inputs::{ConfigInput, SourceFileInput, WorkspaceInput, WorkspaceRoot};
use salsa::{Database, Setter};
use vfs_snapshot::{FileEntry, VfsSnapshot};

pub(crate) use facade::SalsaQueries;
pub use facade::{MemberList, TypeDefList};
#[derive(Clone)]
pub struct DocumentView {
    pub file_id: FileId,
    pub path: Option<PathBuf>,
    pub uri: Option<Uri>,
    pub text: Arc<str>,
    pub line_index: Arc<LineIndex>,
}

impl DocumentView {
    pub fn get_text(&self) -> &str {
        &self.text
    }

    pub fn get_line_col(&self, offset: rowan::TextSize) -> Option<(usize, usize)> {
        self.line_index.get_line_col(offset, &self.text)
    }

    pub fn get_offset(&self, line: usize, col: usize) -> Option<rowan::TextSize> {
        self.line_index.get_offset(line, col, &self.text)
    }

    pub fn get_line_count(&self) -> usize {
        self.line_index.line_count()
    }

    pub fn to_lsp_range(&self, range: rowan::TextRange) -> Option<lsp_types::Range> {
        let start = self.get_line_col(range.start())?;
        let end = self.get_line_col(range.end())?;
        Some(lsp_types::Range {
            start: lsp_types::Position {
                line: start.0 as u32,
                character: start.1 as u32,
            },
            end: lsp_types::Position {
                line: end.0 as u32,
                character: end.1 as u32,
            },
        })
    }

    pub fn to_lsp_position(&self, offset: rowan::TextSize) -> Option<lsp_types::Position> {
        let (line, col) = self.get_line_col(offset)?;
        Some(lsp_types::Position {
            line: line as u32,
            character: col as u32,
        })
    }

    pub fn to_rowan_range(&self, range: lsp_types::Range) -> Option<rowan::TextRange> {
        let start = self.get_offset(range.start.line as usize, range.start.character as usize)?;
        let end = self.get_offset(range.end.line as usize, range.end.character as usize)?;
        Some(rowan::TextRange::new(start, end))
    }

    pub fn get_text_slice(&self, range: rowan::TextRange) -> &str {
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        &self.text[start.min(self.text.len())..end.min(self.text.len())]
    }

    pub fn get_line_range(&self, line: usize) -> Option<rowan::TextRange> {
        let start = self.get_offset(line, 0)?;
        let end = if line + 1 < self.get_line_count() {
            self.get_offset(line + 1, 0)?
        } else {
            rowan::TextSize::from(self.text.len() as u32)
        };
        Some(rowan::TextRange::new(start, end))
    }

    pub fn get_document_lsp_range(&self) -> lsp_types::Range {
        lsp_types::Range {
            start: lsp_types::Position {
                line: 0,
                character: 0,
            },
            end: lsp_types::Position {
                line: self.get_line_count() as u32,
                character: 0,
            },
        }
    }

    pub fn get_uri(&self) -> Option<Uri> {
        self.uri.clone()
    }
}

impl fmt::Debug for DocumentView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DocumentView")
            .field("file_id", &self.file_id)
            .field("path", &self.path)
            .field("line_count", &self.get_line_count())
            .finish()
    }
}

#[salsa::db]
pub(crate) trait SalsaDb: Database {
    /// Cross-file lookup: FileId → file input. When called inside tracked query bodies, salsa records dependency on the file input.
    fn file_input(&self, file_id: FileId) -> Option<SourceFileInput>;
    /// Current workspace input (stable id, in-place content updates → revision bumps on any file change).
    fn workspace_input(&self) -> Option<WorkspaceInput>;
}

#[salsa::db]
#[derive(Clone)]
pub struct SalsaDatabase {
    storage: salsa::Storage<Self>,

    // ── Salsa inputs ──
    config: Option<ConfigInput>,
    /// File-set input (only stores a lightweight FileId list).
    workspace: Option<WorkspaceInput>,

    /// Stable VFS mount point: immutable snapshot, Arc-shared on clone.
    vfs: Arc<VfsSnapshot>,

    /// Shared high-level semantic cache (currently a bridge before full salsa-tracked
    /// semantic queries; cloned snapshots share the same cache).
    semantic_cache: Arc<SemanticCache>,

    /// Next FileId to allocate.
    next_file_id: u32,

    /// Actual execution count of tracked query bodies.
    executed_queries: Arc<std::sync::atomic::AtomicU64>,
}

impl Default for SalsaDatabase {
    fn default() -> Self {
        let executed_queries = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let counter = executed_queries.clone();
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if matches!(event.kind, salsa::EventKind::WillExecute { .. }) {
                counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        })));
        let mut db = Self {
            storage,
            config: None,
            workspace: None,
            vfs: Arc::new(VfsSnapshot::empty()),
            semantic_cache: Arc::new(SemanticCache::default()),
            next_file_id: 0,
            executed_queries,
        };
        db.ensure_workspace();
        db
    }
}

#[salsa::db]
impl Database for SalsaDatabase {}

#[salsa::db]
impl SalsaDb for SalsaDatabase {
    fn file_input(&self, file_id: FileId) -> Option<SourceFileInput> {
        self.vfs.file_entry(file_id).map(|entry| entry.salsa_input)
    }

    fn workspace_input(&self) -> Option<WorkspaceInput> {
        self.workspace
    }
}

impl fmt::Debug for SalsaDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SalsaDatabase")
            .field("file_count", &self.vfs.files().len())
            .field("has_config", &self.config.is_some())
            .finish()
    }
}

impl SalsaDatabase {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drain other clones to obtain exclusive salsa writer access.
    ///
    /// Salsa's input setters already call `cancel_others()` (waiting for other clones to drop);
    /// calling this explicitly before any `&mut self` write path ensures no readers during writes.
    #[inline]
    fn cancel_snapshots(&mut self) {
        self.trigger_cancellation();
    }

    /// Ensure the workspace input exists (create an empty salsa input on first write).
    fn ensure_workspace(&mut self) {
        if self.workspace.is_none() {
            self.workspace = Some(WorkspaceInput::new(
                self,
                Arc::from(Vec::<FileId>::new()),
                Arc::from(Vec::<WorkspaceRoot>::new()),
            ));
        }
    }

    /// Insert or update one file entry in the VFS snapshot.
    ///
    /// When `is_new == true`, also add the FileId to `WorkspaceInput.file_ids`;
    /// pure text/path updates only replace the VFS entry and do not touch the file set.
    fn commit_file_entry(&mut self, file_id: FileId, input: SourceFileInput, is_new: bool) {
        self.semantic_cache.clear();
        let Some(workspace) = self.workspace else {
            return;
        };
        let entry = FileEntry {
            path: input.path(self).clone(),
            uri: input.uri(self).clone(),
            salsa_input: input,
        };

        let vfs = Arc::make_mut(&mut self.vfs);
        vfs.insert_entry(file_id, entry);

        if is_new {
            let mut file_ids = vfs.file_ids().to_vec();
            if !file_ids.contains(&file_id) {
                file_ids.push(file_id);
                file_ids.sort_unstable();
                let file_ids: Arc<[FileId]> = Arc::from(file_ids);
                vfs.set_file_ids(file_ids.clone());
                workspace.set_file_ids(self).to(file_ids);
            }
        }
    }

    /// Remove a file from the workspace file list and VFS snapshot.
    fn workspace_remove_file(&mut self, file_id: FileId) {
        self.semantic_cache.clear();
        let Some(workspace) = self.workspace else {
            return;
        };
        let vfs = Arc::make_mut(&mut self.vfs);
        vfs.remove_entry(file_id);

        let file_ids: Vec<FileId> = vfs
            .file_ids()
            .iter()
            .copied()
            .filter(|&id| id != file_id)
            .collect();
        let file_ids: Arc<[FileId]> = Arc::from(file_ids);
        vfs.set_file_ids(file_ids.clone());
        workspace.set_file_ids(self).to(file_ids);
    }

    // ---- Config ----

    pub fn update_config(&mut self, emmyrc: Arc<Emmyrc>) {
        self.cancel_snapshots();
        self.semantic_cache.clear();
        let (
            language_level,
            special_like,
            non_std_symbols,
            module_patterns,
            module_replace,
            known_doc_tags,
            strict_array_index,
        ) = ConfigInput::parts_from_emmyrc(&emmyrc);
        self.config = Some(ConfigInput::new(
            self,
            language_level,
            special_like,
            non_std_symbols,
            module_patterns,
            module_replace,
            known_doc_tags,
            strict_array_index,
            None,
        ));
    }

    /// Main workspace root (used for require module name derivation).
    pub fn update_main_root(&mut self, root: PathBuf) {
        self.cancel_snapshots();
        self.semantic_cache.clear();
        if let Some(config) = self.config {
            config.set_main_root(self).to(Some(root));
        }
    }

    pub fn main_root(&self) -> Option<PathBuf> {
        self.config
            .and_then(|config| config.main_root(self).clone())
    }

    pub(crate) fn strict_array_index(&self) -> bool {
        self.config
            .map(|config| config.strict_array_index(self))
            .unwrap_or(true)
    }

    /// Shared high-level semantic cache. This is a bridge cache owned by the
    /// database snapshot; it is cleared on any source/config mutation.
    pub(crate) fn semantic_cache(&self) -> &SemanticCache {
        &self.semantic_cache
    }

    /// Run `f` for every workspace file on scoped worker threads.
    ///
    /// Each worker owns its own `SalsaDatabase` clone, sharing the same salsa memo
    /// and the shared high-level semantic cache. `f` must be `Sync` because it is
    /// invoked concurrently from multiple scoped threads.
    pub fn parallel_for_each_file<F>(&self, f: F)
    where
        F: Fn(FileId, &crate::SalsaSemanticModel<'_>) + Sync,
    {
        let file_ids: Vec<FileId> = self.file_ids().to_vec();
        std::thread::scope(|scope| {
            for file_id in file_ids {
                let db = self.clone();
                let f = &f;
                scope.spawn(move || {
                    if let Some(model) = crate::SalsaSemanticModel::new(&db, file_id) {
                        f(file_id, &model);
                    }
                });
            }
        });
    }

    /// Register the built-in std workspace root.
    pub fn add_std_workspace(&mut self, root: PathBuf) {
        self.cancel_snapshots();
        self.semantic_cache.clear();
        self.ensure_workspace();
        let Some(workspace_input) = self.workspace else {
            return;
        };
        let mut roots = workspace_input.roots(self).to_vec();
        roots.retain(|root_entry| !root_entry.id.is_std());
        roots.push(WorkspaceRoot {
            id: WorkspaceId::STD,
            root,
            import: WorkspaceImport::All,
        });
        workspace_input.set_roots(self).to(Arc::from(roots));
    }

    /// Register or replace the main workspace root.
    pub fn add_main_workspace(&mut self, root: PathBuf) {
        self.cancel_snapshots();
        self.semantic_cache.clear();
        self.update_main_root(root.clone());
        self.ensure_workspace();
        let Some(workspace_input) = self.workspace else {
            return;
        };
        let mut roots = workspace_input.roots(self).to_vec();
        roots.retain(|root_entry| !root_entry.id.is_main());
        roots.push(WorkspaceRoot {
            id: WorkspaceId::MAIN,
            root,
            import: WorkspaceImport::All,
        });
        workspace_input.set_roots(self).to(Arc::from(roots));
    }

    /// Register a library workspace (allocates a new `WorkspaceId`).
    pub fn add_library_workspace(&mut self, workspace: &WorkspaceFolder) {
        self.cancel_snapshots();
        self.semantic_cache.clear();
        self.ensure_workspace();
        let Some(workspace_input) = self.workspace else {
            return;
        };
        let mut roots = workspace_input.roots(self).to_vec();
        let id = WorkspaceId {
            id: self.next_library_workspace_id(&roots),
        };
        roots.push(WorkspaceRoot {
            id,
            root: workspace.root.clone(),
            import: workspace.import.clone(),
        });
        workspace_input.set_roots(self).to(Arc::from(roots));
    }

    /// Keep only the std workspace (clear main/library before reload).
    pub fn clear_non_std_workspaces(&mut self) {
        self.cancel_snapshots();
        self.semantic_cache.clear();
        self.ensure_workspace();
        let Some(workspace_input) = self.workspace else {
            return;
        };
        let roots: Vec<WorkspaceRoot> = workspace_input
            .roots(self)
            .iter()
            .filter(|root_entry| root_entry.id.is_std())
            .cloned()
            .collect();
        workspace_input.set_roots(self).to(Arc::from(roots));
    }

    /// Currently registered workspace roots.
    #[allow(unused)]
    pub(crate) fn workspace_roots(&self) -> Arc<[WorkspaceRoot]> {
        self.workspace
            .map(|workspace| workspace.roots(self).clone())
            .unwrap_or_default()
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
        self.cancel_snapshots();
        self.ensure_workspace();
        let mut set = self.vfs.protected_paths().as_ref().clone();
        set.extend(paths);
        Arc::make_mut(&mut self.vfs).set_protected_paths(Arc::new(set));
    }

    /// Currently protected paths (loaded std lib etc.).
    pub fn protected_paths(&self) -> Arc<HashSet<PathBuf>> {
        self.vfs.protected_paths().clone()
    }

    /// Current configured runtime version (used for `---@version` visibility).
    pub fn lua_version(&self) -> Option<emmylua_parser::LuaVersionNumber> {
        self.config
            .map(|config| config.language_level(self).to_lua_version_number())
    }

    // ---- URI / FileId mapping ----

    pub fn lookup_file_id(&self, uri: &Uri) -> Option<FileId> {
        if let Some(path) = uri_to_file_path(uri)
            && let Some(id) = self.vfs.path_to_file_id().get(&path).copied()
        {
            return Some(id);
        }
        self.vfs.uri_to_file_id().get(uri).copied()
    }

    pub fn file_uri(&self, file_id: FileId) -> Option<Uri> {
        self.vfs
            .file_entry(file_id)
            .and_then(|entry| entry.uri.clone())
    }

    pub fn file_path(&self, file_id: FileId) -> Option<PathBuf> {
        self.vfs
            .file_entry(file_id)
            .and_then(|entry| entry.path.clone())
    }

    // ---- File management ----

    pub fn set_file_content(&mut self, uri: &Uri, text: Option<String>) -> FileId {
        self.cancel_snapshots();
        let fid = self.lookup_file_id(uri).unwrap_or_else(|| {
            let id = FileId::new(self.next_file_id);
            self.next_file_id += 1;
            id
        });
        if let Some(text) = text {
            let path = uri_to_file_path(uri);
            self.set_file_inner(fid, path, Some(uri.clone()), text);
        } else {
            self.remove_file_inner(fid);
        }
        fid
    }

    pub fn set_file(&mut self, file_id: FileId, path: Option<PathBuf>, text: String) {
        self.cancel_snapshots();
        let uri = path.as_ref().and_then(file_path_to_uri);
        self.set_file_inner(file_id, path, uri, text);
    }

    pub(crate) fn upsert_file_input(
        &mut self,
        file_id: FileId,
        path: Option<PathBuf>,
        uri: Option<Uri>,
        text: String,
    ) -> SourceFileInput {
        self.ensure_workspace();
        let existing = self.vfs.file_entry(file_id).map(|entry| entry.salsa_input);

        let text: Arc<str> = Arc::from(text);
        if let Some(input) = existing {
            if input.path(self) != &path {
                input.set_path(self).to(path.clone());
            }
            if input.uri(self) != &uri {
                input.set_uri(self).to(uri.clone());
            }
            input.set_text(self).to(text.clone());
            input
        } else {
            SourceFileInput::new(self, text.clone(), path.clone(), uri.clone(), file_id)
        }
    }

    fn set_file_inner(
        &mut self,
        file_id: FileId,
        path: Option<PathBuf>,
        uri: Option<Uri>,
        text: String,
    ) {
        self.ensure_workspace();
        let old = self.vfs.file_entry(file_id);
        let is_new = old.is_none();
        let metadata_changed = old.is_some_and(|entry| {
            entry.path.as_ref() != path.as_ref() || entry.uri.as_ref() != uri.as_ref()
        });
        let input = self.upsert_file_input(file_id, path.clone(), uri.clone(), text);
        if is_new || metadata_changed {
            self.commit_file_entry(file_id, input, is_new);
        }
    }

    /// Replace the whole workspace file set in one salsa write.
    pub(crate) fn replace_workspace_files(
        &mut self,
        file_inputs: HashMap<FileId, SourceFileInput>,
    ) {
        self.cancel_snapshots();
        self.ensure_workspace();
        let Some(workspace) = self.workspace else {
            return;
        };

        let mut files = HashMap::with_capacity(file_inputs.len());
        for (file_id, input) in file_inputs {
            let entry = FileEntry {
                path: input.path(self).clone(),
                uri: input.uri(self).clone(),
                salsa_input: input,
            };
            files.insert(file_id, entry);
        }
        let mut file_ids: Vec<FileId> = files.keys().copied().collect();
        file_ids.sort_unstable();

        self.vfs = Arc::new(VfsSnapshot::from_parts(
            files,
            Arc::from(file_ids.clone()),
            self.vfs.protected_paths().clone(),
        ));
        workspace.set_file_ids(self).to(Arc::from(file_ids));
    }

    /// Current workspace file map (FileId -> SourceFileInput).
    pub(crate) fn file_input_map(&self) -> HashMap<FileId, SourceFileInput> {
        self.vfs
            .files()
            .iter()
            .map(|(&file_id, entry)| (file_id, entry.salsa_input))
            .collect()
    }

    /// Allocate a fresh FileId.
    pub(crate) fn allocate_file_id(&mut self) -> FileId {
        let id = FileId::new(self.next_file_id);
        self.next_file_id += 1;
        id
    }

    pub fn remove_file(&mut self, file_id: FileId) {
        self.cancel_snapshots();
        self.remove_file_inner(file_id);
    }

    fn remove_file_inner(&mut self, file_id: FileId) {
        self.workspace_remove_file(file_id);
    }

    pub fn clear(&mut self) {
        self.cancel_snapshots();
        self.workspace = None;
        self.vfs = Arc::new(VfsSnapshot::empty());
        self.next_file_id = 0;
        self.ensure_workspace();
    }

    /// Current VFS snapshot (immutable, shareable across threads).
    #[allow(dead_code)]
    pub(crate) fn vfs(&self) -> &Arc<VfsSnapshot> {
        &self.vfs
    }

    pub fn file_ids(&self) -> Vec<FileId> {
        self.vfs.file_ids().to_vec()
    }

    /// Main workspace file list.
    pub fn main_workspace_file_ids(&self) -> Vec<FileId> {
        let workspace = self.workspace_input();
        let has_roots = workspace.is_some_and(|workspace| !workspace.roots(self).is_empty());
        if !has_roots {
            // Keep old behavior when no roots are registered: treat all files as main.
            return self.vfs.file_ids().to_vec();
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
        let input = self.file_input(file_id)?;
        Some(input.text(self))
    }

    /// Per-file line index, memoized as a salsa derived query.
    pub fn line_index(&self, file_id: FileId) -> Option<Arc<LineIndex>> {
        let file = self.file_input(file_id)?;
        Some(query::line_index(self, file).clone())
    }

    /// Document view, memoized as a salsa derived query.
    pub fn document(&self, file_id: FileId) -> Option<Arc<DocumentView>> {
        let file = self.file_input(file_id)?;
        Some(query::document(self, file).clone())
    }

    // ── Input accessors (for tracked layer / facade) ──

    pub(crate) fn config_input(&self) -> Option<ConfigInput> {
        self.config
    }

    pub(crate) fn workspace_input(&self) -> Option<WorkspaceInput> {
        self.workspace
    }

    /// Actual execution count of tracked query bodies (diagnostic invalidation granularity).
    pub fn query_execution_count(&self) -> u64 {
        self.executed_queries
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    // ── Facade ──

    /// Module name → module file (for require resolution and handlers such as document_link).
    pub fn module_file_of(&self, module_name: &str) -> Option<FileId> {
        self.q().module_file_of(module_name)
    }

    // ── Reference index ──

    /// All use sites of a declaration (Decl) (cross-file, aggregated through sharded reference index).
    pub fn decl_reference_ranges(&self, decl: &SemanticId) -> Vec<(FileId, rowan::TextRange)> {
        let (Some(workspace), Some(config)) = (self.workspace_input(), self.config_input()) else {
            return Vec::new();
        };
        query::workspace_reference_index(self, workspace, config)
            .decl_refs
            .get(decl)
            .cloned()
            .unwrap_or_default()
    }

    /// All use sites of a member (Member) (cross-file, aggregated through sharded reference index).
    pub fn member_reference_ranges(&self, member: &SemanticId) -> Vec<(FileId, rowan::TextRange)> {
        let (Some(workspace), Some(config)) = (self.workspace_input(), self.config_input()) else {
            return Vec::new();
        };
        query::workspace_reference_index(self, workspace, config)
            .member_refs
            .get(member)
            .cloned()
            .unwrap_or_default()
    }

    /// All definition sites of a member (Member) (cross-file, aggregated through sharded reference index).
    pub fn member_definition_ranges(&self, member: &SemanticId) -> Vec<(FileId, rowan::TextRange)> {
        let (Some(workspace), Some(config)) = (self.workspace_input(), self.config_input()) else {
            return Vec::new();
        };
        query::workspace_reference_index(self, workspace, config)
            .member_defs
            .get(member)
            .cloned()
            .unwrap_or_default()
    }

    /// File → owning workspace id.
    pub fn workspace_id_of(&self, file_id: FileId) -> Option<WorkspaceId> {
        let workspace = self.workspace_input()?;
        query::file_workspace_id(self, workspace, file_id)
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

    /// File → salsa module info (equivalent to ModuleIndex).
    pub fn module_info_of(&self, file_id: FileId) -> Option<ModuleInfo> {
        let workspace = self.workspace_input()?;
        let config = self.config_input()?;
        let index = query::workspace_module_index(self, workspace, config);
        let mut info = index.module_info(file_id)?;
        if let Some(shell) = self.q().module_export_type(file_id) {
            info.export_type = Some(self.q().type_shell_lua(file_id, &shell));
        }
        Some(info)
    }

    /// Module path → module tree node id (empty path returns the root node).
    pub fn module_node(&self, module_path: &str) -> Option<ModuleNodeId> {
        let workspace = self.workspace_input()?;
        let config = self.config_input()?;
        let index = query::workspace_module_index(self, workspace, config);
        index.find_module_node(module_path)
    }

    /// Module tree node details.
    pub fn module_node_info(&self, node_id: ModuleNodeId) -> Option<ModuleNode> {
        let workspace = self.workspace_input()?;
        let config = self.config_input()?;
        let index = query::workspace_module_index(self, workspace, config);
        index.module_node(node_id).cloned()
    }

    /// File id list under a module tree node.
    pub fn module_node_file_ids(&self, node_id: ModuleNodeId) -> Vec<FileId> {
        let (Some(workspace), Some(config)) = (self.workspace_input(), self.config_input()) else {
            return Vec::new();
        };
        let index = query::workspace_module_index(self, workspace, config);
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
        if let Some(workspace) = self.workspace_input() {
            let roots = workspace.roots(self).to_vec();
            if let Some((_, root)) = query::find_workspace_root(&roots, path)
                && let Some(name) = query::module_name_from_path(path, Some(&root))
            {
                return Some(name.to_string());
            }
        }
        let root = self.config_input()?.main_root(self).clone();
        query::module_name_from_path(path, root.as_deref()).map(|name| name.to_string())
    }

    /// Query facade (crate-internal: used by semantic_model and tests).
    #[allow(dead_code)]
    pub(crate) fn q(&self) -> SalsaQueries<'_> {
        SalsaQueries::new(self)
    }
}

use crate::vfs::uri_to_file_path;
