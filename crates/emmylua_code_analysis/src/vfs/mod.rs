mod collect_workspace_files;
mod document;
mod file_id;
mod file_uri_handler;
mod loader;
mod virtual_url;

pub use collect_workspace_files::*;
pub use document::LuaDocument;
use emmylua_parser::{LineIndex, LuaParseError, LuaParser, LuaSyntaxTree};
pub use file_id::{FileId, InFiled};
pub use file_uri_handler::{file_path_to_uri, uri_to_file_path};
use hashbrown::{HashMap, HashSet};
pub use loader::{LuaFileInfo, load_workspace_files, read_file_with_encoding};
use lsp_types::Uri;
use rowan::NodeCache;
use std::path::PathBuf;
use std::sync::Arc;
pub use virtual_url::VirtualUrlGenerator;

use crate::Emmyrc;

#[derive(Debug)]
pub struct Vfs {
    files: Vec<FileData>,
    protected_paths: HashSet<PathBuf>,
    next_file_id: u32,
    uri_map: HashMap<Uri, FileId>,
    path_map: HashMap<PathBuf, FileId>,
    emmyrc: Option<Arc<Emmyrc>>,
    node_cache: NodeCache,
}

impl Default for Vfs {
    fn default() -> Self {
        Self::new()
    }
}

impl Vfs {
    pub fn new() -> Self {
        Vfs {
            files: Vec::new(),
            protected_paths: HashSet::new(),
            next_file_id: 0,
            uri_map: HashMap::new(),
            path_map: HashMap::new(),
            emmyrc: None,
            node_cache: NodeCache::default(),
        }
    }

    pub(crate) fn file(&self, file_id: FileId) -> Option<&FileData> {
        let index = file_id.id as usize;
        self.files
            .get(index)
            .filter(|file| file.file_id != FileId::VIRTUAL)
    }

    pub(crate) fn file_ids(&self) -> Vec<FileId> {
        self.files
            .iter()
            .filter(|file| file.file_id != FileId::VIRTUAL)
            .map(|file| file.file_id)
            .collect()
    }

    pub(crate) fn len(&self) -> usize {
        self.files
            .iter()
            .filter(|file| file.file_id != FileId::VIRTUAL)
            .count()
    }

    pub(crate) fn protected_paths(&self) -> &HashSet<PathBuf> {
        &self.protected_paths
    }

    pub(crate) fn set_protected_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        self.protected_paths = paths.into_iter().collect();
    }

    pub(crate) fn lookup_by_path(&self, path: &PathBuf) -> Option<FileId> {
        self.path_map.get(path).copied()
    }

    pub(crate) fn lookup_by_uri(&self, uri: &Uri) -> Option<FileId> {
        self.uri_map.get(uri).cloned()
    }

    pub(crate) fn insert_at(
        &mut self,
        file_id: FileId,
        uri: Option<Uri>,
        path: Option<PathBuf>,
        text: String,
    ) {
        let index = file_id.id as usize;
        if file_id.id >= self.next_file_id {
            self.next_file_id = file_id.id + 1;
        }

        if index < self.files.len() {
            let old_uri = self.files[index].uri.clone();
            let old_path = self.files[index].path.clone();
            if let Some(uri) = old_uri {
                self.uri_map.remove(&uri);
            }
            if let Some(path) = old_path {
                self.path_map.remove(&path);
            }
        }

        let line_index = Arc::new(LineIndex::parse(&text));
        let tree = self.emmyrc.as_ref().map(|emmyrc| {
            let parse_config = emmyrc.get_parse_config(&mut self.node_cache);
            Arc::new(LuaParser::parse(&text, parse_config))
        });

        let file_data = FileData::new(file_id, uri, path, text, line_index, tree);
        if index < self.files.len() {
            self.files[index] = file_data;
        } else if index == self.files.len() {
            self.files.push(file_data);
        } else {
            self.files.resize_with(index, || {
                FileData::new(
                    FileId::VIRTUAL,
                    None,
                    None,
                    "",
                    Arc::new(LineIndex::parse("")),
                    None,
                )
            });
            self.files.push(file_data);
        }

        let file = &self.files[index];
        if let Some(uri) = &file.uri {
            self.uri_map.insert(uri.clone(), file_id);
        }
        if let Some(path) = &file.path {
            self.path_map.insert(path.clone(), file_id);
        }
    }

    pub(crate) fn remove(&mut self, file_id: FileId) {
        let index = file_id.id as usize;
        if let Some(file_data) = self.files.get(index) {
            let old_uri = file_data.uri.clone();
            let old_path = file_data.path.clone();
            if let Some(uri) = old_uri {
                self.uri_map.remove(&uri);
            }
            if let Some(path) = old_path {
                self.path_map.remove(&path);
            }
        }
        if let Some(file_data) = self.files.get_mut(index) {
            file_data.file_id = FileId::VIRTUAL;
            file_data.uri = None;
            file_data.path = None;
            file_data.text = Arc::from("");
            file_data.line_index = Arc::new(LineIndex::parse(""));
            file_data.tree = None;
        }
    }

    fn allocate_id(&mut self) -> FileId {
        let id = FileId::new(self.next_file_id);
        self.next_file_id += 1;
        id
    }

    pub(crate) fn allocate_file_id(&mut self) -> FileId {
        self.allocate_id()
    }

    pub(crate) fn line_index(&self, file_id: FileId) -> Option<&LineIndex> {
        self.file(file_id).map(|file| file.line_index.as_ref())
    }

    pub fn file_id(&mut self, uri: &Uri) -> FileId {
        self.get_file_id(uri).unwrap_or_else(|| self.allocate_id())
    }

    pub fn get_file_id(&self, uri: &Uri) -> Option<FileId> {
        self.lookup_by_uri(uri)
    }

    pub fn get_uri(&self, id: &FileId) -> Option<Uri> {
        self.file(*id).and_then(|file| file.uri.clone())
    }

    pub fn get_file_path(&self, id: &FileId) -> Option<&PathBuf> {
        self.file(*id).and_then(|file| file.path.as_ref())
    }

    pub fn set_file_content(&mut self, uri: &Uri, data: Option<String>) -> FileId {
        let fid = self.file_id(uri);

        if let Some(data) = data {
            let path = uri_to_file_path(uri);
            self.insert_at(fid, Some(uri.clone()), path, data);
        } else {
            self.remove(fid);
        }
        fid
    }

    pub fn remove_file(&mut self, uri: &Uri) -> Option<FileId> {
        let fid = self.get_file_id(uri)?;
        self.remove(fid);
        Some(fid)
    }

    pub fn update_config(&mut self, emmyrc: Arc<Emmyrc>) {
        self.emmyrc = Some(emmyrc);
    }

    pub fn get_file_content(&self, id: &FileId) -> Option<&str> {
        self.file(*id).map(|file| file.text.as_ref())
    }

    pub fn get_document(&self, id: &FileId) -> Option<LuaDocument<'_>> {
        let file = self.file(*id)?;
        let path = file.path.as_ref()?;
        Some(LuaDocument::new(*id, path, &file.text, &file.line_index))
    }

    pub fn get_syntax_tree(&self, id: &FileId) -> Option<&LuaSyntaxTree> {
        self.file(*id).and_then(|file| file.tree.as_deref())
    }

    pub fn get_file_parse_error(&self, id: &FileId) -> Option<Vec<LuaParseError>> {
        let tree = self.get_syntax_tree(id)?;
        let errors = tree.get_errors();
        if errors.is_empty() {
            return None;
        }

        Some(errors.to_vec())
    }

    pub fn clear(&mut self) {
        self.files.clear();
        self.protected_paths.clear();
        self.next_file_id = 0;
        self.uri_map.clear();
        self.path_map.clear();
        self.emmyrc = None;
        self.node_cache = NodeCache::default();
    }
}

/// Plain file data with its parsed line index / syntax tree.
#[derive(Debug, Clone)]
pub(crate) struct FileData {
    pub(crate) file_id: FileId,
    pub(crate) uri: Option<Uri>,
    pub(crate) path: Option<PathBuf>,
    pub(crate) text: Arc<str>,
    pub(crate) line_index: Arc<LineIndex>,
    pub(crate) tree: Option<Arc<LuaSyntaxTree>>,
}

impl FileData {
    pub(crate) fn new(
        file_id: FileId,
        uri: Option<Uri>,
        path: Option<PathBuf>,
        text: impl Into<Arc<str>>,
        line_index: Arc<LineIndex>,
        tree: Option<Arc<LuaSyntaxTree>>,
    ) -> Self {
        Self {
            file_id,
            uri,
            path,
            text: text.into(),
            line_index,
            tree,
        }
    }
}
