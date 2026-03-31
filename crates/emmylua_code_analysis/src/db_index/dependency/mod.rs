mod file_dependency_relation;

use hashbrown::{HashMap, HashSet};

use file_dependency_relation::FileDependencyRelation;

use crate::FileId;

use super::LuaIndex;

#[derive(Debug)]
pub struct LuaDependencyIndex {
    dependencies: HashMap<FileId, HashSet<FileId>>,
}

impl Default for LuaDependencyIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl LuaDependencyIndex {
    pub fn new() -> Self {
        Self {
            dependencies: HashMap::new(),
        }
    }

    pub fn add_required_file(&mut self, file_id: FileId, dependency_id: FileId) {
        self.dependencies
            .entry(file_id)
            .or_default()
            .insert(dependency_id);
    }

    pub fn get_required_files(&self, file_id: &FileId) -> Option<&HashSet<FileId>> {
        self.dependencies.get(file_id)
    }

    pub fn get_file_dependencies<'a>(&'a self) -> FileDependencyRelation<'a> {
        FileDependencyRelation::new(&self.dependencies)
    }
}

impl LuaIndex for LuaDependencyIndex {
    fn remove(&mut self, file_id: FileId) {
        self.dependencies.remove(&file_id);
    }

    fn clear(&mut self) {
        self.dependencies.clear();
    }
}
