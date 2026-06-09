//! # SemanticModel — 单文件语义查询入口
//!
//! 新架构设计原则：
//! - 直接引用 `SalsaSummaryDatabase`，不经过 `DbIndex`
//! - 每个子模块封装一类查询（member、infer、type_check 等）
//! - 不暴露内部数据结构（无 `get_db()` 等方法）
//!
//! 旧 `semantic/` 模块将在新模块功能完备后逐步废弃。

mod infer;
mod member;

use std::sync::{Arc, RwLock};

use emmylua_parser::{LuaChunk, LuaExpr};

use crate::compilation::SalsaSummaryDatabase;
use crate::{Emmyrc, FileId};

pub use infer::{InferFailReason, InferQuery, InferResult};
pub use member::MemberQuery;

/// 单文件语义模型。直接持有 salsa 数据库的 Arc，所有查询通过 salsa 完成。
///
/// `Clone` 实现允许低成本地在多个位置共享同一个模型。
///
/// # Thread Safety
/// `SalsaSummaryDatabase` 自身不是 `Sync`（salsa 内部使用 `!Sync` storage），
/// 但通过 `Arc<RwLock<>>` 包装后可以安全地在多线程间共享。
#[derive(Clone)]
pub struct SemanticModel {
    file_id: FileId,
    salsa_db: Arc<RwLock<SalsaSummaryDatabase>>,
    emmyrc: Arc<Emmyrc>,
    root: LuaChunk,
}

// `Arc<RwLock<>>` 提供了外层 `Sync` 保证。
unsafe impl Send for SemanticModel {}
unsafe impl Sync for SemanticModel {}

#[allow(dead_code)]
impl SemanticModel {
    pub fn new(
        file_id: FileId,
        salsa_db: Arc<RwLock<SalsaSummaryDatabase>>,
        emmyrc: Arc<Emmyrc>,
        root: LuaChunk,
    ) -> Self {
        Self {
            file_id,
            salsa_db,
            emmyrc,
            root,
        }
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 基本属性
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub fn get_root(&self) -> &LuaChunk {
        &self.root
    }

    pub fn get_emmyrc(&self) -> &Emmyrc {
        &self.emmyrc
    }

    pub fn get_emmyrc_arc(&self) -> Arc<Emmyrc> {
        self.emmyrc.clone()
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // VFS 桥接（临时 — 后续 VFS 独立抽象后移除）
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    pub fn get_document<'a>(&self, vfs: &'a crate::Vfs) -> crate::LuaDocument<'a> {
        vfs.get_document(&self.file_id).expect("always exists")
    }

    pub fn get_file_parse_error(
        &self,
        vfs: &crate::Vfs,
    ) -> Option<Vec<emmylua_parser::LuaParseError>> {
        vfs.get_file_parse_error(&self.file_id)
    }

    pub fn get_root_by_file_id<'a>(
        &self,
        vfs: &'a crate::Vfs,
        file_id: FileId,
    ) -> Option<LuaChunk> {
        Some(vfs.get_syntax_tree(&file_id)?.get_chunk_node())
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 成员查询
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    pub fn members(&self) -> MemberQuery<'_> {
        MemberQuery::new(&self.salsa_db, self.file_id)
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 类型推断
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    /// 获取推断查询器
    pub fn infer(&self) -> InferQuery<'_> {
        InferQuery::new(&self.salsa_db, self.file_id)
    }

    /// 快捷方法：推断表达式类型
    pub fn infer_expr(&self, expr: LuaExpr) -> InferResult {
        self.infer().infer_expr(expr)
    }
}
