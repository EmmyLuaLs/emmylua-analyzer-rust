mod cache_options;

pub use cache_options::{CacheOptions, LuaAnalysisPhase};
use emmylua_parser::LuaSyntaxId;
use std::{collections::HashMap, sync::Arc};

use crate::{db_index::LuaType, FileId, FlowId, LuaDeclId, LuaFunctionType};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CacheKey {
    Expr(LuaSyntaxId),
    Call(LuaSyntaxId, Option<usize>, LuaType),
    FlowNode(LuaDeclId, FlowId)
}

#[derive(Debug)]
pub enum CacheEntry {
    ReadyCache,
    ExprCache(LuaType),
    CallCache(Arc<LuaFunctionType>),
}

#[derive(Debug)]
pub struct LuaInferCache {
    file_id: FileId,
    config: CacheOptions,
    cache: HashMap<CacheKey, CacheEntry>,
}

impl LuaInferCache {
    pub fn new(file_id: FileId, config: CacheOptions) -> Self {
        Self {
            file_id,
            config,
            cache: HashMap::new(),
        }
    }

    pub fn get_config(&self) -> &CacheOptions {
        &self.config
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    // 表达式缓存相关方法
    pub fn ready_cache(&mut self, key: &CacheKey) {
        self.cache.insert(key.clone(), CacheEntry::ReadyCache);
    }

    pub fn add_cache(&mut self, key: &CacheKey, value: CacheEntry) {
        self.cache.insert(key.clone(), value);
    }

    pub fn get(&self, key: &CacheKey) -> Option<&CacheEntry> {
        self.cache.get(key)
    }

    pub fn remove(&mut self, key: &CacheKey) {
        self.cache.remove(key);
    }

    pub fn set_phase(&mut self, phase: LuaAnalysisPhase) {
        self.config.analysis_phase = phase;
    }

    pub fn clear(&mut self) {
        self.cache.clear();
    }
}
