//! 泛型处理模块 — 两阶段设计
//!
//! # 架构
//!
//! ```
//!                     调用点                         定义点
//!   foo::<string>("hello")           fn foo<T>(a: T): T
//!        │                                    │
//!        ▼                                    ▼
//!   GenericBindings::from_explicit_args  ─┐   params → GenericTpl
//!   GenericBindings::infer_from_match   ─┤   收集所有 TplId
//!        │                               │
//!        ▼                               ▼
//!          bindings: { T(func#0) → string }
//!                    │
//!                    ▼
//!            substitute(return_type, &bindings)
//!                    │
//!                    ▼
//!               string  (具体类型)
//! ```
//!
//! # 复杂实例化扩展
//!
//! 当前实现处理简单的泛型替换。更复杂的场景（条件类型、递归别名）
//! 通过在调用方预计算 binding 来处理：
//!
//! - **条件类型** (`T extends string ? A : B`)：
//!   调用方在构建 `GenericBindings` 时评估条件，选择 `A` 或 `B` 作为 binding。
//! - **递归别名** (`type Foo<T> = T | Foo<T[]>`):
//!   调用方使用 `GenericBindings::with_depth` 设置展开深度，`substitute` 到达
//!   限制后保留未展开的 `Generic` 节点。
//! - **Mapped types** (`{ [K in keyof T]: V }`):
//!   调用方在 substitute 之前展开 mapped 的 key 域，然后传入具体化的 Object。

use hashbrown::{HashMap, HashSet};
use smol_str::SmolStr;
use std::sync::Arc;

use crate::{
    GenericTplId, LuaArrayType, LuaGenericType, LuaIntersectionType, LuaMappedType, LuaMemberKey,
    LuaObjectType, LuaTupleStatus, LuaTupleType, LuaType, LuaTypeDeclId, VariadicType,
};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// GenericBindings
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 泛型参数 → 具体类型的不可变映射。
#[derive(Debug, Clone, Default)]
pub struct GenericBindings {
    map: HashMap<GenericTplId, LuaType>,
    self_type: Option<LuaType>,
    /// 最大展开深度，超过后保留未展开的 Generic 节点
    max_depth: u32,
}

impl GenericBindings {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            self_type: None,
            max_depth: 8,
        }
    }

    /// 设置最大展开深度。
    pub fn with_max_depth(mut self, depth: u32) -> Self {
        self.max_depth = depth;
        self
    }

    /// 从显式泛型参数构建（如 `foo::<string, number>()`）。
    pub fn from_explicit_args(type_args: &[LuaType]) -> Self {
        let mut bindings = Self::new();
        for (i, ty) in type_args.iter().enumerate() {
            bindings
                .map
                .insert(GenericTplId::Func(i as u32), ty.clone());
        }
        bindings
    }

    /// 从类泛型参数构建（如 `MyClass<string>`）。
    pub fn from_type_args(type_args: &[LuaType]) -> Self {
        let mut bindings = Self::new();
        for (i, ty) in type_args.iter().enumerate() {
            bindings
                .map
                .insert(GenericTplId::Type(i as u32), ty.clone());
        }
        bindings
    }

    /// 从泛型参数 + 实参类型通过模式匹配推断绑定。
    ///
    /// 逐个匹配参数和实参。例如 `[T, K]` vs `[string, number]` → `{T→string, K→number}`。
    pub fn infer_from_match(params: &[LuaType], args: &[LuaType]) -> Self {
        let mut bindings = Self::new();
        for (i, param) in params.iter().enumerate() {
            let Some(arg) = args.get(i) else { break };
            bindings.match_one(param, arg);
        }
        bindings
    }

    /// 设置 self 类型（用于 `self` 参数的 SelfInfer 替换）。
    pub fn with_self_type(mut self, self_type: LuaType) -> Self {
        self.self_type = Some(self_type);
        self
    }

    /// 合并另一个 GenericBindings（后者覆盖前者）。
    pub fn merge(mut self, other: &GenericBindings) -> Self {
        for (id, ty) in &other.map {
            self.map.insert(*id, ty.clone());
        }
        if other.self_type.is_some() {
            self.self_type = other.self_type.clone();
        }
        self
    }

    /// 获取某个泛型参数的绑定值。
    pub fn get(&self, id: GenericTplId) -> Option<&LuaType> {
        self.map.get(&id)
    }

    /// 获取 self 类型。
    pub fn self_type(&self) -> Option<&LuaType> {
        self.self_type.as_ref()
    }

    /// 是否为空（无绑定且无 self 类型）。
    pub fn is_empty(&self) -> bool {
        self.map.is_empty() && self.self_type.is_none()
    }

    fn match_one(&mut self, pattern: &LuaType, target: &LuaType) {
        match pattern {
            LuaType::TplRef(tpl) => {
                if tpl.get_tpl_id().is_func() {
                    self.map.insert(tpl.get_tpl_id(), target.clone());
                }
            }
            LuaType::Array(arr) => {
                self.match_one(arr.get_base(), target);
            }
            LuaType::Union(u) => {
                if let LuaType::Union(target_u) = target {
                    for (p, t) in u.into_vec().iter().zip(target_u.into_vec().iter()) {
                        self.match_one(p, t);
                    }
                }
            }
            _ => {}
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Substitute
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 用绑定替换类型中的泛型占位符。
///
/// 纯函数，无副作用。使用 `max_depth` 防止无限展开（循环泛型）。
/// 到达深度上限时保留未展开的 `Generic` 节点。
pub fn substitute(ty: &LuaType, bindings: &GenericBindings) -> LuaType {
    if bindings.is_empty() {
        return match ty {
            LuaType::SelfInfer => bindings.self_type().cloned().unwrap_or_else(|| ty.clone()),
            _ => ty.clone(),
        };
    }
    let mut ctx = SubstitutionContext {
        bindings,
        visited: HashSet::new(),
    };
    ctx.substitute_inner(ty, 0)
}

/// 替换上下文：携带 bindings + visited set 用于递归展开和循环检测。
struct SubstitutionContext<'a> {
    bindings: &'a GenericBindings,
    /// 正在展开的 Generic type（通过 base type_id 标识）。
    /// 防止 `type Foo<T> = T | Foo<T[]>` 类递归无限展开。
    visited: HashSet<LuaTypeDeclId>,
}

impl<'a> SubstitutionContext<'a> {
    fn substitute_inner(&mut self, ty: &LuaType, depth: u32) -> LuaType {
        if depth >= self.bindings.max_depth {
            // 到达深度上限，如果有泛型占位符则直接返回（保留未展开形态）
            // 否则返回原类型
            return ty.clone();
        }

        match ty {
            // ── 直接替换 ──
            LuaType::TplRef(tpl) => self
                .bindings
                .get(tpl.get_tpl_id())
                .cloned()
                .unwrap_or_else(|| ty.clone()),
            LuaType::StrTplRef(str_tpl) => {
                if let Some(bound) = self.bindings.get(str_tpl.get_tpl_id()) {
                    let resolved =
                        resolve_str_tpl(str_tpl.get_prefix(), str_tpl.get_suffix(), &bound);
                    if !matches!(resolved, LuaType::Unknown) {
                        return resolved;
                    }
                }
                ty.clone()
            }
            LuaType::SelfInfer => self
                .bindings
                .self_type()
                .cloned()
                .unwrap_or_else(|| ty.clone()),

            // ── 递归替换 ──
            LuaType::Generic(generic) => self.substitute_generic(generic, depth),
            LuaType::Array(arr) => {
                let base = arr.get_base();
                let new_base = self.substitute_inner(base, depth + 1);
                if &new_base == base {
                    ty.clone()
                } else {
                    LuaType::Array(LuaArrayType::new(new_base, arr.get_len().clone()).into())
                }
            }
            LuaType::Union(u) => {
                let old = u.into_vec();
                let new: Vec<LuaType> = old
                    .iter()
                    .map(|m| self.substitute_inner(m, depth + 1))
                    .collect();
                LuaType::from_vec(new)
            }
            LuaType::Intersection(intersection) => {
                let old = intersection.get_types();
                let new: Vec<LuaType> = old
                    .iter()
                    .map(|m| self.substitute_inner(m, depth + 1))
                    .collect();
                LuaIntersectionType::new(new).into()
            }
            LuaType::Object(obj) => self.substitute_object(obj, depth),
            LuaType::Tuple(tuple) => {
                let types = tuple.get_types();
                let mut changed = false;
                let new_types: Vec<LuaType> = types
                    .iter()
                    .map(|t| {
                        let new_t = self.substitute_inner(t, depth + 1);
                        if &new_t != t {
                            changed = true;
                        }
                        new_t
                    })
                    .collect();
                if !changed {
                    ty.clone()
                } else {
                    LuaType::Tuple(LuaTupleType::new(new_types, LuaTupleStatus::DocResolve).into())
                }
            }
            LuaType::TableGeneric(params) => {
                let new_params: Vec<LuaType> = params
                    .iter()
                    .map(|p| self.substitute_inner(p, depth + 1))
                    .collect();
                if new_params.as_slice() == params.as_slice() {
                    ty.clone()
                } else {
                    LuaType::TableGeneric(Arc::new(new_params))
                }
            }
            LuaType::Mapped(mapped) => {
                // Mapped type：替换 value 端。param 端（key 域）由调用方预展开后传入。
                let new_value = self.substitute_inner(&mapped.value, depth + 1);
                if new_value == mapped.value {
                    ty.clone()
                } else {
                    LuaType::Mapped(
                        LuaMappedType::new(
                            mapped.param.clone(),
                            new_value,
                            mapped.is_readonly,
                            mapped.is_optional,
                        )
                        .into(),
                    )
                }
            }
            LuaType::Variadic(variadic) => self.substitute_variadic(variadic, depth),

            // 叶节点：原样返回
            _ => ty.clone(),
        }
    }

    /// 替换 `Generic<T>` — 包含循环检测。
    fn substitute_generic(&mut self, generic: &Arc<LuaGenericType>, depth: u32) -> LuaType {
        let base_id = generic.get_base_type_id();

        // 循环检测：如果同一个类型 ID 已经在展开栈中，停止。
        if !self.visited.insert(base_id.clone()) {
            return LuaType::Generic(generic.clone());
        }

        let old_params = generic.get_params();
        let new_params: Vec<LuaType> = old_params
            .iter()
            .map(|p| self.substitute_inner(p, depth + 1))
            .collect();

        // 如果参数没有变化，不需要重建
        let result = if new_params.as_slice() == old_params.as_slice() {
            LuaType::Generic(generic.clone())
        } else {
            LuaType::Generic(LuaGenericType::new(base_id.clone(), new_params).into())
        };

        self.visited.remove(&base_id);
        result
    }

    fn substitute_object(&mut self, obj: &Arc<LuaObjectType>, depth: u32) -> LuaType {
        let fields = obj.get_fields();
        let mut changed = false;
        let mut new_fields: Vec<(LuaMemberKey, LuaType)> = Vec::with_capacity(fields.len());
        for (key, val) in fields.iter() {
            let new_val = self.substitute_inner(val, depth + 1);
            if &new_val != val {
                changed = true;
            }
            new_fields.push((key.clone(), new_val));
        }
        if !changed {
            return LuaType::Object(obj.clone());
        }
        LuaType::Object(
            LuaObjectType::new_with_fields(
                new_fields.into_iter().collect(),
                obj.get_index_access().to_vec(),
            )
            .into(),
        )
    }

    fn substitute_variadic(&mut self, variadic: &Arc<VariadicType>, depth: u32) -> LuaType {
        match variadic.as_ref() {
            VariadicType::Base(base) => {
                let new_base = self.substitute_inner(base, depth + 1);
                if &new_base == base {
                    LuaType::Variadic(variadic.clone())
                } else {
                    LuaType::Variadic(VariadicType::Base(new_base).into())
                }
            }
            VariadicType::Multi(types) => {
                let mut changed = false;
                let new_types: Vec<LuaType> = types
                    .iter()
                    .map(|t| {
                        let new_t = self.substitute_inner(t, depth + 1);
                        if &new_t != t {
                            changed = true;
                        }
                        new_t
                    })
                    .collect();
                if !changed {
                    LuaType::Variadic(variadic.clone())
                } else {
                    LuaType::Variadic(VariadicType::Multi(new_types).into())
                }
            }
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Helpers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn resolve_str_tpl(prefix: &str, suffix: &str, target: &LuaType) -> LuaType {
    match target {
        LuaType::StringConst(s) => {
            let name = SmolStr::new(format!("{}{}{}", prefix, s.as_str(), suffix));
            LuaType::Ref(LuaTypeDeclId::global(name.as_str()))
        }
        LuaType::DocStringConst(s) => {
            let name = SmolStr::new(format!("{}{}{}", prefix, s.as_str(), suffix));
            LuaType::Ref(LuaTypeDeclId::global(name.as_str()))
        }
        _ => LuaType::Unknown,
    }
}
