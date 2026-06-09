//! 本地推断缓存
//!
//! 轻量级缓存，用于单次推断会话中的 memoization。
//! 与 salsa 的跨文件增量缓存互补：salsa 处理跨文件依赖，
//! 此缓存处理单文件内的递归推断和重复查询。

use emmylua_parser::LuaSyntaxId;
use hashbrown::HashMap;

use crate::FileId;
use crate::LuaType;

/// 缓存条目状态
#[derive(Debug, Clone)]
enum CacheEntry {
    /// 正在计算中（检测递归）
    Computing,
    /// 已缓存
    Cached(LuaType),
}

/// 推断缓存，在单次推断会话中复用
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

    /// 检查缓存中的条目。
    /// - `None` = 缓存未命中，调用者应计算并插入
    /// - `Some(Err(RecursiveInfer))` = 正在计算中，检测到递归
    /// - `Some(Ok(type))` = 缓存命中
    pub fn get(&self, syntax_id: &LuaSyntaxId) -> Option<Result<LuaType, super::InferFailReason>> {
        match self.entries.get(syntax_id) {
            Some(CacheEntry::Cached(ty)) => Some(Ok(ty.clone())),
            Some(CacheEntry::Computing) => Some(Err(super::InferFailReason::RecursiveInfer)),
            None => None,
        }
    }

    /// 插入计算结果
    pub fn insert(&mut self, syntax_id: LuaSyntaxId, ty: LuaType) {
        self.entries.insert(syntax_id, CacheEntry::Cached(ty));
    }

    /// 标记为正在计算（在开始推断前调用，用于递归检测）
    #[allow(dead_code)]
    pub fn mark_computing(&mut self, syntax_id: LuaSyntaxId) {
        self.entries.insert(syntax_id, CacheEntry::Computing);
    }
}
