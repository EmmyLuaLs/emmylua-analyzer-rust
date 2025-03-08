use std::collections::{HashMap, HashSet};

use emmylua_parser::LuaSyntaxId;

use crate::{db_index::LuaType, FileId};

#[derive(Debug)]
pub struct LuaInferConfig {
    require_function: HashSet<String>,
    file_id: FileId,
    expr_type_cache: HashMap<LuaSyntaxId, ExprCache>,
}

#[derive(Debug)]
pub enum ExprCache {
    ReadyCache,
    Cache(LuaType),
}

impl LuaInferConfig {
    pub fn new(file_id: FileId, require_function: HashSet<String>) -> Self {
        Self {
            require_function,
            file_id,
            expr_type_cache: HashMap::new(),
        }
    }

    pub fn is_require_function(&self, function_name: &str) -> bool {
        if self.require_function.contains(function_name) {
            return true;
        }

        function_name == "require"
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub fn mark_ready_cache(&mut self, syntax_id: LuaSyntaxId) {
        self.expr_type_cache
            .insert(syntax_id, ExprCache::ReadyCache);
    }

    pub fn cache_expr_type(&mut self, syntax_id: LuaSyntaxId, ty: LuaType) {
        self.expr_type_cache.insert(syntax_id, ExprCache::Cache(ty));
    }

    pub fn get_cache_expr_type(&self, syntax_id: &LuaSyntaxId) -> Option<&ExprCache> {
        self.expr_type_cache.get(syntax_id)
    }

    pub fn remove_cache(&mut self, syntax_id: &LuaSyntaxId) {
        self.expr_type_cache.remove(syntax_id);
    }
}
