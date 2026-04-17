use hashbrown::{HashMap, HashSet};
use internment::ArcIntern;
use rowan::TextRange;
use smol_str::SmolStr;
use std::{ops::Deref, sync::Arc};

use crate::db_index::LuaMemberKey;
use crate::{AsyncState, DbIndex, InFiled, SemanticModel, first_param_may_not_self};

use super::super::basic_union::{BasicTypeKind, BasicTypeUnion};
use super::super::generic_param::GenericParam;
use super::super::type_decl::LuaTypeDeclId;
use super::super::type_ops::TypeOps;
use super::LuaTypeNode;
use super::lua_type::LuaType;

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

    pub fn cast_down_array_base(&self, db: &DbIndex) -> LuaType {
        let mut ty = LuaType::Never;
        for t in &self.types {
            match t {
                LuaType::IntegerConst(i) => {
                    ty = TypeOps::Union.apply(db, &ty, &LuaType::DocIntegerConst(*i));
                }
                LuaType::FloatConst(_) => {
                    ty = TypeOps::Union.apply(db, &ty, &LuaType::Number);
                }
                LuaType::StringConst(s) => {
                    ty = TypeOps::Union.apply(db, &ty, &LuaType::DocStringConst(s.clone()));
                }
                _ => {
                    ty = TypeOps::Union.apply(db, &ty, t);
                }
            }
        }

        if self.types.is_empty() {
            LuaType::Unknown
        } else {
            ty
        }
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
    ) -> Self {
        Self {
            async_state,
            is_colon_define,
            is_variadic,
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

    pub fn is_method(&self, semantic_model: &SemanticModel, owner_type: Option<&LuaType>) -> bool {
        if self.is_colon_define {
            return true;
        }
        if let Some((name, t)) = self.params.first() {
            match t {
                Some(t) => {
                    if t.is_self_infer() {
                        return true;
                    }
                    match owner_type {
                        Some(owner_type) => {
                            if matches!(owner_type, LuaType::Ref(_) | LuaType::Def(_))
                                && first_param_may_not_self(t)
                            {
                                return false;
                            }
                            if semantic_model.type_check(owner_type, t).is_ok() {
                                return true;
                            }
                            name == "self" && semantic_model.type_check(t, owner_type).is_ok()
                        }
                        None => name == "self",
                    }
                }
                None => name == "self",
            }
        } else {
            false
        }
    }

    pub fn to_call_operator_func_type(&self) -> Arc<LuaFunctionType> {
        let mut params = self.get_params().to_vec();
        if params.first().is_some_and(|(name, _)| name == "@call_self") {
            params.remove(0);
        }

        Arc::new(LuaFunctionType::new(
            self.async_state,
            false,
            self.is_variadic,
            params,
            self.ret.clone(),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaObjectType {
    fields: HashMap<LuaMemberKey, LuaType>,
    index_access: Vec<(LuaType, LuaType)>,
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
                    fields.insert(LuaMemberKey::Name(s.clone()), value_type.clone());
                }
                LuaIndexAccessKey::Type(t) => {
                    index_access.push((t, value_type));
                }
            }
        }

        Self {
            fields,
            index_access,
        }
    }

    pub fn new_with_fields(
        fields: HashMap<LuaMemberKey, LuaType>,
        index_access: Vec<(LuaType, LuaType)>,
    ) -> Self {
        Self {
            fields,
            index_access,
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

    pub fn cast_down_array_base(&self, db: &DbIndex) -> Option<LuaType> {
        if !self.index_access.is_empty() {
            let mut ty = None;
            for (key, value_type) in self.index_access.iter() {
                if matches!(key, LuaType::Integer) {
                    ty = Some(match ty {
                        Some(t) => TypeOps::Union.apply(db, &t, value_type),
                        None => value_type.clone(),
                    });
                }
            }
            return ty;
        }

        let mut ty = None;
        let mut fields = self.fields.iter().collect::<Vec<_>>();
        fields.sort_by_key(|(a, _)| *a);

        for (count, (key, value_type)) in (1..).zip(fields) {
            let idx = match key {
                LuaMemberKey::Integer(i) => i,
                _ => return None,
            };

            if *idx != count {
                return None;
            }

            ty = Some(match ty {
                Some(t) => TypeOps::Union.apply(db, &t, value_type),
                None => value_type.clone(),
            });
        }

        Some(ty.unwrap_or(LuaType::Unknown))
    }
}

impl From<LuaObjectType> for LuaType {
    fn from(t: LuaObjectType) -> Self {
        LuaType::Object(t.into())
    }
}

#[derive(Debug, Clone, Eq)]
pub enum LuaUnionType {
    Basic(BasicTypeUnion),
    Nullable(LuaType),
    Multi(Vec<LuaType>),
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
            Self::Multi(set.into_iter().collect())
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
                return Self::Multi(types);
            }
        }
        Self::Multi(types)
    }

    pub fn into_vec(&self) -> Vec<LuaType> {
        match self {
            LuaUnionType::Basic(basic) => basic.iter().collect(),
            LuaUnionType::Nullable(ty) => vec![ty.clone(), LuaType::Nil],
            LuaUnionType::Multi(types) => types.clone(),
        }
    }

    #[allow(unused, clippy::wrong_self_convention)]
    pub(crate) fn into_set(&self) -> HashSet<LuaType> {
        match self {
            LuaUnionType::Basic(basic) => basic.iter().collect(),
            LuaUnionType::Nullable(ty) => {
                let mut set = HashSet::new();
                set.insert(ty.clone());
                set.insert(LuaType::Nil);
                set
            }
            LuaUnionType::Multi(types) => types.clone().into_iter().collect(),
        }
    }

    pub fn contain_tpl(&self) -> bool {
        self.contain_tpl_children()
    }

    pub fn is_nullable(&self) -> bool {
        match self {
            LuaUnionType::Basic(basic) => basic.contains(BasicTypeKind::Nil),
            LuaUnionType::Nullable(_) => true,
            LuaUnionType::Multi(types) => types.iter().any(|t| t.is_nullable()),
        }
    }

    pub fn is_optional(&self) -> bool {
        match self {
            LuaUnionType::Basic(basic) => basic.contains(BasicTypeKind::Nil),
            LuaUnionType::Nullable(_) => true,
            LuaUnionType::Multi(types) => types.iter().any(|t| t.is_optional()),
        }
    }

    pub fn is_always_truthy(&self) -> bool {
        match self {
            LuaUnionType::Basic(basic) => basic.iter().all(|t| t.is_always_truthy()),
            LuaUnionType::Nullable(_) => false,
            LuaUnionType::Multi(types) => types.iter().all(|t| t.is_always_truthy()),
        }
    }

    pub fn is_always_falsy(&self) -> bool {
        match self {
            LuaUnionType::Basic(basic) => basic.iter().all(|t| t.is_always_falsy()),
            LuaUnionType::Nullable(f) => f.is_always_falsy(),
            LuaUnionType::Multi(types) => types.iter().all(|t| t.is_always_falsy()),
        }
    }
}

impl From<LuaUnionType> for LuaType {
    fn from(t: LuaUnionType) -> Self {
        LuaType::Union(t.into())
    }
}

impl PartialEq for LuaUnionType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (LuaUnionType::Basic(a), LuaUnionType::Basic(b)) => a == b,
            (LuaUnionType::Nullable(a), LuaUnionType::Nullable(b)) => a == b,
            (LuaUnionType::Multi(a), LuaUnionType::Multi(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                let mut a_set: HashSet<_> = a.iter().collect();
                for item in b {
                    if !a_set.remove(item) {
                        return false;
                    }
                }
                a_set.is_empty()
            }
            _ => false,
        }
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

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn into_types(&self) -> Vec<LuaType> {
        self.types.clone()
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
    Multi(Vec<LuaType>),
    Base(LuaType),
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
            VariadicType::Base(t) => Some(t),
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
                    if let LuaType::Variadic(multi) = &types[last] {
                        let rest_offset = idx - last;
                        return multi.get_new_variadic_from(rest_offset);
                    }
                }

                VariadicType::Multi(new_types)
            }
            VariadicType::Base(t) => VariadicType::Base(t.clone()),
        }
    }

    pub fn contain_tpl(&self) -> bool {
        self.contain_tpl_children()
    }

    pub fn get_min_len(&self) -> Option<usize> {
        match self {
            VariadicType::Base(_) => None,
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
            VariadicType::Base(_) => None,
            VariadicType::Multi(types) => {
                let mut total_len = 0;
                for t in types {
                    if let LuaType::Variadic(variadic) = t {
                        let len = variadic.get_max_len()?;
                        total_len += len;
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
}

impl GenericTplId {
    pub fn get_idx(&self) -> usize {
        match self {
            GenericTplId::Type(idx) => *idx as usize,
            GenericTplId::Func(idx) => *idx as usize,
        }
    }

    pub fn is_func(&self) -> bool {
        matches!(self, GenericTplId::Func(_))
    }

    pub fn is_type(&self) -> bool {
        matches!(self, GenericTplId::Type(_))
    }

    pub fn with_idx(&self, idx: u32) -> Self {
        match self {
            GenericTplId::Type(_) => GenericTplId::Type(idx),
            GenericTplId::Func(_) => GenericTplId::Func(idx),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericTpl {
    tpl_id: GenericTplId,
    name: ArcIntern<SmolStr>,
    constraint: Option<LuaType>,
}

impl GenericTpl {
    pub fn new(
        tpl_id: GenericTplId,
        name: ArcIntern<SmolStr>,
        constraint: Option<LuaType>,
    ) -> Self {
        Self {
            tpl_id,
            name,
            constraint,
        }
    }

    pub fn get_tpl_id(&self) -> GenericTplId {
        self.tpl_id
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_constraint(&self) -> Option<&LuaType> {
        self.constraint.as_ref()
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

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaMultiLineUnion {
    unions: Vec<(LuaType, Option<String>)>,
}

impl LuaMultiLineUnion {
    pub fn new(unions: Vec<(LuaType, Option<String>)>) -> Self {
        Self { unions }
    }

    pub fn get_unions(&self) -> &[(LuaType, Option<String>)] {
        &self.unions
    }

    pub fn to_union(&self) -> LuaType {
        let mut types = Vec::new();
        for (t, _) in &self.unions {
            types.push(t.clone());
        }

        LuaType::Union(Arc::new(LuaUnionType::from_vec(types)))
    }

    pub fn contain_tpl(&self) -> bool {
        self.contain_tpl_children()
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaArrayType {
    base: LuaType,
    len: LuaArrayLen,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum LuaArrayLen {
    None,
    Max(i64),
}

impl LuaArrayType {
    pub fn new(base: LuaType, len: LuaArrayLen) -> Self {
        Self { base, len }
    }

    pub fn from_base_type(base: LuaType) -> Self {
        Self {
            base,
            len: LuaArrayLen::None,
        }
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

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaAttributeType {
    params: Vec<(String, Option<LuaType>)>,
}

impl LuaAttributeType {
    pub fn new(params: Vec<(String, Option<LuaType>)>) -> Self {
        Self { params }
    }

    pub fn get_params(&self) -> &[(String, Option<LuaType>)] {
        &self.params
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaConditionalType {
    condition: LuaType,
    true_type: LuaType,
    false_type: LuaType,
    infer_params: Vec<GenericParam>,
    pub has_new: bool,
}

impl LuaConditionalType {
    pub fn new(
        condition: LuaType,
        true_type: LuaType,
        false_type: LuaType,
        infer_params: Vec<GenericParam>,
        has_new: bool,
    ) -> Self {
        Self {
            condition,
            true_type,
            false_type,
            infer_params,
            has_new,
        }
    }

    pub fn get_condition(&self) -> &LuaType {
        &self.condition
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
        LuaType::Conditional(Arc::new(t))
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
