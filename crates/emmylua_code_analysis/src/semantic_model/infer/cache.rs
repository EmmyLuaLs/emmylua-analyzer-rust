//! 本地推断缓存
//!
//! 轻量级缓存，用于单次推断会话中的 memoization。

use emmylua_parser::LuaSyntaxId;
use hashbrown::HashMap;

use crate::FileId;
use crate::LuaType;

use super::InferFailReason;

#[derive(Debug, Clone)]
enum CacheEntry {
    Computing,
    Cached(LuaType),
}

#[derive(Debug)]
pub struct InferCache {
    file_id: FileId,
    entries: HashMap<LuaSyntaxId, CacheEntry>,
}

impl InferCache {
    pub fn new(file_id: FileId) -> Self {
        Self {
            file_id,
            entries: HashMap::new(),
        }
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub fn get(&self, syntax_id: &LuaSyntaxId) -> Option<Result<LuaType, InferFailReason>> {
        match self.entries.get(syntax_id) {
            Some(CacheEntry::Cached(ty)) => Some(Ok(ty.clone())),
            Some(CacheEntry::Computing) => Some(Err(InferFailReason::RecursiveInfer)),
            None => None,
        }
    }

    pub fn insert(&mut self, syntax_id: LuaSyntaxId, ty: LuaType) {
        self.entries.insert(syntax_id, CacheEntry::Cached(ty));
    }

    #[allow(dead_code)]
    pub fn mark_computing(&mut self, syntax_id: LuaSyntaxId) {
        self.entries.insert(syntax_id, CacheEntry::Computing);
    }
}
