//! # VFS stable mount point
//!
//! An immutable filesystem snapshot outside salsa: `SalsaDatabase` holds `Arc<VfsSnapshot>`,
//! sharing the same instance across clones; write paths publish a new snapshot while updating salsa inputs.
//! External URI/Path/Text lookups go through here; salsa queries still get precise dependencies via `SourceFileInput`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use lsp_types::Uri;

use crate::FileId;

use super::inputs::SourceFileInput;

/// VFS entry for a single file.
///
/// `salsa_input` is used only inside `SalsaDatabase`; external code should access
/// through `VfsSnapshot`'s path/uri to avoid holding salsa input handles across versions.
/// File text is not stored here: text updates only write `SourceFileInput`, avoiding O(n) VFS clones per input.
#[derive(Debug, Clone)]
pub(crate) struct FileEntry {
    pub(crate) path: Option<PathBuf>,
    pub(crate) uri: Option<Uri>,
    pub(crate) salsa_input: SourceFileInput,
}

/// Immutable VFS snapshot.
#[derive(Debug, Clone)]
pub(crate) struct VfsSnapshot {
    files: Arc<HashMap<FileId, FileEntry>>,
    path_to_file_id: Arc<HashMap<PathBuf, FileId>>,
    uri_to_file_id: Arc<HashMap<Uri, FileId>>,
    file_ids: Arc<[FileId]>,
    protected_paths: Arc<HashSet<PathBuf>>,
}

impl VfsSnapshot {
    pub(crate) fn empty() -> Self {
        Self {
            files: Arc::new(HashMap::new()),
            path_to_file_id: Arc::new(HashMap::new()),
            uri_to_file_id: Arc::new(HashMap::new()),
            file_ids: Arc::from(Vec::<FileId>::new()),
            protected_paths: Arc::new(HashSet::new()),
        }
    }

    pub(crate) fn from_parts(
        files: HashMap<FileId, FileEntry>,
        file_ids: Arc<[FileId]>,
        protected_paths: Arc<HashSet<PathBuf>>,
    ) -> Self {
        let mut path_to_file_id = HashMap::new();
        let mut uri_to_file_id = HashMap::new();
        for (file_id, entry) in &files {
            if let Some(path) = &entry.path {
                path_to_file_id.insert(path.clone(), *file_id);
            }
            if let Some(uri) = &entry.uri {
                uri_to_file_id.insert(uri.clone(), *file_id);
            }
        }
        Self {
            files: Arc::new(files),
            path_to_file_id: Arc::new(path_to_file_id),
            uri_to_file_id: Arc::new(uri_to_file_id),
            file_ids,
            protected_paths,
        }
    }

    pub(crate) fn files(&self) -> &Arc<HashMap<FileId, FileEntry>> {
        &self.files
    }

    pub(crate) fn path_to_file_id(&self) -> &Arc<HashMap<PathBuf, FileId>> {
        &self.path_to_file_id
    }

    pub(crate) fn uri_to_file_id(&self) -> &Arc<HashMap<Uri, FileId>> {
        &self.uri_to_file_id
    }

    pub(crate) fn file_ids(&self) -> &Arc<[FileId]> {
        &self.file_ids
    }

    pub(crate) fn protected_paths(&self) -> &Arc<HashSet<PathBuf>> {
        &self.protected_paths
    }

    pub(crate) fn file_entry(&self, file_id: FileId) -> Option<&FileEntry> {
        self.files.get(&file_id)
    }

    /// Insert or update a file entry (in place; O(1) on write paths without clone).
    pub(crate) fn insert_entry(&mut self, file_id: FileId, entry: FileEntry) {
        let files = Arc::make_mut(&mut self.files);
        if let Some(old) = files.get(&file_id) {
            if let Some(old_path) = &old.path {
                Arc::make_mut(&mut self.path_to_file_id).remove(old_path);
            }
            if let Some(old_uri) = &old.uri {
                Arc::make_mut(&mut self.uri_to_file_id).remove(old_uri);
            }
        }
        if let Some(path) = &entry.path {
            Arc::make_mut(&mut self.path_to_file_id).insert(path.clone(), file_id);
        }
        if let Some(uri) = &entry.uri {
            Arc::make_mut(&mut self.uri_to_file_id).insert(uri.clone(), file_id);
        }
        files.insert(file_id, entry);
    }

    /// Remove a file entry (in place).
    pub(crate) fn remove_entry(&mut self, file_id: FileId) {
        let files = Arc::make_mut(&mut self.files);
        if let Some(old) = files.remove(&file_id) {
            if let Some(old_path) = &old.path {
                Arc::make_mut(&mut self.path_to_file_id).remove(old_path);
            }
            if let Some(old_uri) = &old.uri {
                Arc::make_mut(&mut self.uri_to_file_id).remove(old_uri);
            }
        }
    }

    /// Replace the file id list.
    pub(crate) fn set_file_ids(&mut self, file_ids: Arc<[FileId]>) {
        self.file_ids = file_ids;
    }

    /// Replace the protected-path set.
    pub(crate) fn set_protected_paths(&mut self, protected_paths: Arc<HashSet<PathBuf>>) {
        self.protected_paths = protected_paths;
    }
}
