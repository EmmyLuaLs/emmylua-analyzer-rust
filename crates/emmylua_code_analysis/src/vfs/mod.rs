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
use rowan::{NodeCache, TextSize};
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
    line_index_map: HashMap<FileId, LineIndex>,
    tree_map: HashMap<FileId, LuaSyntaxTree>,
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
            line_index_map: HashMap::new(),
            tree_map: HashMap::new(),
            emmyrc: None,
            node_cache: NodeCache::default(),
        }
    }

    pub(crate) fn file(&self, file_id: FileId) -> Option<&FileData> {
        let index = file_id.id as usize;
        self.files.get(index)
    }

    pub(crate) fn file_ids(&self) -> Vec<FileId> {
        self.files.iter().map(|file| file.file_id).collect()
    }

    pub(crate) fn len(&self) -> usize {
        self.files.len()
    }

    pub(crate) fn files(&self) -> &Vec<FileData> {
        &self.files
    }

    pub(crate) fn protected_paths(&self) -> &HashSet<PathBuf> {
        &self.protected_paths
    }

    pub(crate) fn set_protected_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        self.protected_paths = paths.into_iter().collect();
    }

    pub(crate) fn lookup_by_path(&self, path: &PathBuf) -> Option<FileId> {
        self.files
            .iter()
            .find(|file| file.path.as_ref() == Some(path))
            .map(|file| file.file_id)
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
        if index >= self.next_file_id as usize {
            return;
        }

        let file_data = FileData::new(file_id, uri, path, text);
        match index {
            i if i < self.files.len() => {
                self.files[i].text = file_data.text;
            }
            i if i == self.files.len() => {
                self.files.push(file_data);
            }
            _ => {}
        }
    }

    pub(crate) fn remove(&mut self, file_id: FileId) {
        let index = file_id.id as usize;
        if let Some(file_data) = self.files.get_mut(index) {
            file_data.uri = None;
            file_data.path = None;
        }
    }

    fn allocate_id(&mut self) -> FileId {
        let id = FileId::new(self.next_file_id);
        self.next_file_id += 1;
        id
    }

    pub fn file_id(&mut self, uri: &Uri) -> FileId {
        self.get_file_id(uri).unwrap_or_else(|| self.allocate_id())
    }

    fn virtual_file_id(&mut self, uri: &Uri) -> FileId {
        self.file_id(uri)
    }

    pub fn get_file_id(&self, uri: &Uri) -> Option<FileId> {
        self.lookup_by_uri(uri)
    }

    pub fn get_uri(&self, id: &FileId) -> Option<Uri> {
        self.files.get(id.id as usize).and_then(|file| file.uri.clone())
    }

    pub fn get_file_path(&self, id: &FileId) -> Option<&PathBuf> {
        self.files.get(id.id as usize).and_then(|file| file.path.as_ref())
    }

    pub fn set_file_content(&mut self, uri: &Uri, data: Option<String>) -> FileId {
        let fid = self.file_id(uri);

        if let Some(data) = data {
            let line_index = LineIndex::parse(&data);
            let parse_config = self
                .emmyrc
                .as_ref()
                .expect("emmyrc set")
                .get_parse_config(&mut self.node_cache);
            let tree = LuaParser::parse(&data, parse_config);
            self.tree_map.insert(fid, tree);
            self.line_index_map.insert(fid, line_index);
            let path = uri_to_file_path(uri);
            self.insert_at(fid, Some(uri.clone()), path, data);
        } else {
            self.line_index_map.remove(&fid);
            self.tree_map.remove(&fid);
            self.remove(fid);
        }
        fid
    }

    pub fn set_remote_file_content(&mut self, uri: &Uri, data: Option<String>) -> FileId {
        let fid = self.virtual_file_id(&uri);
        log::debug!("virtual file_id: {:?}, uri: {}", fid, uri.as_str());

        if let Some(data) = data {
            let line_index = LineIndex::parse(&data);
            let parse_config = self
                .emmyrc
                .as_ref()
                .expect("emmyrc set")
                .get_parse_config(&mut self.node_cache);
            let tree = LuaParser::parse(&data, parse_config);
            self.tree_map.insert(fid, tree);
            self.line_index_map.insert(fid, line_index);
            self.insert_at(fid, Some(uri.clone()), None, data);
        } else {
            self.line_index_map.remove(&fid);
            self.tree_map.remove(&fid);
            self.remove(fid);
        }
        fid
    }

    pub fn remove_file(&mut self, uri: &Uri) -> Option<FileId> {
        let fid = self.get_file_id(uri)?;
        self.line_index_map.remove(&fid);
        self.tree_map.remove(&fid);
        self.remove(fid);
        Some(fid)
    }

    pub fn update_config(&mut self, emmyrc: Arc<Emmyrc>) {
        self.emmyrc = Some(emmyrc);
    }

    pub fn get_file_content(&self, id: &FileId) -> Option<&str> {
        self.files.get(id.id as usize).map(|file| file.text.as_ref())
    }

    pub fn get_document(&self, id: &FileId) -> Option<LuaDocument<'_>> {
        let path = self.get_file_path(id)?;
        let text = self.get_file_content(id)?;
        let line_index = self.line_index_map.get(id)?;
        Some(LuaDocument::new(*id, path, text, line_index))
    }

    pub fn get_syntax_tree(&self, id: &FileId) -> Option<&LuaSyntaxTree> {
        self.tree_map.get(id)
    }

    pub fn get_file_parse_error(&self, id: &FileId) -> Option<Vec<LuaParseError>> {
        let tree = self.tree_map.get(id)?;
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
        self.line_index_map.clear();
        self.tree_map.clear();
        self.emmyrc = None;
        self.node_cache = NodeCache::default();
    }
}

/// Plain file data.
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
