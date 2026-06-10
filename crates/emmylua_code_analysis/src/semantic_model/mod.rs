//! # SemanticModel — 单文件语义查询入口
//!
//! 新架构设计原则：
//! - 直接引用 `SalsaSummaryDatabase`，不经过 `DbIndex`
//! - 每个子模块封装一类查询（member、infer、type_check 等）
//! - 不暴露内部数据结构（无 `get_db()` 等方法）
//!
//! 旧 `semantic/` 模块将在新模块功能完备后逐步废弃。

mod generic;
mod infer;
mod member;
mod reference;
mod type_check;
mod visibility;

use std::sync::{Arc, RwLock};

use emmylua_parser::{LuaChunk, LuaExpr, LuaParseError, LuaSyntaxNode, LuaSyntaxToken};

use crate::compilation::{
    SalsaDeclId, SalsaDeclTreeSummary, SalsaDocVisibilityKindSummary, SalsaNameUseSummary,
    SalsaSummaryDatabase,
};
use crate::{
    Emmyrc, FileId, LuaDocument, LuaMemberKey, LuaSemanticDeclId, LuaType, SemanticDeclLevel, Vfs,
};

pub use generic::{GenericBindings, substitute as substitute_generic};
pub use infer::{InferFailReason, InferQuery, InferResult};
pub use member::MemberQuery;
pub use type_check::{TypeCheckFailReason, TypeCheckResult};

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

    pub fn get_document<'a>(&self, vfs: &'a Vfs) -> LuaDocument<'a> {
        vfs.get_document(&self.file_id).expect("always exists")
    }

    pub fn get_file_parse_error(&self, vfs: &Vfs) -> Option<Vec<LuaParseError>> {
        vfs.get_file_parse_error(&self.file_id)
    }

    pub fn get_root_by_file_id(&self, vfs: &Vfs, file_id: FileId) -> Option<LuaChunk> {
        Some(vfs.get_syntax_tree(&file_id)?.get_chunk_node())
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 成员查询
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    pub fn members(&self) -> MemberQuery {
        MemberQuery::new(self.salsa_db.clone(), self.file_id)
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 类型推断
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    pub fn infer(&self) -> InferQuery {
        InferQuery::new(self.salsa_db.clone(), self.file_id, self.emmyrc.clone())
    }

    /// 快捷方法：推断表达式类型
    pub fn infer_expr(&self, expr: LuaExpr) -> InferResult {
        self.infer().infer_expr(expr)
    }

    /// 推断成员类型：给定前缀类型和 key，返回成员类型。
    pub fn infer_member_type(
        &self,
        prefix_type: &LuaType,
        member_key: &LuaMemberKey,
    ) -> InferResult {
        self.infer().infer_member_type(prefix_type, member_key)
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 类型检查
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    /// 检查 source 类型是否兼容 compact 类型。
    pub fn type_check(&self, source: &LuaType, compact: &LuaType) -> TypeCheckResult {
        type_check::check_type_compact(self.emmyrc.clone(), source, compact)
    }

    /// 详细模式类型检查。
    pub fn type_check_detail(&self, source: &LuaType, compact: &LuaType) -> TypeCheckResult {
        type_check::check_type_compact_detail(self.emmyrc.clone(), source, compact)
    }

    /// 判断声明在给定 token 位置是否可见。
    ///
    /// `visibility` 是从 doc tag 中解析出的可见性注解。
    /// 如果为 `None`，则仅检查 emmyrc `private_name` 模式。
    pub fn is_visible(
        &self,
        token: LuaSyntaxToken,
        decl_id: &LuaSemanticDeclId,
        visibility: Option<&SalsaDocVisibilityKindSummary>,
    ) -> Option<bool> {
        let db = self.salsa_db.read().unwrap_or_else(|e| e.into_inner());
        let infer = self.infer();
        visibility::check_visibility(
            &db, &infer, self.file_id, &self.emmyrc, token, decl_id, visibility,
        )
    }

    /// 检查 AST 节点是否是对目标声明的引用。
    pub fn is_reference_to(
        &self,
        node: LuaSyntaxNode,
        decl_id: &LuaSemanticDeclId,
        level: SemanticDeclLevel,
    ) -> Option<bool> {
        let db = self.salsa_db.read().unwrap_or_else(|e| e.into_inner());
        reference::is_reference_to(&db, self.file_id, &node, decl_id, level)
    }

    /// 查找 AST 节点引用的声明。
    pub fn find_decl_by_node(
        &self,
        node: LuaSyntaxNode,
        level: SemanticDeclLevel,
    ) -> Option<LuaSemanticDeclId> {
        let db = self.salsa_db.read().unwrap_or_else(|e| e.into_inner());
        reference::find_decl(&db, self.file_id, &node, level)
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 声明查询
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    /// 获取当前文件的声明树。
    pub fn decl_tree(&self) -> Option<Arc<SalsaDeclTreeSummary>> {
        let db = self.salsa_db.read().unwrap_or_else(|e| e.into_inner());
        db.file().decl_tree(self.file_id)
    }

    /// 查询某个声明的所有引用。
    pub fn decl_references(&self, decl_id: SalsaDeclId) -> Option<Vec<SalsaNameUseSummary>> {
        let db = self.salsa_db.read().unwrap_or_else(|e| e.into_inner());
        db.lexical().decl_references(self.file_id, decl_id)
    }

    /// 获取内部 salsa_db 引用（仅供内部子模块使用）。
    pub(crate) fn salsa_db(&self) -> &Arc<RwLock<SalsaSummaryDatabase>> {
        &self.salsa_db
    }
}
