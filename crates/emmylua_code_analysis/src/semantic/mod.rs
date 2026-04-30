mod cache;
mod decl;
mod generic;
mod guard;
mod infer;
mod member;
mod overload_resolve;
mod reference;
mod semantic_info;
mod type_check;
pub(crate) mod type_queries;
mod visibility;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

pub use cache::{CacheEntry, CacheOptions, LuaAnalysisPhase, LuaInferCache};
pub use decl::{
    enum_variable_is_param, parse_require_module_info, parse_require_module_info_by_name_expr,
};
use emmylua_parser::{
    LuaAstNode, LuaCallExpr, LuaChunk, LuaClosureExpr, LuaExpr, LuaIndexExpr, LuaIndexKey,
    LuaLocalStat, LuaParseError, LuaSyntaxNode, LuaSyntaxToken, LuaTableExpr,
};
use infer::infer_expr_list_types;
pub use infer::infer_index_expr;
pub use infer::infer_table_field_value_should_be;
use lsp_types::Uri;
pub use member::LuaMemberInfo;
pub use member::find_index_operations;
pub use member::get_member_map;
use member::{
    find_member_origin_owner, find_members_in_scope, find_members_with_key_in_scope,
    get_member_map_in_scope,
};
use reference::is_reference_to;
use rowan::{NodeOrToken, TextRange, TextSize};
pub use semantic_info::SemanticInfo;
pub(crate) use semantic_info::{infer_node_semantic_decl, resolve_global_decl_id};
use semantic_info::{
    infer_node_semantic_info, infer_token_semantic_decl, infer_token_semantic_info,
};
use smol_str::SmolStr;
pub(crate) use type_check::check_type_compact;
use type_check::is_sub_type_of;
pub use visibility::check_module_visibility;
use visibility::check_visibility;

pub use crate::semantic::member::find_members;
pub use crate::semantic::member::find_members_with_key;
use crate::semantic::type_check::check_type_compact_detail;
use crate::{
    AsyncState, DbIndex, Emmyrc, LuaDecl, LuaDeclId, LuaDocument, LuaSemanticDeclId,
    LuaSignature, LuaSignatureId, LuaType, LuaTypeDecl, LuaTypeDeclId,
    SalsaCallExplainSummary, SalsaDocTypeLoweredKind, SalsaDocTypeLoweredObjectFieldKey,
    SalsaDocTypeNodeKey, SalsaGlobalRootSummary, SalsaMemberRootSummary, SalsaMemberTargetSummary,
    SalsaSemanticDeclSummary, SalsaSemanticMemberSummary, SalsaSemanticSignatureSummary,
    SalsaSignatureExplainSummary, SalsaSignatureReturnExplainSummary,
    SalsaSignatureTypeExplainSummary, SalsaSummaryHost, SalsaSyntaxIdSummary, SalsaTypeCandidateSummary,
    CompilationModuleInfo, FileId, LuaCompilation, WorkspaceId,
};
use crate::{
    LuaArrayType, LuaFunctionType, LuaIndexAccessKey, LuaIntersectionType, LuaMemberId,
    LuaMemberKey, LuaObjectType, LuaTupleStatus, LuaTupleType, LuaTypeOwner, VariadicType,
};
pub use generic::*;
pub use guard::{InferGuard, InferGuardRef};
pub use infer::InferFailReason;
pub(crate) use infer::infer_bind_value_type;
pub use infer::infer_call_expr_func;
pub(crate) use infer::infer_expr_root;
pub(crate) use infer::infer_expr;
pub use infer::infer_param;
pub(crate) use infer::infer_table_should_be_root;
pub(crate) use infer::infer_table_should_be;
use overload_resolve::resolve_signature;
pub use semantic_info::SemanticDeclLevel;
pub use type_check::{TypeCheckFailReason, TypeCheckResult};

pub use generic::get_keyof_members;
pub use infer::{DocTypeInferContext, infer_doc_type};
pub(crate) use generic::{build_self_type_inner, get_keyof_members_inner};
pub(crate) use member::{
    find_members_with_key_inner,
};

#[derive(Debug)]
pub struct SemanticModel<'a> {
    file_id: FileId,
    compilation: &'a LuaCompilation,
    summary: &'a SalsaSummaryHost,
    infer_cache: RefCell<LuaInferCache>,
    emmyrc: Arc<Emmyrc>,
    root: LuaChunk,
}

unsafe impl<'a> Send for SemanticModel<'a> {}
unsafe impl<'a> Sync for SemanticModel<'a> {}

impl<'a> SemanticModel<'a> {
    pub fn new(
        file_id: FileId,
        compilation: &'a LuaCompilation,
        summary: &'a SalsaSummaryHost,
        infer_config: LuaInferCache,
        emmyrc: Arc<Emmyrc>,
        root: LuaChunk,
    ) -> Self {
        Self {
            file_id,
            compilation,
            summary,
            infer_cache: RefCell::new(infer_config),
            emmyrc,
            root,
        }
    }

    fn db(&self) -> &DbIndex {
        self.compilation.legacy_db()
    }

    pub fn get_document(&'_ self) -> LuaDocument<'_> {
        self.compilation
            .get_document(self.file_id)
            .expect("always exists")
    }

    pub fn get_module(&self) -> Option<&CompilationModuleInfo> {
        self.compilation.find_module_by_file_id(self.file_id)
    }

    pub fn workspace_id(&self) -> Option<WorkspaceId> {
        Some(
            self.compilation
                .module_workspace_id(self.file_id)
                .unwrap_or(WorkspaceId::MAIN),
        )
    }

    pub fn get_compilation(&self) -> &LuaCompilation {
        self.compilation
    }

    pub fn get_summary(&self) -> &SalsaSummaryHost {
        self.summary
    }

    pub fn get_document_by_file_id(&'_ self, file_id: FileId) -> Option<LuaDocument<'_>> {
        self.compilation.get_document(file_id)
    }

    pub fn get_document_by_uri(&'_ self, uri: &Uri) -> Option<LuaDocument<'_>> {
        self.compilation.get_document_by_uri(uri)
    }

    pub fn get_root_by_file_id(&self, file_id: FileId) -> Option<LuaChunk> {
        self.compilation.get_root(file_id)
    }

    pub fn get_file_parse_error(&self) -> Option<Vec<LuaParseError>> {
        self.compilation.get_file_parse_error(self.file_id)
    }

    pub fn infer_expr(&self, expr: LuaExpr) -> Result<LuaType, InferFailReason> {
        if let LuaExpr::CallExpr(call_expr) = expr.clone()
            && call_expr.is_require()
            && let Some(typ) = self.summary_require_module_export_type(call_expr)
        {
            return Ok(typ);
        }

        if let LuaExpr::CallExpr(call_expr) = expr.clone()
            && let Some(typ) = self.summary_call_return_type(call_expr)
        {
            return Ok(typ);
        }

        infer_expr(
            self.compilation,
            self.db(),
            &mut self.infer_cache.borrow_mut(),
            expr,
        )
    }

    pub fn infer_table_should_be(&self, table: LuaTableExpr) -> Option<LuaType> {
        infer_table_should_be(
            self.compilation,
            self.db(),
            &mut self.infer_cache.borrow_mut(),
            table,
        )
        .ok()
    }

    pub fn get_member_infos(&self, prefix_type: &LuaType) -> Option<Vec<LuaMemberInfo>> {
        find_members_in_scope(
            self.compilation,
            self.file_id,
            prefix_type,
        )
    }

    pub fn get_member_info_with_key(
        &self,
        prefix_type: &LuaType,
        member_key: LuaMemberKey,
        find_all: bool,
    ) -> Option<Vec<LuaMemberInfo>> {
        find_members_with_key_in_scope(
            self.compilation,
            self.file_id,
            prefix_type,
            member_key,
            find_all,
        )
    }

    pub fn get_member_info_map(
        &self,
        prefix_type: &LuaType,
    ) -> Option<HashMap<LuaMemberKey, Vec<LuaMemberInfo>>> {
        get_member_map_in_scope(
            self.compilation,
            self.file_id,
            prefix_type,
        )
    }

    pub fn type_check(&self, source: &LuaType, compact_type: &LuaType) -> TypeCheckResult {
        check_type_compact(self.db(), source, compact_type)
    }

    pub fn type_check_detail(&self, source: &LuaType, compact_type: &LuaType) -> TypeCheckResult {
        check_type_compact_detail(self.db(), source, compact_type)
    }

    pub fn get_signature(&self, signature_id: &LuaSignatureId) -> Option<&LuaSignature> {
        self.compilation.get_signature(signature_id)
    }

    pub fn get_decl(&self, decl_id: &LuaDeclId) -> Option<&LuaDecl> {
        self.db().get_decl_index().get_decl(decl_id)
    }

    pub fn signature_id_of_decl(&self, decl: LuaSemanticDeclId) -> Option<LuaSignatureId> {
        match decl {
            LuaSemanticDeclId::LuaDecl(decl_id) => {
                let type_cache = self.compilation.get_type_cache(&decl_id.into())?;
                match type_cache {
                    crate::LuaTypeCache::InferType(LuaType::Signature(signature_id)) => {
                        Some(*signature_id)
                    }
                    _ => None,
                }
            }
            LuaSemanticDeclId::Member(member_id) => {
                let type_cache = self.compilation.get_type_cache(&member_id.into())?;
                match type_cache {
                    crate::LuaTypeCache::InferType(LuaType::Signature(signature_id)) => {
                        Some(*signature_id)
                    }
                    _ => None,
                }
            }
            LuaSemanticDeclId::Signature(signature_id) => Some(signature_id),
            _ => None,
        }
    }

    pub fn infer_call_expr_func(
        &self,
        call_expr: LuaCallExpr,
        arg_count: Option<usize>,
    ) -> Option<Arc<LuaFunctionType>> {
        let prefix_expr = call_expr.get_prefix_expr()?;
        let call_expr_type = infer_expr(
            self.compilation,
            self.db(),
            &mut self.infer_cache.borrow_mut(),
            prefix_expr,
        )
        .ok()?;
        infer_call_expr_func(
            self.db(),
            &mut self.infer_cache.borrow_mut(),
            call_expr,
            call_expr_type,
            &InferGuard::new(),
            arg_count,
        )
        .ok()
    }

    pub fn call_explain(&self, call_expr: LuaCallExpr) -> Option<SalsaCallExplainSummary> {
        self.summary
            .semantic()
            .file()
            .call_explain(self.file_id, call_expr.get_position())
    }

    pub fn signature_explain(
        &self,
        signature_id: LuaSignatureId,
    ) -> Option<SalsaSignatureExplainSummary> {
        self.signature_summary(signature_id)
            .map(|summary| summary.explain)
    }

    pub fn signature_summary(
        &self,
        signature_id: LuaSignatureId,
    ) -> Option<SalsaSemanticSignatureSummary> {
        self.summary
            .semantic()
            .file()
            .signature_summary(self.file_id, signature_id.get_position())
    }

    pub fn decl_summary(&self, decl_id: LuaDeclId) -> Option<SalsaSemanticDeclSummary> {
        self.summary
            .semantic()
            .file()
            .decl_summary(decl_id.file_id, u32::from(decl_id.position).into())
    }

    pub fn member_summary(&self, member_id: LuaMemberId) -> Option<SalsaSemanticMemberSummary> {
        self.summary.semantic().file().member_summary_by_syntax_id(
            member_id.file_id,
            SalsaSyntaxIdSummary::from(*member_id.get_syntax_id()),
        )
    }

    pub fn infer_member_owner_type(&self, member_id: LuaMemberId) -> Option<LuaType> {
        let member_summary = self.member_summary(member_id)?;
        self.infer_member_target_owner_type(member_id.file_id, &member_summary.member_type.target)
    }

    pub fn infer_member_access_owner_type(&self, member_id: LuaMemberId) -> Option<LuaType> {
        if let Some(LuaSemanticDeclId::Member(origin_member_id)) =
            self.get_member_origin_owner(member_id)
        {
            if let Some(owner_type) = self.infer_member_owner_type(origin_member_id) {
                return Some(owner_type);
            }

            if let Some(owner_type) = self.infer_member_owner_type_from_syntax(origin_member_id) {
                return Some(owner_type);
            }
        }

        self.infer_member_owner_type(member_id)
            .or_else(|| self.infer_member_owner_type_from_syntax(member_id))
    }

    pub fn resolved_call_signature_id(&self, call_expr: LuaCallExpr) -> Option<LuaSignatureId> {
        let call_explain = self.call_explain(call_expr)?;
        call_explain
            .resolved_signature_offset
            .map(|offset| LuaSignatureId::new(self.file_id, offset))
    }

    /// 推断表达式列表类型, 位于最后的表达式会触发多值推断
    pub fn infer_expr_list_types(
        &self,
        exprs: &[LuaExpr],
        var_count: Option<usize>,
    ) -> Vec<(LuaType, TextRange)> {
        let cache = &mut self.infer_cache.borrow_mut();
        infer_expr_list_types(self.db(), cache, exprs, var_count, |db, cache, expr| {
            Ok(infer_expr_root(db, cache, expr).unwrap_or(LuaType::Unknown))
        })
        .unwrap_or_default()
    }

    /// 推断值已经绑定的类型(不是推断值的类型). 例如从右值推断左值类型, 从调用参数推断函数参数类型
    pub fn infer_bind_value_type(&self, expr: LuaExpr) -> Option<LuaType> {
        if let Some(typ) = self.summary_call_arg_expected_type(expr.clone()) {
            return Some(typ);
        }

        if let Some(typ) = self.summary_table_field_expected_type(expr.clone()) {
            return Some(typ);
        }

        infer_bind_value_type(
            self.compilation,
            self.db(),
            &mut self.infer_cache.borrow_mut(),
            expr,
        )
    }

    pub fn infer_closure_return_info(&self, closure: LuaClosureExpr) -> Option<(bool, LuaType)> {
        let typ = self
            .infer_bind_value_type(closure.clone().into())
            .map(normalize_expected_closure_type)
            .unwrap_or(LuaType::Unknown);

        match typ {
            LuaType::DocFunction(func_type) => {
                return Some((true, func_type.get_ret().clone()));
            }
            LuaType::Signature(signature_id) => {
                let signature = self.db().get_signature_index().get(&signature_id)?;
                return Some((
                    signature.resolve_return == crate::SignatureReturnStatus::DocResolve,
                    signature.get_return_type(),
                ));
            }
            _ => {}
        }

        let signature_id = LuaSignatureId::from_closure(self.file_id, &closure);
        if let Some(summary) = self.signature_summary(signature_id)
            && let Some(return_type) =
                self.summary_signature_doc_return_type(&summary.explain.returns)
        {
            return Some((true, return_type));
        }

        let signature = self.db().get_signature_index().get(&signature_id)?;
        Some((
            signature.resolve_return == crate::SignatureReturnStatus::DocResolve,
            signature.get_return_type(),
        ))
    }

    fn summary_program_point_type(&self, expr: LuaExpr) -> Option<LuaType> {
        let program_point_offset = TextSize::from(u32::from(expr.get_position()));
        match expr {
            LuaExpr::CallExpr(call_expr) if call_expr.is_require() => {
                self.summary_require_module_export_type(call_expr)
            }
            LuaExpr::NameExpr(_) => self
                .summary
                .types()
                .name_at(self.file_id, program_point_offset, program_point_offset)
                .and_then(|info| self.summary_candidates_to_lua_type(&info.candidates)),
            LuaExpr::IndexExpr(_) => self
                .summary
                .types()
                .member_at(self.file_id, program_point_offset, program_point_offset)
                .and_then(|info| self.summary_candidates_to_lua_type(&info.candidates)),
            _ => None,
        }
    }

    fn summary_candidates_to_lua_type(
        &self,
        candidates: &[SalsaTypeCandidateSummary],
    ) -> Option<LuaType> {
        let types = candidates
            .iter()
            .filter_map(|candidate| self.summary_candidate_to_lua_type(candidate))
            .collect::<Vec<_>>();

        match types.as_slice() {
            [] => None,
            [single] => Some(single.clone()),
            _ => Some(LuaType::from_vec(types)),
        }
    }

    fn summary_candidate_to_lua_type(
        &self,
        candidate: &SalsaTypeCandidateSummary,
    ) -> Option<LuaType> {
        let mut types = candidate
            .explicit_type_offsets
            .iter()
            .filter_map(|type_key| self.summary_type_key_to_lua_type(*type_key))
            .collect::<Vec<_>>();

        if types.is_empty()
            && let Some(initializer_offset) = candidate.initializer_offset
            && let Some(expr) = self.find_expr_at_offset(initializer_offset)
            && let Ok(expr_type) = infer_expr_root(self.db(), &mut self.infer_cache.borrow_mut(), expr)
        {
            types.push(expr_type);
        }

        if types.is_empty()
            && let Some(signature_offset) = candidate.signature_offset
        {
            types.push(LuaType::Signature(LuaSignatureId::new(
                self.file_id,
                signature_offset,
            )));
        }

        match types.as_slice() {
            [] => None,
            [single] => Some(single.clone()),
            _ => Some(LuaType::from_vec(types)),
        }
    }

    fn summary_type_key_to_lua_type(&self, type_key: SalsaDocTypeNodeKey) -> Option<LuaType> {
        let resolved = self
            .summary
            .doc()
            .resolved_type_by_key(self.file_id, type_key)?;
        self.summary_lowered_type_to_lua_type(&resolved.lowered.kind)
    }

    fn summary_lowered_type_to_lua_type(&self, kind: &SalsaDocTypeLoweredKind) -> Option<LuaType> {
        match kind {
            SalsaDocTypeLoweredKind::Unknown => Some(LuaType::Unknown),
            SalsaDocTypeLoweredKind::Name { name } => self.summary_named_type_to_lua_type(name),
            SalsaDocTypeLoweredKind::Literal { text } => {
                self.summary_literal_type_to_lua_type(text)
            }
            SalsaDocTypeLoweredKind::Nullable { inner_type } => {
                let inner = self.summary_type_ref_to_lua_type(inner_type)?;
                Some(LuaType::from_vec(vec![inner, LuaType::Nil]))
            }
            SalsaDocTypeLoweredKind::Union { item_types }
            | SalsaDocTypeLoweredKind::MultiLineUnion { item_types }
            | SalsaDocTypeLoweredKind::Tuple { item_types } => {
                let types = item_types
                    .iter()
                    .filter_map(|type_ref| self.summary_type_ref_to_lua_type(type_ref))
                    .collect::<Vec<_>>();
                match types.as_slice() {
                    [] => None,
                    [single] => Some(single.clone()),
                    _ => Some(LuaType::from_vec(types)),
                }
            }
            SalsaDocTypeLoweredKind::Intersection { item_types } => {
                let types = item_types
                    .iter()
                    .filter_map(|type_ref| self.summary_type_ref_to_lua_type(type_ref))
                    .collect::<Vec<_>>();
                (!types.is_empty()).then_some(LuaIntersectionType::new(types).into())
            }
            SalsaDocTypeLoweredKind::Array { item_type } => self
                .summary_type_ref_to_lua_type(item_type)
                .map(|base| LuaType::Array(LuaArrayType::from_base_type(base).into())),
            SalsaDocTypeLoweredKind::Function {
                is_async,
                is_sync,
                params,
                returns,
                ..
            } => {
                let async_state = if *is_async {
                    AsyncState::Async
                } else if *is_sync {
                    AsyncState::Sync
                } else {
                    AsyncState::None
                };
                let params = params
                    .iter()
                    .map(|param| {
                        let mut typ = self.summary_type_ref_to_lua_type(&param.doc_type);
                        if param.is_nullable {
                            typ = match typ {
                                Some(typ) if !typ.is_nullable() => {
                                    Some(LuaType::from_vec(vec![typ, LuaType::Nil]))
                                }
                                other => other,
                            };
                        }
                        if param.is_dots {
                            typ = Some(match typ {
                                Some(typ) => LuaType::Variadic(VariadicType::Base(typ).into()),
                                None => LuaType::Variadic(VariadicType::Base(LuaType::Any).into()),
                            });
                        }

                        (param.name.clone().unwrap_or_default().to_string(), typ)
                    })
                    .collect::<Vec<_>>();
                let return_types = returns
                    .iter()
                    .filter_map(|ret| self.summary_type_ref_to_lua_type(&ret.doc_type))
                    .collect::<Vec<_>>();
                let ret = match return_types.as_slice() {
                    [] => LuaType::Nil,
                    [single] => single.clone(),
                    _ => LuaTupleType::new(return_types, LuaTupleStatus::DocResolve).into(),
                };

                Some(LuaFunctionType::new(async_state, false, false, params, ret).into())
            }
            SalsaDocTypeLoweredKind::Object { fields } => {
                let object_fields = fields
                    .iter()
                    .filter_map(|field| {
                        let key = match &field.key {
                            SalsaDocTypeLoweredObjectFieldKey::Name(name)
                            | SalsaDocTypeLoweredObjectFieldKey::String(name) => {
                                LuaIndexAccessKey::String(name.clone())
                            }
                            SalsaDocTypeLoweredObjectFieldKey::Integer(value) => {
                                LuaIndexAccessKey::Integer(*value)
                            }
                            SalsaDocTypeLoweredObjectFieldKey::Type(type_ref) => {
                                LuaIndexAccessKey::Type(
                                    self.summary_type_ref_to_lua_type(type_ref)?,
                                )
                            }
                        };
                        let value_type = self.summary_type_ref_to_lua_type(&field.value_type)?;
                        Some((key, value_type))
                    })
                    .collect::<Vec<_>>();
                Some(LuaObjectType::new(object_fields).into())
            }
            SalsaDocTypeLoweredKind::Variadic { item_type } => self
                .summary_type_ref_to_lua_type(item_type)
                .map(|base| LuaType::Variadic(VariadicType::Base(base).into())),
            _ => None,
        }
    }

    fn summary_signature_type_to_lua_type(
        &self,
        typ: &SalsaSignatureTypeExplainSummary,
    ) -> Option<LuaType> {
        if let Some(lowered) = &typ.lowered {
            return self.summary_lowered_type_to_lua_type(&lowered.kind);
        }

        self.summary_type_ref_to_lua_type(&typ.type_ref)
    }

    fn summary_signature_doc_return_type(
        &self,
        returns: &[SalsaSignatureReturnExplainSummary],
    ) -> Option<LuaType> {
        let row = returns
            .iter()
            .flat_map(|ret| ret.items.iter())
            .map(|item| {
                self.summary_signature_type_to_lua_type(&item.doc_type)
                    .unwrap_or(LuaType::Unknown)
            })
            .collect::<Vec<_>>();

        match row.as_slice() {
            [] => None,
            [single] => Some(single.clone()),
            _ => Some(LuaSignature::row_to_return_type(row)),
        }
    }

    fn summary_call_return_type(&self, call_expr: LuaCallExpr) -> Option<LuaType> {
        let call_explain = self.call_explain(call_expr)?;
        self.summary_signature_doc_return_type(&call_explain.returns)
            .or_else(|| call_explain.resolved_signature_offset.map(|_| LuaType::Nil))
    }

    fn summary_call_arg_expected_type(&self, expr: LuaExpr) -> Option<LuaType> {
        let call_arg_list = expr.get_parent::<emmylua_parser::LuaCallArgList>()?;
        let call_expr = call_arg_list.get_parent::<LuaCallExpr>()?;
        let arg_index = call_arg_list.get_args().position(|arg| arg == expr)?;
        let call_explain = self.call_explain(call_expr)?;
        let expected = call_explain
            .args
            .get(arg_index)?
            .expected_doc_type
            .as_ref()?;
        self.summary_signature_type_to_lua_type(expected)
    }

    fn summary_table_field_expected_type(&self, expr: LuaExpr) -> Option<LuaType> {
        let expr_syntax_id = SalsaSyntaxIdSummary::from(expr.get_syntax_id());
        if let Some(property) = self
            .summary
            .file()
            .property_by_value_expr_syntax_id(self.file_id, expr_syntax_id)
            && let Some(target) =
                crate::extend_property_owner_with_key(&property.owner, &property.key)
            && let Some(member_type) = self.summary.types().member(self.file_id, target)
        {
            let candidates = member_type
                .candidates
                .into_iter()
                .filter(|candidate| candidate.value_expr_syntax_id != Some(expr_syntax_id))
                .collect::<Vec<_>>();
            if let Some(typ) = self.summary_candidates_to_lua_type(&candidates) {
                return Some(typ);
            }
        }

        let table_field = expr.ancestors::<emmylua_parser::LuaTableField>().next()?;
        let field_key = table_field.get_field_key()?;
        let table_expr = table_field.get_parent::<LuaTableExpr>()?;
        let member_key = self.get_member_key(&field_key)?;
        if let Some(type_name) = self.summary_table_expr_declared_named_type(table_expr.clone())
            && let Some(typ) =
                self.compilation_named_type_member_expected_type(type_name.as_str(), &member_key)
        {
            return Some(typ);
        }
        let table_type = self
            .summary_table_expr_declared_type(table_expr.clone())
            .or_else(|| self.infer_table_should_be(table_expr.clone()))?;
        if let Some(typ) = self.compilation_member_expected_type(&table_type, &member_key) {
            return Some(typ);
        }
        self.infer_member_type(&table_type, &member_key).ok()
    }

    fn summary_table_expr_declared_type(&self, table_expr: LuaTableExpr) -> Option<LuaType> {
        let local_stat = table_expr.ancestors::<LuaLocalStat>().next()?;
        let local_name = local_stat.get_local_name_list().next()?;
        let decl = self.find_decl(
            local_name.syntax().clone().into(),
            SemanticDeclLevel::default(),
        )?;
        let LuaSemanticDeclId::LuaDecl(decl_id) = decl else {
            return None;
        };
        let decl_summary = self.decl_summary(decl_id)?;
        self.summary_value_shell_to_lua_type(&decl_summary.value_shell)
    }

    fn summary_table_expr_declared_named_type(&self, table_expr: LuaTableExpr) -> Option<SmolStr> {
        let local_stat = table_expr.ancestors::<LuaLocalStat>().next()?;
        let local_name = local_stat.get_local_name_list().next()?;
        let decl = self.find_decl(
            local_name.syntax().clone().into(),
            SemanticDeclLevel::default(),
        )?;
        let LuaSemanticDeclId::LuaDecl(decl_id) = decl else {
            return None;
        };
        let decl_summary = self.decl_summary(decl_id)?;
        self.summary_value_shell_named_type(&decl_summary.value_shell)
    }

    fn compilation_member_expected_type(
        &self,
        table_type: &LuaType,
        member_key: &LuaMemberKey,
    ) -> Option<LuaType> {
        match table_type {
            LuaType::Ref(type_decl_id) | LuaType::Def(type_decl_id) => {
                self.compilation_type_member_expected_type(type_decl_id, member_key)
            }
            LuaType::Union(union) => union
                .into_vec()
                .into_iter()
                .filter(|item| !matches!(item, LuaType::Nil))
                .find_map(|item| self.compilation_member_expected_type(&item, member_key)),
            _ => None,
        }
    }

    fn compilation_type_member_expected_type(
        &self,
        type_decl_id: &LuaTypeDeclId,
        member_key: &LuaMemberKey,
    ) -> Option<LuaType> {
        self.compilation_named_type_member_expected_type(type_decl_id.get_name(), member_key)
    }

    fn compilation_named_type_member_expected_type(
        &self,
        type_name: &str,
        member_key: &LuaMemberKey,
    ) -> Option<LuaType> {
        let member_name = member_key.get_name()?;
        let workspace_id = self.compilation.resolved_workspace_id(self.file_id);
        let member = self.compilation.find_type_merged_member(
            self.file_id,
            type_name,
            Some(workspace_id),
            member_name,
        )?;

        match member.source {
            crate::compilation::CompilationMemberSource::Property(_) => {
                let syntax_offset = member.syntax_offset?;
                let property = self
                    .summary
                    .file()
                    .property_at(member.file_id, syntax_offset)?;
                self.summary_property_to_lua_type(&property)
            }
            crate::compilation::CompilationMemberSource::RuntimeMember => None,
        }
    }

    fn summary_property_to_lua_type(
        &self,
        property: &crate::SalsaPropertySummary,
    ) -> Option<LuaType> {
        let mut typ = property
            .doc_type_offset
            .and_then(|type_key| self.summary_type_key_to_lua_type(type_key))
            .or_else(|| {
                property.value_expr_offset.and_then(|expr_offset| {
                    self.find_expr_at_offset(expr_offset).and_then(|expr| {
                        infer_expr_root(self.db(), &mut self.infer_cache.borrow_mut(), expr).ok()
                    })
                })
            })?;

        if property.is_nullable && !typ.is_nullable() {
            typ = LuaType::from_vec(vec![typ, LuaType::Nil]);
        }

        Some(typ)
    }

    fn summary_value_shell_to_lua_type(
        &self,
        value_shell: &crate::SalsaSemanticValueShellSummary,
    ) -> Option<LuaType> {
        let types = value_shell
            .candidate_type_offsets
            .iter()
            .filter_map(|type_key| self.summary_type_key_to_lua_type(*type_key))
            .collect::<Vec<_>>();

        match types.as_slice() {
            [] => None,
            [single] => Some(single.clone()),
            _ => Some(LuaType::from_vec(types)),
        }
    }

    fn summary_value_shell_named_type(
        &self,
        value_shell: &crate::SalsaSemanticValueShellSummary,
    ) -> Option<SmolStr> {
        value_shell
            .candidate_type_offsets
            .iter()
            .find_map(|type_key| {
                let resolved = self
                    .summary
                    .doc()
                    .resolved_type_by_key(self.file_id, *type_key)?;
                match &resolved.lowered.kind {
                    SalsaDocTypeLoweredKind::Name { name } => Some(name.clone()),
                    _ => None,
                }
            })
    }

    fn summary_type_ref_to_lua_type(&self, type_ref: &crate::SalsaDocTypeRef) -> Option<LuaType> {
        let crate::SalsaDocTypeRef::Node(type_key) = type_ref else {
            return None;
        };

        self.summary_type_key_to_lua_type(*type_key)
    }

    fn summary_named_type_to_lua_type(&self, name: &str) -> Option<LuaType> {
        self.named_type_to_lua_type_in_file(self.file_id, name)
    }

    pub fn find_type_decl(&self, name: &str) -> Option<&LuaTypeDecl> {
        self.find_type_decl_in_file(self.file_id, name)
    }

    pub fn find_type_decl_in_file(&self, file_id: FileId, name: &str) -> Option<&LuaTypeDecl> {
        self.compilation.find_type_decl(file_id, name)
    }

    pub fn get_type_decl(&self, decl_id: &LuaTypeDeclId) -> Option<&LuaTypeDecl> {
        self.compilation.get_type_decl(decl_id)
    }

    fn named_type_to_lua_type_in_file(&self, file_id: FileId, name: &str) -> Option<LuaType> {
        let builtin = match name {
            "unknown" => Some(LuaType::Unknown),
            "any" => Some(LuaType::Any),
            "nil" => Some(LuaType::Nil),
            "table" => Some(LuaType::Table),
            "userdata" => Some(LuaType::Userdata),
            "function" | "fun" => Some(LuaType::Function),
            "thread" => Some(LuaType::Thread),
            "boolean" | "bool" => Some(LuaType::Boolean),
            "string" => Some(LuaType::String),
            "integer" => Some(LuaType::Integer),
            "number" => Some(LuaType::Number),
            "io" => Some(LuaType::Io),
            _ => None,
        };
        if builtin.is_some() {
            return builtin;
        }

        let type_decl = self.find_type_decl_in_file(file_id, name)?;
        Some(LuaType::Ref(type_decl.get_id()))
    }

    fn infer_member_target_owner_type(
        &self,
        file_id: FileId,
        target: &SalsaMemberTargetSummary,
    ) -> Option<LuaType> {
        let mut owner_type = match &target.root {
            SalsaMemberRootSummary::Global(SalsaGlobalRootSummary::Env) => LuaType::Global,
            SalsaMemberRootSummary::Global(SalsaGlobalRootSummary::Name(name)) => {
                let global_decl_id = resolve_global_decl_id(
                    self.db(),
                    &mut self.infer_cache.borrow_mut(),
                    name,
                    None,
                );

                global_decl_id
                    .map(|decl_id| self.get_type(decl_id.into()))
                    .or_else(|| self.named_type_to_lua_type_in_file(file_id, name))?
            }
            SalsaMemberRootSummary::LocalDecl { decl_id, .. } => {
                self.get_type(LuaDeclId::new(file_id, decl_id.as_position()).into())
            }
        };

        for segment in target.owner_segments.iter() {
            owner_type = self
                .infer_member_type(&owner_type, &LuaMemberKey::from(segment.as_str()))
                .ok()?;
        }

        Some(owner_type)
    }

    fn infer_member_owner_type_from_syntax(&self, member_id: LuaMemberId) -> Option<LuaType> {
        let root = self
            .db()
            .get_vfs()
            .get_syntax_tree(&member_id.file_id)?
            .get_red_root();
        let cur_node = member_id.get_syntax_id().to_node_from_root(&root)?;
        let index_expr = LuaIndexExpr::cast(cur_node)?;

        index_expr.get_prefix_expr().map(|prefix_expr| {
            self.infer_expr(prefix_expr.clone())
                .unwrap_or(LuaType::SelfInfer)
        })
    }

    fn summary_literal_type_to_lua_type(&self, text: &str) -> Option<LuaType> {
        match text {
            "true" => return Some(LuaType::DocBooleanConst(true)),
            "false" => return Some(LuaType::DocBooleanConst(false)),
            "nil" => return Some(LuaType::Nil),
            _ => {}
        }

        if let Ok(value) = text.parse::<i64>() {
            return Some(LuaType::DocIntegerConst(value));
        }
        if let Ok(value) = text.parse::<f64>() {
            return Some(LuaType::FloatConst(value));
        }

        let unquoted = match text.as_bytes() {
            [b'"', .., b'"'] | [b'\'', .., b'\''] if text.len() >= 2 => {
                Some(&text[1..text.len() - 1])
            }
            _ => None,
        };

        unquoted.map(|value| LuaType::DocStringConst(SmolStr::new(value).into()))
    }

    fn find_expr_at_offset(&self, syntax_offset: TextSize) -> Option<LuaExpr> {
        self.root
            .descendants::<LuaExpr>()
            .find(|expr| expr.get_position() == syntax_offset)
    }

    fn summary_semantic_info_for_expr_node(&self, node: LuaSyntaxNode) -> Option<SemanticInfo> {
        if !LuaExpr::can_cast(node.kind().into()) {
            return None;
        }

        let expr = LuaExpr::cast(node.clone())?;
        let typ = self.summary_program_point_type(expr.clone())?;
        let semantic_decl = self.summary_expr_semantic_decl(&expr).or_else(|| {
            infer_node_semantic_decl(
                self.db(),
                &mut self.infer_cache.borrow_mut(),
                node,
                SemanticDeclLevel::default(),
            )
        });
        Some(SemanticInfo { typ, semantic_decl })
    }

    fn summary_semantic_info_for_expr_token(&self, token: LuaSyntaxToken) -> Option<SemanticInfo> {
        let parent = token.parent()?;
        if !LuaExpr::can_cast(parent.kind().into()) {
            return None;
        }

        let expr = LuaExpr::cast(parent)?;
        let typ = self.summary_program_point_type(expr.clone())?;
        let semantic_decl = self.summary_expr_semantic_decl(&expr).or_else(|| {
            infer_token_semantic_decl(
                self.db(),
                &mut self.infer_cache.borrow_mut(),
                token,
                SemanticDeclLevel::default(),
            )
        });
        Some(SemanticInfo { typ, semantic_decl })
    }

    pub fn get_semantic_info(
        &self,
        node_or_token: NodeOrToken<LuaSyntaxNode, LuaSyntaxToken>,
    ) -> Option<SemanticInfo> {
        match node_or_token {
            NodeOrToken::Node(node) => {
                if let Some(info) = self.summary_semantic_info_for_expr_node(node.clone()) {
                    return Some(info);
                }

                infer_node_semantic_info(
                    self.compilation,
                    self.db(),
                    &mut self.infer_cache.borrow_mut(),
                    node,
                )
            }
            NodeOrToken::Token(token) => {
                if let Some(info) = self.summary_semantic_info_for_expr_token(token.clone()) {
                    return Some(info);
                }

                infer_token_semantic_info(
                    self.compilation,
                    self.db(),
                    &mut self.infer_cache.borrow_mut(),
                    token,
                )
            }
        }
    }

    pub fn find_decl(
        &self,
        node_or_token: NodeOrToken<LuaSyntaxNode, LuaSyntaxToken>,
        level: SemanticDeclLevel,
    ) -> Option<LuaSemanticDeclId> {
        match node_or_token {
            NodeOrToken::Node(node) => LuaExpr::cast(node.clone())
                .and_then(|expr| self.summary_expr_semantic_decl(&expr))
                .or_else(|| {
                    infer_node_semantic_decl(
                        self.db(),
                        &mut self.infer_cache.borrow_mut(),
                        node,
                        level,
                    )
                }),
            NodeOrToken::Token(token) => infer_token_semantic_decl(
                self.db(),
                &mut self.infer_cache.borrow_mut(),
                token,
                level,
            ),
        }
    }

    pub fn is_reference_to(
        &self,
        node: LuaSyntaxNode,
        semantic_decl_id: LuaSemanticDeclId,
        level: SemanticDeclLevel,
    ) -> bool {
        is_reference_to(
            self.db(),
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
            self.compilation,
            self.file_id,
            &self.emmyrc,
            &mut self.infer_cache.borrow_mut(),
            token,
            property_owner,
        )
        .unwrap_or(true)
    }

    pub fn is_sub_type_of(
        &self,
        sub_type_ref_id: &LuaTypeDeclId,
        super_type_ref_id: &LuaTypeDeclId,
    ) -> bool {
        is_sub_type_of(self.db(), sub_type_ref_id, super_type_ref_id)
    }

    pub fn get_emmyrc(&self) -> &Emmyrc {
        &self.emmyrc
    }

    pub fn get_emmyrc_arc(&self) -> Arc<Emmyrc> {
        self.emmyrc.clone()
    }

    pub fn get_root(&self) -> &LuaChunk {
        &self.root
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub fn get_cache(&self) -> &RefCell<LuaInferCache> {
        &self.infer_cache
    }

    pub fn get_type(&self, type_owner: LuaTypeOwner) -> LuaType {
        self.compilation
            .get_type_cache(&type_owner)
            .map(|cache| cache.as_type())
            .unwrap_or(&LuaType::Unknown)
            .clone()
    }

    pub fn get_member_key(&self, index_key: &LuaIndexKey) -> Option<LuaMemberKey> {
        LuaMemberKey::from_index_key(self.db(), &mut self.infer_cache.borrow_mut(), index_key).ok()
    }

    pub fn infer_member_type(
        &self,
        prefix_type: &LuaType,
        member_key: &LuaMemberKey,
    ) -> Result<LuaType, InferFailReason> {
        member::infer_raw_member_type(self.db(), prefix_type, member_key)
    }

    pub fn get_member_origin_owner(&self, member_id: LuaMemberId) -> Option<LuaSemanticDeclId> {
        find_member_origin_owner(self.db(), &mut self.infer_cache.borrow_mut(), member_id)
    }

    fn summary_expr_semantic_decl(&self, expr: &LuaExpr) -> Option<LuaSemanticDeclId> {
        match expr {
            LuaExpr::NameExpr(name_expr) => self
                .summary
                .semantic()
                .file()
                .decl_summary_by_syntax_id(self.file_id, name_expr.get_syntax_id().into())
                .map(|summary| {
                    LuaSemanticDeclId::LuaDecl(LuaDeclId::new(
                        self.file_id,
                        summary.decl_type.decl_id.as_position(),
                    ))
                }),
            LuaExpr::IndexExpr(index_expr) => self
                .summary
                .lexical()
                .member_resolution_by_syntax_id(self.file_id, index_expr.get_syntax_id().into())
                .and_then(|member_use| self.summary_member_decl_from_target(member_use.target))
                .or_else(|| {
                    self.summary
                        .file()
                        .member_by_syntax_id(self.file_id, index_expr.get_syntax_id().into())
                        .map(|_| {
                            LuaSemanticDeclId::Member(LuaMemberId::new(
                                index_expr.get_syntax_id(),
                                self.file_id,
                            ))
                        })
                })
                .or_else(|| {
                    self.summary
                        .semantic()
                        .file()
                        .member_summary_by_syntax_id(
                            self.file_id,
                            index_expr.get_syntax_id().into(),
                        )
                        .map(|_| {
                            LuaSemanticDeclId::Member(LuaMemberId::new(
                                index_expr.get_syntax_id(),
                                self.file_id,
                            ))
                        })
                }),
            LuaExpr::CallExpr(call_expr) if call_expr.is_require() => {
                self.summary_require_module_semantic_decl(call_expr.clone())
            }
            _ => self.summary_module_export_semantic_decl(expr.clone()),
        }
    }

    fn summary_member_decl_from_target(
        &self,
        target: crate::SalsaMemberTargetId,
    ) -> Option<LuaSemanticDeclId> {
        let member_summary = self
            .summary
            .file()
            .members(self.file_id)?;
        let member = member_summary
            .members
            .iter()
            .find(|member| member.target == target)?;

        Some(LuaSemanticDeclId::Member(LuaMemberId::new(
            member.syntax_id.to_lua_syntax_id(),
            self.file_id,
        )))
    }

    fn summary_module_export_semantic_decl(&self, expr: LuaExpr) -> Option<LuaSemanticDeclId> {
        let export_expr = crate::module_query::export::module_export_expr(self.db(), self.file_id)?;
        if export_expr.syntax() != expr.syntax() {
            return None;
        }

        crate::module_query::export::compilation_module_semantic_id(self.compilation, self.file_id)
    }

    pub fn get_index_decl_type(&self, index_expr: LuaIndexExpr) -> Option<LuaType> {
        let cache = &mut self.infer_cache.borrow_mut();
        infer_index_expr(self.compilation, self.db(), cache, index_expr, false)
            .ok()
    }

    fn summary_require_module_export_type(&self, call_expr: LuaCallExpr) -> Option<LuaType> {
        let module_path = crate::module_query::identity::require_call_module_path(call_expr)?;
        self.compilation
            .find_required_module_export_type(&module_path)
    }

    fn summary_require_module_semantic_decl(
        &self,
        call_expr: LuaCallExpr,
    ) -> Option<LuaSemanticDeclId> {
        let module_path = crate::module_query::identity::require_call_module_path(call_expr)?;
        self.compilation
            .find_required_module_semantic_id(&module_path)
    }
}

fn normalize_expected_closure_type(typ: LuaType) -> LuaType {
    let LuaType::Union(union) = &typ else {
        return typ;
    };

    let non_nil_types = union
        .into_vec()
        .into_iter()
        .filter(|item| !matches!(item, LuaType::Nil))
        .collect::<Vec<_>>();

    match non_nil_types.as_slice() {
        [single @ LuaType::DocFunction(_)] | [single @ LuaType::Signature(_)] => single.clone(),
        _ => typ,
    }
}
