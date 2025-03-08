mod document;
mod file_id;
mod file_uri_handler;
mod loader;
mod virtual_url;

pub use document::LuaDocument;
use emmylua_parser::{LineIndex, LuaParser, LuaSyntaxTree};
pub use file_id::{FileId, InFiled};
pub use file_uri_handler::{file_path_to_uri, uri_to_file_path};
pub use loader::{load_workspace_files, read_file_with_encoding, LuaFileInfo};
use lsp_types::Uri;
use rowan::{NodeCache, TextRange};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
pub use virtual_url::VirtualUrlGenerator;

use crate::Emmyrc;

#[derive(Debug)]
pub struct Vfs {
    file_id_map: HashMap<PathBuf, u32>,
    file_path_map: HashMap<u32, PathBuf>,
    file_data: Vec<Option<String>>,
    line_index_map: HashMap<FileId, LineIndex>,
    tree_map: HashMap<FileId, LuaSyntaxTree>,
    emmyrc: Option<Arc<Emmyrc>>,
    node_cache: NodeCache,
}

impl Vfs {
    pub fn new() -> Self {
        Vfs {
            file_id_map: HashMap::new(),
            file_path_map: HashMap::new(),
            file_data: Vec::new(),
            line_index_map: HashMap::new(),
            tree_map: HashMap::new(),
            emmyrc: None,
            node_cache: NodeCache::default(),
        }
    }

    pub fn file_id(&mut self, uri: &Uri) -> FileId {
        let path = uri_to_file_path(uri).unwrap();
        if let Some(&id) = self.file_id_map.get(&path) {
            FileId { id }
        } else {
            let id = self.file_data.len() as u32;
            self.file_id_map.insert(path.clone(), id);
            self.file_path_map.insert(id, path);
            self.file_data.push(None);
            FileId { id }
        }
    }

    pub fn get_file_id(&self, uri: &Uri) -> Option<FileId> {
        let path = uri_to_file_path(uri)?;
        self.file_id_map.get(&path).map(|&id| FileId { id })
    }

    pub fn get_uri(&self, id: &FileId) -> Option<Uri> {
        let path = self.file_path_map.get(&id.id)?;
        Some(file_path_to_uri(path)?)
    }

    pub fn get_file_path(&self, id: &FileId) -> Option<&PathBuf> {
        self.file_path_map.get(&id.id)
    }

    pub fn set_file_content(&mut self, uri: &Uri, data: Option<String>) -> FileId {
        let fid = self.file_id(uri);
        log::debug!("file_id: {:?}, uri: {}", fid, uri.as_str());

        if let Some(data) = &data {
            let line_index = LineIndex::parse(&data);
            let parse_config = self
                .emmyrc
                .as_ref()
                .unwrap()
                .get_parse_config(&mut self.node_cache);
            let tree = LuaParser::parse(&data, parse_config);
            self.tree_map.insert(fid, tree);
            self.line_index_map.insert(fid, line_index);
        } else {
            self.line_index_map.remove(&fid);
            self.tree_map.remove(&fid);
        }
        self.file_data[fid.id as usize] = data;
        fid
    }

    pub fn update_config(&mut self, emmyrc: Arc<Emmyrc>) {
        self.emmyrc = Some(emmyrc);
    }

    pub fn get_file_content(&self, id: &FileId) -> Option<&String> {
        let opt = &self.file_data[id.id as usize];
        if let Some(s) = opt {
            Some(s)
        } else {
            None
        }
    }

    pub fn get_document(&self, id: &FileId) -> Option<LuaDocument> {
        let path = self.file_path_map.get(&id.id)?;
        let text = self.get_file_content(id)?;
        let line_index = self.line_index_map.get(id)?;
        Some(LuaDocument::new(*id, path, text, line_index))
    }

    pub fn get_syntax_tree(&self, id: &FileId) -> Option<&LuaSyntaxTree> {
        self.tree_map.get(id)
    }

    pub fn get_file_parse_error(&self, id: &FileId) -> Option<Vec<(String, TextRange)>> {
        let mut errors = Vec::new();
        let tree = self.tree_map.get(id)?;
        for error in tree.get_errors() {
            errors.push((error.message.clone(), error.range.clone()));
        }

        if errors.is_empty() {
            None
        } else {
            Some(errors)
        }
    }

    pub fn get_all_file_ids(&self) -> Vec<FileId> {
        self.file_data
            .iter()
            .enumerate()
            .filter_map(|(id, _)| {
                if id == FileId::VIRTUAL.id as usize {
                    None
                } else {
                    Some(FileId { id: id as u32 })
                }
            })
            .collect()
    }
}
