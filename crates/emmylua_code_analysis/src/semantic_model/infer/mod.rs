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

mod cache;

use std::cell::RefCell;
use std::sync::{Arc, RwLock};

use emmylua_parser::{
    LuaAstNode, LuaClosureExpr, LuaExpr, LuaLiteralExpr, LuaLiteralToken, LuaNameExpr, NumberResult,
};
use smol_str::SmolStr;

use crate::compilation::{
    SalsaDeclTypeInfoSummary, SalsaDocTypeDefSummary, SalsaDocVisibilityKindSummary,
    SalsaNameUseResolutionSummary, SalsaSummaryDatabase,
};
use crate::{
    FileId, LuaDeclId, LuaSignatureId, LuaType, LuaTypeDeclId, LuaUnionType, VariadicType,
};

pub use cache::InferCache;

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
/// - 持有 `SalsaSummaryDatabase` 引用用于快速路径
/// - 本地 `InferCache` 用于单次会话 memoization
/// - `file_id` 指明当前分析的文件
pub struct InferQuery<'db> {
    db: &'db Arc<RwLock<SalsaSummaryDatabase>>,
    file_id: FileId,
    cache: RefCell<InferCache>,
}

impl<'db> InferQuery<'db> {
    pub(crate) fn new(db: &'db Arc<RwLock<SalsaSummaryDatabase>>, file_id: FileId) -> Self {
        Self {
            db,
            file_id,
            cache: RefCell::new(InferCache::new(file_id)),
        }
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
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
            let db = self.db.read().unwrap_or_else(|e| e.into_inner());
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
            // 后续 phase 中实现
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
            1 => types.pop().expect("unreachable"),
            _ => {
                // 多个命名类型 → union
                LuaType::Union(LuaUnionType::from_vec(types).into())
            }
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
        // 内建基础类型
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

        // 查 type_def 获取可见性和泛型参数
        let type_def = db.doc().type_def_by_name(self.file_id, name.as_str())?;

        let type_id = self.type_decl_id_from_visibility(name.as_str(), &type_def.visibility);

        if type_def.generic_params.is_empty() {
            // 无泛型参数 → 直接引用
            return Some(LuaType::Ref(type_id));
        }

        // 有泛型参数 → 尝试用默认值填充
        self.resolve_generic_type(db, type_id, &type_def)
    }

    /// 根据可见性构造 LuaTypeDeclId。
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

    /// 解析带泛型参数的类型。
    /// 如果所有泛型参数都有默认值，构造 LuaType::Ref(type_id)；
    /// 否则返回 LuaType::Any（无法确定具体类型）。
    fn resolve_generic_type(
        &self,
        db: &SalsaSummaryDatabase,
        type_id: LuaTypeDeclId,
        type_def: &SalsaDocTypeDefSummary,
    ) -> Option<LuaType> {
        // 检查所有泛型参数是否有默认类型
        let has_all_defaults = type_def
            .generic_params
            .iter()
            .all(|p| p.default_type_offset.is_some());

        if !has_all_defaults {
            // 无法填充泛型 → 返回基础引用，泛型参数留待实例化时确定
            return Some(LuaType::Ref(type_id));
        }

        // 所有参数都有默认值 → 可以直接用 Ref
        // 完整的泛型实例化需要在 doc type 系统中展开默认值
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
            LuaExpr::CallExpr(_call) => {
                // Phase 3+: 函数调用推断
                Err(InferFailReason::NotImplemented)
            }
            LuaExpr::IndexExpr(_index) => {
                // Phase 3+: 索引表达式推断
                Err(InferFailReason::NotImplemented)
            }
            LuaExpr::TableExpr(_table) => {
                // Phase 3+: 表推断
                Err(InferFailReason::NotImplemented)
            }
            LuaExpr::BinaryExpr(_binary) => {
                // Phase 3+: 二元运算推断
                Err(InferFailReason::NotImplemented)
            }
            LuaExpr::UnaryExpr(_unary) => {
                // Phase 3+: 一元运算推断
                Err(InferFailReason::NotImplemented)
            }
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
    ///
    /// 完整链路（salsa-first）：
    /// 1. 特殊名称处理（self → 方法上下文，_G → Global）
    /// 2. Salsa types::name() → SalsaNameTypeInfoSummary
    /// 3. 提取 decl_type → named_type_names → LuaType
    /// 4. 全局查找 → SalsaTypeQueries::global()
    fn infer_name(&self, name_expr: LuaNameExpr) -> InferResult {
        let name_token = name_expr.get_name_token().ok_or(InferFailReason::None)?;
        let name = name_token.get_name_text();

        // 特殊名称
        match name {
            "self" => return self.infer_self(&name_expr),
            "_G" => return Ok(LuaType::Global),
            _ => {}
        }

        let syntax_offset = name_expr.get_position();
        let db = self.db.read().unwrap_or_else(|e| e.into_inner());

        // 路径 1：通过 salsa types 查询名称的类型信息
        if let Some(name_info) = db.types().name(self.file_id, syntax_offset) {
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
    /// 全局变量/函数可以在多个文件中定义，优先级如下：
    /// 1. 如果全局类型查询返回了带 annotation 的结果 → 使用该类型
    /// 2. 如果存在全局函数定义 → 函数类型优先
    /// 3. 如果存在全局变量定义且有命名类型 → 使用该类型
    /// 4. 如果只有一处定义 → 返回该类型
    /// 5. 完全无法推断 → Any
    fn infer_global_name(&self, db: &SalsaSummaryDatabase, name: &str) -> InferResult {
        // 1. 查询 salsa 全局类型索引（聚合了跨文件信息）
        if let Some(global_info) = db.types().global(self.file_id, name) {
            if let Some(candidate) = global_info.candidates.first() {
                // 有 annotation 的类型优先
                if !candidate.named_type_names.is_empty() {
                    return Ok(self.resolve_named_types(db, &candidate.named_type_names));
                }
                // 有显式类型偏移 → 也是 annotation
                if !candidate.explicit_type_offsets.is_empty() {
                    // 需要 VFS + doc type 展开，后续 phase
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
                // 全局变量存在但无法推断具体类型 → Any
                return Ok(LuaType::Any);
            }
        }

        // 4. 无任何定义 → Any
        Ok(LuaType::Any)
    }

    /// 推断 self 的类型。
    ///
    /// self 出现在方法定义中，其类型是包含该方法的类/表。
    /// 通过向上查找包含 self 声明的函数签名来确定。
    fn infer_self(&self, name_expr: &LuaNameExpr) -> InferResult {
        let db = self.db.read().unwrap_or_else(|e| e.into_inner());

        // 查询 self 名称的类型信息
        if let Some(name_info) = db.types().name(self.file_id, name_expr.get_position()) {
            if let Some(decl_type) = name_info.decl_type {
                if let Some(ty) = self.resolve_decl_type(&db, decl_type) {
                    return Ok(ty);
                }
            }

            // 如果 decl_type 没有直接类型，self 可能是隐式参数
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
