#[allow(unused)]
mod analyzer;
mod decl;
mod global;
mod member;
mod module;
mod summary_builder;
mod test;

use std::sync::Arc;

use crate::{
    Emmyrc, FileId, InferFailReason, LuaIndex, LuaInferCache, LuaType, LuaTypeDeclId,
    db_index::DbIndex,
    semantic::SemanticModel,
};
pub use decl::*;
pub use global::*;
pub(crate) use member::*;
use emmylua_parser::{LuaBlock, LuaExpr};
pub use module::*;
pub use summary_builder::*;

pub(crate) fn analyze_func_body_missing_return_flags_with<F>(
    body: LuaBlock,
    infer_expr_type: &mut F,
) -> Result<(bool, bool), InferFailReason>
where
    F: FnMut(&LuaExpr) -> Result<LuaType, InferFailReason>,
{
    analyzer::analyze_func_body_missing_return_flags_with(body, infer_expr_type)
}

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

    pub fn find_module_by_file_id(&self, file_id: FileId) -> Option<CompilationModuleInfo> {
        project_module_info(&self.db, file_id)
    }

    pub fn find_module_by_require_path(&self, module_path: &str) -> Option<CompilationModuleInfo> {
        find_module_by_require_path(&self.db, module_path)
    }

    pub fn resolve_module_export_type(&self, file_id: FileId) -> Option<LuaType> {
        resolve_projected_module_export_type(&self.db, file_id)
    }

    pub fn find_type_generic_params(
        &self,
        type_decl_id: &LuaTypeDeclId,
    ) -> Option<Vec<CompilationGenericParamInfo>> {
        find_compilation_type_generic_params(&self.db, type_decl_id)
    }

    pub fn update_index(&mut self, file_ids: Vec<FileId>) {
        self.db.sync_summary_workspaces();
        for file_id in file_ids {
            if !self.db.sync_summary_file(file_id) {
                log::warn!("file_id {:?} not found in vfs for summary sync", file_id);
            }
        }
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
