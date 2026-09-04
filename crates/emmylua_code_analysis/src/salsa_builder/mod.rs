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

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use emmylua_parser::{LineIndex, LuaSyntaxTree};
use lsp_types::Uri;

use crate::analysis_state::VfsState;
use crate::vfs::file_path_to_uri;
use crate::{Emmyrc, FileId, WorkspaceFolder, WorkspaceImport};
pub use def::*;
use inputs::{
    ConfigInput, ConfigInputData, SourceFileInput, SourceFileInputData, WorkspaceInput,
    WorkspaceInputData, WorkspaceRoot,
};

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

pub(crate) trait SalsaDb {
    /// Cross-file lookup: FileId → file input. When called inside tracked query bodies, salsa records dependency on the file input.
    fn file_input(&self, file_id: FileId) -> Option<SourceFileInput>;
    /// Raw per-file input data (plain map).
    fn source_file_data(&self, file_id: FileId) -> Option<&SourceFileInputData>;
    /// Raw config data.
    fn config_data(&self) -> Option<&ConfigInputData>;
    /// Raw workspace data.
    fn workspace_data(&self) -> Option<&WorkspaceInputData>;
    /// Current workspace input (stable id, in-place content updates → revision bumps on any file change).
    fn workspace_input(&self) -> Option<WorkspaceInput>;
    /// Returns the lazy per-file facts cell. The map is kept in sync with `file_input`;
    /// the fallback cell is only a defensive measure and should never be used.
    fn file_facts_cell(&self, file_id: FileId) -> &OnceLock<Arc<facts::FileFacts>>;
    /// Returns the lazy per-file flow-tree cell.
    fn flow_tree_cell(&self, file_id: FileId) -> &OnceLock<Arc<flow::FlowTree>>;
    /// Returns the lazy per-file syntax-tree cell.
    fn syntax_tree_cell(&self, file_id: FileId) -> &OnceLock<Arc<LuaSyntaxTree>>;
    /// Returns the lazy per-file line-index cell.
    fn line_index_cell(&self, file_id: FileId) -> &OnceLock<Arc<LineIndex>>;
    /// Returns the lazy per-file document cell.
    fn document_cell(&self, file_id: FileId) -> &OnceLock<Arc<DocumentView>>;
    /// Returns the lazy per-file exports cell.
    fn file_exports_cell(&self, file_id: FileId) -> &OnceLock<Arc<exports::FileExports>>;
    /// Returns the lazy export-shard cell.
    fn export_shard_cell(&self, shard: u8) -> &OnceLock<Arc<exports::ExportShard>>;
    /// Returns the lazy per-file references cell.
    fn file_references_cell(&self, file_id: FileId) -> &OnceLock<Arc<query::FileReferences>>;
    /// Returns the lazy deprecated-shard cell.
    fn deprecated_shard_cell(&self, shard: u8) -> &OnceLock<Arc<query::DeprecatedShard>>;
    /// Returns the lazy module-shard cell.
    fn module_shard_cell(&self, shard: u8) -> &OnceLock<Arc<query::ModuleShard>>;
    /// Returns the lazy reference-shard cell.
    fn reference_shard_cell(&self, shard: u8) -> &OnceLock<Arc<query::ReferenceShard>>;
    /// Returns the plain workspace index cache. The indexes are not Salsa queries;
    /// tracked consumers read `WorkspaceInput.revision` for invalidation.
    fn workspace_index_cache(&self) -> &Mutex<query::WorkspaceIndexCache>;
}

#[derive(Clone)]
pub struct SalsaDatabase {
    // ── Plain config/data ──
    config: Option<ConfigInputData>,
    /// File-set data (only stores a lightweight FileId list).
    workspace: Option<WorkspaceInputData>,

    /// Plain VFS state independent of Salsa inputs.
    vfs: Arc<VfsState>,

    /// Plain per-file input data, kept separate from VFS.
    file_inputs: Arc<HashMap<FileId, SourceFileInputData>>,

    /// Plain per-file facts cache. Unlike Salsa's tracked `file_facts`, this is a
    /// normal lazy cache; file writes invalidate only the affected entry (or all
    /// entries when config/workspace roots change).
    file_facts: Arc<HashMap<FileId, OnceLock<Arc<facts::FileFacts>>>>,

    /// Plain per-file control-flow graph cache (same invalidation as `file_facts`).
    flow_trees: Arc<HashMap<FileId, OnceLock<Arc<flow::FlowTree>>>>,

    /// Plain per-file syntax/line-index/document caches.
    syntax_trees: Arc<HashMap<FileId, OnceLock<Arc<LuaSyntaxTree>>>>,
    line_indexes: Arc<HashMap<FileId, OnceLock<Arc<LineIndex>>>>,
    documents: Arc<HashMap<FileId, OnceLock<Arc<DocumentView>>>>,

    /// Plain per-file exports / shard caches.
    file_exports: Arc<HashMap<FileId, OnceLock<Arc<exports::FileExports>>>>,
    export_shards: Arc<HashMap<u8, OnceLock<Arc<exports::ExportShard>>>>,

    /// Plain per-file references and remaining shard caches.
    file_references: Arc<HashMap<FileId, OnceLock<Arc<query::FileReferences>>>>,
    deprecated_shards: Arc<HashMap<u8, OnceLock<Arc<query::DeprecatedShard>>>>,
    module_shards: Arc<HashMap<u8, OnceLock<Arc<query::ModuleShard>>>>,
    reference_shards: Arc<HashMap<u8, OnceLock<Arc<query::ReferenceShard>>>>,

    /// Plain merged workspace indexes (type/member/decl/module/reference).
    workspace_index: Arc<Mutex<query::WorkspaceIndexCache>>,

    /// Next FileId to allocate.
    next_file_id: u32,

    /// Actual execution count of tracked query bodies.
    executed_queries: Arc<std::sync::atomic::AtomicU64>,
}

impl Default for SalsaDatabase {
    fn default() -> Self {
        let executed_queries = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let mut db = Self {
            config: None,
            workspace: None,
            vfs: Arc::new(VfsState::new()),
            file_inputs: Arc::new(HashMap::new()),
            file_facts: Arc::new(HashMap::new()),
            flow_trees: Arc::new(HashMap::new()),
            syntax_trees: Arc::new(HashMap::new()),
            line_indexes: Arc::new(HashMap::new()),
            documents: Arc::new(HashMap::new()),
            file_exports: Arc::new(HashMap::new()),
            export_shards: Arc::new(HashMap::new()),
            file_references: Arc::new(HashMap::new()),
            deprecated_shards: Arc::new(HashMap::new()),
            module_shards: Arc::new(HashMap::new()),
            reference_shards: Arc::new(HashMap::new()),
            workspace_index: Arc::new(Mutex::new(query::WorkspaceIndexCache::new())),
            next_file_id: 0,
            executed_queries,
        };
        db.ensure_workspace();
        db
    }
}

impl SalsaDb for SalsaDatabase {
    fn file_input(&self, file_id: FileId) -> Option<SourceFileInput> {
        self.file_inputs
            .contains_key(&file_id)
            .then(|| SourceFileInput::new(file_id))
    }

    fn source_file_data(&self, file_id: FileId) -> Option<&SourceFileInputData> {
        self.file_inputs.get(&file_id)
    }

    fn config_data(&self) -> Option<&ConfigInputData> {
        self.config.as_ref()
    }

    fn workspace_data(&self) -> Option<&WorkspaceInputData> {
        self.workspace.as_ref()
    }

    fn workspace_input(&self) -> Option<WorkspaceInput> {
        self.workspace.is_some().then_some(WorkspaceInput)
    }

    fn file_facts_cell(&self, file_id: FileId) -> &OnceLock<Arc<facts::FileFacts>> {
        self.file_facts.get(&file_id).unwrap_or_else(|| {
            // Every public file-mutation path inserts/removes a cell in `file_facts`
            // alongside `file_inputs`. This static fallback is only to keep the trait
            // object signature total if an internal invariant is ever violated.
            static MISSING: OnceLock<Arc<facts::FileFacts>> = OnceLock::new();
            &MISSING
        })
    }

    fn flow_tree_cell(&self, file_id: FileId) -> &OnceLock<Arc<flow::FlowTree>> {
        self.flow_trees.get(&file_id).unwrap_or_else(|| {
            static MISSING: OnceLock<Arc<flow::FlowTree>> = OnceLock::new();
            &MISSING
        })
    }

    fn syntax_tree_cell(&self, file_id: FileId) -> &OnceLock<Arc<LuaSyntaxTree>> {
        self.syntax_trees.get(&file_id).unwrap_or_else(|| {
            static MISSING: OnceLock<Arc<LuaSyntaxTree>> = OnceLock::new();
            &MISSING
        })
    }

    fn line_index_cell(&self, file_id: FileId) -> &OnceLock<Arc<LineIndex>> {
        self.line_indexes.get(&file_id).unwrap_or_else(|| {
            static MISSING: OnceLock<Arc<LineIndex>> = OnceLock::new();
            &MISSING
        })
    }

    fn document_cell(&self, file_id: FileId) -> &OnceLock<Arc<DocumentView>> {
        self.documents.get(&file_id).unwrap_or_else(|| {
            static MISSING: OnceLock<Arc<DocumentView>> = OnceLock::new();
            &MISSING
        })
    }

    fn file_exports_cell(&self, file_id: FileId) -> &OnceLock<Arc<exports::FileExports>> {
        self.file_exports.get(&file_id).unwrap_or_else(|| {
            static MISSING: OnceLock<Arc<exports::FileExports>> = OnceLock::new();
            &MISSING
        })
    }

    fn export_shard_cell(&self, shard: u8) -> &OnceLock<Arc<exports::ExportShard>> {
        self.export_shards.get(&shard).unwrap_or_else(|| {
            static MISSING: OnceLock<Arc<exports::ExportShard>> = OnceLock::new();
            &MISSING
        })
    }

    fn file_references_cell(&self, file_id: FileId) -> &OnceLock<Arc<query::FileReferences>> {
        self.file_references.get(&file_id).unwrap_or_else(|| {
            static MISSING: OnceLock<Arc<query::FileReferences>> = OnceLock::new();
            &MISSING
        })
    }

    fn deprecated_shard_cell(&self, shard: u8) -> &OnceLock<Arc<query::DeprecatedShard>> {
        self.deprecated_shards.get(&shard).unwrap_or_else(|| {
            static MISSING: OnceLock<Arc<query::DeprecatedShard>> = OnceLock::new();
            &MISSING
        })
    }

    fn module_shard_cell(&self, shard: u8) -> &OnceLock<Arc<query::ModuleShard>> {
        self.module_shards.get(&shard).unwrap_or_else(|| {
            static MISSING: OnceLock<Arc<query::ModuleShard>> = OnceLock::new();
            &MISSING
        })
    }

    fn reference_shard_cell(&self, shard: u8) -> &OnceLock<Arc<query::ReferenceShard>> {
        self.reference_shards.get(&shard).unwrap_or_else(|| {
            static MISSING: OnceLock<Arc<query::ReferenceShard>> = OnceLock::new();
            &MISSING
        })
    }

    fn workspace_index_cache(&self) -> &Mutex<query::WorkspaceIndexCache> {
        &self.workspace_index
    }
}

impl fmt::Debug for SalsaDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SalsaDatabase")
            .field("file_count", &self.vfs.len())
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
        // No Salsa snapshots remain; kept as no-op for call-site compatibility.
    }

    /// Ensure the workspace input exists (create an empty salsa input on first write).
    fn ensure_workspace(&mut self) {
        if self.workspace.is_none() {
            self.workspace = Some(WorkspaceInputData::new(
                Arc::from(Vec::<FileId>::new()),
                Arc::from(Vec::<WorkspaceRoot>::new()),
                0,
            ));
        }
    }

    /// Bump the global workspace revision. Every file/config/root mutation that can
    /// change any merged workspace index must call this so Salsa consumers of the
    /// plain `WorkspaceIndexCache` are invalidated.
    fn bump_workspace_revision(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let revision = workspace.revision.saturating_add(1);
        self.workspace = Some(WorkspaceInputData::new(
            workspace.file_ids.clone(),
            workspace.roots.clone(),
            revision,
        ));
    }

    fn set_workspace_file_ids(&mut self, file_ids: Arc<[FileId]>) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        self.workspace = Some(WorkspaceInputData::new(
            file_ids,
            workspace.roots.clone(),
            workspace.revision,
        ));
    }

    fn set_workspace_roots(&mut self, roots: Arc<[WorkspaceRoot]>) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        self.workspace = Some(WorkspaceInputData::new(
            workspace.file_ids.clone(),
            roots,
            workspace.revision,
        ));
    }

    /// Drop all lazily built per-file facts and recreate empty cells for current files.
    ///
    /// Called after configuration or workspace-root changes, where every file's facts
    /// may need to be rebuilt because parser features or workspace identity changed.
    fn reset_export_shards(&mut self) {
        self.export_shards = Arc::new(
            (0..exports::EXPORT_SHARDS)
                .map(|shard| (shard, OnceLock::new()))
                .collect(),
        );
    }

    fn reset_other_shard_caches(&mut self) {
        let shards = 0..exports::EXPORT_SHARDS;
        self.deprecated_shards = Arc::new(
            shards
                .clone()
                .map(|shard| (shard, OnceLock::new()))
                .collect(),
        );
        self.module_shards = Arc::new(
            shards
                .clone()
                .map(|shard| (shard, OnceLock::new()))
                .collect(),
        );
        self.reference_shards = Arc::new(shards.map(|shard| (shard, OnceLock::new())).collect());
    }

    fn reset_file_facts_cache(&mut self) {
        let file_ids = self.vfs.file_ids();
        self.file_facts = Arc::new(
            file_ids
                .iter()
                .copied()
                .map(|file_id| (file_id, OnceLock::new()))
                .collect(),
        );
        self.flow_trees = Arc::new(
            file_ids
                .iter()
                .copied()
                .map(|file_id| (file_id, OnceLock::new()))
                .collect(),
        );
        self.syntax_trees = Arc::new(
            file_ids
                .iter()
                .copied()
                .map(|file_id| (file_id, OnceLock::new()))
                .collect(),
        );
        self.line_indexes = Arc::new(
            file_ids
                .iter()
                .copied()
                .map(|file_id| (file_id, OnceLock::new()))
                .collect(),
        );
        self.documents = Arc::new(
            file_ids
                .iter()
                .copied()
                .map(|file_id| (file_id, OnceLock::new()))
                .collect(),
        );
        self.file_exports = Arc::new(
            file_ids
                .iter()
                .copied()
                .map(|file_id| (file_id, OnceLock::new()))
                .collect(),
        );
        self.file_references = Arc::new(
            file_ids
                .iter()
                .copied()
                .map(|file_id| (file_id, OnceLock::new()))
                .collect(),
        );
        self.reset_export_shards();
        self.reset_other_shard_caches();
    }

    /// Mark one file's facts/flow-tree/syntax caches stale. The actual build stays lazy.
    fn invalidate_file_facts(&mut self, file_id: FileId) {
        Arc::make_mut(&mut self.file_facts).insert(file_id, OnceLock::new());
        Arc::make_mut(&mut self.flow_trees).insert(file_id, OnceLock::new());
        Arc::make_mut(&mut self.syntax_trees).insert(file_id, OnceLock::new());
        Arc::make_mut(&mut self.line_indexes).insert(file_id, OnceLock::new());
        Arc::make_mut(&mut self.documents).insert(file_id, OnceLock::new());
        Arc::make_mut(&mut self.file_exports).insert(file_id, OnceLock::new());
        Arc::make_mut(&mut self.file_references).insert(file_id, OnceLock::new());
        // Shards aggregate multiple files; reset them on any file change for simplicity.
        self.reset_export_shards();
        self.reset_other_shard_caches();
    }

    /// Insert or update one file entry in the VFS snapshot.
    ///
    /// When `is_new == true`, also add the FileId to `WorkspaceInput.file_ids`;
    /// pure text/path updates only replace the VFS entry and do not touch the file set.
    fn commit_file_entry(&mut self, file_id: FileId, input: SourceFileInputData, is_new: bool) {
        let text = input.text.to_string();
        let path = input.path.clone();
        let uri = input.uri.clone();
        Arc::make_mut(&mut self.vfs).insert_at(file_id, uri, path, text);
        Arc::make_mut(&mut self.file_inputs).insert(file_id, input);
        Arc::make_mut(&mut self.file_facts).insert(file_id, OnceLock::new());
        Arc::make_mut(&mut self.flow_trees).insert(file_id, OnceLock::new());
        Arc::make_mut(&mut self.syntax_trees).insert(file_id, OnceLock::new());
        Arc::make_mut(&mut self.line_indexes).insert(file_id, OnceLock::new());
        Arc::make_mut(&mut self.documents).insert(file_id, OnceLock::new());
        Arc::make_mut(&mut self.file_exports).insert(file_id, OnceLock::new());
        Arc::make_mut(&mut self.file_references).insert(file_id, OnceLock::new());
        self.reset_export_shards();
        self.reset_other_shard_caches();
        self.bump_workspace_revision();

        if is_new {
            let file_ids = self.vfs.file_ids();
            let file_ids: Arc<[FileId]> = Arc::from(file_ids);
            self.set_workspace_file_ids(file_ids);
        }
    }

    /// Remove a file from the workspace file list and VFS snapshot.
    fn workspace_remove_file(&mut self, file_id: FileId) {
        Arc::make_mut(&mut self.vfs).remove(file_id);
        Arc::make_mut(&mut self.file_inputs).remove(&file_id);
        Arc::make_mut(&mut self.file_facts).remove(&file_id);
        Arc::make_mut(&mut self.flow_trees).remove(&file_id);
        Arc::make_mut(&mut self.syntax_trees).remove(&file_id);
        Arc::make_mut(&mut self.line_indexes).remove(&file_id);
        Arc::make_mut(&mut self.documents).remove(&file_id);
        Arc::make_mut(&mut self.file_exports).remove(&file_id);
        Arc::make_mut(&mut self.file_references).remove(&file_id);
        self.reset_export_shards();
        self.reset_other_shard_caches();
        self.bump_workspace_revision();

        let file_ids: Arc<[FileId]> = Arc::from(self.vfs.file_ids());
        self.set_workspace_file_ids(file_ids);
    }

    // ---- Config ----

    pub fn update_config(&mut self, emmyrc: Arc<Emmyrc>) {
        self.cancel_snapshots();
        let (
            language_level,
            special_like,
            non_std_symbols,
            module_patterns,
            module_replace,
            known_doc_tags,
            strict_array_index,
        ) = ConfigInput::parts_from_emmyrc(&emmyrc);
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
        self.bump_workspace_revision();
    }

    /// Main workspace root (used for require module name derivation).
    pub fn update_main_root(&mut self, root: PathBuf) {
        self.cancel_snapshots();
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
        self.bump_workspace_revision();
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
        self.ensure_workspace();
        let Some(workspace_input) = self.workspace.as_ref() else {
            return;
        };
        let mut roots = workspace_input.roots.to_vec();
        roots.retain(|root_entry| !root_entry.id.is_std());
        roots.push(WorkspaceRoot {
            id: WorkspaceId::STD,
            root,
            import: WorkspaceImport::All,
        });
        self.set_workspace_roots(Arc::from(roots));
        self.reset_file_facts_cache();
        self.bump_workspace_revision();
    }

    /// Register or replace the main workspace root.
    pub fn add_main_workspace(&mut self, root: PathBuf) {
        self.cancel_snapshots();
        self.update_main_root(root.clone());
        self.ensure_workspace();
        let Some(workspace_input) = self.workspace.as_ref() else {
            return;
        };
        let mut roots = workspace_input.roots.to_vec();
        roots.retain(|root_entry| !root_entry.id.is_main());
        roots.push(WorkspaceRoot {
            id: WorkspaceId::MAIN,
            root,
            import: WorkspaceImport::All,
        });
        self.set_workspace_roots(Arc::from(roots));
        self.reset_file_facts_cache();
        self.bump_workspace_revision();
    }

    /// Register a library workspace (allocates a new `WorkspaceId`).
    pub fn add_library_workspace(&mut self, workspace: &WorkspaceFolder) {
        self.cancel_snapshots();
        self.ensure_workspace();
        let Some(workspace_input) = self.workspace.as_ref() else {
            return;
        };
        let mut roots = workspace_input.roots.to_vec();
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
        self.bump_workspace_revision();
    }

    /// Keep only the std workspace (clear main/library before reload).
    pub fn clear_non_std_workspaces(&mut self) {
        self.cancel_snapshots();
        self.ensure_workspace();
        let Some(workspace_input) = self.workspace.as_ref() else {
            return;
        };
        let roots: Vec<WorkspaceRoot> = workspace_input
            .roots
            .iter()
            .filter(|root_entry| root_entry.id.is_std())
            .cloned()
            .collect();
        self.set_workspace_roots(Arc::from(roots));
        self.reset_file_facts_cache();
        self.bump_workspace_revision();
    }

    /// Currently registered workspace roots.
    #[allow(unused)]
    pub(crate) fn workspace_roots(&self) -> Arc<[WorkspaceRoot]> {
        self.workspace
            .as_ref()
            .map(|workspace| workspace.roots.clone())
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
        let mut set = self.vfs.protected_paths().clone();
        set.extend(paths);
        Arc::make_mut(&mut self.vfs).set_protected_paths(set);
    }

    /// Currently protected paths (loaded std lib etc.).
    pub fn protected_paths(&self) -> Arc<HashSet<PathBuf>> {
        Arc::new(self.vfs.protected_paths().clone())
    }

    /// Current configured runtime version (used for `---@version` visibility).
    pub fn lua_version(&self) -> Option<emmylua_parser::LuaVersionNumber> {
        self.config
            .as_ref()
            .map(|config| config.language_level.to_lua_version_number())
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
        _file_id: FileId,
        path: Option<PathBuf>,
        uri: Option<Uri>,
        text: String,
    ) -> SourceFileInputData {
        self.ensure_workspace();
        let text: Arc<str> = Arc::from(text);
        SourceFileInputData::new(text, path, uri)
    }

    fn set_file_inner(
        &mut self,
        file_id: FileId,
        path: Option<PathBuf>,
        uri: Option<Uri>,
        text: String,
    ) {
        self.ensure_workspace();
        let old = self.vfs.file(file_id);
        let is_new = old.is_none();
        let metadata_changed = old.is_some_and(|file| {
            file.path.as_ref() != path.as_ref() || file.uri.as_ref() != uri.as_ref()
        });
        let input = self.upsert_file_input(file_id, path.clone(), uri.clone(), text);
        if is_new || metadata_changed {
            self.commit_file_entry(file_id, input, is_new);
        } else {
            // A pure text update does not change VFS metadata or the file set, but
            // the stored input data and per-file caches are now stale.
            Arc::make_mut(&mut self.file_inputs).insert(file_id, input);
            self.invalidate_file_facts(file_id);
            self.bump_workspace_revision();
        }
    }

    /// Replace the whole workspace file set in one salsa write.
    pub(crate) fn replace_workspace_files(
        &mut self,
        file_inputs: HashMap<FileId, SourceFileInputData>,
    ) {
        self.cancel_snapshots();
        self.ensure_workspace();

        let protected_paths = self
            .vfs
            .protected_paths()
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut vfs = VfsState::new();
        vfs.set_protected_paths(protected_paths);
        let mut new_file_inputs = HashMap::with_capacity(file_inputs.len());
        for (file_id, input) in file_inputs {
            let text = input.text.to_string();
            let path = input.path.clone();
            let uri = input.uri.clone();
            vfs.insert_at(file_id, uri, path, text);
            new_file_inputs.insert(file_id, input);
        }

        let file_ids: Arc<[FileId]> = Arc::from(vfs.file_ids());
        self.vfs = Arc::new(vfs);
        self.file_inputs = Arc::new(new_file_inputs);
        self.file_facts = Arc::new(
            self.file_inputs
                .keys()
                .copied()
                .map(|file_id| (file_id, OnceLock::new()))
                .collect(),
        );
        self.flow_trees = Arc::new(
            self.file_inputs
                .keys()
                .copied()
                .map(|file_id| (file_id, OnceLock::new()))
                .collect(),
        );
        self.syntax_trees = Arc::new(
            self.file_inputs
                .keys()
                .copied()
                .map(|file_id| (file_id, OnceLock::new()))
                .collect(),
        );
        self.line_indexes = Arc::new(
            self.file_inputs
                .keys()
                .copied()
                .map(|file_id| (file_id, OnceLock::new()))
                .collect(),
        );
        self.documents = Arc::new(
            self.file_inputs
                .keys()
                .copied()
                .map(|file_id| (file_id, OnceLock::new()))
                .collect(),
        );
        self.file_exports = Arc::new(
            self.file_inputs
                .keys()
                .copied()
                .map(|file_id| (file_id, OnceLock::new()))
                .collect(),
        );
        self.file_references = Arc::new(
            self.file_inputs
                .keys()
                .copied()
                .map(|file_id| (file_id, OnceLock::new()))
                .collect(),
        );
        self.reset_export_shards();
        self.reset_other_shard_caches();
        self.bump_workspace_revision();
        self.set_workspace_file_ids(file_ids);
    }

    /// Current workspace file map (FileId -> source data).
    pub(crate) fn file_input_map(&self) -> HashMap<FileId, SourceFileInputData> {
        self.file_inputs.as_ref().clone()
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
        self.vfs = Arc::new(VfsState::new());
        self.file_inputs = Arc::new(HashMap::new());
        self.file_facts = Arc::new(HashMap::new());
        self.flow_trees = Arc::new(HashMap::new());
        self.syntax_trees = Arc::new(HashMap::new());
        self.line_indexes = Arc::new(HashMap::new());
        self.documents = Arc::new(HashMap::new());
        self.file_exports = Arc::new(HashMap::new());
        self.reset_export_shards();
        self.file_references = Arc::new(HashMap::new());
        self.reset_other_shard_caches();
        self.workspace_index = Arc::new(Mutex::new(query::WorkspaceIndexCache::new()));
        self.next_file_id = 0;
        self.ensure_workspace();
        self.bump_workspace_revision();
    }

    /// Current VFS snapshot (immutable, shareable across threads).
    #[allow(dead_code)]
    pub(crate) fn vfs(&self) -> &Arc<VfsState> {
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
        self.vfs.file(file_id).map(|file| file.text.as_ref())
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
        self.config.is_some().then_some(ConfigInput)
    }

    pub(crate) fn workspace_input(&self) -> Option<WorkspaceInput> {
        self.workspace.is_some().then_some(WorkspaceInput)
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
        let mut out = Vec::new();
        for ws_id in query::all_workspace_ids(self, workspace) {
            let index = query::workspace_reference_index_for(self, workspace, config, ws_id);
            if let Some(ranges) = index.decl_refs.get(decl) {
                out.extend(ranges.iter().copied());
            }
        }
        out
    }

    /// All use sites of a member (Member) (cross-file, aggregated through sharded reference index).
    pub fn member_reference_ranges(&self, member: &SemanticId) -> Vec<(FileId, rowan::TextRange)> {
        let (Some(workspace), Some(config)) = (self.workspace_input(), self.config_input()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for ws_id in query::all_workspace_ids(self, workspace) {
            let index = query::workspace_reference_index_for(self, workspace, config, ws_id);
            if let Some(ranges) = index.member_refs.get(member) {
                out.extend(ranges.iter().copied());
            }
        }
        out
    }

    /// All definition sites of a member (Member) (cross-file, aggregated through sharded reference index).
    pub fn member_definition_ranges(&self, member: &SemanticId) -> Vec<(FileId, rowan::TextRange)> {
        let (Some(workspace), Some(config)) = (self.workspace_input(), self.config_input()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for ws_id in query::all_workspace_ids(self, workspace) {
            let index = query::workspace_reference_index_for(self, workspace, config, ws_id);
            if let Some(ranges) = index.member_defs.get(member) {
                out.extend(ranges.iter().copied());
            }
        }
        out
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
        let ws_id =
            query::file_workspace_id(self, workspace, file_id).unwrap_or(WorkspaceId::REMOTE);
        let index = query::workspace_module_index_for(self, workspace, config, ws_id);
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
        for ws_id in query::all_workspace_ids(self, workspace) {
            let index = query::workspace_module_index_for(self, workspace, config, ws_id);
            if let Some(node_id) = index.find_module_node(module_path) {
                return Some(node_id);
            }
        }
        None
    }

    /// Module tree node details.
    pub fn module_node_info(&self, node_id: ModuleNodeId) -> Option<ModuleNode> {
        let workspace = self.workspace_input()?;
        let config = self.config_input()?;
        let index =
            query::workspace_module_index_for(self, workspace, config, node_id.workspace_id);
        index.module_node(node_id).cloned()
    }

    /// File id list under a module tree node.
    pub fn module_node_file_ids(&self, node_id: ModuleNodeId) -> Vec<FileId> {
        let (Some(workspace), Some(config)) = (self.workspace_input(), self.config_input()) else {
            return Vec::new();
        };
        let index =
            query::workspace_module_index_for(self, workspace, config, node_id.workspace_id);
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
