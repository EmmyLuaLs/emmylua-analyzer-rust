//! 类型推断模块
//!
//! 新架构：salsa-first 双路径推断
//! - 快速路径：查 `SalsaSummaryDatabase` 中已有的类型注释（named_type_names）
//! - 慢速路径：基于 AST 遍历的本地推断（literal、closure 等自包含表达式）
//!
//! 名称推断（NameExpr）完整链路：
//!   1. Salsa lexical → 名称解析到哪个声明
//!   2. Salsa types → 声明的类型元数据（SalsaDeclTypeInfoSummary）
//!   3. named_type_names → 直接构造 LuaType::Ref / Def
//!   4. explicit_type_offsets → 需 VFS + doc type 展开（后续 phase）
//!   5. 全局查找 → SalsaTypeQueries::global()

mod binary;
mod cache;
mod call;
mod index;
mod member;
mod table;
mod ternary;
mod unary;

use std::cell::RefCell;
use std::sync::Arc;

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaClosureExpr, LuaExpr, LuaLiteralExpr, LuaLiteralToken, LuaNameExpr,
    LuaSyntaxKind, NumberResult,
};
use rowan::TextRange;
use smol_str::SmolStr;

use crate::compilation::{
    SalsaDeclId, SalsaDeclTypeInfoSummary, SalsaDocTypeDefKindSummary, SalsaDocTypeDefSummary,
    SalsaDocTypeLoweredKind, SalsaDocTypeLoweredNode, SalsaDocTypeLoweredParam, SalsaDocTypeRef,
    SalsaDocVisibilityKindSummary, SalsaLexicalUseSummary, SalsaNameUseResolutionSummary,
    SalsaSummaryDatabase,
};
use crate::semantic_model::SigQuery;
use crate::semantic_model::offset_types::DeclPosition;
use crate::semantic_model::type_check::check_type_compact;
use crate::{
    AsyncState, Emmyrc, FileId, LuaArrayLen, LuaArrayType, LuaDeclId, LuaFunctionType,
    LuaMemberKey, LuaSignatureId, LuaType, LuaTypeDeclId, LuaUnionType, VariadicType,
};

use super::type_check::TypeCheckFailReason;

pub use cache::InferCache;
use call::infer_call_expr;
use index::infer_index_expr;
use table::infer_table_expr;

pub type InferResult = Result<LuaType, InferFailReason>;

/// 推断失败原因
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferFailReason {
    /// 无法推断（静默失败）
    None,
    /// 递归推断检测
    RecursiveInfer,
    /// 字段未找到
    FieldNotFound,
    /// 无法解析声明类型
    UnResolveDeclType(LuaDeclId),
    /// 无法解析模块导出
    UnResolveModuleExport(FileId),
    /// 尚未实现（新 semantic_model 的占位）
    NotImplemented,
}

impl InferFailReason {
    pub fn is_need_resolve(&self) -> bool {
        matches!(
            self,
            InferFailReason::UnResolveDeclType(_) | InferFailReason::UnResolveModuleExport(_)
        )
    }
}

/// 类型推断查询器。通过 `SemanticModel::infer()` 获取。
///
/// 设计要点：
/// - 持有 `SalsaSummaryDatabase` 的 Arc 用于快速路径
/// - 共享 `InferCache`（由 `SemanticModel` 持有，跨 checker 调用复用）
/// - `file_id` 指明当前分析的文件
pub struct InferQuery<'db> {
    db: &'db SalsaSummaryDatabase,
    file_id: FileId,
    emmyrc: Arc<Emmyrc>,
    cache: &'db RefCell<InferCache>,
    pub(super) root: emmylua_parser::LuaChunk,
}

impl<'db> InferQuery<'db> {
    pub(crate) fn with_cache(
        db: &'db SalsaSummaryDatabase,
        file_id: FileId,
        emmyrc: Arc<Emmyrc>,
        cache: &'db RefCell<InferCache>,
        root: emmylua_parser::LuaChunk,
    ) -> Self {
        Self {
            db,
            file_id,
            emmyrc,
            cache,
            root,
        }
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub(crate) fn sig_query(&self) -> SigQuery {
        SigQuery::new(self.db, self.file_id)
    }

    pub(super) fn read_db(&self) -> &SalsaSummaryDatabase {
        &self.db
    }

    /// 类型检查快捷方法。
    pub(super) fn check_type_compact(
        &self,
        source: &LuaType,
        compact: &LuaType,
    ) -> Result<(), TypeCheckFailReason> {
        check_type_compact(self.emmyrc.clone(), source, compact)
    }

    /// 推断成员类型。给前缀类型和 key，返回成员类型。
    pub fn infer_member_type(
        &self,
        prefix_type: &LuaType,
        member_key: &LuaMemberKey,
    ) -> InferResult {
        let db = self.read_db();
        member::infer_member_impl(self, &db, prefix_type, member_key)
    }

    /// 推断表达式列表的类型。
    ///
    /// 处理多返回值（Variadic）展开和 `var_count` 截断。
    pub fn infer_expr_list_types(
        &self,
        exprs: &[LuaExpr],
        var_count: Option<usize>,
    ) -> Result<Vec<(LuaType, TextRange)>, InferFailReason> {
        let mut value_types = Vec::new();
        for (idx, expr) in exprs.iter().enumerate() {
            if let Some(max_count) = var_count {
                if value_types.len() >= max_count {
                    break;
                }
            }

            let expr_type = self.infer_expr(expr.clone())?;

            // 多返回值展开
            if let Some(max_count) = var_count {
                if expr_type.contain_multi_return() && idx < max_count {
                    for i in idx..max_count {
                        if let Some(typ) = expr_type.get_result_slot_type(i - idx) {
                            value_types.push((typ, expr.get_range()));
                        } else {
                            break;
                        }
                    }
                    break;
                }
            }

            match &expr_type {
                LuaType::Variadic(variadic) => {
                    match variadic.as_ref() {
                        VariadicType::Base(base) => {
                            value_types.push((base.clone(), expr.get_range()));
                        }
                        VariadicType::Multi(types) => {
                            for t in types {
                                value_types.push((t.clone(), expr.get_range()));
                            }
                        }
                    }
                    break;
                }
                _ => value_types.push((expr_type, expr.get_range())),
            }
        }
        Ok(value_types)
    }

    /// 推断表应该符合的目标类型（如 `@type` 标注）。
    pub fn infer_table_should_be(
        &self,
        table_expr: emmylua_parser::LuaTableExpr,
    ) -> Option<LuaType> {
        let parent = table_expr.syntax().parent()?;
        let db = self.read_db();

        // Case: local a = { ... } with @type annotation
        if let Some(local) = emmylua_parser::LuaLocalStat::cast(parent.clone()) {
            let names: Vec<_> = local.get_local_name_list().collect();
            let exprs: Vec<_> = local.get_value_exprs().collect();
            let idx = exprs
                .iter()
                .position(|e| e.get_range() == table_expr.get_range())?;
            let name = names.get(idx)?;
            let name_tk = name.get_name_token()?;
            // Try salsa type info first
            if let Some(name_info) = db.types().name(self.file_id, name_tk.get_position()) {
                if let Some(dt) = name_info.decl_type {
                    if let Some(ty) = self.resolve_decl_type(&db, dt) {
                        return Some(ty);
                    }
                }
            }
            // Fallback: look up @type annotation via doc type_tags
            if let Some(doc) = db.doc().summary(self.file_id) {
                for type_tag in &doc.type_tags {
                    if type_tag.owner.kind
                        == crate::compilation::SalsaDocOwnerKindSummary::LocalStat
                        && type_tag.owner.syntax_offset == Some(local.get_position())
                    {
                        let types: Vec<LuaType> = type_tag
                            .type_offsets
                            .iter()
                            .filter_map(|key| {
                                db.doc()
                                    .resolved_type_by_key(self.file_id, *key)
                                    .and_then(|r| lowered_node_to_lua_type(&r.lowered))
                            })
                            .collect();
                        return match types.len() {
                            0 => None,
                            1 => types.into_iter().next(),
                            _ => Some(LuaType::Union(LuaUnionType::from_vec(types).into())),
                        };
                    }
                }
            }
        }

        // Case: a.b = { ... } — assign stat
        if let Some(assign) = emmylua_parser::LuaAssignStat::cast(parent.clone()) {
            let (vars, exprs) = assign.get_var_and_expr_list();
            let idx = exprs
                .iter()
                .position(|e| e.get_range() == table_expr.get_range())?;
            let var = vars.get(idx)?;
            return self.infer_expr(var.clone().into()).ok();
        }

        // Case: nested table field — delegate to parent table
        if emmylua_parser::LuaTableField::cast(parent).is_some() {
            // table field inside another table — recursion not needed for current use cases
            return None;
        }

        None
    }

    /// 推断值绑定的目标类型（右值 → 左值类型推断）。
    /// Salsa-native：通过 AST parent 找到绑定变量，查询其类型。
    pub fn infer_bind_value_type(&self, expr: LuaExpr) -> Option<LuaType> {
        let parent = expr.syntax().parent()?;
        // Case 1: local f: SomeType = expr — find the local name and its type
        if let Some(local) = emmylua_parser::LuaLocalStat::cast(parent.clone()) {
            let names: Vec<_> = local.get_local_name_list().collect();
            let exprs: Vec<_> = local.get_value_exprs().collect();
            let idx = exprs
                .iter()
                .position(|e| e.get_range() == expr.get_range())?;
            let name = names.get(idx)?;
            let name_tk = name.get_name_token()?;
            let db = self.read_db();
            // Try salsa type info for the local declaration
            let name_info = db.types().name(self.file_id, name_tk.get_position())?;
            if let Some(dt) = name_info.decl_type {
                return self.resolve_decl_type(&db, dt);
            }
        }
        // Case 2: function definition — closure IS the body, return Signature
        if emmylua_parser::LuaLocalFuncStat::cast(parent.clone()).is_some()
            || emmylua_parser::LuaFuncStat::cast(parent.clone()).is_some()
        {
            if let Some(closure) = LuaClosureExpr::cast(expr.syntax().clone()) {
                let sig_id = LuaSignatureId::from_closure(self.file_id, &closure);
                return Some(LuaType::Signature(sig_id));
            }
        }

        // Case 2b: table field closure — look up field type from class definition
        if let Some(table_field) = emmylua_parser::LuaTableField::cast(parent.clone()) {
            if let Some(closure) = LuaClosureExpr::cast(expr.syntax().clone()) {
                // Try to resolve the field type via the table's class definition
                if let Some(field_key) = table_field.get_field_key() {
                    let member_key = match &field_key {
                        emmylua_parser::LuaIndexKey::Name(token) => {
                            LuaMemberKey::Name(SmolStr::new(token.get_name_text()))
                        }
                        emmylua_parser::LuaIndexKey::String(token) => {
                            LuaMemberKey::Name(SmolStr::new(token.get_text()))
                        }
                        emmylua_parser::LuaIndexKey::Integer(token) => {
                            let val = match token.get_number_value() {
                                NumberResult::Int(i) => i,
                                NumberResult::Uint(u) => u as i64,
                                NumberResult::Float(f) => f as i64,
                                NumberResult::Number => 0,
                            };
                            LuaMemberKey::Integer(val)
                        }
                        _ => {
                            // Expr key — fallback to Signature
                            let sig_id = LuaSignatureId::from_closure(self.file_id, &closure);
                            return Some(LuaType::Signature(sig_id));
                        }
                    };
                    if let Some(parent_table) =
                        table_field.get_parent::<emmylua_parser::LuaTableExpr>()
                    {
                        if let Some(table_type) = self.infer_table_should_be(parent_table) {
                            if let Ok(member_type) =
                                self.infer_member_type(&table_type, &member_key)
                            {
                                return Some(member_type);
                            }
                        }
                    }
                }
                // Fallback: return Signature
                let sig_id = LuaSignatureId::from_closure(self.file_id, &closure);
                return Some(LuaType::Signature(sig_id));
            }
        }
        // Case 3: f = expr — find assign target type
        if let Some(assign) = emmylua_parser::LuaAssignStat::cast(parent) {
            let (vars, exprs) = assign.get_var_and_expr_list();
            let idx = exprs
                .iter()
                .position(|e| e.get_range() == expr.get_range())?;
            let var = vars.get(idx)?;
            return self.infer_expr(var.clone().into()).ok();
        }
        None
    }

    /// 推断表达式的类型。
    ///
    /// 流程：
    /// 1. 检查本地缓存
    /// 2. 尝试 salsa 快速路径（名称解析 → 声明类型）
    /// 3. AST 遍历推断
    pub fn infer_expr(&self, expr: LuaExpr) -> InferResult {
        let syntax_id = expr.get_syntax_id();

        // 1. 本地缓存
        if let Some(cached) = self.cache.borrow().get(&syntax_id) {
            if let LuaExpr::NameExpr(name) = expr {}
            return cached;
        }

        // 2. 尝试 salsa 快速路径
        if let Some(ty) = self.lookup_salsa_type(&expr) {
            if let LuaExpr::NameExpr(name) = expr {}
            self.cache.borrow_mut().insert(syntax_id, ty.clone());
            return Ok(ty);
        }

        // 3. AST 推断
        let result = self.infer_expr_ast(expr);

        // 缓存结果
        match &result {
            Ok(ty) => {
                self.cache.borrow_mut().insert(syntax_id, ty.clone());
            }
            Err(InferFailReason::None)
            | Err(InferFailReason::RecursiveInfer)
            | Err(InferFailReason::NotImplemented) => {
                self.cache.borrow_mut().insert(syntax_id, LuaType::Unknown);
                return Ok(LuaType::Unknown);
            }
            Err(InferFailReason::FieldNotFound) => {
                self.cache.borrow_mut().insert(syntax_id, LuaType::Never);
                return Ok(LuaType::Never);
            }
            _ => {
                // 需要 resolve 的错误不缓存，下次可能成功
            }
        }

        result
    }

    fn lookup_salsa_type(&self, expr: &LuaExpr) -> Option<LuaType> {
        if let LuaExpr::NameExpr(name) = expr {
            let db = self.read_db();
            let name_info = db.types().name(self.file_id, name.get_position())?;
            if let Some(decl_type) = name_info.decl_type {
                return self.resolve_decl_type(&db, decl_type);
            }
            // decl_type 为空时，通过声明 ID 查找类型
            if let SalsaNameUseResolutionSummary::LocalDecl(decl_id) = name_info.name_use.resolution
            {
                if let Some(dt) = db.types().decl(self.file_id, decl_id) {
                    return self.resolve_decl_type(&db, dt);
                }
            }
        }
        None
    }

    pub(crate) fn resolve_decl_type(
        &self,
        db: &SalsaSummaryDatabase,
        decl_type: SalsaDeclTypeInfoSummary,
    ) -> Option<LuaType> {
        self.resolve_decl_type_depth(db, decl_type, 0)
    }

    fn resolve_decl_type_depth(
        &self,
        db: &SalsaSummaryDatabase,
        decl_type: SalsaDeclTypeInfoSummary,
        depth: usize,
    ) -> Option<LuaType> {
        if depth > 10 {
            return None;
        }
        // Priority 0.4: named types (must come before value expr heuristic —
        // e.g., `local a = {}` with `@type ClassName` should resolve to ClassName)
        if !decl_type.named_type_names.is_empty() {
            let ty = self.resolve_named_types(db, &decl_type.named_type_names);
            // If result is generic Function, check if decl is actually a closure
            if matches!(ty, LuaType::Function) {
                if let Some(sid) = &decl_type.value_expr_syntax_id {
                    if !matches!(
                        sid.kind,
                        LuaSyntaxKind::LiteralExpr
                            | LuaSyntaxKind::NameExpr
                            | LuaSyntaxKind::IndexExpr
                            | LuaSyntaxKind::CallExpr
                            | LuaSyntaxKind::BinaryExpr
                            | LuaSyntaxKind::UnaryExpr
                    ) {
                        return Some(LuaType::Signature(LuaSignatureId::from_position(
                            self.file_id,
                            sid.start_offset,
                        )));
                    }
                }
            }
            return Some(ty);
        }
        // Priority 1.5: explicit type offsets (before Signature/closures)
        if !decl_type.explicit_type_offsets.is_empty() {
            let key = decl_type.explicit_type_offsets.first()?;
            if let Some(resolved) = db.doc().resolved_type_by_key(self.file_id, *key) {
                return lowered_node_to_lua_type(&resolved.lowered);
            }
            if let Some(lowered) = db.doc().lowered_type_by_key(self.file_id, *key) {
                return lowered_node_to_lua_type(&lowered);
            }
        }
        // Priority 0: Signature from function declarations
        if let Some(offset) = decl_type.value_signature_offset {
            return Some(LuaType::Signature(LuaSignatureId::from_position(
                self.file_id,
                offset,
            )));
        }
        // Priority 0.5: value expression IS a closure → Signature
        // (only when no explicit type annotation)
        if let Some(sid) = &decl_type.value_expr_syntax_id {
            if !matches!(
                sid.kind,
                LuaSyntaxKind::LiteralExpr
                    | LuaSyntaxKind::NameExpr
                    | LuaSyntaxKind::IndexExpr
                    | LuaSyntaxKind::CallExpr
                    | LuaSyntaxKind::BinaryExpr
                    | LuaSyntaxKind::UnaryExpr
            ) {
                return Some(LuaType::Signature(LuaSignatureId::from_position(
                    self.file_id,
                    sid.start_offset,
                )));
            }
        }
        // Priority 0.6: value expression is a NameExpr → trace through to the name's type
        // (e.g., `local x = p` where p is typed → x gets p's type)
        if let Some(sid) = &decl_type.value_expr_syntax_id {
            if sid.kind == LuaSyntaxKind::NameExpr {
                // Path A: lexical name resolution by syntax_id (most precise)
                if let Some(name_use) = db
                    .lexical()
                    .name_resolution_by_syntax_id(self.file_id, *sid)
                {
                    if let SalsaNameUseResolutionSummary::LocalDecl(decl_id) = &name_use.resolution
                    {
                        if let Some(dt) = db.types().decl(self.file_id, *decl_id) {
                            if let Some(ty) = self.resolve_decl_type_depth(db, dt, depth + 1) {
                                return Some(ty);
                            }
                        }
                    }
                }
                // Path B: fallback via name type info at start_offset
                if let Some(name_info) = db.types().name(self.file_id, sid.start_offset) {
                    if let Some(dt) = &name_info.decl_type {
                        if let Some(ty) = self.resolve_decl_type_depth(db, dt.clone(), depth + 1) {
                            return Some(ty);
                        }
                    }
                    if let SalsaNameUseResolutionSummary::LocalDecl(decl_id) =
                        &name_info.name_use.resolution
                    {
                        if let Some(dt) = db.types().decl(self.file_id, *decl_id) {
                            if let Some(ty) = self.resolve_decl_type_depth(db, dt, depth + 1) {
                                return Some(ty);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// 将命名类型列表转换为 LuaType。
    /// 多个名称表示 union（如 `: string | number`）。
    fn resolve_named_types(&self, db: &SalsaSummaryDatabase, names: &[SmolStr]) -> LuaType {
        let mut types: Vec<LuaType> = names
            .iter()
            .filter_map(|name| self.resolve_single_named_type(db, name))
            .collect();

        match types.len() {
            0 => LuaType::Unknown,
            1 => types.pop().expect("len checked above"),
            _ => LuaType::Union(LuaUnionType::from_vec(types).into()),
        }
    }

    /// 解析单个命名类型。
    /// 例如 "string" → LuaType::Ref(global("string"))
    ///     "MyClass" → 根据可见性决定 local/global
    fn resolve_single_named_type(
        &self,
        db: &SalsaSummaryDatabase,
        name: &SmolStr,
    ) -> Option<LuaType> {
        match name.as_str() {
            "nil" => return Some(LuaType::Nil),
            "any" => return Some(LuaType::Any),
            "boolean" => return Some(LuaType::Boolean),
            "string" => return Some(LuaType::String),
            "number" => return Some(LuaType::Number),
            "integer" | "int" => return Some(LuaType::Integer),
            "function" => return Some(LuaType::Function),
            "table" => return Some(LuaType::Table),
            "thread" => return Some(LuaType::Thread),
            "userdata" => return Some(LuaType::Userdata),
            _ => {}
        }

        // 跨文件查找类型定义：先查当前文件，再遍历其他文件
        let tq = crate::semantic_model::TypeQuery::new(db, self.file_id);
        let (type_def, def_file_id) = tq.get_def_with_file(name.as_str())?;

        // For alias types, resolve through value_type_offset to the underlying type
        if type_def.kind == SalsaDocTypeDefKindSummary::Alias {
            if let Some(key) = &type_def.value_type_offset {
                let file_id = self.file_id;
                if let Some(resolved) = db.doc().resolved_type_by_key(file_id, *key) {
                    match &resolved.lowered.kind {
                        SalsaDocTypeLoweredKind::Function {
                            params, returns, ..
                        } => {
                            let func_params: Vec<(String, Option<LuaType>)> = params
                                .iter()
                                .map(|p| {
                                    let name =
                                        p.name.clone().unwrap_or(SmolStr::new("")).to_string();
                                    let ty = match &p.doc_type {
                                        SalsaDocTypeRef::Node(pk) => db
                                            .doc()
                                            .lowered_type_by_key(file_id, *pk)
                                            .and_then(|n| lowered_node_to_lua_type(&n)),
                                        _ => None,
                                    };
                                    (name, ty)
                                })
                                .collect();
                            let is_colon = func_params
                                .first()
                                .map(|(n, _)| n == "self")
                                .unwrap_or(false);
                            let ret = returns
                                .first()
                                .and_then(|r| match &r.doc_type {
                                    SalsaDocTypeRef::Node(rk) => db
                                        .doc()
                                        .lowered_type_by_key(file_id, *rk)
                                        .and_then(|n| lowered_node_to_lua_type(&n)),
                                    _ => None,
                                })
                                .unwrap_or(LuaType::Nil);
                            let is_var = params.last().is_some_and(|p| p.is_dots);
                            return Some(LuaType::DocFunction(
                                LuaFunctionType::new(
                                    AsyncState::None,
                                    is_colon,
                                    is_var,
                                    func_params,
                                    ret,
                                    None,
                                )
                                .into(),
                            ));
                        }
                        _ => {
                            if let Some(ty) = lowered_node_to_lua_type(&resolved.lowered) {
                                return Some(ty);
                            }
                        }
                    }
                }
            }
        }

        let type_id = self.type_decl_id_from_visibility(name.as_str(), &type_def.visibility);

        if type_def.generic_params.is_empty() {
            return Some(LuaType::Ref(type_id));
        }

        self.resolve_generic_type(db, type_id, &type_def)
    }

    /// 通过 doc type_tags 查找 @type 注解。
    fn resolve_doc_type_for_decl(
        &self,
        db: &SalsaSummaryDatabase,
        decl_id: SalsaDeclId,
    ) -> Option<LuaType> {
        let doc = db.doc().summary(self.file_id)?;

        // 路径 A: owner binding 系统
        if let Some(resolves) = db.doc().owner_resolves_for_decl(self.file_id, decl_id) {
            for resolve in &resolves {
                let owner_offset = resolve.owner_offset;
                for tag in &doc.type_tags {
                    if tag.owner.syntax_offset == Some(owner_offset)
                        && let Some(first_key) = tag.type_offsets.first()
                    {
                        let lowered = db.doc().resolved_type_by_key(self.file_id, *first_key)?;
                        return lowered_node_to_lua_type(&lowered.lowered);
                    }
                }
            }
        }

        // 路径 B: 用 decl_tree + type_tags 做名称匹配回退
        let decl_pos: rowan::TextSize = decl_id.as_text_size();
        let decl_tree = db.file().decl_tree(self.file_id)?;
        let decl_name = decl_tree
            .decls
            .iter()
            .find(|d| d.start_offset == decl_pos)
            .map(|d| d.name.clone())?;

        for tag in &doc.type_tags {
            // type_tag 在 decl 之前
            if tag.syntax_offset >= decl_pos || tag.type_offsets.is_empty() {
                continue;
            }
            let dist = u32::from(decl_pos) - u32::from(tag.syntax_offset);
            if dist > 200 {
                continue;
            }
            // 验证 tag 的 owner 位置的 decl 名是否匹配
            if let Some(owner_off) = tag.owner.syntax_offset {
                // owner 的范围内有同名声明
                if decl_tree.decls.iter().any(|d| {
                    d.start_offset >= owner_off
                        && d.start_offset < decl_pos + rowan::TextSize::from(50u32)
                        && d.name == decl_name
                }) {
                    let first_key = &tag.type_offsets[0];
                    return db
                        .doc()
                        .resolved_type_by_key(self.file_id, *first_key)
                        .and_then(|r| lowered_node_to_lua_type(&r.lowered));
                }
            }
        }

        None
    }

    /// 通过 AST 遍历直接查找 @type 注解。
    /// 先用 lexical resolution 找到声明位置，再匹配注释→owner。
    fn resolve_type_annotation_for_name(
        &self,
        db: &SalsaSummaryDatabase,
        name: &str,
        name_expr: &LuaNameExpr,
    ) -> Option<LuaType> {
        let call_pos = name_expr.get_position();

        // 通过 lexical resolution 找到声明位点
        let lex_use = db.lexical().use_at(self.file_id, call_pos);
        let decl_pos: rowan::TextSize = {
            let lex_use = lex_use?;
            match &lex_use {
                SalsaLexicalUseSummary::Name { resolution, .. } => match resolution {
                    SalsaNameUseResolutionSummary::LocalDecl(decl_id) => decl_id.as_text_size(),
                    _ => return None,
                },
                _ => return None,
            }
        };

        // 找到声明位点对应的 LuaLocalStat
        let local = self
            .root
            .descendants::<emmylua_parser::LuaLocalStat>()
            .find(|local| {
                local.get_local_name_list().any(|n| {
                    n.get_name_token()
                        .map(|t| t.get_position() == decl_pos)
                        .unwrap_or(false)
                })
            });
        let local = local?;
        let local_pos = local.get_position();
        let local_pos = local.get_position();

        // 方式 A: get_owner() 匹配
        let mut owner_count = 0;
        for comment in self.root.descendants::<emmylua_parser::LuaComment>() {
            let owner_pos = comment.get_owner().map(|o| o.get_position());
            owner_count += 1;
            if owner_pos == Some(local_pos) {
                return Self::extract_type_from_comment(&comment, self);
            }
        }

        // 方式 B: prev_sibling 直接找（不依赖 get_owner）
        let prev = local.syntax().prev_sibling();
        if let Some(prev) = prev {
            if let Some(comment) = emmylua_parser::LuaComment::cast(prev) {
                return Self::extract_type_from_comment(&comment, self);
            }
        }

        None
    }

    /// 从注释中提取第一个 @type 注解的类型。
    fn extract_type_from_comment(
        comment: &emmylua_parser::LuaComment,
        infer: &InferQuery,
    ) -> Option<LuaType> {
        use crate::compilation::SalsaDocTypeNodeKey;
        use emmylua_parser::LuaDocTagType;
        for tag in comment.descendants::<LuaDocTagType>() {
            for doc_type in tag.get_type_list() {
                let key = SalsaDocTypeNodeKey::from(doc_type.clone());
                let db = infer.read_db();
                if let Some(resolved) = db.doc().resolved_type_by_key(infer.get_file_id(), key) {
                    return lowered_node_to_lua_type(&resolved.lowered);
                }
            }
        }
        None
    }

    /// 通过 decl_tree + doc type_tags 按名称查找 @type 注解
    fn resolve_doc_type_by_name(&self, db: &SalsaSummaryDatabase, name: &str) -> Option<LuaType> {
        let decl_tree = db.file().decl_tree(self.file_id)?;
        let decl = decl_tree.decls.iter().find(|d| d.name.as_str() == name)?;

        // 路径 A: 通过 owner_resolves 精确匹配
        if let Some(ty) = self.resolve_doc_type_for_decl(db, decl.id) {
            return Some(ty);
        }

        // 路径 B: 扫描 type_tags，找 decl 之前最近的 @type 注解
        let doc = db.doc().summary(self.file_id)?;
        let mut best_key: Option<&crate::compilation::SalsaDocTypeNodeKey> = None;
        let mut best_dist: usize = usize::MAX;
        for tag in &doc.type_tags {
            if tag.syntax_offset < decl.start_offset {
                let dist: u32 = (decl.start_offset - tag.syntax_offset).into();
                let d = dist as usize;
                if d < best_dist {
                    if let Some(key) = tag.type_offsets.first() {
                        best_dist = d;
                        best_key = Some(key);
                    }
                }
            }
        }
        if let Some(key) = best_key {
            let lowered = db.doc().resolved_type_by_key(self.file_id, *key)?;
            return lowered_node_to_lua_type(&lowered.lowered);
        }
        None
    }

    fn type_decl_id_from_visibility(
        &self,
        name: &str,
        visibility: &SalsaDocVisibilityKindSummary,
    ) -> LuaTypeDeclId {
        match visibility {
            SalsaDocVisibilityKindSummary::Private => LuaTypeDeclId::local(self.file_id, name),
            _ => LuaTypeDeclId::global(name),
        }
    }

    fn resolve_generic_type(
        &self,
        _db: &SalsaSummaryDatabase,
        type_id: LuaTypeDeclId,
        type_def: &SalsaDocTypeDefSummary,
    ) -> Option<LuaType> {
        let has_all_defaults = type_def
            .generic_params
            .iter()
            .all(|p| p.default_type_offset.is_some());

        if !has_all_defaults {
            return Some(LuaType::Ref(type_id));
        }

        Some(LuaType::Ref(type_id))
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // AST 慢速路径 — 表达式分发
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    fn infer_expr_ast(&self, expr: LuaExpr) -> InferResult {
        match expr {
            LuaExpr::LiteralExpr(literal) => self.infer_literal(literal),
            LuaExpr::ClosureExpr(closure) => self.infer_closure(closure),
            LuaExpr::NameExpr(name) => self.infer_name(name),
            LuaExpr::ParenExpr(paren) => {
                let inner = paren.get_expr().ok_or(InferFailReason::None)?;
                self.infer_expr(inner)
            }
            LuaExpr::CallExpr(call) => infer_call_expr(self, call),
            LuaExpr::IndexExpr(index) => infer_index_expr(self, index),
            LuaExpr::TableExpr(table) => infer_table_expr(self, table),
            LuaExpr::BinaryExpr(binary) => binary::infer_binary_expr(self, binary),
            LuaExpr::UnaryExpr(unary) => unary::infer_unary_expr(self, unary),
            LuaExpr::TernaryExpr(ternary) => ternary::infer_ternary_expr(self, ternary),
        }
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 字面量推断
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    fn infer_literal(&self, expr: LuaLiteralExpr) -> InferResult {
        match expr.get_literal().ok_or(InferFailReason::None)? {
            LuaLiteralToken::Nil(_) => Ok(LuaType::Nil),
            LuaLiteralToken::Bool(b) => Ok(LuaType::BooleanConst(b.is_true())),
            LuaLiteralToken::Number(num) => match num.get_number_value() {
                NumberResult::Int(i) => Ok(LuaType::IntegerConst(i)),
                NumberResult::Float(f) => Ok(LuaType::FloatConst(f)),
                _ => Ok(LuaType::Number),
            },
            LuaLiteralToken::String(s) => {
                Ok(LuaType::StringConst(SmolStr::new(s.get_value()).into()))
            }
            LuaLiteralToken::Dots(_) => {
                Ok(LuaType::Variadic(VariadicType::Base(LuaType::Any).into()))
            }
            _ => Ok(LuaType::Any),
        }
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 闭包推断
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    fn infer_closure(&self, closure: LuaClosureExpr) -> InferResult {
        let sig_id = LuaSignatureId::from_closure(self.file_id, &closure);
        Ok(LuaType::Signature(sig_id))
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 名称推断
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    /// 推断名称表达式的类型。
    fn infer_name(&self, name_expr: LuaNameExpr) -> InferResult {
        let name_token = name_expr.get_name_token().ok_or(InferFailReason::None)?;
        let name = name_token.get_name_text();

        match name {
            "self" => return self.infer_self(&name_expr),
            "_G" => return Ok(LuaType::Global),
            _ => {}
        }

        let db = self.read_db();

        // 路径 0：检查 closure 是否在 decl 范围内（如 local function / local f = function()）
        if let Some(decl_tree) = db.file().decl_tree(self.file_id) {
            if let Some(decl) = decl_tree.decls.iter().find(|d| d.name.as_str() == name) {
                for closure in self.root.descendants::<LuaClosureExpr>() {
                    let pos = closure.get_position();
                    if pos >= decl.start_offset && pos < decl.end_offset {
                        return Ok(LuaType::Signature(LuaSignatureId::from_closure(
                            self.file_id,
                            &closure,
                        )));
                    }
                }
            }
        }

        // 路径 1：通过 salsa types 查询名称的类型信息
        let mut resolved_via_decl = false;
        if let Some(name_info) = db.types().name(self.file_id, name_expr.get_position()) {
            if let Some(ref decl_type) = name_info.decl_type {
                if let Some(ty) = self.resolve_decl_type(&db, decl_type.clone()) {
                    return Ok(ty);
                }
            }
            if let SalsaNameUseResolutionSummary::LocalDecl(decl_id) =
                &name_info.name_use.resolution
            {
                if let Some(dt) = db.types().decl(self.file_id, *decl_id) {
                    if let Some(ty) = self.resolve_decl_type(&db, dt) {
                        return Ok(ty);
                    }
                }
                if let Some(ty) = self.resolve_doc_type_for_decl(&db, *decl_id) {
                    return Ok(ty);
                }
                resolved_via_decl = true;
            }
        }

        // 路径 1b：通过 lexical name use 查找 @type 注解（处理 types().name() 返回空或 resolution 非 LocalDecl 的情况）
        if !resolved_via_decl {
            if let Some(lex_use) = db.lexical().use_at(self.file_id, name_expr.get_position()) {
                if let SalsaLexicalUseSummary::Name { resolution, .. } = &lex_use {
                    if let SalsaNameUseResolutionSummary::LocalDecl(decl_id) = resolution {
                        if let Some(dt) = db.types().decl(self.file_id, *decl_id) {
                            if let Some(ty) = self.resolve_decl_type(&db, dt) {
                                return Ok(ty);
                            }
                        }
                        if let Some(ty) = self.resolve_doc_type_for_decl(&db, *decl_id) {
                            return Ok(ty);
                        }
                    }
                }
            }
        }

        // 路径 1.5：通过 decl_tree 查找，如果有 closure 值表达式 → 从 AST 创建 Signature
        if let Some(decl_tree) = db.file().decl_tree(self.file_id) {
            if let Some(decl) = decl_tree.decls.iter().find(|d| d.name.as_str() == name) {
                if let Some(val_expr) = &decl.value_expr_syntax_id {
                    // Function-related decls (FuncStat, LocalFuncStat) contain a closure
                    let is_func = matches!(
                        val_expr.kind,
                        LuaSyntaxKind::ClosureExpr
                            | LuaSyntaxKind::FuncStat
                            | LuaSyntaxKind::LocalFuncStat
                    );
                    if is_func {
                        // Find the innermost closure near the value expression
                        for closure in self.root.descendants::<LuaClosureExpr>() {
                            let pos = closure.get_position();
                            if pos >= val_expr.start_offset && pos < val_expr.end_offset {
                                let sig_id = LuaSignatureId::from_closure(self.file_id, &closure);
                                return Ok(LuaType::Signature(sig_id));
                            }
                        }
                    }
                }
            }
        }

        // 路径 1.6：通过 decl_tree + doc type_tags 查找 @type 注解
        if let Some(ty) = self.resolve_type_annotation_for_name(&db, name, &name_expr) {
            return Ok(ty);
        }

        // 路径 2：通过 decl_tree + doc type_tags 查找
        if let Some(ty) = self.resolve_doc_type_by_name(&db, name) {
            return Ok(ty);
        }

        // 路径 3：全局名称查找
        self.infer_global_name(&db, name)
    }

    /// 尝试作为全局名称推断类型。
    ///
    /// 优先级：
    /// 1. 全局类型查询返回了带 annotation 的结果 → 使用该类型
    /// 2. 全局函数定义 → 函数类型优先
    /// 3. 全局变量定义且有命名类型 → 使用该类型
    /// 4. 完全无法推断 → Any
    fn infer_global_name(&self, db: &SalsaSummaryDatabase, name: &str) -> InferResult {
        // 1. salsa 全局类型索引
        if let Some(global_info) = db.types().global(self.file_id, name) {
            if let Some(candidate) = global_info.candidates.first() {
                if !candidate.named_type_names.is_empty() {
                    return Ok(self.resolve_named_types(db, &candidate.named_type_names));
                }
            }
        }

        // 2. 全局函数定义 — 函数类型优先
        if let Some(global_fn) = db.module().exported_global_function(self.file_id) {
            if global_fn.name == name {
                return Ok(LuaType::Signature(LuaSignatureId::from_position(
                    self.file_id,
                    global_fn.signature_offset,
                )));
            }
        }

        // 3. 全局变量 — 有命名类型的定义
        if let Some(global_var) = db.module().exported_global_variable(self.file_id) {
            if global_var.name == name {
                if let Some(decl_type) = db.types().decl(self.file_id, global_var.decl_id) {
                    if !decl_type.named_type_names.is_empty() {
                        return Ok(self.resolve_named_types(db, &decl_type.named_type_names));
                    }
                }
                return Ok(LuaType::Any);
            }
        }

        // 4. 无任何定义 → Any
        Ok(LuaType::Any)
    }

    /// 推断 self 的类型。
    fn infer_self(&self, name_expr: &LuaNameExpr) -> InferResult {
        let db = self.read_db();

        if let Some(name_info) = db.types().name(self.file_id, name_expr.get_position()) {
            if let Some(decl_type) = name_info.decl_type {
                if let Some(ty) = self.resolve_decl_type(&db, decl_type) {
                    return Ok(ty);
                }
            }

            if let SalsaNameUseResolutionSummary::LocalDecl(decl_id) = name_info.name_use.resolution
            {
                if let Some(decl_info) = db.types().decl(self.file_id, decl_id) {
                    if !decl_info.named_type_names.is_empty() {
                        return Ok(self.resolve_named_types(&db, &decl_info.named_type_names));
                    }
                }
            }
        }

        Err(InferFailReason::NotImplemented)
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 类型降级工具
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub(super) fn lowered_node_to_lua_type(node: &SalsaDocTypeLoweredNode) -> Option<LuaType> {
    match &node.kind {
        SalsaDocTypeLoweredKind::Unknown => Some(LuaType::Any),
        SalsaDocTypeLoweredKind::Name { name } => match name.as_str() {
            "any" | "unknown" => Some(LuaType::Any),
            "nil" => Some(LuaType::Nil),
            "false" => Some(LuaType::BooleanConst(false)),
            "true" => Some(LuaType::BooleanConst(true)),
            "boolean" | "bool" => Some(LuaType::Boolean),
            "string" => Some(LuaType::String),
            "number" => Some(LuaType::Number),
            "integer" | "int" => Some(LuaType::Integer),
            "function" => Some(LuaType::Function),
            "table" => Some(LuaType::Table),
            "thread" => Some(LuaType::Thread),
            "userdata" => Some(LuaType::Userdata),
            _ => Some(LuaType::Ref(LuaTypeDeclId::global(name))),
        },
        SalsaDocTypeLoweredKind::Array { item_type: _ } => Some(LuaType::Array(
            LuaArrayType::new(LuaType::Unknown, LuaArrayLen::None).into(),
        )),
        SalsaDocTypeLoweredKind::Variadic { item_type: _ } => {
            Some(LuaType::Variadic(VariadicType::Base(LuaType::Any).into()))
        }
        SalsaDocTypeLoweredKind::Literal { text } => {
            let s = text.as_str();
            match s {
                "nil" => Some(LuaType::Nil),
                "true" => Some(LuaType::BooleanConst(true)),
                "false" => Some(LuaType::BooleanConst(false)),
                _ => {
                    // Try integer / float / string
                    if let Ok(n) = s.parse::<i64>() {
                        Some(LuaType::IntegerConst(n))
                    } else if let Ok(f) = s.parse::<f64>() {
                        Some(LuaType::FloatConst(f))
                    } else {
                        Some(LuaType::DocStringConst(SmolStr::new(s).into()))
                    }
                }
            }
        }
        _ => None,
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 函数调用推断
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 解析后的函数信息 — salsa-native 替代 `Arc<LuaFunctionType>`。
#[derive(Debug, Clone)]
pub struct CallFunctionInfo {
    pub params: Vec<(String, Option<LuaType>)>,
    pub is_colon_define: bool,
    pub is_variadic: bool,
    pub return_type: LuaType,
    pub is_async: bool,
}

impl InferQuery<'_> {
    /// 推断调用表达式的目标函数信息。
    /// 纯 salsa 实现，不依赖旧 DbIndex。
    pub fn infer_call_expr_func(
        &self,
        call_expr: emmylua_parser::LuaCallExpr,
        _arg_count: Option<usize>,
    ) -> Option<CallFunctionInfo> {
        let prefix = call_expr.get_prefix_expr()?;
        let prefix_type = self.infer_expr(prefix).ok()?;
        resolve_call_info(self, &prefix_type)
    }
}

fn resolve_call_info(infer: &InferQuery, ty: &LuaType) -> Option<CallFunctionInfo> {
    match ty {
        LuaType::DocFunction(func) => Some(CallFunctionInfo {
            params: func.get_params().to_vec(),
            is_colon_define: func.is_colon_define(),
            is_variadic: func.get_params().last().is_some_and(|(n, _)| n == "..."),
            return_type: func.get_ret().clone(),
            is_async: matches!(func.get_async_state(), AsyncState::Async),
        }),
        LuaType::Signature(sig_id) => {
            let db = infer.read_db();
            let explain = db
                .doc()
                .signature()
                .explain(infer.get_file_id(), sig_id.get_position())?;
            let params = explain
                .params
                .iter()
                .map(|p| {
                    let ty = p
                        .doc_type
                        .as_ref()
                        .and_then(|dt| lowered_node_to_lua_type(dt.lowered.as_ref()?));
                    (p.name.to_string(), ty)
                })
                .collect();
            let return_type = explain
                .returns
                .first()
                .and_then(|r| r.items.first())
                .and_then(|item| lowered_node_to_lua_type(item.doc_type.lowered.as_ref()?))
                .unwrap_or(LuaType::Unknown);
            let is_colon = explain.signature.is_method;
            let is_vararg = explain.signature.params.iter().any(|p| p.is_vararg);
            let is_async = explain.tag_properties.iter().any(|tag| {
                tag.entries.iter().any(|e| {
                    matches!(
                        e,
                        crate::compilation::SalsaDocTagPropertyEntrySummary::Async
                    )
                })
            });
            Some(CallFunctionInfo {
                params,
                is_colon_define: is_colon,
                is_variadic: is_vararg,
                return_type,
                is_async,
            })
        }
        LuaType::Union(u) => {
            for m in u.into_vec() {
                if let Some(info) = resolve_call_info(infer, &m) {
                    return Some(info);
                }
            }
            None
        }
        LuaType::Generic(g) => resolve_call_info(infer, &g.get_base_type()),
        // Alias types: resolve Ref/Def through type_def → value_type_offset
        LuaType::Ref(id) | LuaType::Def(id) => {
            let db = infer.read_db();
            let file_id = infer.get_file_id();
            let type_def = db.doc().type_def_by_name(file_id, id.get_name())?;
            if type_def.kind == SalsaDocTypeDefKindSummary::Alias {
                if let Some(key) = &type_def.value_type_offset {
                    if let Some(resolved) = db.doc().resolved_type_by_key(file_id, *key) {
                        // Handle Function lowered kind directly — build CallFunctionInfo
                        if let SalsaDocTypeLoweredKind::Function {
                            params, returns, ..
                        } = &resolved.lowered.kind
                        {
                            let func_params: Vec<(String, Option<LuaType>)> = params
                                .iter()
                                .map(|p| {
                                    let name =
                                        p.name.clone().unwrap_or(SmolStr::new("")).to_string();
                                    let ty = match &p.doc_type {
                                        SalsaDocTypeRef::Node(pk) => db
                                            .doc()
                                            .lowered_type_by_key(file_id, *pk)
                                            .and_then(|n| lowered_node_to_lua_type(&n)),
                                        _ => None,
                                    };
                                    (name, ty)
                                })
                                .collect();
                            let is_colon = func_params
                                .first()
                                .map(|(n, _)| n == "self")
                                .unwrap_or(false);
                            let ret = returns
                                .first()
                                .and_then(|r| match &r.doc_type {
                                    SalsaDocTypeRef::Node(rk) => db
                                        .doc()
                                        .lowered_type_by_key(file_id, *rk)
                                        .and_then(|n| lowered_node_to_lua_type(&n)),
                                    _ => None,
                                })
                                .unwrap_or(LuaType::Nil);
                            let is_var = params.last().is_some_and(|p| p.is_dots);
                            return Some(CallFunctionInfo {
                                params: func_params,
                                is_colon_define: is_colon,
                                is_variadic: is_var,
                                return_type: ret,
                                is_async: false,
                            });
                        }
                        // For non-Function types, recurse
                        if let Some(ty) = lowered_node_to_lua_type(&resolved.lowered) {
                            return resolve_call_info(infer, &ty);
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}
