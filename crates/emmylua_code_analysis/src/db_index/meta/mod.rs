use std::collections::HashSet;

use crate::FileId;

use super::traits::LuaIndex;

#[derive(Debug)]
pub struct MetaFile {
    meta_files: HashSet<FileId>,
}

impl MetaFile {
    pub fn new() -> Self {
        Self {
            meta_files: HashSet::new(),
        }
    }

    pub fn add_meta_file(&mut self, file_id: FileId) {
        self.meta_files.insert(file_id);
    }

    pub fn is_meta_file(&self, file_id: &FileId) -> bool {
        self.meta_files.contains(file_id)
    }
}

impl LuaIndex for MetaFile {
    fn remove(&mut self, file_id: FileId) {
        self.meta_files.remove(&file_id);
    }

    fn clear(&mut self) {
        self.meta_files.clear();
    }
}
