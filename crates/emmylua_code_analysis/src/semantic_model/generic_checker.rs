//! GenericChecker — 基于 salsa 的泛型约束检查器
//!
//! 支持：
//! - 类型形参占位符（rigid placeholder）：`U extends T = "x"` 模式
//! - 参数→泛型映射：`@param value U` → arg 映射到 generic "U"
//! - 三条调用路径：call explain → prefix signature → name lookup

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaNameExpr};

use crate::compilation::{
    SalsaCallExplainSummary, SalsaDocTypeDefSummary, SalsaDocTypeLoweredKind,
    SalsaSignatureExplainSummary,
};
use crate::semantic_model::SigQuery;
use crate::semantic_model::infer::{self, InferQuery};
use crate::{
    FileId, LuaType, SalsaDocOwnerKindSummary, SalsaDocTypeLoweredNode, SalsaMemberRootSummary,
    SalsaSummaryDatabase,
};

#[derive(Debug, Clone)]
struct GenericBinding {
    name: String,
    constraint: Option<LuaType>,
    /// 约束是否为 rigid（引用另一个泛型参数，如 `U extends T`）
    is_rigid: bool,
    /// 默认值是否本身也是泛型引用（如 `T` 在 `U extends T = T`）
    default_is_infer: bool,
    default: Option<LuaType>,
    actual: Option<LuaType>,
}

#[derive(Debug, Clone)]
pub struct ConstraintViolation {
    pub param_name: String,
    pub constraint: LuaType,
    pub actual: LuaType,
}

pub struct GenericChecker {
    bindings: Vec<GenericBinding>,
}

impl GenericChecker {
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 构造器
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    pub fn from_call(
        call_expr: &LuaCallExpr,
        sig_query: &SigQuery,
        infer: &InferQuery,
        resolve_type_name: &impl Fn(&str) -> Option<LuaType>,
    ) -> Option<Self> {
        // 路径 1: salsa call explain
        if let Some(call) = sig_query.call_explain(call_expr.get_position()) {
            if let Some(sig) = call.resolved_signature.as_ref() {
                if !sig.generics.is_empty() {
                    return Self::build_from_sig(sig).map(|c| {
                        c.fill_call_args(call_expr, &call, sig, infer, resolve_type_name)
                    });
                }
                // 回退: 签名没有泛型，尝试从 class 级别获取
                // (如 M.new(a) 的泛型 T 来自 @class GenericTest<T>)
                if let Some(checker) =
                    Self::from_call_class_generics(&call, call_expr, sig, infer, resolve_type_name)
                {
                    return Some(checker);
                }
            }
        }

        // 路径 2: 从前缀类型推断签名（方法调用等）
        if let Some(sig) = Self::get_prefix_signature(call_expr, infer, sig_query) {
            return Self::build_from_sig(&sig)
                .map(|c| c.fill_from_params(call_expr, &sig, infer, resolve_type_name));
        }

        // 路径 3: 直接查找被调用函数的名称声明获取 @generic 信息
        if let Some(checker) =
            Self::from_call_name_lookup(call_expr, sig_query, infer, resolve_type_name)
        {
            return Some(checker);
        }

        None
    }

    /// 路径 3: 从调用表达式中的函数名直接查找签名
    fn from_call_name_lookup(
        call_expr: &LuaCallExpr,
        sig_query: &SigQuery,
        infer: &InferQuery,
        resolve_type_name: &impl Fn(&str) -> Option<LuaType>,
    ) -> Option<Self> {
        let prefix = call_expr.get_prefix_expr()?;

        // 处理简单的 NameExpr: f(...) → 查找 f 的签名
        if let Some(name_expr) = LuaNameExpr::cast(prefix.syntax().clone()) {
            let file_id = infer.get_file_id();
            let db = infer.read_db();

            // 通过 salsa 名称类型系统查找声明
            let name_info = db.types().name(file_id, name_expr.get_position())?;
            if let Some(dt) = &name_info.decl_type {
                // 从 @generic doc tag 获取泛型
                if let Some(offset) = dt.value_signature_offset {
                    if let Some(sig) = sig_query.explain(file_id, offset)
                        && !sig.generics.is_empty()
                    {
                        return Self::build_from_sig(&sig).map(|c| {
                            c.fill_from_params(call_expr, &sig, infer, resolve_type_name)
                        });
                    }
                }
            }
        }

        // 处理 IndexExpr: M.new(…) → 推断前缀类型
        if let Ok(prefix_type) = infer.infer_expr(prefix.clone()) {
            match &prefix_type {
                LuaType::Signature(sid) => {
                    let file_id = infer.get_file_id();
                    if let Some(sig) = sig_query.explain(file_id, sid.get_position())
                        && !sig.generics.is_empty()
                    {
                        return Self::build_from_sig(&sig).map(|c| {
                            c.fill_from_params(call_expr, &sig, infer, resolve_type_name)
                        });
                    }
                }
                LuaType::DocFunction(func) => {
                    if let LuaType::Signature(sid) = func.get_ret() {
                        let file_id = infer.get_file_id();
                        if let Some(sig) = sig_query.explain(file_id, sid.get_position())
                            && !sig.generics.is_empty()
                        {
                            return Self::build_from_sig(&sig).map(|c| {
                                c.fill_from_params(call_expr, &sig, infer, resolve_type_name)
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        None
    }

    /// 当签名自身的 generics 为空时，从 callee_member 的 class 级别获取泛型。
    fn from_call_class_generics(
        call: &SalsaCallExplainSummary,
        call_expr: &LuaCallExpr,
        sig: &SalsaSignatureExplainSummary,
        infer: &InferQuery,
        resolve_type_name: &impl Fn(&str) -> Option<LuaType>,
    ) -> Option<Self> {
        let lexical = call.lexical_call.as_ref()?;
        let callee_member = lexical.callee_member.as_ref()?;
        let target = callee_member.as_summary();

        // member root 指向局部变量名（如 "M"），需要通过类型注解找到 class
        let var_name = match &target.root {
            SalsaMemberRootSummary::LocalDecl { name, decl_id } => {
                let file_id = infer.get_file_id();
                let db = infer.read_db();

                // 方式 1: 通过 salsa name type 查询
                if let Some(name_info) = db.types().name(file_id, decl_id.as_text_size()) {
                    if let Some(dt) = &name_info.decl_type {
                        if !dt.named_type_names.is_empty() {
                            return Self::build_class_checker(
                                &dt.named_type_names[0],
                                db,
                                file_id,
                                call_expr,
                                sig,
                                infer,
                                resolve_type_name,
                            );
                        }
                    }
                }

                // 方式 2: 通过 doc type_tags 查找 @class 注解
                if let Some(doc) = db.doc().summary(file_id) {
                    for type_tag in &doc.type_tags {
                        if type_tag.owner.syntax_offset == Some(decl_id.as_text_size())
                            && !type_tag.type_offsets.is_empty()
                        {
                            if let Some(resolved) = db
                                .doc()
                                .resolved_type_by_key(file_id, type_tag.type_offsets[0])
                            {
                                if let SalsaDocTypeLoweredKind::Name { name: type_name } =
                                    &resolved.lowered.kind
                                {
                                    return Self::build_class_checker(
                                        type_name,
                                        db,
                                        file_id,
                                        call_expr,
                                        sig,
                                        infer,
                                        resolve_type_name,
                                    );
                                }
                            }
                        }
                    }
                }

                name.to_string()
            }
            _ => return None,
        };

        // fallback: 直接以 var_name 构建
        let file_id = infer.get_file_id();
        let db = infer.read_db();
        Self::build_class_checker(
            &var_name,
            db,
            file_id,
            call_expr,
            sig,
            infer,
            resolve_type_name,
        )
    }

    fn build_class_checker(
        class_name: &str,
        db: impl std::ops::Deref<Target = SalsaSummaryDatabase>,
        file_id: FileId,
        call_expr: &LuaCallExpr,
        sig: &SalsaSignatureExplainSummary,
        infer: &InferQuery,
        resolve_type_name: &impl Fn(&str) -> Option<LuaType>,
    ) -> Option<Self> {
        let type_def = db.doc().type_def_by_name(file_id, class_name)?;
        if type_def.generic_params.is_empty() {
            return None;
        }

        let raw: Vec<RawGeneric> = type_def
            .generic_params
            .iter()
            .map(|p| RawGeneric {
                name: p.name.to_string(),
                c_lt: p.type_offset.and_then(|key| {
                    db.doc()
                        .resolved_type_by_key(file_id, key)
                        .map(|r| r.lowered)
                }),
                d_lt: p.default_type_offset.and_then(|key| {
                    db.doc()
                        .resolved_type_by_key(file_id, key)
                        .map(|r| r.lowered)
                }),
            })
            .collect();

        let checker = Self {
            bindings: raw
                .iter()
                .map(|r| {
                    let (c, rigid) = resolve_constraint(r.c_lt.as_ref(), &raw);
                    let d = r
                        .d_lt
                        .as_ref()
                        .and_then(|lt| resolve_type_ref(Some(lt), &raw));
                    let d_is_infer = r
                        .d_lt
                        .as_ref()
                        .map(|lt| generic_ref_name(lt, &raw).is_some())
                        .unwrap_or(false);
                    GenericBinding {
                        name: r.name.clone(),
                        constraint: c,
                        is_rigid: rigid,
                        default_is_infer: d_is_infer,
                        default: d,
                        actual: None,
                    }
                })
                .collect(),
        };

        let mut checker = checker.fill_from_params(call_expr, sig, infer, resolve_type_name);
        checker.fill_by_index(call_expr, infer, resolve_type_name);
        Some(checker)
    }

    fn get_prefix_signature(
        call_expr: &LuaCallExpr,
        infer: &InferQuery,
        sig_query: &SigQuery,
    ) -> Option<SalsaSignatureExplainSummary> {
        let prefix = call_expr.get_prefix_expr()?;
        let prefix_type = infer.infer_expr(prefix).ok()?;
        if let LuaType::Signature(sid) = &prefix_type {
            let fid = infer.get_file_id();
            sig_query.explain(fid, sid.get_position())
        } else {
            None
        }
    }

    pub fn from_type_def(
        type_def: &SalsaDocTypeDefSummary,
        def_file_id: FileId,
        sig_query: &SigQuery,
    ) -> Self {
        let db = sig_query.db();

        // 收集 raw lowered types（与 from_signature 逻辑一致）
        let raw: Vec<RawGeneric> = type_def
            .generic_params
            .iter()
            .map(|p| RawGeneric {
                name: p.name.to_string(),
                c_lt: p.type_offset.and_then(|key| {
                    db.doc()
                        .resolved_type_by_key(def_file_id, key)
                        .map(|r| r.lowered)
                }),
                d_lt: p.default_type_offset.and_then(|key| {
                    db.doc()
                        .resolved_type_by_key(def_file_id, key)
                        .map(|r| r.lowered)
                }),
            })
            .collect();

        Self {
            bindings: raw
                .iter()
                .map(|r| {
                    let (c, rigid) = resolve_constraint(r.c_lt.as_ref(), &raw);
                    let d = r
                        .d_lt
                        .as_ref()
                        .and_then(|lt| resolve_type_ref(Some(lt), &raw));
                    let d_is_infer = r
                        .d_lt
                        .as_ref()
                        .map(|lt| generic_ref_name(lt, &raw).is_some())
                        .unwrap_or(false);
                    GenericBinding {
                        name: r.name.clone(),
                        constraint: c,
                        is_rigid: rigid,
                        default_is_infer: d_is_infer,
                        default: d,
                        actual: None,
                    }
                })
                .collect(),
        }
    }

    pub fn from_signature(explain: &SalsaSignatureExplainSummary) -> Self {
        let raw: Vec<RawGeneric> = explain
            .generics
            .iter()
            .flat_map(|g| &g.params)
            .map(|p| RawGeneric {
                name: p.name.to_string(),
                c_lt: p.bound_type.as_ref().and_then(|bt| bt.lowered.clone()),
                d_lt: p.default_type.as_ref().and_then(|dt| dt.lowered.clone()),
            })
            .collect();

        Self {
            bindings: raw
                .iter()
                .map(|r| {
                    let (c, rigid) = resolve_constraint(r.c_lt.as_ref(), &raw);
                    let d = r
                        .d_lt
                        .as_ref()
                        .and_then(|lt| resolve_type_ref(Some(lt), &raw));
                    let d_is_infer = r
                        .d_lt
                        .as_ref()
                        .map(|lt| generic_ref_name(lt, &raw).is_some())
                        .unwrap_or(false);
                    GenericBinding {
                        name: r.name.clone(),
                        constraint: c,
                        is_rigid: rigid,
                        default_is_infer: d_is_infer,
                        default: d,
                        actual: None,
                    }
                })
                .collect(),
        }
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 填充 actual
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    fn build_from_sig(sig: &SalsaSignatureExplainSummary) -> Option<Self> {
        Some(Self::from_signature(sig))
    }

    fn fill_call_args(
        mut self,
        call_expr: &LuaCallExpr,
        call: &SalsaCallExplainSummary,
        sig: &SalsaSignatureExplainSummary,
        infer: &InferQuery,
        resolve_type_name: &impl Fn(&str) -> Option<LuaType>,
    ) -> Self {
        for (i, gt) in call.call_generic_types.iter().enumerate() {
            if let Some(b) = self.bindings.get_mut(i) {
                b.actual = gt
                    .lowered
                    .as_ref()
                    .and_then(infer::lowered_node_to_lua_type);
            }
        }
        self.fill_from_params(call_expr, sig, infer, resolve_type_name)
    }

    /// 按索引直接填充：arg[i] → binding[i]（不依赖 signature params）
    fn fill_by_index(
        &mut self,
        call_expr: &LuaCallExpr,
        infer: &InferQuery,
        resolve_type_name: &impl Fn(&str) -> Option<LuaType>,
    ) {
        let args: Vec<_> = call_expr
            .get_args_list()
            .into_iter()
            .flat_map(|al| al.get_args())
            .collect();
        for (i, arg) in args.iter().enumerate() {
            if let Some(b) = self.bindings.get_mut(i) {
                if b.actual.is_none() {
                    // 先尝试标准推断
                    b.actual = infer_arg_type(arg, infer, resolve_type_name);
                    // 回退：通过 @type 注解直接解析
                    if b.actual.is_none() {
                        b.actual = resolve_arg_type_annotation(arg, infer);
                    }
                }
            }
        }
    }

    fn fill_from_params(
        mut self,
        call_expr: &LuaCallExpr,
        sig: &SalsaSignatureExplainSummary,
        infer: &InferQuery,
        resolve_type_name: &impl Fn(&str) -> Option<LuaType>,
    ) -> Self {
        let args: Vec<_> = call_expr
            .get_args_list()
            .into_iter()
            .flat_map(|al| al.get_args())
            .collect();

        for (pi, param) in sig.params.iter().enumerate() {
            let known: Vec<String> = self.bindings.iter().map(|b| b.name.clone()).collect();
            let generic_names: Vec<String> = param
                .doc_type
                .as_ref()
                .and_then(|dt| dt.lowered.as_ref())
                .map(|lt| extract_infer_names(lt, &known))
                .unwrap_or_default();

            if let Some(arg) = args.get(pi) {
                if let Some(arg_ty) = infer_arg_type(arg, infer, resolve_type_name) {
                    if generic_names.is_empty() {
                        if let Some(b) = self.bindings.get_mut(pi) {
                            if b.actual.is_none() {
                                b.actual = Some(arg_ty);
                            }
                        }
                    } else {
                        for gname in &generic_names {
                            if let Some(b) = self.bindings.iter_mut().find(|b| b.name == *gname) {
                                if b.actual.is_none() {
                                    b.actual = Some(arg_ty.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        self
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 检查
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.bindings.iter().position(|b| b.name == name)
    }

    pub fn check_actuals(
        &self,
        model: &crate::semantic_model::SemanticModel,
        mut on_violation: impl FnMut(&ConstraintViolation),
    ) {
        for b in &self.bindings {
            if let (Some(c), Some(a)) = (&b.constraint, &b.actual) {
                if model.type_check_detail(c, a).is_err() {
                    on_violation(&ConstraintViolation {
                        param_name: b.name.clone(),
                        constraint: c.clone(),
                        actual: a.clone(),
                    });
                }
            }
        }
    }

    pub fn check_defaults(
        &self,
        model: &crate::semantic_model::SemanticModel,
        mut on_violation: impl FnMut(&ConstraintViolation),
    ) {
        for b in &self.bindings {
            let Some(default) = &b.default else { continue };
            let Some(constraint) = &b.constraint else {
                continue;
            };

            // 类型形参占位符: constraint 引用另一个泛型，default 也必须是泛型引用
            if b.is_rigid && !b.default_is_infer {
                on_violation(&ConstraintViolation {
                    param_name: b.name.clone(),
                    constraint: constraint.clone(),
                    actual: default.clone(),
                });
                continue;
            }

            if model.type_check_detail(constraint, default).is_err() {
                on_violation(&ConstraintViolation {
                    param_name: b.name.clone(),
                    constraint: constraint.clone(),
                    actual: default.clone(),
                });
            }
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Infer / Rigid 解析
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

struct RawGeneric {
    name: String,
    c_lt: Option<SalsaDocTypeLoweredNode>,
    d_lt: Option<SalsaDocTypeLoweredNode>,
}

/// 解析约束：检测泛型引用（`Infer { "T" }` 或 `Name { "T" }` 且 T 在 raw 中）。
/// 返回 (resolved_type, is_rigid=true 表示引用另一个泛型)
fn resolve_constraint(
    lt: Option<&SalsaDocTypeLoweredNode>,
    raw: &[RawGeneric],
) -> (Option<LuaType>, bool) {
    let Some(lt) = lt else { return (None, false) };
    if let Some(gname) = generic_ref_name(lt, raw) {
        // 约束引用另一个泛型 → rigid!
        let resolved = raw.iter().find(|r| r.name == gname).and_then(|r| {
            r.c_lt
                .as_ref()
                .and_then(|c_lt| resolve_type_ref(Some(c_lt), raw))
                .or_else(|| {
                    r.d_lt
                        .as_ref()
                        .and_then(|d_lt| resolve_type_ref(Some(d_lt), raw))
                })
        });
        return (resolved, true);
    }
    (resolve_type_ref(Some(lt), raw), false)
}

/// 从 lowered type 提取泛型引用名（如果它引用了 raw 中的某个泛型参数）
fn generic_ref_name(lt: &SalsaDocTypeLoweredNode, raw: &[RawGeneric]) -> Option<String> {
    match &lt.kind {
        SalsaDocTypeLoweredKind::Infer { generic_name } => Some(generic_name.to_string()),
        SalsaDocTypeLoweredKind::Name { name } => {
            if raw.iter().any(|r| r.name.as_str() == name.as_str()) {
                Some(name.to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// 解析类型引用（约束/默认值），展开泛型引用
fn resolve_type_ref(lt: Option<&SalsaDocTypeLoweredNode>, raw: &[RawGeneric]) -> Option<LuaType> {
    let lt = lt?;
    if let Some(gname) = generic_ref_name(lt, raw) {
        return raw
            .iter()
            .find(|r| r.name == gname)
            .and_then(|r| {
                r.c_lt
                    .as_ref()
                    .and_then(|c_lt| resolve_type_ref(Some(c_lt), raw))
                    .or_else(|| {
                        r.d_lt
                            .as_ref()
                            .and_then(|d_lt| resolve_type_ref(Some(d_lt), raw))
                    })
            })
            .or_else(|| Some(LuaType::Any));
    }
    infer::lowered_node_to_lua_type(lt)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 辅助
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 从 lowered type 提取引用的泛型名。
/// 支持 `Infer { "T" }` 和 `Name { "T" }`（当 T 是已知泛型参数时）。
fn extract_infer_names(node: &SalsaDocTypeLoweredNode, known_generics: &[String]) -> Vec<String> {
    let mut names = Vec::new();
    match &node.kind {
        SalsaDocTypeLoweredKind::Infer { generic_name } => {
            names.push(generic_name.to_string());
        }
        SalsaDocTypeLoweredKind::Name { name } => {
            if known_generics.iter().any(|g| g.as_str() == name.as_str()) {
                names.push(name.to_string());
            }
        }
        _ => {}
    }
    names
}

fn infer_arg_type(
    arg: &emmylua_parser::LuaExpr,
    infer: &InferQuery,
    resolve_type_name: &impl Fn(&str) -> Option<LuaType>,
) -> Option<LuaType> {
    if let Some(name) = extract_string_literal(arg) {
        if let Some(ty) = resolve_builtin(&name).or_else(|| resolve_type_name(&name)) {
            return Some(ty);
        }
    }
    infer.infer_expr(arg.clone()).ok()
}

fn extract_string_literal(expr: &emmylua_parser::LuaExpr) -> Option<String> {
    use emmylua_parser::LuaLiteralToken;
    match expr {
        emmylua_parser::LuaExpr::LiteralExpr(lit) => match lit.get_literal()? {
            LuaLiteralToken::String(s) => Some(s.get_value()),
            _ => None,
        },
        _ => None,
    }
}

/// 直接从 @type 注解解析参数类型（绕过 infer 缓存）。
fn resolve_arg_type_annotation(
    arg: &emmylua_parser::LuaExpr,
    infer: &InferQuery,
) -> Option<LuaType> {
    let name_expr = LuaNameExpr::cast(arg.syntax().clone())?;
    let name_text = name_expr.get_name_token()?.get_name_text().to_string();

    let db = infer.read_db();
    let file_id = infer.get_file_id();
    let doc = db.doc().summary(file_id)?;
    let decl_tree = db.file().decl_tree(file_id)?;

    for type_tag in &doc.type_tags {
        let Some(owner_offset) = type_tag.owner.syntax_offset else {
            continue;
        };
        if type_tag.owner.kind != SalsaDocOwnerKindSummary::LocalStat {
            continue;
        }
        // 查找匹配的声明
        if let Some(_decl) = decl_tree.decls.iter().find(|d| {
            d.name.as_str() == name_text.as_str()
                && d.start_offset >= owner_offset
                && u32::from(d.start_offset - owner_offset) < 50
        }) {
            if let Some(first_key) = type_tag.type_offsets.first() {
                if let Some(resolved) = db.doc().resolved_type_by_key(file_id, *first_key) {
                    return infer::lowered_node_to_lua_type(&resolved.lowered);
                }
            }
        }
    }
    None
}

fn resolve_builtin(name: &str) -> Option<LuaType> {
    match name {
        "nil" => Some(LuaType::Nil),
        "any" | "unknown" => Some(LuaType::Any),
        "boolean" | "bool" => Some(LuaType::Boolean),
        "string" => Some(LuaType::String),
        "number" => Some(LuaType::Number),
        "integer" | "int" => Some(LuaType::Integer),
        "function" => Some(LuaType::Function),
        "table" => Some(LuaType::Table),
        "thread" => Some(LuaType::Thread),
        "userdata" => Some(LuaType::Userdata),
        _ => None,
    }
}
