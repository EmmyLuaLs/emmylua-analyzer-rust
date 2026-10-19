use crate::def::lua_type::GenericParam;
use crate::member_key::LuaMemberKey;
use crate::{AsyncState, InFiled, LuaAttributeUse};
use hashbrown::{HashMap, HashSet};
use internment::ArcIntern;
use rowan::TextRange;
use rustc_hash::FxBuildHasher;
use smol_str::SmolStr;
use std::{
    array,
    cell::OnceCell,
    hash::{BuildHasher, Hash, Hasher},
    iter::FusedIterator,
    num::NonZeroU64,
    ops::Deref,
    rc::Rc,
    slice,
};

use super::LuaTypeNode;
use super::lua_type::LuaType;
use crate::semantic_db::def::lua_type::basic_union::{
    BasicTypeKind, BasicTypeUnion, BasicTypeUnionIter,
};
use crate::semantic_db::def::lua_type::type_decl::LuaTypeDeclId;

fn cached_type_hash(value: impl Hash) -> NonZeroU64 {
    // 利用零值表示未计算
    NonZeroU64::new(FxBuildHasher.hash_one(value)).unwrap_or(NonZeroU64::MIN)
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaTupleType {
    types: Vec<LuaType>,
    pub status: LuaTupleStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LuaTupleStatus {
    DocResolve,
    InferResolve,
}

impl LuaTupleType {
    pub fn new(types: Vec<LuaType>, status: LuaTupleStatus) -> Self {
        Self { types, status }
    }

    pub fn get_types(&self) -> &[LuaType] {
        &self.types
    }

    pub fn get_type(&self, idx: usize) -> Option<&LuaType> {
        if let Some(ty) = self.types.get(idx) {
            return Some(ty);
        }

        if self.types.is_empty() {
            return None;
        }

        let last_id = self.types.len() - 1;
        let last_type = self.types.get(last_id)?;
        if let LuaType::Variadic(variadic) = last_type {
            return variadic.get_type(idx - last_id);
        }

        None
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    pub fn contain_tpl(&self) -> bool {
        self.contain_tpl_children()
    }

    pub fn is_infer_resolve(&self) -> bool {
        matches!(self.status, LuaTupleStatus::InferResolve)
    }
}

impl From<LuaTupleType> for LuaType {
    fn from(t: LuaTupleType) -> Self {
        LuaType::Tuple(t.into())
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaFunctionType {
    async_state: AsyncState,
    is_colon_define: bool,
    is_variadic: bool,
    generic_params: Option<Vec<GenericTpl>>,
    params: Vec<(String, Option<LuaType>)>,
    ret: LuaType,
}

impl LuaFunctionType {
    pub fn new(
        async_state: AsyncState,
        is_colon_define: bool,
        is_variadic: bool,
        params: Vec<(String, Option<LuaType>)>,
        ret: LuaType,
        generic_params: Option<Vec<GenericTpl>>,
    ) -> Self {
        let generic_params = generic_params.filter(|params| !params.is_empty());
        Self {
            async_state,
            is_colon_define,
            is_variadic,
            generic_params,
            params,
            ret,
        }
    }

    pub fn get_async_state(&self) -> AsyncState {
        self.async_state
    }

    pub fn is_colon_define(&self) -> bool {
        self.is_colon_define
    }

    pub fn get_params(&self) -> &[(String, Option<LuaType>)] {
        &self.params
    }

    pub fn get_generic_params(&self) -> &[GenericTpl] {
        self.generic_params.as_deref().unwrap_or(&[])
    }

    pub fn get_ret(&self) -> &LuaType {
        &self.ret
    }

    pub fn is_variadic(&self) -> bool {
        self.is_variadic
    }

    pub fn get_variadic_ret(&self) -> VariadicType {
        if let LuaType::Variadic(variadic) = &self.ret {
            return variadic.deref().clone();
        }

        VariadicType::Base(self.ret.clone())
    }

    pub fn contain_tpl(&self) -> bool {
        self.contain_tpl_children()
    }

    pub fn contain_self(&self) -> bool {
        self.is_colon_define
            || self
                .params
                .iter()
                .any(|(name, t)| name == "self" || t.as_ref().is_some_and(|t| t.is_self_infer()))
            || self.ret.is_self_infer()
    }

    // pub fn is_method(&self, semantic_model: &SemanticModel, owner_type: Option<&LuaType>) -> bool {
    //     if self.is_colon_define {
    //         return true;
    //     }
    //     if let Some((name, t)) = self.params.first() {
    //         match t {
    //             Some(t) => {
    //                 if t.is_self_infer() {
    //                     return true;
    //                 }
    //                 match owner_type {
    //                     Some(owner_type) => {
    //                         if matches!(owner_type, LuaType::Ref(_) | LuaType::Def(_))
    //                             && first_param_may_not_self(t)
    //                         {
    //                             return false;
    //                         }
    //                         if semantic_model.type_check(owner_type, t).is_ok() {
    //                             return true;
    //                         }
    //                         name == "self" && semantic_model.type_check(t, owner_type).is_ok()
    //                     }
    //                     None => name == "self",
    //                 }
    //             }
    //             None => name == "self",
    //         }
    //     } else {
    //         false
    //     }
    // }

    pub fn to_call_operator_func_type(&self) -> Rc<LuaFunctionType> {
        let mut params = self.get_params().to_vec();
        if params.first().is_some_and(|(name, _)| name == "@call_self") {
            params.remove(0);
        }

        Rc::new(LuaFunctionType::new(
            self.async_state,
            false,
            self.is_variadic,
            params,
            self.ret.clone(),
            self.generic_params.clone(),
        ))
    }
}

impl From<LuaFunctionType> for LuaType {
    fn from(t: LuaFunctionType) -> Self {
        LuaType::DocFunction(t.into())
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum LuaIndexAccessKey {
    Integer(i64),
    String(SmolStr),
    Type(LuaType),
}

#[derive(Debug, Clone)]
pub struct LuaObjectType {
    fields: HashMap<LuaMemberKey, LuaType>,
    index_access: Vec<(LuaType, LuaType)>,
    hash: OnceCell<NonZeroU64>,
}

impl LuaObjectType {
    pub fn new(object_fields: Vec<(LuaIndexAccessKey, LuaType)>) -> Self {
        let mut fields = HashMap::new();
        let mut index_access = Vec::new();
        for (key, value_type) in object_fields {
            match key {
                LuaIndexAccessKey::Integer(i) => {
                    fields.insert(LuaMemberKey::Integer(i), value_type);
                }
                LuaIndexAccessKey::String(s) => {
                    fields.insert(LuaMemberKey::Name(s), value_type);
                }
                LuaIndexAccessKey::Type(t) => {
                    index_access.push((t, value_type));
                }
            }
        }

        Self {
            fields,
            index_access,
            hash: OnceCell::new(),
        }
    }

    pub fn new_with_fields(
        fields: HashMap<LuaMemberKey, LuaType>,
        index_access: Vec<(LuaType, LuaType)>,
    ) -> Self {
        Self {
            fields,
            index_access,
            hash: OnceCell::new(),
        }
    }

    pub fn get_fields(&self) -> &HashMap<LuaMemberKey, LuaType> {
        &self.fields
    }

    pub fn get_index_access(&self) -> &[(LuaType, LuaType)] {
        &self.index_access
    }

    pub fn get_field(&self, key: &LuaMemberKey) -> Option<&LuaType> {
        self.fields.get(key)
    }

    pub fn contain_tpl(&self) -> bool {
        self.contain_tpl_children()
    }

    // pub fn cast_down_array_base(&self, db: &DbIndex) -> Option<LuaType> {
    //     if !self.index_access.is_empty() {
    //         let mut ty = None;
    //         for (key, value_type) in self.index_access.iter() {
    //             if matches!(key, LuaType::Integer) {
    //                 ty = Some(match ty {
    //                     Some(t) => TypeOps::Union.apply(db, &t, value_type),
    //                     None => value_type.clone(),
    //                 });
    //             }
    //         }
    //         return ty;
    //     }

    //     let mut ty = None;
    //     let mut fields = self.fields.iter().collect::<Vec<_>>();
    //     fields.sort_by_key(|(a, _)| *a);

    //     for (count, (key, value_type)) in (1..).zip(fields) {
    //         let idx = match key {
    //             LuaMemberKey::Integer(i) => i,
    //             _ => return None,
    //         };

    //         if *idx != count {
    //             return None;
    //         }

    //         ty = Some(match ty {
    //             Some(t) => TypeOps::Union.apply(db, &t, value_type),
    //             None => value_type.clone(),
    //         });
    //     }

    //     Some(ty.unwrap_or(LuaType::Unknown))
    // }
}

impl PartialEq for LuaObjectType {
    fn eq(&self, other: &Self) -> bool {
        if self
            .hash
            .get()
            .zip(other.hash.get())
            .is_some_and(|(left, right)| left != right)
        {
            return false;
        }
        self.fields == other.fields && self.index_access == other.index_access
    }
}

impl Eq for LuaObjectType {}

impl Hash for LuaObjectType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash
            .get_or_init(|| {
                // 字段声明顺序不影响相等性, 索引签名仍保留顺序.
                let fields_hash = self.fields.iter().fold(0u64, |hash, field| {
                    hash.wrapping_add(FxBuildHasher.hash_one(field))
                });
                cached_type_hash((self.fields.len(), fields_hash, &self.index_access))
            })
            .hash(state);
    }
}

impl Hash for LuaUnionType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            LuaUnionType::Basic(basic) => {
                0u8.hash(state);
                basic.hash(state);
            }
            LuaUnionType::Nullable(ty) => {
                1u8.hash(state);
                ty.hash(state);
            }
            LuaUnionType::Multi(types) => {
                2u8.hash(state);
                types.hash(state);
            }
        }
    }
}

impl Hash for LuaStringTplType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.prefix.hash(state);
        self.tpl_id.hash(state);
        self.name.hash(state);
        self.suffix.hash(state);
        self.constraint.hash(state);
    }
}

impl From<LuaObjectType> for LuaType {
    fn from(t: LuaObjectType) -> Self {
        LuaType::Object(t.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LuaUnionType {
    Basic(BasicTypeUnion),
    Nullable(LuaType),
    Multi(LuaUnionMembers),
}

/// 只读的联合类型成员, 首次求 hash 时缓存结果.
#[derive(Debug, Clone)]
pub struct LuaUnionMembers {
    types: Vec<LuaType>,
    hash: OnceCell<NonZeroU64>,
}

impl LuaUnionMembers {
    pub fn new(types: Vec<LuaType>) -> Self {
        Self {
            types,
            hash: OnceCell::new(),
        }
    }

    pub fn get_types(&self) -> &[LuaType] {
        &self.types
    }
}

impl PartialEq for LuaUnionMembers {
    fn eq(&self, other: &Self) -> bool {
        if self.types.len() != other.types.len()
            || self
                .hash
                .get()
                .zip(other.hash.get())
                .is_some_and(|(left, right)| left != right)
        {
            return false;
        }
        let mut types: HashSet<_> = self.types.iter().collect();
        for ty in &other.types {
            if !types.remove(ty) {
                return false;
            }
        }
        types.is_empty()
    }
}

impl Eq for LuaUnionMembers {}

impl Hash for LuaUnionMembers {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash
            .get_or_init(|| {
                let members_hash = self.types.iter().fold(0u64, |hash, ty| {
                    hash.wrapping_add(FxBuildHasher.hash_one(ty))
                });
                cached_type_hash((self.types.len(), members_hash))
            })
            .hash(state);
    }
}

impl LuaUnionType {
    pub fn from_set(mut set: HashSet<LuaType>) -> Self {
        let mut all_basic = true;
        let mut basic_type = BasicTypeUnion::new();
        for ty in &set {
            if let Some(basic_kind) = BasicTypeKind::from_type(ty) {
                basic_type.add(basic_kind);
            } else {
                all_basic = false;
                break;
            }
        }

        if all_basic {
            return Self::Basic(basic_type);
        }

        if set.len() == 2 && set.contains(&LuaType::Nil) {
            set.remove(&LuaType::Nil);
            if let Some(first) = set.iter().next() {
                return Self::Nullable(first.clone());
            }
            Self::Nullable(LuaType::Unknown)
        } else {
            Self::Multi(LuaUnionMembers::new(set.into_iter().collect()))
        }
    }

    pub fn from_vec(types: Vec<LuaType>) -> Self {
        let mut all_basic = true;
        let mut basic_type = BasicTypeUnion::new();
        for ty in &types {
            if let Some(basic_kind) = BasicTypeKind::from_type(ty) {
                basic_type.add(basic_kind);
            } else {
                all_basic = false;
                break;
            }
        }

        if all_basic {
            return Self::Basic(basic_type);
        }

        if types.len() == 2 {
            if types.contains(&LuaType::Nil) {
                let non_nil_type = types.iter().find(|t| !matches!(t, LuaType::Nil));
                if let Some(ty) = non_nil_type {
                    return Self::Nullable(ty.clone());
                }
            } else {
                return Self::Multi(LuaUnionMembers::new(types));
            }
        }
        Self::Multi(LuaUnionMembers::new(types))
    }

    #[inline]
    pub fn iter(&self) -> UnionIter<'_> {
        match self {
            LuaUnionType::Basic(basic) => UnionIter::Basic(basic.iter()),
            LuaUnionType::Nullable(ty) => UnionIter::Nullable([ty, &LuaType::Nil].into_iter()),
            LuaUnionType::Multi(types) => UnionIter::Multi(types.get_types().iter()),
        }
    }

    pub fn into_vec(&self) -> Vec<LuaType> {
        match self {
            LuaUnionType::Basic(basic) => basic.iter().cloned().collect(),
            LuaUnionType::Nullable(ty) => vec![ty.clone(), LuaType::Nil],
            LuaUnionType::Multi(types) => types.get_types().to_vec(),
        }
    }

    #[allow(unused, clippy::wrong_self_convention)]
    pub(crate) fn into_set(&self) -> HashSet<LuaType> {
        self.iter().cloned().collect()
    }

    pub fn contain_tpl(&self) -> bool {
        self.contain_tpl_children()
    }

    pub fn is_nullable(&self) -> bool {
        match self {
            LuaUnionType::Basic(basic) => basic.contains(BasicTypeKind::Nil),
            LuaUnionType::Nullable(_) => true,
            LuaUnionType::Multi(types) => types.get_types().iter().any(|t| t.is_nullable()),
        }
    }

    pub fn is_optional(&self) -> bool {
        match self {
            LuaUnionType::Basic(basic) => basic.contains(BasicTypeKind::Nil),
            LuaUnionType::Nullable(_) => true,
            LuaUnionType::Multi(types) => types.get_types().iter().any(|t| t.is_optional()),
        }
    }

    pub fn is_always_truthy(&self) -> bool {
        match self {
            LuaUnionType::Basic(basic) => basic.iter().all(|t| t.is_always_truthy()),
            LuaUnionType::Nullable(_) => false,
            LuaUnionType::Multi(types) => types.get_types().iter().all(|t| t.is_always_truthy()),
        }
    }

    pub fn is_always_falsy(&self) -> bool {
        match self {
            LuaUnionType::Basic(basic) => basic.iter().all(|t| t.is_always_falsy()),
            LuaUnionType::Nullable(f) => f.is_always_falsy(),
            LuaUnionType::Multi(types) => types.get_types().iter().all(|t| t.is_always_falsy()),
        }
    }
}

impl From<LuaUnionType> for LuaType {
    fn from(t: LuaUnionType) -> Self {
        LuaType::Union(t.into())
    }
}

#[derive(Debug, Clone)]
pub enum UnionIter<'a> {
    Basic(BasicTypeUnionIter),
    Nullable(array::IntoIter<&'a LuaType, 2>),
    Multi(slice::Iter<'a, LuaType>),
}

impl<'a> Iterator for UnionIter<'a> {
    type Item = &'a LuaType;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            UnionIter::Basic(iter) => iter.next(),
            UnionIter::Nullable(iter) => iter.next(),
            UnionIter::Multi(iter) => iter.next(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            UnionIter::Basic(iter) => iter.size_hint(),
            UnionIter::Nullable(iter) => iter.size_hint(),
            UnionIter::Multi(iter) => iter.size_hint(),
        }
    }
}

impl DoubleEndedIterator for UnionIter<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            UnionIter::Basic(iter) => iter.next_back(),
            UnionIter::Nullable(iter) => iter.next_back(),
            UnionIter::Multi(iter) => iter.next_back(),
        }
    }
}

impl ExactSizeIterator for UnionIter<'_> {}
impl FusedIterator for UnionIter<'_> {}

impl<'a> IntoIterator for &'a LuaUnionType {
    type Item = &'a LuaType;
    type IntoIter = UnionIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaIntersectionType {
    types: Vec<LuaType>,
}

impl LuaIntersectionType {
    pub fn new(types: Vec<LuaType>) -> Self {
        Self { types }
    }

    pub fn get_types(&self) -> &[LuaType] {
        &self.types
    }

    pub fn contain_tpl(&self) -> bool {
        self.contain_tpl_children()
    }
}

impl From<LuaIntersectionType> for LuaType {
    fn from(t: LuaIntersectionType) -> Self {
        LuaType::Intersection(t.into())
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum LuaAliasCallKind {
    KeyOf,
    Index,
    Extends,
    Add,
    Sub,
    Select,
    Unpack,
    RawGet,
    Merge,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaAliasCallType {
    call_kind: LuaAliasCallKind,
    operand: Vec<LuaType>,
}

impl LuaAliasCallType {
    pub fn new(call_kind: LuaAliasCallKind, operand: Vec<LuaType>) -> Self {
        Self { call_kind, operand }
    }

    pub fn get_operands(&self) -> &Vec<LuaType> {
        &self.operand
    }

    pub fn get_call_kind(&self) -> LuaAliasCallKind {
        self.call_kind
    }

    pub fn contain_tpl(&self) -> bool {
        self.contain_tpl_children()
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaGenericType {
    base: LuaTypeDeclId,
    params: Vec<LuaType>,
}

impl LuaGenericType {
    pub fn new(base: LuaTypeDeclId, params: Vec<LuaType>) -> Self {
        Self { base, params }
    }

    pub fn get_base_type(&self) -> LuaType {
        LuaType::Ref(self.base.clone())
    }

    pub fn get_base_type_id(&self) -> LuaTypeDeclId {
        self.base.clone()
    }

    pub fn get_base_type_id_ref(&self) -> &LuaTypeDeclId {
        &self.base
    }

    pub fn get_params(&self) -> &Vec<LuaType> {
        &self.params
    }

    pub fn contain_tpl(&self) -> bool {
        self.contain_tpl_children()
    }
}

impl From<LuaGenericType> for LuaType {
    fn from(t: LuaGenericType) -> Self {
        LuaType::Generic(t.into())
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum VariadicType {
    /// Result slots whose types are already known, with an optional variadic tail.
    /// A function declared with `---@return string, integer` has two result slots:
    /// `Multi([String, Integer])`. A call can return `'a', 1`.
    Multi(Vec<LuaType>),

    /// A declared `T...`; a tuple `T` supplies separate result slots.
    ///
    /// ```lua
    /// ---@param ... [string, integer]...
    /// local function forward(...) return ... end
    /// forward('a', 1)
    /// ```
    /// The annotation gives `Base(Tuple([String, Integer]))`. This expands to
    /// `Multi([String, Integer])`: two separate results, a string and an integer.
    /// With a generic `T`, expansion can wait until `T` is known.
    Base(LuaType),

    /// Any number of whole values of the same type.
    ///
    /// ```lua
    /// ---@param ... [string, integer]
    /// local function tuples(...) local packed = {...} end
    /// tuples({'a', 1}, {'b', 2})
    /// ```
    /// Here, each argument is a tuple table. The type of `...` is
    /// `Repeat(Tuple([String, Integer]))`. In `packed`, element 1 is `{'a', 1}`
    /// and element 2 is `{'b', 2}`; both elements have the whole tuple type.
    Repeat(LuaType),
}

impl VariadicType {
    pub fn get_type(&self, idx: usize) -> Option<&LuaType> {
        match self {
            VariadicType::Multi(types) => {
                let types_len = types.len();
                if types_len == 0 {
                    return None;
                }

                if idx + 1 < types.len() {
                    Some(&types[idx])
                } else {
                    let last_idx = types_len - 1;
                    let last_ty = &types[last_idx];
                    let offset = idx - last_idx;
                    if let LuaType::Variadic(variadic) = last_ty {
                        variadic.get_type(offset)
                    } else if offset == 0 {
                        Some(last_ty)
                    } else {
                        None
                    }
                }
            }
            VariadicType::Base(t) => match t {
                LuaType::Tuple(tuple) => tuple.get_types().get(idx),
                _ => Some(t),
            },
            VariadicType::Repeat(t) => Some(t),
        }
    }

    pub fn get_new_variadic_from(&self, idx: usize) -> VariadicType {
        match self {
            VariadicType::Multi(types) => {
                if types.is_empty() {
                    return VariadicType::Multi(Vec::new());
                }

                let mut new_types = Vec::new();
                if idx < types.len() {
                    new_types.extend_from_slice(&types[idx..]);
                } else {
                    let last = types.len() - 1;
                    let rest_offset = idx - last;
                    match &types[last] {
                        LuaType::Variadic(multi) => {
                            return multi.get_new_variadic_from(rest_offset);
                        }
                        LuaType::Union(union) if types[last].contain_multi_return() => {
                            let tails = union
                                .iter()
                                .map(|ty| {
                                    let tail = VariadicType::Multi(vec![ty.clone()])
                                        .get_new_variadic_from(rest_offset);
                                    LuaType::Variadic(Rc::new(tail))
                                })
                                .collect();
                            return VariadicType::Multi(vec![LuaType::from_vec(tails)]);
                        }
                        _ => {}
                    }
                }

                VariadicType::Multi(new_types)
            }
            VariadicType::Base(LuaType::Tuple(tuple)) => {
                VariadicType::Multi(tuple.get_types().iter().skip(idx).cloned().collect())
            }
            VariadicType::Base(t) => VariadicType::Base(t.clone()),
            VariadicType::Repeat(t) => VariadicType::Repeat(t.clone()),
        }
    }

    pub fn contain_tpl(&self) -> bool {
        self.contain_tpl_children()
    }

    pub fn get_min_len(&self) -> Option<usize> {
        match self {
            VariadicType::Repeat(_) => None,
            VariadicType::Base(base) => match base {
                LuaType::Tuple(tuple) => Some(tuple.get_types().len()),
                _ => None,
            },
            VariadicType::Multi(types) => {
                let mut total_len = 0;
                for t in types {
                    if let LuaType::Variadic(variadic) = t {
                        let len = match variadic.get_min_len() {
                            Some(len) => len,
                            None => return Some(total_len),
                        };
                        total_len += len;
                    } else {
                        total_len += 1;
                    }
                }
                Some(total_len)
            }
        }
    }

    pub fn get_max_len(&self) -> Option<usize> {
        match self {
            VariadicType::Repeat(_) => None,
            VariadicType::Base(base) => match base {
                LuaType::Tuple(tuple) => Some(tuple.get_types().len()),
                _ => None,
            },
            VariadicType::Multi(types) => {
                let mut total_len = 0;
                for t in types {
                    if let LuaType::Variadic(variadic) = t {
                        let len = variadic.get_max_len()?;
                        total_len += len;
                    } else if t.contain_multi_return() {
                        return None;
                    } else {
                        total_len += 1;
                    }
                }
                Some(total_len)
            }
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaInstanceType {
    base: LuaType,
    range: InFiled<TextRange>,
}

impl LuaInstanceType {
    pub fn new(base: LuaType, range: InFiled<TextRange>) -> Self {
        Self { base, range }
    }

    pub fn get_base(&self) -> &LuaType {
        &self.base
    }

    pub fn get_range(&self) -> &InFiled<TextRange> {
        &self.range
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum GenericTplId {
    Type(u32),
    Func(u32),
    ConditionalInfer(u32),
}

impl GenericTplId {
    pub fn get_idx(&self) -> usize {
        match self {
            GenericTplId::Type(idx) => *idx as usize,
            GenericTplId::Func(idx) => *idx as usize,
            GenericTplId::ConditionalInfer(idx) => *idx as usize,
        }
    }

    pub fn is_func(&self) -> bool {
        matches!(self, GenericTplId::Func(_))
    }

    pub fn is_type(&self) -> bool {
        matches!(self, GenericTplId::Type(_))
    }

    pub fn is_conditional_infer(&self) -> bool {
        matches!(self, GenericTplId::ConditionalInfer(_))
    }

    pub fn with_idx(&self, idx: u32) -> Self {
        match self {
            GenericTplId::Type(_) => GenericTplId::Type(idx),
            GenericTplId::Func(_) => GenericTplId::Func(idx),
            GenericTplId::ConditionalInfer(_) => GenericTplId::ConditionalInfer(idx),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GenericTpl {
    tpl_id: GenericTplId,
    param: GenericParam,
}

impl GenericTpl {
    pub fn new(
        tpl_id: GenericTplId,
        name: SmolStr,
        constraint: Option<LuaType>,
        default_type: Option<LuaType>,
        is_const: bool,
        attributes: Option<Vec<LuaAttributeUse>>,
    ) -> Self {
        Self {
            tpl_id,
            param: GenericParam::new(name, constraint, default_type, is_const, attributes),
        }
    }

    pub fn get_tpl_id(&self) -> GenericTplId {
        self.tpl_id
    }

    pub fn get_param(&self) -> &GenericParam {
        &self.param
    }

    pub fn get_name(&self) -> &str {
        self.param.name.as_str()
    }

    pub fn is_const(&self) -> bool {
        self.param.is_const
    }

    pub fn with_const(&self, is_const: bool) -> Self {
        let mut param = self.param.clone();
        param.is_const = is_const;
        Self {
            tpl_id: self.tpl_id,
            param,
        }
    }

    pub fn get_constraint(&self) -> Option<&LuaType> {
        self.param.constraint.as_ref()
    }

    pub fn get_default_type(&self) -> Option<&LuaType> {
        self.param.default.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaStringTplType {
    prefix: ArcIntern<String>,
    tpl_id: GenericTplId,
    name: ArcIntern<String>,
    suffix: ArcIntern<String>,
    constraint: Option<LuaType>,
}

impl LuaStringTplType {
    pub fn new(
        prefix: &str,
        name: &str,
        tpl_id: GenericTplId,
        suffix: &str,
        constraint: Option<LuaType>,
    ) -> Self {
        Self {
            prefix: ArcIntern::new(prefix.to_string()),
            tpl_id,
            name: ArcIntern::new(name.to_string()),
            suffix: ArcIntern::new(suffix.to_string()),
            constraint,
        }
    }

    pub fn get_prefix(&self) -> &str {
        &self.prefix
    }

    pub fn get_tpl_id(&self) -> GenericTplId {
        self.tpl_id
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_suffix(&self) -> &str {
        &self.suffix
    }

    pub fn get_constraint(&self) -> Option<&LuaType> {
        self.constraint.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct LuaMultiLineUnion {
    unions: Vec<(LuaType, Option<String>)>,
    hash: OnceCell<NonZeroU64>,
}

impl LuaMultiLineUnion {
    pub fn new(unions: Vec<(LuaType, Option<String>)>) -> Self {
        Self {
            unions,
            hash: OnceCell::new(),
        }
    }

    pub fn get_unions(&self) -> &[(LuaType, Option<String>)] {
        &self.unions
    }

    pub fn iter(
        &self,
    ) -> impl DoubleEndedIterator<Item = &LuaType> + ExactSizeIterator + FusedIterator {
        self.unions.iter().map(|(ty, _)| ty)
    }

    pub fn to_union(&self) -> LuaType {
        let types = self.iter().cloned().collect();
        LuaType::Union(Rc::new(LuaUnionType::from_vec(types)))
    }

    pub fn contain_tpl(&self) -> bool {
        self.contain_tpl_children()
    }

    pub fn is_optional(&self) -> bool {
        let mut all_basic = true;
        let mut optional = false;
        for ty in self.iter() {
            if ty.is_nil() {
                return true;
            }
            all_basic &= BasicTypeKind::from_type(ty).is_some();
            optional |= ty.is_optional();
        }
        // 转为 Basic Union 后仅由 nil 决定可选性
        !all_basic && optional
    }
}

impl PartialEq for LuaMultiLineUnion {
    fn eq(&self, other: &Self) -> bool {
        if self
            .hash
            .get()
            .zip(other.hash.get())
            .is_some_and(|(left, right)| left != right)
        {
            return false;
        }
        self.unions == other.unions
    }
}

impl Eq for LuaMultiLineUnion {}

impl Hash for LuaMultiLineUnion {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash
            .get_or_init(|| cached_type_hash(&self.unions))
            .hash(state);
    }
}

#[derive(Debug, Clone)]
pub struct LuaArrayType {
    base: LuaType,
    len: LuaArrayLen,
    hash: OnceCell<NonZeroU64>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum LuaArrayLen {
    None,
    Max(i64),
    Min(i64),
}

impl LuaArrayType {
    pub fn new(base: LuaType, len: LuaArrayLen) -> Self {
        Self {
            base,
            len,
            hash: OnceCell::new(),
        }
    }

    pub fn from_base_type(base: LuaType) -> Self {
        Self::new(base, LuaArrayLen::None)
    }

    pub fn get_base(&self) -> &LuaType {
        &self.base
    }

    pub fn get_len(&self) -> &LuaArrayLen {
        &self.len
    }

    pub fn contain_tpl(&self) -> bool {
        self.contain_tpl_children()
    }
}

impl PartialEq for LuaArrayType {
    fn eq(&self, other: &Self) -> bool {
        if self
            .hash
            .get()
            .zip(other.hash.get())
            .is_some_and(|(left, right)| left != right)
        {
            return false;
        }
        self.base == other.base && self.len == other.len
    }
}

impl Eq for LuaArrayType {}

impl Hash for LuaArrayType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash
            .get_or_init(|| cached_type_hash((&self.base, &self.len)))
            .hash(state);
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaConditionalType {
    checked_type: LuaType,
    extends_type: LuaType,
    true_type: LuaType,
    false_type: LuaType,
    infer_params: Vec<GenericParam>,
    pub has_new: bool,
}

impl LuaConditionalType {
    pub fn new(
        checked_type: LuaType,
        extends_type: LuaType,
        true_type: LuaType,
        false_type: LuaType,
        infer_params: Vec<GenericParam>,
        has_new: bool,
    ) -> Self {
        Self {
            checked_type,
            extends_type,
            true_type,
            false_type,
            infer_params,
            has_new,
        }
    }

    pub fn get_checked_type(&self) -> &LuaType {
        &self.checked_type
    }

    pub fn get_extends_type(&self) -> &LuaType {
        &self.extends_type
    }

    pub fn get_true_type(&self) -> &LuaType {
        &self.true_type
    }

    pub fn get_false_type(&self) -> &LuaType {
        &self.false_type
    }

    pub fn get_infer_params(&self) -> &[GenericParam] {
        &self.infer_params
    }

    pub fn contain_tpl(&self) -> bool {
        self.contain_tpl_children()
    }
}

impl From<LuaConditionalType> for LuaType {
    fn from(t: LuaConditionalType) -> Self {
        LuaType::Conditional(Rc::new(t))
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaMappedType {
    pub param: (GenericTplId, GenericParam),
    pub value: LuaType,
    pub is_readonly: bool,
    pub is_optional: bool,
}

impl LuaMappedType {
    pub fn new(
        param: (GenericTplId, GenericParam),
        value: LuaType,
        is_readonly: bool,
        is_optional: bool,
    ) -> Self {
        Self {
            param,
            value,
            is_readonly,
            is_optional,
        }
    }

    pub fn contain_tpl(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use std::{hash::BuildHasher, ptr};

    use rustc_hash::FxBuildHasher;

    use crate::{LuaArrayType, LuaMultiLineUnion, LuaObjectType, LuaType, LuaUnionType};

    #[test]
    fn array_types_keep_structural_identity_after_hashing() {
        let array = LuaArrayType::from_base_type(LuaType::String);
        let equal = LuaArrayType::from_base_type(LuaType::String);

        let hash = FxBuildHasher.hash_one(&array);
        assert_eq!(array, equal);
        assert_eq!(FxBuildHasher.hash_one(&equal), hash);
    }

    #[test]
    fn test_union_iterator() {
        assert_eq!(LuaUnionType::from_vec(vec![]).iter().next(), None);

        let basic = LuaUnionType::from_vec(vec![LuaType::Never, LuaType::Nil, LuaType::String]);
        let mut iter = basic.iter();
        assert_eq!(iter.len(), 3);
        assert_eq!(iter.next(), Some(&LuaType::Nil));
        assert_eq!(iter.next_back(), Some(&LuaType::Never));
        assert_eq!(iter.len(), 1);
        assert_eq!(iter.next(), Some(&LuaType::String));
        assert_eq!(iter.size_hint(), (0, Some(0)));
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next_back(), None);

        let object: LuaType = LuaObjectType::new(vec![]).into();
        for (union, last) in [
            (LuaUnionType::Nullable(object.clone()), LuaType::Nil),
            (
                LuaUnionType::from_vec(vec![object, LuaType::IntegerConst(1)]),
                LuaType::IntegerConst(1),
            ),
        ] {
            let first = match &union {
                LuaUnionType::Nullable(ty) => ty,
                LuaUnionType::Multi(members) => &members.get_types()[0],
                _ => unreachable!(),
            };
            let mut iter = union.iter();
            assert_eq!(iter.len(), 2);
            assert!(ptr::eq(iter.next().unwrap(), first));
            assert_eq!(iter.next_back(), Some(&last));
            assert_eq!(iter.len(), 0);
            assert_eq!(iter.next(), None);
        }
    }

    #[test]
    fn test_multiline_union_optional() {
        for (members, expected) in [
            (vec![], false),
            (vec![LuaType::Any], false),
            (vec![LuaType::Any, LuaType::IntegerConst(1)], true),
            (vec![LuaType::Nil], true),
        ] {
            let union = LuaMultiLineUnion::new(members.into_iter().map(|ty| (ty, None)).collect());
            assert_eq!(
                LuaType::MultiLineUnion(union.into()).is_optional(),
                expected
            );
        }
    }
}
