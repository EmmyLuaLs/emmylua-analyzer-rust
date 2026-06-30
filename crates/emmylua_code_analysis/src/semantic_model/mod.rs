//! # SemanticModel — 单文件语义查询入口
//!
//! 新架构设计原则：
//! - 直接引用 `SalsaSummaryDatabase`，不经过 `DbIndex`
//! - 查询按职责分为独立 Query 对象：
//!   * [`TypeQuery`] — 类型定义、属性、doc 属性
//!   * [`DeclQuery`] — 声明、引用、可见性
//!   * [`SigQuery`] — 签名、调用解释、泛型约束
//!   * [`InferQuery`] — 类型推断
//!   * [`MemberQuery`] — 成员查询
//! - SemanticModel 提供工厂方法和常用快捷方法

mod decl_query;
mod generic;
pub(crate) mod generic_checker;
pub mod humanize;
mod infer;
mod member;
pub mod offset_types;
mod reference;
mod sig_query;
pub mod signature;
mod type_check;
mod type_query;
mod visibility;

use std::cell::RefCell;
use std::sync::Arc;

use emmylua_parser::{
    LuaCallExpr, LuaChunk, LuaExpr, LuaParseError, LuaSyntaxNode, LuaSyntaxToken,
};
use rowan::TextSize;

use crate::compilation::{
    SalsaDeclId, SalsaDeclTreeSummary, SalsaDocTagPropertyEntrySummary, SalsaDocTagPropertySummary,
    SalsaDocTypeDefSummary, SalsaDocVisibilityKindSummary, SalsaNameUseSummary,
    SalsaPropertySummary, SalsaSignatureExplainSummary, SalsaSignatureIndexSummary,
    SalsaSummaryDatabase, TypeDefEntry, WorkspacePropertyEntry,
};
use crate::{
    Emmyrc, FileId, LuaMemberKey, LuaSemanticDeclId, LuaType, LuaTypeDeclId, SemanticDeclLevel,
};

pub use decl_query::DeclQuery;
pub use infer::{CallFunctionInfo, InferCache, InferFailReason, InferQuery, InferResult};
pub use member::MemberQuery;
pub use offset_types::{DeclPosition, OwnerPosition};
pub use sig_query::SigQuery;
pub use type_check::{TypeCheckFailReason, TypeCheckResult};
pub use type_query::TypeQuery;

/// 单文件语义模型。
pub struct SemanticModel<'db> {
    file_id: FileId,
    salsa_db: &'db SalsaSummaryDatabase,
    emmyrc: Arc<Emmyrc>,
    root: LuaChunk,
    infer_cache: RefCell<InferCache>,
}

unsafe impl<'db> Send for SemanticModel<'db> {}
unsafe impl<'db> Sync for SemanticModel<'db> {}

impl<'db> Clone for SemanticModel<'db> {
    fn clone(&self) -> Self {
        Self {
            file_id: self.file_id,
            salsa_db: self.salsa_db,
            emmyrc: self.emmyrc.clone(),
            root: self.root.clone(),
            infer_cache: RefCell::new(InferCache::new(self.file_id)),
        }
    }
}

impl<'db> SemanticModel<'db> {
    pub fn new(
        file_id: FileId,
        salsa_db: &'db SalsaSummaryDatabase,
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

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub fn get_root(&self) -> &LuaChunk {
        &self.root
    }

    pub fn get_emmyrc(&self) -> &Emmyrc {
        &self.emmyrc
    }

    /// Offset to (line, col) — 0-based, via salsa-tracked line_index.
    pub fn offset_to_line_col(&self, offset: TextSize) -> Option<(usize, usize)> {
        self.salsa_db.offset_to_line_col(self.file_id, offset)
    }

    pub fn get_file_parse_error(&self) -> Option<Vec<LuaParseError>> {
        self.salsa_db.get_file_parse_error(self.file_id)
    }

    pub fn get_root_by_file_id(&self, file_id: FileId) -> Option<LuaChunk> {
        Some(self.salsa_db.get_syntax_tree(file_id)?.get_chunk_node())
    }

    /// 类型定义查询：`model.types().get_def("ClassName")`
    pub fn types(&self) -> TypeQuery {
        TypeQuery::new(self.salsa_db.clone(), self.file_id)
    }

    /// 声明查询：`model.decls().find_by_node(...)`
    pub fn decls(&self) -> DeclQuery {
        DeclQuery::new(
            self.salsa_db,
            self.file_id,
            self.emmyrc.clone(),
            self.root.clone(),
            RefCell::new(InferCache::new(self.file_id)),
        )
    }

    /// 签名/调用查询：`model.sigs().get(file_id, offset)`
    pub fn sigs(&self) -> SigQuery {
        SigQuery::new(self.salsa_db, self.file_id)
    }

    /// 成员查询（已有）：`model.members().find(...)`
    pub fn members(&self) -> MemberQuery {
        MemberQuery::new(self.salsa_db, self.file_id)
    }

    /// 检查类型上是否存在某个成员。对 UndefinedField 检查是权威的——存在返回 true。
    pub fn has_member(&self, prefix_type: &LuaType, key: &LuaMemberKey) -> bool {
        self.infer().infer_member_type(prefix_type, key).is_ok()
    }

    /// 查找节点引用的声明，同时返回声明范围是否覆盖整个节点。
    pub fn find_decl_covers_node(
        &self,
        node: LuaSyntaxNode,
        level: SemanticDeclLevel,
    ) -> Option<(LuaSemanticDeclId, bool)> {
        self.decls().find_covers_node(node, level)
    }

    /// 类型推断：`model.infer().infer_expr(expr)`
    pub fn infer(&self) -> InferQuery<'_> {
        InferQuery::with_cache(
            self.salsa_db,
            self.file_id,
            self.emmyrc.clone(),
            &self.infer_cache,
            self.root.clone(),
        )
    }

    pub fn type_check(&self, source: &LuaType, compact: &LuaType) -> TypeCheckResult {
        type_check::check_type_compact(self.emmyrc.clone(), source, compact)
    }

    pub fn type_check_detail(&self, source: &LuaType, compact: &LuaType) -> TypeCheckResult {
        type_check::check_type_compact_with_db(
            self.emmyrc.clone(),
            self.salsa_db,
            self.file_id,
            source,
            compact,
            true,
        )
    }

    // -- 类型 --
    pub fn get_type_def(&self, name: &str) -> Option<SalsaDocTypeDefSummary> {
        self.types().get_def(name)
    }

    pub fn get_type_def_with_file(&self, name: &str) -> Option<(SalsaDocTypeDefSummary, FileId)> {
        self.types().get_def_with_file(name)
    }

    pub fn is_enum_type(&self, tid: &LuaTypeDeclId) -> bool {
        self.types().is_enum(tid)
    }

    pub fn is_class_type(&self, tid: &LuaTypeDeclId) -> bool {
        self.types().is_class(tid)
    }

    pub fn is_alias_type(&self, tid: &LuaTypeDeclId) -> bool {
        self.types().is_alias(tid)
    }

    pub fn count_type_def_files(&self, name: &str) -> usize {
        self.types().count_files(name)
    }

    pub fn type_def_entries(&self, name: &str) -> Option<Vec<TypeDefEntry>> {
        self.types().entries(name)
    }

    pub fn file_type_names(&self) -> Vec<String> {
        self.types().file_names()
    }

    pub fn resolve_doc_type_name(&self, name: &str) -> Option<LuaType> {
        self.types().resolve_name(name)
    }

    pub fn get_member_infos(&self, pt: &LuaType) -> Option<Vec<LuaMemberKey>> {
        self.types().member_keys(pt)
    }

    pub fn get_property_entries(&self, n: &str) -> Option<Vec<WorkspacePropertyEntry>> {
        self.types().property_entries(n)
    }

    pub fn get_doc_properties(
        &self,
        fid: FileId,
        off: OwnerPosition,
    ) -> Option<SalsaDocTagPropertySummary> {
        self.types().doc_properties(fid, off)
    }

    pub fn decl_has_doc_property(
        &self,
        fid: FileId,
        off: OwnerPosition,
        e: SalsaDocTagPropertyEntrySummary,
    ) -> bool {
        self.types().has_doc_property(fid, off, e)
    }

    pub fn get_attribute_params(
        &self,
        td: &SalsaDocTypeDefSummary,
    ) -> Option<Vec<(String, Option<LuaType>)>> {
        self.types().attribute_params(td)
    }

    // -- 声明 --
    pub fn decl_tree(&self) -> Option<Arc<SalsaDeclTreeSummary>> {
        self.decls().tree()
    }

    pub fn decl_references(&self, did: SalsaDeclId) -> Option<Vec<SalsaNameUseSummary>> {
        self.decls().references(did)
    }

    pub fn get_decl_range(&self, pos: DeclPosition) -> Option<rowan::TextRange> {
        self.decls().range(pos)
    }

    pub fn get_type_by_decl_position(&self, pos: DeclPosition) -> Option<LuaType> {
        self.decls().type_at(pos)
    }

    pub fn find_decl_by_node(
        &self,
        n: LuaSyntaxNode,
        l: SemanticDeclLevel,
    ) -> Option<LuaSemanticDeclId> {
        self.decls().find_by_node(n, l)
    }

    pub fn is_reference_to(
        &self,
        n: LuaSyntaxNode,
        d: &LuaSemanticDeclId,
        l: SemanticDeclLevel,
    ) -> Option<bool> {
        self.decls().is_reference_to(n, d, l)
    }

    pub fn is_visible(
        &self,
        t: LuaSyntaxToken,
        d: &LuaSemanticDeclId,
        v: Option<&SalsaDocVisibilityKindSummary>,
    ) -> Option<bool> {
        self.decls().is_visible(t, d, v)
    }

    pub fn get_decl_property(&self, d: &LuaSemanticDeclId) -> Option<Vec<SalsaPropertySummary>> {
        self.decls().property(d)
    }

    pub fn get_member_key(&self, fk: emmylua_parser::LuaIndexKey) -> Option<LuaMemberKey> {
        self.decls().member_key(fk)
    }

    // -- 签名 --
    pub fn get_signature(&self, fid: FileId, off: TextSize) -> Option<signature::SignatureInfo> {
        self.sigs().get(fid, off)
    }

    pub fn signatures(&self) -> Option<Arc<SalsaSignatureIndexSummary>> {
        self.sigs().all()
    }

    pub fn signature_explain(
        &self,
        fid: FileId,
        off: TextSize,
    ) -> Option<SalsaSignatureExplainSummary> {
        self.sigs().explain(fid, off)
    }

    pub fn signature_by_id(
        &self,
        fid: FileId,
        off: TextSize,
    ) -> Option<SalsaSignatureExplainSummary> {
        self.sigs().explain(fid, off)
    }

    pub fn get_call_explain(
        &self,
        off: TextSize,
    ) -> Option<crate::compilation::SalsaCallExplainSummary> {
        self.sigs().call_explain(off)
    }

    pub fn lowered_to_lua_type(
        &self,
        l: &crate::compilation::SalsaDocTypeLoweredNode,
    ) -> Option<LuaType> {
        self.sigs().lowered_to_type(l)
    }

    pub fn resolve_type_def_generic_constraints(
        &self,
        td: &SalsaDocTypeDefSummary,
        fid: FileId,
    ) -> Vec<(Option<LuaType>, Option<LuaType>)> {
        self.sigs().type_generic_constraints(td, fid)
    }

    pub fn resolve_signature_generic_constraints(
        &self,
        e: &SalsaSignatureExplainSummary,
    ) -> Vec<(Option<LuaType>, Option<LuaType>)> {
        self.sigs().signature_generic_constraints(e)
    }

    // -- 推断 --
    pub fn infer_expr(&self, expr: LuaExpr) -> InferResult {
        self.infer().infer_expr(expr)
    }

    pub fn infer_expr_list_types(
        &self,
        exprs: &[LuaExpr],
        vc: Option<usize>,
    ) -> Result<Vec<(LuaType, rowan::TextRange)>, InferFailReason> {
        self.infer().infer_expr_list_types(exprs, vc)
    }

    pub fn infer_table_should_be(&self, expr: emmylua_parser::LuaTableExpr) -> Option<LuaType> {
        self.infer().infer_table_should_be(expr)
    }

    pub fn infer_bind_value_type(&self, expr: LuaExpr) -> Option<LuaType> {
        self.infer().infer_bind_value_type(expr)
    }

    pub fn infer_call_expr_func(
        &self,
        ce: LuaCallExpr,
        ac: Option<usize>,
    ) -> Option<CallFunctionInfo> {
        self.infer().infer_call_expr_func(ce, ac)
    }

    pub fn infer_member_type(&self, pt: &LuaType, mk: &LuaMemberKey) -> InferResult {
        self.infer().infer_member_type(pt, mk)
    }

    pub(crate) fn salsa_db(&self) -> &SalsaSummaryDatabase {
        &self.salsa_db
    }
}
