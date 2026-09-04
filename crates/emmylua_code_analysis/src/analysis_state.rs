//! Phase 0: non-Salsa analysis state skeleton.
//!
//! This module is the target home for the future `Vfs` and `WorkspaceIndex`.
//! It currently contains only the core file container and placeholder index types;
//! migration will move `SalsaDatabase` file-management and workspace-index code here
//! step by step while keeping Salsa working for the semantic layer.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use lsp_types::Uri;
use rowan::TextSize;

use crate::FileId;

/// Plain file data, independent of Salsa inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileData {
    pub file_id: FileId,
    pub uri: Option<Uri>,
    pub path: Option<PathBuf>,
    pub text: Arc<str>,
}

impl FileData {
    pub fn new(
        file_id: FileId,
        uri: Option<Uri>,
        path: Option<PathBuf>,
        text: impl Into<Arc<str>>,
    ) -> Self {
        Self {
            file_id,
            uri,
            path,
            text: text.into(),
        }
    }

    pub fn text_len(&self) -> TextSize {
        TextSize::from(self.text.len() as u32)
    }
}

/// A plain, mutable VFS that does not depend on Salsa.
#[derive(Debug, Clone, Default)]
pub struct VfsState {
    files: HashMap<FileId, FileData>,
    path_to_file_id: HashMap<PathBuf, FileId>,
    uri_to_file_id: HashMap<Uri, FileId>,
    protected_paths: std::collections::HashSet<PathBuf>,
    next_file_id: u32,
}

impl VfsState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn file(&self, file_id: FileId) -> Option<&FileData> {
        self.files.get(&file_id)
    }

    pub fn file_ids(&self) -> Vec<FileId> {
        let mut ids: Vec<FileId> = self.files.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn lookup_by_uri(&self, uri: &Uri) -> Option<FileId> {
        self.uri_to_file_id.get(uri).copied()
    }

    pub fn lookup_by_path(&self, path: &PathBuf) -> Option<FileId> {
        self.path_to_file_id.get(path).copied()
    }

    pub fn insert(&mut self, uri: Uri, path: Option<PathBuf>, text: String) -> FileId {
        let file_id = self.lookup_by_uri(&uri).unwrap_or_else(|| {
            let id = FileId::new(self.next_file_id);
            self.next_file_id += 1;
            id
        });
        self.remove_index_entries(file_id);
        self.files.insert(
            file_id,
            FileData::new(file_id, Some(uri.clone()), path.clone(), text),
        );
        if let Some(path) = path {
            self.path_to_file_id.insert(path, file_id);
        }
        self.uri_to_file_id.insert(uri, file_id);
        file_id
    }

    pub fn insert_at(
        &mut self,
        file_id: FileId,
        uri: Option<Uri>,
        path: Option<PathBuf>,
        text: String,
    ) {
        self.remove_index_entries(file_id);
        if let Some(path) = &path {
            self.path_to_file_id.insert(path.clone(), file_id);
        }
        if let Some(uri) = &uri {
            self.uri_to_file_id.insert(uri.clone(), file_id);
        }
        self.files
            .insert(file_id, FileData::new(file_id, uri, path, text));
        if file_id.id >= self.next_file_id {
            self.next_file_id = file_id.id + 1;
        }
    }

    pub fn protected_paths(&self) -> &std::collections::HashSet<PathBuf> {
        &self.protected_paths
    }

    pub fn set_protected_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        self.protected_paths = paths.into_iter().collect();
    }

    pub fn remove(&mut self, file_id: FileId) -> Option<FileData> {
        let removed = self.files.remove(&file_id);
        if let Some(file) = &removed {
            if let Some(path) = &file.path {
                self.path_to_file_id.remove(path);
            }
            if let Some(uri) = &file.uri {
                self.uri_to_file_id.remove(uri);
            }
        }
        removed
    }

    fn remove_index_entries(&mut self, file_id: FileId) {
        if let Some(old) = self.files.get(&file_id) {
            if let Some(path) = &old.path {
                self.path_to_file_id.remove(path);
            }
            if let Some(uri) = &old.uri {
                self.uri_to_file_id.remove(uri);
            }
        }
    }
}

/// Future home of the non-Salsa workspace index.
#[derive(Debug, Default)]
pub struct WorkspaceIndex {
    // Will contain type/member/decl/module indexes in later phases.
}

/// Top-level non-Salsa analysis state.
#[derive(Debug, Default)]
pub struct AnalysisState {
    pub vfs: VfsState,
    pub workspace: WorkspaceIndex,
}

impl AnalysisState {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn vfs_insert_lookup_remove() {
        let mut vfs = VfsState::new();
        let uri = Uri::from_str("file:///tmp/a.lua").unwrap();
        let id = vfs.insert(
            uri.clone(),
            Some(PathBuf::from("/tmp/a.lua")),
            "local a = 1".into(),
        );

        assert_eq!(vfs.lookup_by_uri(&uri), Some(id));
        assert_eq!(vfs.lookup_by_path(&PathBuf::from("/tmp/a.lua")), Some(id));
        assert_eq!(vfs.file(id).unwrap().text.as_ref(), "local a = 1");

        vfs.remove(id);
        assert!(vfs.file(id).is_none());
        assert!(vfs.lookup_by_uri(&uri).is_none());
    }
}
