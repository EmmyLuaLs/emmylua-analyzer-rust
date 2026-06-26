mod decl;
mod decl_projections;
mod func_body;
mod global;
mod member;
mod module;
mod operator;
mod resolve;
mod summary_builder;
mod test;
mod type_projection;

use std::sync::Arc;

pub use crate::compilation::summary_builder::salsa_db::SalsaSummaryDatabase;
use crate::{
    Emmyrc, FileId, LuaInferCache, db_index::DbIndex, semantic::SemanticModel,
    semantic_model::SemanticModel as NewSemanticModel,
};
pub use decl::*;
pub(crate) use decl_projections::*;
pub(crate) use global::*;
pub(crate) use member::*;
pub use module::*;
pub(crate) use operator::*;
pub use summary_builder::*;
pub(crate) use type_projection::*;

pub(crate) use func_body::analyze_func_body_missing_return_flags_with;

#[derive(Debug)]
pub struct LuaCompilation {
    db: DbIndex,
    salsa_db: SalsaSummaryDatabase,
    emmyrc: Arc<Emmyrc>,
}

impl LuaCompilation {
    pub fn new(emmyrc: Arc<Emmyrc>) -> Self {
        let mut db = DbIndex::new();
        db.update_config(emmyrc.clone());
        let mut salsa_db = SalsaSummaryDatabase::default();
        salsa_db.update_config(emmyrc.clone());
        Self {
            db,
            salsa_db,
            emmyrc,
        }
    }

    /// 旧 SemanticModel（兼容现有 checker）。
    pub fn get_semantic_model(&'_ self, file_id: FileId) -> Option<SemanticModel<'_>> {
        let cache = LuaInferCache::new(file_id, Default::default());
        let tree = self.salsa_db.get_syntax_tree(file_id)?;
        Some(SemanticModel::new(
            file_id,
            &self.db,
            cache,
            self.emmyrc.clone(),
            tree.get_chunk_node(),
        ))
    }

    /// 新 SemanticModel（直接使用 salsa_db）。
    pub fn semantic_model(&self, file_id: FileId) -> Option<NewSemanticModel<'_>> {
        let tree = self.salsa_db.get_syntax_tree(file_id)?;
        Some(NewSemanticModel::new(
            file_id,
            &self.salsa_db,
            self.emmyrc.clone(),
            tree.get_chunk_node(),
        ))
    }

    pub fn update_index(&mut self, _file_ids: Vec<FileId>) {}

    pub fn remove_index(&mut self, file_ids: Vec<FileId>) {
        for &fid in &file_ids {
            self.salsa_db.remove_file(fid);
        }
    }

    pub fn clear_index(&mut self) {
        self.salsa_db.clear();
    }

    pub fn get_db(&self) -> &DbIndex {
        &self.db
    }

    pub fn get_db_mut(&mut self) -> &mut DbIndex {
        &mut self.db
    }

    pub fn get_salsa_db(&self) -> &SalsaSummaryDatabase {
        &self.salsa_db
    }

    /// VFS + salsa 联动：更新文件内容。
    pub fn update_file_by_uri(&mut self, uri: &lsp_types::Uri, text: Option<String>) -> FileId {
        let file_id = self.salsa_db.lookup_file_id(uri)
            .unwrap_or_else(|| self.salsa_db.intern_uri(uri));

        if let Some(ref file_text) = text {
            let path = self.salsa_db.file_path(file_id).cloned();
            let is_remote = self.salsa_db.is_remote_file(file_id);
            self.salsa_db.set_file(file_id, path, file_text.clone(), is_remote);
        } else {
            self.salsa_db.remove_file(file_id);
        }
        file_id
    }

    /// All file IDs currently in salsa DB (replaces old VFS `get_all_file_ids`).
    pub fn get_all_file_ids(&self) -> Vec<FileId> {
        self.salsa_db.file_ids()
    }

    /// Sync a file from old VFS (already has FileId) to salsa DB.
    pub fn sync_file_to_salsa(&mut self, file_id: FileId, text: String) {
        let path = self.db.get_vfs().get_file_path(&file_id).cloned();
        let is_remote = self.db.get_vfs().is_remote_file(&file_id);
        self.salsa_db.set_file(file_id, path, text, is_remote);
    }

    /// Remove a file by URI from the salsa DB.
    pub fn remove_file_by_uri(&mut self, uri: &lsp_types::Uri) -> Option<FileId> {
        self.salsa_db.remove_file_by_uri(uri)
    }

    pub fn update_config(&mut self, config: Arc<Emmyrc>) {
        self.emmyrc = config.clone();
        self.salsa_db.update_config(config.clone());
    }
}
