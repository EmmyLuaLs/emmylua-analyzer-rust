mod analyzer;
mod test;

use std::sync::Arc;

use crate::{db_index::DbIndex, semantic::SemanticModel, Emmyrc, FileId, InFiled, LuaIndex};

#[derive(Debug)]
pub struct LuaCompilation {
    db: DbIndex,
    emmyrc: Arc<Emmyrc>,
}

impl LuaCompilation {
    pub fn new(emmyrc: Arc<Emmyrc>) -> Self {
        let mut compilation = Self {
            db: DbIndex::new(),
            emmyrc: emmyrc.clone(),
        };

        compilation.db.update_config(emmyrc.clone());
        compilation
    }

    pub fn get_semantic_model(&self, file_id: FileId) -> Option<SemanticModel> {
        let config = self.emmyrc.get_infer_config(file_id);
        let tree = self.db.get_vfs().get_syntax_tree(&file_id)?;
        Some(SemanticModel::new(
            file_id,
            &self.db,
            config,
            self.emmyrc.clone(),
            tree.get_chunk_node(),
        ))
    }

    pub fn update_index(&mut self, file_ids: Vec<FileId>) {
        let mut need_analyzed_files = vec![];
        for file_id in file_ids {
            let tree = self.db.get_vfs().get_syntax_tree(&file_id).unwrap();
            need_analyzed_files.push(InFiled {
                file_id,
                value: tree.get_chunk_node(),
            });
        }

        analyzer::analyze(&mut self.db, need_analyzed_files, self.emmyrc.clone());
    }

    pub fn remove_index(&mut self, file_ids: Vec<FileId>) {
        self.db.remove_index(file_ids);
    }

    pub fn clear_index(&mut self) {
        self.db.clear();
    }

    pub fn get_db(&self) -> &DbIndex {
        &self.db
    }

    pub fn get_db_mut(&mut self) -> &mut DbIndex {
        &mut self.db
    }

    pub fn update_config(&mut self, config: Arc<Emmyrc>) {
        self.emmyrc = config.clone();
        self.db.update_config(config);
    }
}
