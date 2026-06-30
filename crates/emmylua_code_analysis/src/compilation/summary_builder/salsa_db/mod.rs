mod facade;
pub(crate) mod inputs;
#[cfg(test)]
mod tests;
mod tracked;

use std::{fmt, path::PathBuf, sync::Arc};

use emmylua_parser::{LuaParser, LuaSyntaxTree};
use hashbrown::HashMap;
use lsp_types::Uri;
use rowan::NodeCache;

use crate::{Emmyrc, FileId};
use inputs::SummarySourceFileInput;

pub use facade::{
    SalsaSummaryDocQueries, SalsaSummaryFileQueries, SalsaSummaryFlowQueries,
    SalsaSummaryLexicalQueries, SalsaSummaryModuleQueries, SalsaSummarySemanticQueries,
    SalsaSummaryTypeQueries,
};
use inputs::{SalsaSummaryConfig, SummaryConfigInput};

#[salsa::db]
pub trait SummaryDb: salsa::Database {
    /// Look up a cached syntax tree. Returns `None` if not yet parsed.
    fn lookup_syntax_tree(&self, file_id: FileId) -> Option<&LuaSyntaxTree>;
}

#[salsa::db]
pub struct SalsaSummaryDatabase {
    storage: salsa::Storage<Self>,

    // ── VFS: URI/path ↔ FileId mapping ──
    path_to_file_id: HashMap<PathBuf, FileId>,
    file_id_to_path: HashMap<FileId, PathBuf>,
    remote_uri_to_file_id: HashMap<Uri, FileId>,

    // ── Salsa inputs ──
    files: HashMap<FileId, SummarySourceFileInput>,
    config: Option<SummaryConfigInput>,

    // ── Syntax tree cache (LuaSyntaxTree uses GreenNode → Send + Sync) ──
    // Writes only through `set_file` (`&mut self`), reads through `&self`.
    // No lock needed — Rust's borrow checker guarantees mutual exclusion.
    syntax_trees: HashMap<FileId, LuaSyntaxTree>,
}

impl Default for SalsaSummaryDatabase {
    fn default() -> Self {
        Self {
            storage: salsa::Storage::default(),
            path_to_file_id: HashMap::new(),
            file_id_to_path: HashMap::new(),
            remote_uri_to_file_id: HashMap::new(),
            files: HashMap::new(),
            config: None,
            syntax_trees: HashMap::new(),
        }
    }
}

impl SalsaSummaryDatabase {
    pub(crate) fn config_input(&self) -> Option<SummaryConfigInput> {
        self.config
    }

    pub(crate) fn file_ids(&self) -> Vec<FileId> {
        self.files.keys().copied().collect()
    }
}

#[salsa::db]
impl salsa::Database for SalsaSummaryDatabase {}

#[salsa::db]
impl SummaryDb for SalsaSummaryDatabase {
    fn lookup_syntax_tree(&self, file_id: FileId) -> Option<&LuaSyntaxTree> {
        self.syntax_trees.get(&file_id)
    }
}

impl fmt::Debug for SalsaSummaryDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SalsaSummaryDatabase")
            .field("file_count", &self.files.len())
            .field("has_config", &self.config.is_some())
            .finish()
    }
}

// ── Public API ──
impl SalsaSummaryDatabase {
    // ── Config ──

    pub fn update_config(&mut self, emmyrc: Arc<Emmyrc>) {
        self.config = Some(SummaryConfigInput::new(
            self,
            SalsaSummaryConfig::from_emmyrc(emmyrc),
        ));
    }

    // ── URI / FileId mapping ──

    /// Look up an existing FileId for a URI (does not create).
    pub fn lookup_file_id(&self, uri: &Uri) -> Option<FileId> {
        if let Some(path) = uri_to_file_path(uri) {
            self.path_to_file_id.get(&path).copied()
        } else {
            self.remote_uri_to_file_id.get(uri).copied()
        }
    }

    /// Intern a URI and return its FileId, creating one if not yet known.
    pub fn intern_uri(&mut self, uri: &Uri) -> FileId {
        if let Some(path) = uri_to_file_path(uri) {
            if let Some(&id) = self.path_to_file_id.get(&path) {
                return id;
            }
            let id = self.next_file_id();
            self.path_to_file_id.insert(path.clone(), id);
            self.file_id_to_path.insert(id, path);
            id
        } else {
            if let Some(&id) = self.remote_uri_to_file_id.get(uri) {
                return id;
            }
            let id = self.next_file_id();
            self.remote_uri_to_file_id.insert(uri.clone(), id);
            id
        }
    }

    /// Get the URI for a FileId.
    pub fn file_uri(&self, file_id: FileId) -> Option<Uri> {
        let path = self.file_id_to_path.get(&file_id)?;
        file_path_to_uri(path)
    }

    /// Get the file path for a FileId.
    pub fn file_path(&self, file_id: FileId) -> Option<&PathBuf> {
        self.file_id_to_path.get(&file_id)
    }

    fn next_file_id(&self) -> FileId {
        let mut id = self.files.len() as u32;
        loop {
            let fid = FileId::new(id);
            if !self.files.contains_key(&fid) {
                return fid;
            }
            id += 1;
        }
    }

    // ── File management ──

    /// Set file content by URI. Creates FileId if needed.
    /// `text` = `None` removes the file.
    pub fn set_file_content(&mut self, uri: &Uri, text: Option<String>) -> FileId {
        let fid = self.intern_uri(uri);
        if let Some(text) = text {
            let path = self.file_id_to_path.get(&fid).cloned();
            let is_remote = self.remote_uri_to_file_id.contains_key(uri);
            self.set_file(fid, path, text, is_remote);
        } else {
            self.remove_file(fid);
        }
        fid
    }

    /// Set file content by FileId (direct, for batch updates).
    /// Parses and caches the syntax tree immediately (`&mut self`).
    pub fn set_file(
        &mut self,
        file_id: FileId,
        path: Option<PathBuf>,
        text: String,
        is_remote: bool,
    ) {
        // Track path mapping
        if let Some(ref p) = path {
            self.path_to_file_id.insert(p.clone(), file_id);
            self.file_id_to_path.insert(file_id, p.clone());
        }

        // Parse and cache the syntax tree now.
        // All subsequent `parse_chunk` calls will hit the cache.
        if let Some(config) = self.config {
            let cfg = config.config(self);
            let mut node_cache = NodeCache::default();
            let parse_config = cfg.to_parse_config(&mut node_cache);
            let tree = LuaParser::parse(&text, parse_config);
            self.syntax_trees.insert(file_id, tree);
        }

        let input = SummarySourceFileInput::new(self, file_id, path, text, is_remote);
        self.files.insert(file_id, input);
    }

    /// Get file text from salsa input (zero-copy via `#[returns(ref)]`).
    pub fn get_file_text(&self, file_id: FileId) -> Option<&str> {
        let input = self.files.get(&file_id)?;
        Some(input.text(self))
    }

    /// Check if a file is remote.
    pub fn is_remote_file(&self, file_id: FileId) -> bool {
        self.files
            .get(&file_id)
            .map(|f| f.is_remote(self))
            .unwrap_or(false)
    }

    /// Remove a file by FileId.
    pub fn remove_file(&mut self, file_id: FileId) {
        self.files.remove(&file_id);
        self.syntax_trees.remove(&file_id);

        if let Some(path) = self.file_id_to_path.remove(&file_id) {
            self.path_to_file_id.remove(&path);
        }
        self.remote_uri_to_file_id
            .retain(|_, &mut fid| fid != file_id);
    }

    /// Remove a file by URI.
    pub fn remove_file_by_uri(&mut self, uri: &Uri) -> Option<FileId> {
        let fid = self.lookup_file_id(uri)?;
        self.remove_file(fid);
        Some(fid)
    }

    /// Clear all state.
    pub fn clear(&mut self) {
        self.files.clear();
        self.path_to_file_id.clear();
        self.file_id_to_path.clear();
        self.remote_uri_to_file_id.clear();
        self.syntax_trees.clear();
    }

    /// Clone a cached syntax tree (for callers that need ownership).
    pub fn get_syntax_tree(&self, file_id: FileId) -> Option<LuaSyntaxTree> {
        self.syntax_trees.get(&file_id).cloned()
    }

    /// Get parse errors for a file (from cached syntax tree).
    pub fn get_file_parse_error(
        &self,
        file_id: FileId,
    ) -> Option<Vec<emmylua_parser::LuaParseError>> {
        let tree = self.get_syntax_tree(file_id)?;
        let errors = tree.get_errors().to_vec();
        if errors.is_empty() {
            None
        } else {
            Some(errors)
        }
    }

    /// Convert a text offset to (line, col) — 0-based.
    pub fn offset_to_line_col(
        &self,
        file_id: FileId,
        offset: rowan::TextSize,
    ) -> Option<(usize, usize)> {
        use emmylua_parser::LineIndex;
        let input = self.files.get(&file_id)?;
        let cached = tracked::tracked_line_index(self, *input);
        let text = input.text(self);
        cached.0.get_line_col(offset, text)
    }

    // ── Facade accessors ──

    pub fn file(&self) -> SalsaSummaryFileQueries<'_> {
        SalsaSummaryFileQueries::new(self)
    }

    pub fn doc(&self) -> SalsaSummaryDocQueries<'_> {
        SalsaSummaryDocQueries::new(self)
    }

    pub fn lexical(&self) -> SalsaSummaryLexicalQueries<'_> {
        SalsaSummaryLexicalQueries::new(self)
    }

    pub fn flow(&self) -> SalsaSummaryFlowQueries<'_> {
        SalsaSummaryFlowQueries::new(self)
    }

    pub fn module(&self) -> SalsaSummaryModuleQueries<'_> {
        SalsaSummaryModuleQueries::new(self)
    }

    pub fn semantic(&self) -> SalsaSummarySemanticQueries<'_> {
        SalsaSummarySemanticQueries::new(self)
    }

    pub fn types(&self) -> SalsaSummaryTypeQueries<'_> {
        SalsaSummaryTypeQueries::new(self)
    }
}

// ── Re-export URI helpers ──
use crate::vfs::{file_path_to_uri, uri_to_file_path};
