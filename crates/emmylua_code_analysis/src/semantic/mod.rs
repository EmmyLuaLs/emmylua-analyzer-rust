mod cache;
mod decl;
mod generic;
mod infer;
mod member;
mod overload_resolve;
mod reference;
mod semantic_info;
mod type_check;
mod visibility;

use std::cell::RefCell;
use std::collections::HashMap;
use std::{collections::HashSet, sync::Arc};

pub use cache::{CacheEntry, CacheKey, CacheOptions, LuaAnalysisPhase, LuaInferCache};
pub use decl::{enum_variable_is_param, parse_require_module_info};
use emmylua_parser::{
    LuaCallExpr, LuaChunk, LuaExpr, LuaIndexKey, LuaParseError, LuaSyntaxNode, LuaSyntaxToken,
    LuaTableExpr,
};
pub use infer::infer_index_expr;
use infer::{infer_bind_value_type, infer_expr_list_types};
pub use infer::{infer_table_field_value_should_be, infer_table_should_be};
use lsp_types::Uri;
pub use member::find_index_operations;
pub use member::get_member_map;
pub use member::LuaMemberInfo;
use member::{find_member_origin_owner, find_members};
use reference::is_reference_to;
use rowan::{NodeOrToken, TextRange};
pub(crate) use semantic_info::infer_node_semantic_decl;
pub use semantic_info::SemanticInfo;
use semantic_info::{
    infer_node_semantic_info, infer_token_semantic_decl, infer_token_semantic_info,
};
pub(crate) use type_check::check_type_compact;
use type_check::is_sub_type_of;
pub use visibility::check_export_visibility;
use visibility::check_visibility;

use crate::semantic::member::find_members_with_key;
use crate::{db_index::LuaTypeDeclId, Emmyrc, LuaDocument, LuaSemanticDeclId};
use crate::{
    db_index::{DbIndex, LuaType},
    FileId,
};
use crate::{LuaFunctionType, LuaMemberId, LuaMemberKey, LuaTypeOwner};
pub use generic::*;
pub use infer::infer_param;
pub use infer::InferFailReason;
pub(crate) use infer::{infer_call_expr_func, infer_expr};
use overload_resolve::resolve_signature;
pub use semantic_info::SemanticDeclLevel;
pub use type_check::{TypeCheckFailReason, TypeCheckResult};

#[derive(Debug)]
pub struct SemanticModel<'a> {
    file_id: FileId,
    db: &'a DbIndex,
    infer_cache: RefCell<LuaInferCache>,
    emmyrc: Arc<Emmyrc>,
    root: LuaChunk,
}

unsafe impl<'a> Send for SemanticModel<'a> {}
unsafe impl<'a> Sync for SemanticModel<'a> {}

impl<'a> SemanticModel<'a> {
    pub fn new(
        file_id: FileId,
        db: &'a DbIndex,
        infer_config: LuaInferCache,
        emmyrc: Arc<Emmyrc>,
        root: LuaChunk,
    ) -> Self {
        Self {
            file_id,
            db,
            infer_cache: RefCell::new(infer_config),
            emmyrc,
            root,
        }
    }

    pub fn get_document(&self) -> LuaDocument {
        self.db.get_vfs().get_document(&self.file_id).unwrap()
    }

    pub fn get_document_by_file_id(&self, file_id: FileId) -> Option<LuaDocument> {
        self.db.get_vfs().get_document(&file_id)
    }

    pub fn get_document_by_uri(&self, uri: &Uri) -> Option<LuaDocument> {
        let file_id = self.db.get_vfs().get_file_id(&uri)?;
        self.db.get_vfs().get_document(&file_id)
    }

    pub fn get_root_by_file_id(&self, file_id: FileId) -> Option<LuaChunk> {
        Some(
            self.db
                .get_vfs()
                .get_syntax_tree(&file_id)?
                .get_chunk_node(),
        )
    }

    pub fn get_file_parse_error(&self) -> Option<Vec<LuaParseError>> {
        self.db.get_vfs().get_file_parse_error(&self.file_id)
    }

    pub fn infer_expr(&self, expr: LuaExpr) -> Result<LuaType, InferFailReason> {
        infer_expr(self.db, &mut self.infer_cache.borrow_mut(), expr)
    }

    pub fn infer_table_should_be(&self, table: LuaTableExpr) -> Option<LuaType> {
        infer_table_should_be(self.db, &mut self.infer_cache.borrow_mut(), table).ok()
    }

    pub fn get_member_infos(&self, prefix_type: &LuaType) -> Option<Vec<LuaMemberInfo>> {
        find_members(self.db, prefix_type)
    }

    pub fn get_member_info_with_key(
        &self,
        prefix_type: &LuaType,
        member_key: LuaMemberKey,
        find_all: bool,
    ) -> Option<Vec<LuaMemberInfo>> {
        find_members_with_key(self.db, prefix_type, member_key, find_all)
    }

    pub fn get_member_info_map(
        &self,
        prefix_type: &LuaType,
    ) -> Option<HashMap<LuaMemberKey, Vec<LuaMemberInfo>>> {
        get_member_map(self.db, prefix_type)
    }

    pub fn type_check(&self, source: &LuaType, compact_type: &LuaType) -> TypeCheckResult {
        check_type_compact(self.db, source, compact_type)
    }

    pub fn infer_call_expr_func(
        &self,
        call_expr: LuaCallExpr,
        arg_count: Option<usize>,
    ) -> Option<Arc<LuaFunctionType>> {
        let prefix_expr = call_expr.get_prefix_expr()?;
        let call_expr_type = infer_expr(
            self.db,
            &mut self.infer_cache.borrow_mut(),
            prefix_expr.into(),
        )
        .ok()?;
        infer_call_expr_func(
            self.db,
            &mut self.infer_cache.borrow_mut(),
            call_expr,
            call_expr_type,
            &mut InferGuard::new(),
            arg_count,
        )
        .ok()
    }

    /// 推断表达式列表类型, 位于最后的表达式会触发多值推断
    pub fn infer_expr_list_types(
        &self,
        exprs: &[LuaExpr],
        var_count: Option<usize>,
    ) -> Vec<(LuaType, TextRange)> {
        infer_expr_list_types(
            self.db,
            &mut self.infer_cache.borrow_mut(),
            exprs,
            var_count,
        )
    }

    /// 推断值已经绑定的类型(不是推断值的类型). 例如从右值推断左值类型, 从调用参数推断函数参数类型
    pub fn infer_bind_value_type(&self, expr: LuaExpr) -> Option<LuaType> {
        infer_bind_value_type(self.db, &mut self.infer_cache.borrow_mut(), expr)
    }

    pub fn get_semantic_info(
        &self,
        node_or_token: NodeOrToken<LuaSyntaxNode, LuaSyntaxToken>,
    ) -> Option<SemanticInfo> {
        match node_or_token {
            NodeOrToken::Node(node) => {
                infer_node_semantic_info(self.db, &mut self.infer_cache.borrow_mut(), node)
            }
            NodeOrToken::Token(token) => {
                infer_token_semantic_info(self.db, &mut self.infer_cache.borrow_mut(), token)
            }
        }
    }

    pub fn find_decl(
        &self,
        node_or_token: NodeOrToken<LuaSyntaxNode, LuaSyntaxToken>,
        level: SemanticDeclLevel,
    ) -> Option<LuaSemanticDeclId> {
        match node_or_token {
            NodeOrToken::Node(node) => {
                infer_node_semantic_decl(self.db, &mut self.infer_cache.borrow_mut(), node, level)
            }
            NodeOrToken::Token(token) => {
                infer_token_semantic_decl(self.db, &mut self.infer_cache.borrow_mut(), token, level)
            }
        }
    }

    pub fn is_reference_to(
        &self,
        node: LuaSyntaxNode,
        semantic_decl_id: LuaSemanticDeclId,
        level: SemanticDeclLevel,
    ) -> bool {
        is_reference_to(
            self.db,
            &mut self.infer_cache.borrow_mut(),
            node,
            semantic_decl_id,
            level,
        )
        .unwrap_or(false)
    }

    pub fn is_semantic_visible(
        &self,
        token: LuaSyntaxToken,
        property_owner: LuaSemanticDeclId,
    ) -> bool {
        check_visibility(
            self.db,
            self.file_id,
            &self.emmyrc,
            &mut self.infer_cache.borrow_mut(),
            token,
            property_owner,
        )
        .unwrap_or(true)
    }

    pub fn is_sub_type_of(
        &mut self,
        sub_type_ref_id: &LuaTypeDeclId,
        super_type_ref_id: &LuaTypeDeclId,
    ) -> bool {
        is_sub_type_of(self.db, sub_type_ref_id, super_type_ref_id)
    }

    pub fn get_emmyrc(&self) -> &Emmyrc {
        &self.emmyrc
    }

    pub fn get_root(&self) -> &LuaChunk {
        &self.root
    }

    pub fn get_db(&self) -> &DbIndex {
        self.db
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub fn get_config(&self) -> &RefCell<LuaInferCache> {
        &self.infer_cache
    }

    pub fn get_type(&self, type_owner: LuaTypeOwner) -> LuaType {
        self.db
            .get_type_index()
            .get_type_cache(&type_owner)
            .map(|cache| cache.as_type())
            .unwrap_or(&LuaType::Unknown)
            .clone()
    }

    pub fn get_member_key(&self, index_key: &LuaIndexKey) -> Option<LuaMemberKey> {
        LuaMemberKey::from_index_key(self.db, &mut self.infer_cache.borrow_mut(), index_key).ok()
    }

    pub fn infer_member_type(
        &self,
        prefix_type: &LuaType,
        member_key: &LuaMemberKey,
    ) -> Result<LuaType, InferFailReason> {
        member::infer_raw_member_type(self.db, prefix_type, member_key)
    }

    pub fn get_member_origin_owner(&self, member_id: LuaMemberId) -> Option<LuaSemanticDeclId> {
        find_member_origin_owner(self.db, &mut self.infer_cache.borrow_mut(), member_id)
    }
}

/// Guard to prevent infinite recursion
/// Some type may reference itself, so we need to check if we have already inferred this type
#[derive(Debug)]
pub struct InferGuard {
    guard: HashSet<LuaTypeDeclId>,
}

impl InferGuard {
    pub fn new() -> Self {
        Self {
            guard: HashSet::default(),
        }
    }

    pub fn check(&mut self, type_id: &LuaTypeDeclId) -> Result<(), InferFailReason> {
        if self.guard.contains(type_id) {
            return Err(InferFailReason::RecursiveInfer);
        }
        self.guard.insert(type_id.clone());
        Ok(())
    }
}
