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

use std::cell::RefCell;
use std::sync::{Arc, RwLock};

use emmylua_parser::{LuaChunk, LuaExpr, LuaParseError, LuaSyntaxNode, LuaSyntaxToken};

use smol_str::SmolStr;

use rowan::TextSize;

use crate::compilation::{
    SalsaDeclId, SalsaDeclTreeSummary, SalsaDocOwnerKindSummary, SalsaDocOwnerSummary,
    SalsaDocTagPropertyEntrySummary, SalsaDocTypeDefKindSummary, SalsaDocVisibilityKindSummary,
    SalsaNameUseSummary, SalsaPropertyKeySummary, SalsaSignatureExplainSummary,
    SalsaSignatureIndexSummary, SalsaSummaryDatabase,
};
use crate::{
    Emmyrc, FileId, LuaDocument, LuaMemberKey, LuaSemanticDeclId, LuaType, LuaTypeDeclId,
    SemanticDeclLevel, Vfs,
};

pub use generic::{GenericBindings, substitute as substitute_generic};
pub use infer::{InferCache, InferFailReason, InferQuery, InferResult};
pub use member::MemberQuery;
pub use type_check::{TypeCheckFailReason, TypeCheckResult};

/// 单文件语义模型。直接持有 salsa 数据库的 Arc，所有查询通过 salsa 完成。
///
/// `Clone` 实现允许低成本地在多个位置共享同一个模型。
///
/// # Thread Safety
/// `SalsaSummaryDatabase` 自身不是 `Sync`（salsa 内部使用 `!Sync` storage），
/// 但通过 `Arc<RwLock<>>` 包装后可以安全地在多线程间共享。
pub struct SemanticModel {
    file_id: FileId,
    salsa_db: Arc<RwLock<SalsaSummaryDatabase>>,
    emmyrc: Arc<Emmyrc>,
    root: LuaChunk,
    infer_cache: RefCell<InferCache>,
}

unsafe impl Send for SemanticModel {}
unsafe impl Sync for SemanticModel {}

/// Clone 创建新的 `InferCache`（克隆不共享推断缓存）。
impl Clone for SemanticModel {
    fn clone(&self) -> Self {
        Self {
            file_id: self.file_id,
            salsa_db: self.salsa_db.clone(),
            emmyrc: self.emmyrc.clone(),
            root: self.root.clone(),
            infer_cache: RefCell::new(InferCache::new(self.file_id)),
        }
    }
}

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
            infer_cache: RefCell::new(InferCache::new(file_id)),
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

    pub fn infer(&self) -> InferQuery<'_> {
        InferQuery::with_cache(
            self.salsa_db.clone(),
            self.file_id,
            self.emmyrc.clone(),
            &self.infer_cache,
        )
    }

    /// 快捷方法：推断表达式类型
    pub fn infer_expr(&self, expr: LuaExpr) -> InferResult {
        self.infer().infer_expr(expr)
    }

    /// 推断表达式列表的类型（处理多返回值展开）。
    pub fn infer_expr_list_types(
        &self,
        exprs: &[LuaExpr],
        var_count: Option<usize>,
    ) -> Result<Vec<(LuaType, rowan::TextRange)>, InferFailReason> {
        self.infer().infer_expr_list_types(exprs, var_count)
    }

    /// 推断值绑定的目标类型。
    pub fn infer_bind_value_type(&self, expr: LuaExpr) -> Option<LuaType> {
        self.infer().infer_bind_value_type(expr)
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

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 成员查询（check_field 等 checker 使用）
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    /// 获取某个类型的所有成员 key（跨文件合并，通过 salsa workspace 聚合索引）。
    ///
    /// O(1) 查询 — 索引由 salsa 自动构建和缓存。
    /// 当任何文件 summary 变更时自动失效重建。
    pub fn get_member_infos(&self, prefix_type: &LuaType) -> Option<Vec<LuaMemberKey>> {
        let db = self.salsa_db.read().unwrap_or_else(|e| e.into_inner());
        get_member_keys_workspace(&db, prefix_type)
    }

    /// 判断类型 ID 是否指向 enum。
    pub fn is_enum_type(&self, type_id: &LuaTypeDeclId) -> bool {
        let db = self.salsa_db.read().unwrap_or_else(|e| e.into_inner());
        db.doc()
            .type_def_by_name(self.file_id, type_id.get_name())
            .is_some_and(|def| matches!(def.kind, SalsaDocTypeDefKindSummary::Enum))
    }

    /// 判断类型 ID 是否指向 class。
    pub fn is_class_type(&self, type_id: &LuaTypeDeclId) -> bool {
        let db = self.salsa_db.read().unwrap_or_else(|e| e.into_inner());
        db.doc()
            .type_def_by_name(self.file_id, type_id.get_name())
            .is_some_and(|def| matches!(def.kind, SalsaDocTypeDefKindSummary::Class))
    }

    /// 检查声明是否有指定的 doc tag 属性。
    pub fn decl_has_doc_property(
        &self,
        file_id: FileId,
        offset: TextSize,
        entry: SalsaDocTagPropertyEntrySummary,
    ) -> bool {
        let db = self.salsa_db.read().unwrap_or_else(|e| e.into_inner());
        let owner = SalsaDocOwnerSummary {
            kind: SalsaDocOwnerKindSummary::None,
            syntax_offset: Some(offset),
        };
        db.doc()
            .tag_property(file_id, owner)
            .is_some_and(|p| p.entries.iter().any(|e| *e == entry))
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 签名查询（check_return_count 等 checker 使用）
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    /// 获取文件中所有签名。
    pub fn signatures(&self) -> Option<Arc<SalsaSignatureIndexSummary>> {
        let db = self.salsa_db.read().unwrap_or_else(|e| e.into_inner());
        db.doc().signature().summary(self.file_id)
    }

    /// 获取某个签名偏移处的已解析签名信息。
    pub fn signature_explain(
        &self,
        file_id: FileId,
        offset: TextSize,
    ) -> Option<SalsaSignatureExplainSummary> {
        let db = self.salsa_db.read().unwrap_or_else(|e| e.into_inner());
        db.doc().signature().explain(file_id, offset)
    }

    /// 通过 LuaSignatureId（文件 + TextSize）查签名。
    pub fn signature_by_id(
        &self,
        file_id: FileId,
        offset: TextSize,
    ) -> Option<SalsaSignatureExplainSummary> {
        self.signature_explain(file_id, offset)
    }

    /// 获取内部 salsa_db 引用（仅供内部子模块使用）。
    pub(crate) fn salsa_db(&self) -> &Arc<RwLock<SalsaSummaryDatabase>> {
        &self.salsa_db
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 内部工具
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 通过 salsa workspace 聚合索引收集成员 key。O(1) 查询。
fn get_member_keys_workspace(
    db: &SalsaSummaryDatabase,
    prefix_type: &LuaType,
) -> Option<Vec<LuaMemberKey>> {
    match prefix_type {
        LuaType::Ref(type_id) | LuaType::Def(type_id) => {
            let entries = db.workspace().properties_of_type(type_id.get_name())?;
            let keys: Vec<LuaMemberKey> = entries
                .iter()
                .map(|e| match &e.key {
                    SalsaPropertyKeySummary::Name(n) => LuaMemberKey::Name(SmolStr::new(n.as_str())),
                    SalsaPropertyKeySummary::Integer(i) => LuaMemberKey::Integer(*i),
                    _ => LuaMemberKey::None,
                })
                .filter(|k| !matches!(k, LuaMemberKey::None))
                .collect();
            if keys.is_empty() { None } else { Some(keys) }
        }
        LuaType::Union(u) => {
            let mut all = Vec::new();
            for m in u.into_vec() {
                if let Some(keys) = get_member_keys_workspace(db, &m) {
                    all.extend(keys);
                }
            }
            if all.is_empty() { None } else { Some(all) }
        }
        _ => None,
    }
}
