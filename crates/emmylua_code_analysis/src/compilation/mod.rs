mod analyzer;
mod test;

use std::sync::Arc;

pub(crate) use analyzer::does_func_body_always_return_or_exit;

use crate::{
    Emmyrc, FileId, InFiled, LuaIndex, LuaInferCache, db_index::DbIndex, semantic::SemanticModel,
};

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

    pub fn get_semantic_model(&'_ self, file_id: FileId) -> Option<SemanticModel<'_>> {
        let cache = LuaInferCache::new(file_id, Default::default());
        let tree = self.db.get_vfs().get_syntax_tree(&file_id)?;
        Some(SemanticModel::new(
            file_id,
            &self.db,
            cache,
            self.emmyrc.clone(),
            tree.get_chunk_node(),
        ))
    }

    pub fn update_index(&mut self, file_ids: Vec<FileId>) {
        let mut need_analyzed_files = vec![];
        for file_id in file_ids {
            let tree = match self.db.get_vfs().get_syntax_tree(&file_id) {
                Some(tree) => tree,
                None => {
                    log::warn!("file_id {:?} not found in vfs", file_id);
                    continue;
                }
            };
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
