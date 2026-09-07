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
use hashbrown::HashMap;
pub use loader::{LuaFileInfo, load_workspace_files, read_file_with_encoding};
use lsp_types::Uri;
use rowan::NodeCache;
use std::collections::{HashMap as StdHashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
pub use virtual_url::VirtualUrlGenerator;

use crate::Emmyrc;
use crate::analysis_state::FileData;

#[derive(Debug)]
pub struct Vfs {
    file_id_map: HashMap<PathBuf, FileId>,
    file_path_map: HashMap<u32, PathBuf>,
    remote_file_id_map: HashMap<Uri, FileId>,
    files: StdHashMap<FileId, FileData>,
    protected_paths: HashSet<PathBuf>,
    next_file_id: u32,
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
            file_id_map: HashMap::new(),
            file_path_map: HashMap::new(),
            remote_file_id_map: HashMap::new(),
            files: StdHashMap::new(),
            protected_paths: HashSet::new(),
            next_file_id: 0,
            line_index_map: HashMap::new(),
            tree_map: HashMap::new(),
            emmyrc: None,
            node_cache: NodeCache::default(),
        }
    }

    pub(crate) fn file(&self, file_id: FileId) -> Option<&FileData> {
        self.files.get(&file_id)
    }

    pub(crate) fn file_ids(&self) -> Vec<FileId> {
        let mut ids: Vec<FileId> = self.files.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    pub(crate) fn len(&self) -> usize {
        self.files.len()
    }

    pub(crate) fn files(&self) -> &StdHashMap<FileId, FileData> {
        &self.files
    }

    pub(crate) fn protected_paths(&self) -> &HashSet<PathBuf> {
        &self.protected_paths
    }

    pub(crate) fn set_protected_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        self.protected_paths = paths.into_iter().collect();
    }

    pub(crate) fn lookup_by_path(&self, path: &PathBuf) -> Option<FileId> {
        self.file_id_map.get(path).copied()
    }

    pub(crate) fn lookup_by_uri(&self, uri: &Uri) -> Option<FileId> {
        if let Some(path) = uri_to_file_path(uri) {
            if let Some(id) = self.file_id_map.get(&path) {
                return Some(*id);
            }
        }
        self.remote_file_id_map.get(uri).copied()
    }

    pub(crate) fn insert_at(
        &mut self,
        file_id: FileId,
        uri: Option<Uri>,
        path: Option<PathBuf>,
        text: String,
    ) {
        if let Some(path) = &path {
            self.file_id_map.insert(path.clone(), file_id);
            self.file_path_map.insert(file_id.id, path.clone());
        }
        if let Some(uri) = &uri {
            if path.is_none() {
                self.remote_file_id_map.insert(uri.clone(), file_id);
            }
        }
        self.files
            .insert(file_id, FileData::new(file_id, uri, path, Arc::from(text)));
        if file_id.id >= self.next_file_id {
            self.next_file_id = file_id.id + 1;
        }
    }

    pub(crate) fn remove(&mut self, file_id: FileId) -> Option<FileData> {
        let removed = self.files.remove(&file_id);
        if let Some(file) = &removed {
            if let Some(path) = &file.path {
                self.file_id_map.remove(path);
            }
            if let Some(uri) = &file.uri {
                self.remote_file_id_map.remove(uri);
            }
        }
        self.file_path_map.remove(&file_id.id);
        removed
    }

    fn allocate_id(&mut self) -> FileId {
        let id = FileId::new(self.next_file_id);
        self.next_file_id += 1;
        id
    }

    pub fn file_id(&mut self, uri: &Uri) -> FileId {
        if let Some(path) = uri_to_file_path(uri) {
            if let Some(&id) = self.file_id_map.get(&path) {
                return id;
            }
            let id = self.allocate_id();
            self.file_id_map.insert(path.clone(), id);
            self.file_path_map.insert(id.id, path);
            id
        } else {
            if let Some(id) = self.remote_file_id_map.get(uri) {
                return *id;
            }
            let id = self.allocate_id();
            self.remote_file_id_map.insert(uri.clone(), id);
            id
        }
    }

    fn virtual_file_id(&mut self, uri: &Uri) -> FileId {
        if let Some(id) = self.remote_file_id_map.get(uri) {
            *id
        } else {
            let id = self.allocate_id();
            self.remote_file_id_map.insert(uri.clone(), id);
            id
        }
    }

    pub fn get_file_id(&self, uri: &Uri) -> Option<FileId> {
        if let Some(path) = uri_to_file_path(uri) {
            if let Some(id) = self.file_id_map.get(&path) {
                return Some(*id);
            }
        }
        self.remote_file_id_map.get(uri).copied()
    }

    pub fn get_uri(&self, id: &FileId) -> Option<Uri> {
        let path = self.file_path_map.get(&id.id)?;
        file_path_to_uri(path)
    }

    pub fn get_file_path(&self, id: &FileId) -> Option<&PathBuf> {
        self.file_path_map.get(&id.id)
    }

    pub fn set_file_content(&mut self, uri: &Uri, data: Option<String>) -> FileId {
        let fid = self.file_id(uri);
        log::debug!("file_id: {:?}, uri: {}", fid, uri.as_str());

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
        if let Some(path) = self.file_path_map.remove(&fid.id) {
            self.file_id_map.remove(&path);
        }
        self.line_index_map.remove(&fid);
        self.tree_map.remove(&fid);
        self.remove(fid);
        Some(fid)
    }

    pub fn update_config(&mut self, emmyrc: Arc<Emmyrc>) {
        self.emmyrc = Some(emmyrc);
    }

    pub fn get_file_content(&self, id: &FileId) -> Option<&str> {
        self.files.get(id).map(|file| file.text.as_ref())
    }

    pub fn get_document(&self, id: &FileId) -> Option<LuaDocument<'_>> {
        let path = self.file_path_map.get(&id.id)?;
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

    pub fn get_all_local_file_ids(&self) -> Vec<FileId> {
        self.files
            .iter()
            .filter(|(_, file)| file.path.is_some())
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn get_all_file_ids(&self) -> Vec<FileId> {
        self.files.keys().copied().collect()
    }

    pub fn is_remote_file(&self, id: &FileId) -> bool {
        self.files.get(id).is_some_and(|file| file.path.is_none())
    }

    pub fn clear(&mut self) {
        self.files.clear();
        self.protected_paths.clear();
        self.next_file_id = 0;
        self.file_id_map.clear();
        self.file_path_map.clear();
        self.line_index_map.clear();
        self.tree_map.clear();
        self.emmyrc = None;
        self.node_cache = NodeCache::default();
    }
}
