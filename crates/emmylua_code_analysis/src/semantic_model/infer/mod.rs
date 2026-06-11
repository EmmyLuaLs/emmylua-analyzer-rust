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
mod unary;

use std::cell::RefCell;
use std::sync::{Arc, RwLock};

use emmylua_parser::{
    LuaAstNode, LuaClosureExpr, LuaExpr, LuaLiteralExpr, LuaLiteralToken, LuaNameExpr, NumberResult,
};
use rowan::TextRange;
use smol_str::SmolStr;

use crate::compilation::{
    SalsaDeclTypeInfoSummary, SalsaDocTypeDefSummary, SalsaDocVisibilityKindSummary,
    SalsaNameUseResolutionSummary, SalsaSummaryDatabase,
};
use crate::{
    FileId, LuaDeclId, LuaMemberKey, LuaSignatureId, LuaType, LuaTypeDeclId, LuaUnionType,
    TypeCheckFailReason, VariadicType,
};

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
pub struct InferQuery<'a> {
    db: Arc<RwLock<SalsaSummaryDatabase>>,
    file_id: FileId,
    emmyrc: Arc<crate::Emmyrc>,
    cache: &'a RefCell<InferCache>,
}

impl<'a> InferQuery<'a> {
    pub(crate) fn with_cache(
        db: Arc<RwLock<SalsaSummaryDatabase>>,
        file_id: FileId,
        emmyrc: Arc<crate::Emmyrc>,
        cache: &'a RefCell<InferCache>,
    ) -> Self {
        Self { db, file_id, emmyrc, cache }
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub(super) fn read_db(&self) -> impl std::ops::Deref<Target = SalsaSummaryDatabase> + '_ {
        self.db.read().unwrap_or_else(|e| e.into_inner())
    }

    /// 类型检查快捷方法。
    pub(super) fn check_type_compact(
        &self,
        source: &LuaType,
        compact: &LuaType,
    ) -> Result<(), crate::semantic_model::type_check::TypeCheckFailReason> {
        crate::semantic_model::type_check::check_type_compact(
            self.emmyrc.clone(),
            source,
            compact,
        )
    }

    /// 推断成员类型。给前缀类型和 key，返回成员类型。
    pub fn infer_member_type(&self, prefix_type: &LuaType, member_key: &LuaMemberKey) -> InferResult {
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

    /// 推断值绑定的目标类型（右值 → 左值类型推断）。
    ///
    /// 例如 `local x: number = expr` 中，从 `expr` 推断 `x` 的类型为 `number`。
    /// 当前仅实现赋值语句场景，其余返回 `None`。
    pub fn infer_bind_value_type(&self, _expr: LuaExpr) -> Option<LuaType> {
        // TODO: 完整实现需要 parent node 检查
        // - LuaAssignStat: 找到对应的 var，推断 var 的类型
        // - LuaTableField: 找到包含的表，推断字段类型
        // - LuaCallArgList: 找到调用的函数，推断参数类型
        None
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 主入口
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

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
            return cached;
        }

        // 2. 尝试 salsa 快速路径
        if let Some(ty) = self.lookup_salsa_type(&expr) {
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
                self.cache.borrow_mut().insert(syntax_id, LuaType::Nil);
                return Ok(LuaType::Nil);
            }
            _ => {
                // 需要 resolve 的错误不缓存，下次可能成功
            }
        }

        result
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // Salsa 快速路径
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    /// 尝试通过 salsa 类型索引直接获取表达式类型。
    /// 当前只能处理名称表达式（NameExpr）。
    fn lookup_salsa_type(&self, expr: &LuaExpr) -> Option<LuaType> {
        if let LuaExpr::NameExpr(name) = expr {
            let db = self.read_db();
            let name_info = db.types().name(self.file_id, name.get_position())?;
            let decl_type = name_info.decl_type?;
            return self.resolve_decl_type(&db, decl_type);
        }
        None
    }

    /// 将 SalsaDeclTypeInfoSummary 转换为 LuaType。
    /// 优先使用 named_type_names（简单、不需要 VFS），
    /// 其次尝试 explicit_type_offsets（需要 doc type 展开）。
    fn resolve_decl_type(
        &self,
        db: &SalsaSummaryDatabase,
        decl_type: SalsaDeclTypeInfoSummary,
    ) -> Option<LuaType> {
        // 路径 1：命名类型（如 `: string`, `: MyClass`）
        if !decl_type.named_type_names.is_empty() {
            return Some(self.resolve_named_types(db, &decl_type.named_type_names));
        }

        // 路径 2：显式类型偏移（如 `: string | number` 等复合类型）
        // 需要 VFS + doc type 展开，后续 phase 实现
        if !decl_type.explicit_type_offsets.is_empty() {
            // 留空：需要 infer_compilation_doc_type_keys → VFS
        }

        // 路径 3：从初始化表达式推断
        if decl_type.value_expr_syntax_id.is_some() {
            // 留空：需要递归推断 value expression
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

        let type_def = db.doc().type_def_by_name(self.file_id, name.as_str())?;
        let type_id = self.type_decl_id_from_visibility(name.as_str(), &type_def.visibility);

        if type_def.generic_params.is_empty() {
            return Some(LuaType::Ref(type_id));
        }

        self.resolve_generic_type(db, type_id, &type_def)
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
            LuaLiteralToken::Dots(_) => Ok(LuaType::Variadic(
                VariadicType::Base(LuaType::Any).into(),
            )),
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

        // 路径 1：通过 salsa types 查询名称的类型信息
        if let Some(name_info) = db.types().name(self.file_id, name_expr.get_position()) {
            if let Some(decl_type) = name_info.decl_type {
                if let Some(ty) = self.resolve_decl_type(&db, decl_type) {
                    return Ok(ty);
                }
            }
        }

        // 路径 2：全局名称查找
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

            if let SalsaNameUseResolutionSummary::LocalDecl(decl_id) =
                name_info.name_use.resolution
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
