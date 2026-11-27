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

    /// Parameter slots, including the receiver and any expanding or repeated tail.
    pub fn get_variadic_params(&self) -> VariadicType {
        let mut types = Vec::with_capacity(self.params.len() + usize::from(self.is_colon_define));
        if self.is_colon_define {
            types.push(LuaType::SelfInfer);
        }
        for (index, (name, ty)) in self.params.iter().enumerate() {
            let ty = ty.clone().unwrap_or(LuaType::Unknown);
            let is_last = index + 1 == self.params.len();
            let is_vararg_tail = is_last && self.is_variadic && name == "...";

            // Multi expands only its tail. A bound prefix pack must supply its
            // own slots so that parameters after it keep their positions.
            if !is_last
                && let LuaType::Variadic(pack) = &ty
                && pack.get_max_len().is_some()
            {
                pack.append_fixed_slots(&mut types);
            } else if is_vararg_tail && !matches!(ty, LuaType::Variadic(_)) {
                types.push(LuaType::Variadic(Rc::new(VariadicType::Repeat(ty))));
            } else {
                types.push(ty);
            }
        }
        VariadicType::Multi(types)
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

/// A canonical union type.
///
/// The representation is an implementation detail. Callers must build unions
/// through [`LuaType::union`] / [`LuaType::optional`], which guarantee:
///
/// - nested unions are flattened and `Never` members are removed;
/// - members are deduplicated;
/// - an empty union is `Never`, and a single member is that member itself;
/// - an all-basic member set is stored as [`BasicTypeUnion`];
/// - exactly `{T, nil}` is stored as `Nullable(T)`;
/// - every other member set is stored as [`LuaUnionMembers`] (length >= 2).
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaUnionType {
    repr: LuaUnionRepr,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
enum LuaUnionRepr {
    Basic(BasicTypeUnion),
    Nullable(LuaType),
    Members(LuaUnionMembers),
}

/// 只读的联合类型成员, 首次求 hash 时缓存结果.
#[derive(Debug, Clone)]
pub struct LuaUnionMembers {
    types: Vec<LuaType>,
    hash: OnceCell<NonZeroU64>,
}

impl LuaUnionMembers {
    pub(crate) fn new(types: Vec<LuaType>) -> Self {
        Self {
            types,
            hash: OnceCell::new(),
        }
    }

    pub fn get_types(&self) -> &[LuaType] {
        &self.types
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}

impl PartialEq for LuaUnionMembers {
    fn eq(&self, other: &Self) -> bool {
        if self.types.len() != other.types.len() {
            return false;
        }
        if self
            .hash
            .get()
            .zip(other.hash.get())
            .is_some_and(|(left, right)| left != right)
        {
            return false;
        }

        // Unions are small in practice. A linear membership check avoids the
        // allocation that a per-comparison `HashSet` would perform; only large
        // unions pay for the hash set.
        if self.types.len() <= 8 {
            self.types
                .iter()
                .all(|ty| other.types.iter().any(|other_ty| other_ty == ty))
        } else {
            let mut members: HashSet<LuaType> = self.types.iter().cloned().collect();
            other.types.iter().all(|ty| members.remove(ty))
        }
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
    fn basic(basic: BasicTypeUnion) -> Self {
        debug_assert!(!basic.is_empty(), "a Basic union must not be empty");
        debug_assert!(
            basic.iter().nth(1).is_some(),
            "a single basic type must collapse to that type"
        );
        Self {
            repr: LuaUnionRepr::Basic(basic),
        }
    }

    fn nullable(ty: LuaType) -> Self {
        debug_assert!(!ty.is_nil(), "Nullable(nil) must collapse to Nil");
        debug_assert!(!ty.is_never(), "Never must be absorbed by the union");
        debug_assert!(
            !ty.is_union(),
            "a nested union must be flattened before being stored"
        );
        debug_assert!(
            BasicTypeKind::from_type(&ty).is_none(),
            "an all-basic set must use the Basic representation"
        );
        Self {
            repr: LuaUnionRepr::Nullable(ty),
        }
    }

    fn members(members: Vec<LuaType>) -> Self {
        debug_assert!(members.len() >= 2, "a one-member set is not a union");
        debug_assert!(
            members.iter().all(|ty| !ty.is_never() && !ty.is_union()),
            "unions must be flat and must not contain Never"
        );
        debug_assert!(
            !(members.len() == 2 && members.iter().any(LuaType::is_nil)),
            "{{T, nil}} must use the Nullable representation"
        );
        debug_assert!(
            !members
                .iter()
                .all(|ty| BasicTypeKind::from_type(ty).is_some()),
            "an all-basic set must use the Basic representation"
        );
        debug_assert!(
            (1..members.len()).all(|index| !members[..index].contains(&members[index])),
            "union members must be deduplicated"
        );
        Self {
            repr: LuaUnionRepr::Members(LuaUnionMembers::new(members)),
        }
    }

    #[inline]
    pub fn iter(&self) -> UnionIter<'_> {
        match &self.repr {
            LuaUnionRepr::Basic(basic) => UnionIter::Basic(basic.iter()),
            LuaUnionRepr::Nullable(ty) => UnionIter::Nullable([ty, &LuaType::Nil].into_iter()),
            LuaUnionRepr::Members(types) => UnionIter::Multi(types.get_types().iter()),
        }
    }

    pub fn len(&self) -> usize {
        match &self.repr {
            LuaUnionRepr::Basic(basic) => basic.iter().len(),
            LuaUnionRepr::Nullable(_) => 2,
            LuaUnionRepr::Members(types) => types.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        // Canonical unions always contain at least two members.
        false
    }

    pub fn into_vec(&self) -> Vec<LuaType> {
        self.iter().cloned().collect()
    }

    #[allow(unused, clippy::wrong_self_convention)]
    pub(crate) fn into_set(&self) -> HashSet<LuaType> {
        self.iter().cloned().collect()
    }

    pub fn contain_tpl(&self) -> bool {
        match &self.repr {
            // Basic members are primitive types and cannot contain a template.
            LuaUnionRepr::Basic(_) => false,
            _ => self.iter().any(LuaType::contain_tpl),
        }
    }

    pub fn is_nullable(&self) -> bool {
        match &self.repr {
            LuaUnionRepr::Basic(basic) => basic.contains(BasicTypeKind::Nil),
            LuaUnionRepr::Nullable(_) => true,
            LuaUnionRepr::Members(_) => self.iter().any(LuaType::is_nullable),
        }
    }

    pub fn is_optional(&self) -> bool {
        match &self.repr {
            // `any` / `unknown` are optional as standalone types, but a union
            // of basic types is only optional through its nil member; keep the
            // long-standing Basic-union behavior.
            LuaUnionRepr::Basic(basic) => basic.contains(BasicTypeKind::Nil),
            _ => self.iter().any(LuaType::is_optional),
        }
    }

    pub fn is_always_truthy(&self) -> bool {
        self.iter().all(LuaType::is_always_truthy)
    }

    pub fn is_always_falsy(&self) -> bool {
        self.iter().all(LuaType::is_always_falsy)
    }
}

impl From<LuaUnionType> for LuaType {
    fn from(t: LuaUnionType) -> Self {
        LuaType::Union(t.into())
    }
}

impl LuaType {
    /// Build a canonical union from arbitrary member types.
    ///
    /// This is the only public way to construct a union. It flattens nested
    /// unions, removes `Never`, deduplicates members and collapses the empty
    /// and single-member cases. `Nil` is an ordinary member, not a wrapper.
    pub fn union(types: impl IntoIterator<Item = LuaType>) -> LuaType {
        Self::union_vec(types.into_iter().collect())
    }

    /// `Self::union` for callers that already own a `Vec`, reusing its buffer.
    pub(crate) fn union_vec(types: Vec<LuaType>) -> LuaType {
        if types.is_empty() {
            return LuaType::Never;
        }
        if types.len() == 1 {
            return types.into_iter().next().expect("len checked");
        }

        // Common case: `T | nil` / `string | integer`. No member buffer or
        // intermediate dedup structure is needed because the bitset already
        // makes the set unique.
        let mut basic = BasicTypeUnion::new();
        let mut all_basic = true;
        for ty in &types {
            match BasicTypeKind::from_type(ty) {
                Some(BasicTypeKind::Never) => {}
                Some(kind) => basic.add(kind),
                None => {
                    all_basic = false;
                    break;
                }
            }
        }
        if all_basic {
            return union_from_basic(basic);
        }

        let mut types = types;
        if types.iter().any(|ty| matches!(ty, LuaType::Union(_))) {
            // Reverse once so popping preserves the caller's member order.
            let mut stack: Vec<LuaType> = types.into_iter().rev().collect();
            let mut flattened = Vec::with_capacity(stack.len());
            while let Some(ty) = stack.pop() {
                match ty {
                    LuaType::Union(union) => stack.extend(union.iter().rev().cloned()),
                    LuaType::Never => {}
                    other => flattened.push(other),
                }
            }
            types = flattened;
        } else {
            types.retain(|ty| !ty.is_never());
        }

        dedup_union_members(&mut types);
        union_from_normalized(types)
    }

    /// `ty | nil`, returning `ty` unchanged when it is already nullable.
    pub fn optional(ty: LuaType) -> LuaType {
        if ty.is_nullable() {
            ty
        } else {
            Self::union_vec(vec![ty, LuaType::Nil])
        }
    }

    /// Iterate the union components. A non-union type yields itself once.
    pub fn components(&self) -> LuaTypeComponents<'_> {
        match self {
            LuaType::Union(union) => LuaTypeComponents::Union(union.iter()),
            other => LuaTypeComponents::One(Some(other)),
        }
    }

    /// Keep only the union components accepted by `keep` and rebuild a
    /// canonical union. Returns `self` unchanged when nothing is filtered, so
    /// no-op narrowing does not allocate.
    pub fn filter_components(&self, keep: impl Fn(&LuaType) -> bool) -> LuaType {
        match self {
            LuaType::Union(union) => {
                if union.iter().all(&keep) {
                    return self.clone();
                }
                let kept: Vec<LuaType> = union.iter().filter(|ty| keep(ty)).cloned().collect();
                LuaType::union_vec(kept)
            }
            other => {
                if keep(other) {
                    other.clone()
                } else {
                    LuaType::Never
                }
            }
        }
    }
}

fn union_from_basic(basic: BasicTypeUnion) -> LuaType {
    let mut iter = basic.iter();
    let Some(first) = iter.next() else {
        return LuaType::Never;
    };
    if iter.next().is_none() {
        return first.clone();
    }
    LuaType::Union(Rc::new(LuaUnionType::basic(basic)))
}

fn union_from_normalized(mut types: Vec<LuaType>) -> LuaType {
    if types.is_empty() {
        return LuaType::Never;
    }
    if types.len() == 1 {
        return types.pop().expect("len checked");
    }

    let mut basic = BasicTypeUnion::new();
    let mut all_basic = true;
    for ty in &types {
        match BasicTypeKind::from_type(ty) {
            Some(BasicTypeKind::Never) => {}
            Some(kind) => basic.add(kind),
            None => {
                all_basic = false;
                break;
            }
        }
    }
    if all_basic {
        return union_from_basic(basic);
    }

    if types.len() == 2 && types.iter().any(LuaType::is_nil) {
        let other = types
            .into_iter()
            .find(|ty| !ty.is_nil())
            .expect("the other member is not nil");
        return LuaType::Union(Rc::new(LuaUnionType::nullable(other)));
    }

    LuaType::Union(Rc::new(LuaUnionType::members(types)))
}

fn dedup_union_members(types: &mut Vec<LuaType>) {
    const SMALL_UNION: usize = 16;
    if types.len() <= SMALL_UNION {
        let mut index = 1;
        while index < types.len() {
            if types[..index].contains(&types[index]) {
                types.remove(index);
            } else {
                index += 1;
            }
        }
    } else {
        let mut seen = HashSet::with_capacity(types.len());
        let mut deduped = Vec::with_capacity(types.len());
        for ty in types.drain(..) {
            if seen.insert(ty.clone()) {
                deduped.push(ty);
            }
        }
        *types = deduped;
    }
}

/// Iterator over the components of a [`LuaType`]; non-unions yield themselves.
#[derive(Debug, Clone)]
pub enum LuaTypeComponents<'a> {
    One(Option<&'a LuaType>),
    Union(UnionIter<'a>),
}

impl<'a> Iterator for LuaTypeComponents<'a> {
    type Item = &'a LuaType;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            LuaTypeComponents::One(ty) => ty.take(),
            LuaTypeComponents::Union(iter) => iter.next(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            LuaTypeComponents::One(Some(_)) => (1, Some(1)),
            LuaTypeComponents::One(None) => (0, Some(0)),
            LuaTypeComponents::Union(iter) => iter.size_hint(),
        }
    }
}

impl DoubleEndedIterator for LuaTypeComponents<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            LuaTypeComponents::One(ty) => ty.take(),
            LuaTypeComponents::Union(iter) => iter.next_back(),
        }
    }
}

impl ExactSizeIterator for LuaTypeComponents<'_> {}
impl FusedIterator for LuaTypeComponents<'_> {}

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
    /// Append the slots of a pack whose length is known.
    fn append_fixed_slots(&self, slots: &mut Vec<LuaType>) {
        match self {
            Self::Base(LuaType::Tuple(tuple)) => slots.extend_from_slice(tuple.get_types()),
            Self::Multi(types) => {
                for ty in types {
                    if let LuaType::Variadic(pack) = ty {
                        pack.append_fixed_slots(slots);
                    } else {
                        slots.push(ty.clone());
                    }
                }
            }
            _ => unreachable!("a fixed pack must have a known sequence of slots"),
        }
    }

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
    /// Parameter of a named class/type definition, scoped by its declaration location.
    ClassType {
        file_id: u32,
        start: u32,
        index: u32,
    },
    /// Syntax position keeps nested mapped keys distinct from other generics.
    Mapped(u32),
    ConditionalInfer(u32),
}

impl GenericTplId {
    pub fn get_idx(&self) -> usize {
        match self {
            GenericTplId::Type(idx) => *idx as usize,
            GenericTplId::Func(idx) => *idx as usize,
            GenericTplId::ClassType { index, .. } => *index as usize,
            GenericTplId::Mapped(idx) => *idx as usize,
            GenericTplId::ConditionalInfer(idx) => *idx as usize,
        }
    }

    pub fn is_func(&self) -> bool {
        matches!(self, GenericTplId::Func(_))
    }

    pub fn is_type(&self) -> bool {
        matches!(self, GenericTplId::Type(_) | GenericTplId::ClassType { .. })
    }

    pub fn is_conditional_infer(&self) -> bool {
        matches!(self, GenericTplId::ConditionalInfer(_))
    }

    pub fn with_idx(&self, idx: u32) -> Self {
        match self {
            GenericTplId::Type(_) => GenericTplId::Type(idx),
            GenericTplId::Func(_) => GenericTplId::Func(idx),
            GenericTplId::ClassType {
                file_id, start, ..
            } => GenericTplId::ClassType {
                file_id: *file_id,
                start: *start,
                index: idx,
            },
            GenericTplId::Mapped(_) => GenericTplId::Mapped(idx),
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
        LuaType::union(self.iter().cloned())
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
    use std::hash::BuildHasher;

    use rustc_hash::FxBuildHasher;

    use crate::{LuaArrayType, LuaMultiLineUnion, LuaObjectType, LuaType};

    fn array(base: LuaType) -> LuaType {
        LuaType::Array(LuaArrayType::from_base_type(base).into())
    }

    #[test]
    fn array_types_keep_structural_identity_after_hashing() {
        let array = LuaArrayType::from_base_type(LuaType::String);
        let equal = LuaArrayType::from_base_type(LuaType::String);

        let hash = FxBuildHasher.hash_one(&array);
        assert_eq!(array, equal);
        assert_eq!(FxBuildHasher.hash_one(&equal), hash);
    }

    #[test]
    fn union_is_canonical() {
        assert_eq!(LuaType::union(Vec::new()), LuaType::Never);
        assert_eq!(LuaType::union(vec![LuaType::String]), LuaType::String);
        assert_eq!(
            LuaType::union(vec![LuaType::String, LuaType::String]),
            LuaType::String
        );
        assert_eq!(
            LuaType::union(vec![LuaType::Never, LuaType::String]),
            LuaType::String
        );
        assert_eq!(LuaType::union(vec![LuaType::Nil]), LuaType::Nil);

        let object: LuaType = LuaObjectType::new(vec![]).into();
        assert_eq!(LuaType::union(vec![object.clone(), object.clone()]), object);
    }

    #[test]
    fn union_flattens_nested_unions() {
        let first = array(LuaType::String);
        let second = array(LuaType::Number);
        let third = array(LuaType::Boolean);
        let inner = LuaType::union(vec![first.clone(), second.clone()]);
        let outer = LuaType::union(vec![inner, third.clone()]);

        let LuaType::Union(union) = &outer else {
            panic!("expected a union");
        };
        assert_eq!(union.len(), 3);
        assert!(outer.components().all(|ty| !ty.is_union()));
        assert!(outer.components().any(|ty| ty == &first));
        assert!(outer.components().any(|ty| ty == &second));
        assert!(outer.components().any(|ty| ty == &third));
    }

    #[test]
    fn union_equality_and_hash_ignore_member_order() {
        let first = array(LuaType::String);
        let second = array(LuaType::Number);
        let left = LuaType::union(vec![first.clone(), second.clone()]);
        let right = LuaType::union(vec![second, first]);

        assert_eq!(left, right);
        assert_eq!(
            FxBuildHasher.hash_one(&left),
            FxBuildHasher.hash_one(&right)
        );
    }

    #[test]
    fn union_filter_components_keeps_canonical_form() {
        let first = array(LuaType::String);
        let second = array(LuaType::Number);
        let union = LuaType::union(vec![first.clone(), second.clone(), LuaType::Nil]);

        let unchanged = union.filter_components(|_| true);
        assert_eq!(unchanged, union);

        let non_nil = union.filter_components(|ty| !ty.is_nil());
        assert!(!non_nil.is_nullable());
        assert!(non_nil.components().any(|ty| ty == &first));
        assert!(non_nil.components().any(|ty| ty == &second));

        assert_eq!(union.filter_components(|ty| ty.is_nil()), LuaType::Nil);
        assert_eq!(
            LuaType::union(vec![LuaType::String, LuaType::Nil]),
            LuaType::optional(LuaType::String)
        );
    }

    #[test]
    fn test_union_iterator() {
        let basic = LuaType::union(vec![LuaType::Never, LuaType::Nil, LuaType::String]);
        let LuaType::Union(basic) = &basic else {
            panic!("expected a union");
        };
        let mut iter = basic.iter();
        assert_eq!(iter.len(), 2);
        assert_eq!(iter.next(), Some(&LuaType::Nil));
        assert_eq!(iter.next_back(), Some(&LuaType::String));
        assert_eq!(iter.len(), 0);
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next_back(), None);

        let object: LuaType = LuaObjectType::new(vec![]).into();
        for (union, last) in [
            (LuaType::optional(object.clone()), LuaType::Nil),
            (
                LuaType::union(vec![object, LuaType::IntegerConst(1)]),
                LuaType::IntegerConst(1),
            ),
        ] {
            let LuaType::Union(union) = &union else {
                panic!("expected a union");
            };
            let first = union.iter().next().expect("first member").clone();
            let mut iter = union.iter();
            assert_eq!(iter.len(), 2);
            assert_eq!(iter.next(), Some(&first));
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
