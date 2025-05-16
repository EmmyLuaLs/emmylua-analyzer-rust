use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    ops::Deref,
    sync::Arc,
};

use internment::ArcIntern;
use rowan::TextRange;
use smol_str::SmolStr;

use crate::{
    db_index::{LuaMemberKey, LuaSignatureId},
    DbIndex, InFiled,
};

use super::{type_decl::LuaTypeDeclId, TypeOps};

#[derive(Debug, Clone)]
pub enum LuaType {
    Unknown,
    Any,
    Nil,
    Table,
    Userdata,
    Function,
    Thread,
    Boolean,
    String,
    Integer,
    Number,
    Io,
    SelfInfer,
    Global,
    BooleanConst(bool),
    StringConst(ArcIntern<SmolStr>),
    IntegerConst(i64),
    FloatConst(f64),
    TableConst(InFiled<TextRange>),
    Ref(LuaTypeDeclId),
    Def(LuaTypeDeclId),
    Array(Arc<LuaType>),
    Tuple(Arc<LuaTupleType>),
    DocFunction(Arc<LuaFunctionType>),
    Object(Arc<LuaObjectType>),
    Union(Arc<LuaUnionType>),
    Intersection(Arc<LuaIntersectionType>),
    Generic(Arc<LuaGenericType>),
    TableGeneric(Arc<Vec<LuaType>>),
    TplRef(Arc<GenericTpl>),
    StrTplRef(Arc<LuaStringTplType>),
    Variadic(Arc<VariadicType>),
    Signature(LuaSignatureId),
    Instance(Arc<LuaInstanceType>),
    DocStringConst(ArcIntern<SmolStr>),
    DocIntegerConst(i64),
    DocBooleanConst(bool),
    Namespace(ArcIntern<SmolStr>),
    Call(Arc<LuaAliasCallType>),
    MultiLineUnion(Arc<LuaMultiLineUnion>),
}

impl PartialEq for LuaType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (LuaType::Unknown, LuaType::Unknown) => true,
            (LuaType::Any, LuaType::Any) => true,
            (LuaType::Nil, LuaType::Nil) => true,
            (LuaType::Table, LuaType::Table) => true,
            (LuaType::Userdata, LuaType::Userdata) => true,
            (LuaType::Function, LuaType::Function) => true,
            (LuaType::Thread, LuaType::Thread) => true,
            (LuaType::Boolean, LuaType::Boolean) => true,
            (LuaType::String, LuaType::String) => true,
            (LuaType::Integer, LuaType::Integer) => true,
            (LuaType::Number, LuaType::Number) => true,
            (LuaType::Io, LuaType::Io) => true,
            (LuaType::SelfInfer, LuaType::SelfInfer) => true,
            (LuaType::Global, LuaType::Global) => true,
            (LuaType::BooleanConst(a), LuaType::BooleanConst(b)) => a == b,
            (LuaType::StringConst(a), LuaType::StringConst(b)) => a == b,
            (LuaType::IntegerConst(a), LuaType::IntegerConst(b)) => a == b,
            (LuaType::FloatConst(a), LuaType::FloatConst(b)) => a == b,
            (LuaType::TableConst(a), LuaType::TableConst(b)) => a == b,
            (LuaType::Ref(a), LuaType::Ref(b)) => a == b,
            (LuaType::Def(a), LuaType::Def(b)) => a == b,
            (LuaType::Array(a), LuaType::Array(b)) => a == b,
            (LuaType::Call(a), LuaType::Call(b)) => a == b,
            (LuaType::Tuple(a), LuaType::Tuple(b)) => a == b,
            (LuaType::DocFunction(a), LuaType::DocFunction(b)) => a == b,
            (LuaType::Object(a), LuaType::Object(b)) => a == b,
            (LuaType::Union(a), LuaType::Union(b)) => a == b,
            (LuaType::Intersection(a), LuaType::Intersection(b)) => a == b,
            (LuaType::Generic(a), LuaType::Generic(b)) => a == b,
            (LuaType::TableGeneric(a), LuaType::TableGeneric(b)) => a == b,
            (LuaType::TplRef(a), LuaType::TplRef(b)) => a == b,
            (LuaType::StrTplRef(a), LuaType::StrTplRef(b)) => a == b,
            (LuaType::Variadic(a), LuaType::Variadic(b)) => a == b,
            (LuaType::DocBooleanConst(a), LuaType::DocBooleanConst(b)) => a == b,
            (LuaType::Signature(a), LuaType::Signature(b)) => a == b,
            (LuaType::Instance(a), LuaType::Instance(b)) => a == b,
            (LuaType::DocStringConst(a), LuaType::DocStringConst(b)) => a == b,
            (LuaType::DocIntegerConst(a), LuaType::DocIntegerConst(b)) => a == b,
            (LuaType::Namespace(a), LuaType::Namespace(b)) => a == b,
            (LuaType::MultiLineUnion(a), LuaType::MultiLineUnion(b)) => a == b,
            _ => false, // 不同变体之间不相等
        }
    }
}

impl Eq for LuaType {}

impl Hash for LuaType {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            LuaType::Unknown => 0.hash(state),
            LuaType::Any => 1.hash(state),
            LuaType::Nil => 2.hash(state),
            LuaType::Table => 3.hash(state),
            LuaType::Userdata => 4.hash(state),
            LuaType::Function => 5.hash(state),
            LuaType::Thread => 6.hash(state),
            LuaType::Boolean => 7.hash(state),
            LuaType::String => 8.hash(state),
            LuaType::Integer => 9.hash(state),
            LuaType::Number => 10.hash(state),
            LuaType::Io => 11.hash(state),
            LuaType::SelfInfer => 12.hash(state),
            LuaType::Global => 13.hash(state),
            LuaType::BooleanConst(a) => (14, a).hash(state),
            LuaType::StringConst(a) => (15, a).hash(state),
            LuaType::IntegerConst(a) => (16, a).hash(state),
            LuaType::FloatConst(a) => (17, a.to_bits()).hash(state),
            LuaType::TableConst(a) => (18, a).hash(state),
            LuaType::Ref(a) => (19, a).hash(state),
            LuaType::Def(a) => (20, a).hash(state),
            LuaType::Array(a) => (22, a).hash(state),
            LuaType::Call(a) => (23, a).hash(state),
            LuaType::Tuple(a) => (25, a).hash(state),
            LuaType::DocFunction(a) => (26, a).hash(state),
            LuaType::Object(a) => {
                let ptr = Arc::as_ptr(a);
                (27, ptr).hash(state)
            }
            LuaType::Union(a) => {
                let ptr = Arc::as_ptr(a);
                (28, ptr).hash(state)
            }
            LuaType::Intersection(a) => {
                let ptr = Arc::as_ptr(a);
                (29, ptr).hash(state)
            }
            LuaType::Generic(a) => {
                let ptr = Arc::as_ptr(a);
                (30, ptr).hash(state)
            }
            LuaType::TableGeneric(a) => {
                let ptr = Arc::as_ptr(a);
                (31, ptr).hash(state)
            }
            LuaType::TplRef(a) => (32, a).hash(state),
            LuaType::StrTplRef(a) => (33, a).hash(state),
            LuaType::Variadic(a) => {
                let ptr = Arc::as_ptr(a);
                (34, ptr).hash(state)
            }
            LuaType::DocBooleanConst(a) => (35, a).hash(state),
            LuaType::Signature(a) => (36, a).hash(state),
            LuaType::Instance(a) => (37, a).hash(state),
            LuaType::DocStringConst(a) => (38, a).hash(state),
            LuaType::DocIntegerConst(a) => (39, a).hash(state),
            LuaType::Namespace(a) => (40, a).hash(state),
            LuaType::MultiLineUnion(a) => {
                let ptr = Arc::as_ptr(a);
                (43, ptr).hash(state)
            }
        }
    }
}

#[allow(unused)]
impl LuaType {
    pub fn is_unknown(&self) -> bool {
        matches!(self, LuaType::Unknown)
    }

    pub fn is_nil(&self) -> bool {
        matches!(self, LuaType::Nil)
    }

    pub fn is_table(&self) -> bool {
        matches!(
            self,
            LuaType::Table
                | LuaType::TableGeneric(_)
                | LuaType::TableConst(_)
                | LuaType::Global
                | LuaType::Tuple(_)
                | LuaType::Array(_)
        )
    }

    pub fn is_userdata(&self) -> bool {
        matches!(self, LuaType::Userdata)
    }

    pub fn is_thread(&self) -> bool {
        matches!(self, LuaType::Thread)
    }

    pub fn is_boolean(&self) -> bool {
        matches!(
            self,
            LuaType::BooleanConst(_) | LuaType::Boolean | LuaType::DocBooleanConst(_)
        )
    }

    pub fn is_string(&self) -> bool {
        matches!(
            self,
            LuaType::StringConst(_) | LuaType::String | LuaType::DocStringConst(_)
        )
    }

    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            LuaType::IntegerConst(_) | LuaType::Integer | LuaType::DocIntegerConst(_)
        )
    }

    pub fn is_number(&self) -> bool {
        matches!(
            self,
            LuaType::Number | LuaType::Integer | LuaType::IntegerConst(_) | LuaType::FloatConst(_)
        )
    }

    pub fn is_io(&self) -> bool {
        matches!(self, LuaType::Io)
    }

    pub fn is_ref(&self) -> bool {
        matches!(self, LuaType::Ref(_))
    }

    pub fn is_def(&self) -> bool {
        matches!(self, LuaType::Def(_))
    }

    pub fn is_custom_type(&self) -> bool {
        matches!(self, LuaType::Ref(_) | LuaType::Def(_))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, LuaType::Array(_))
    }

    pub fn is_nullable(&self) -> bool {
        match self {
            LuaType::Nil => true,
            LuaType::Union(u) => u.types.iter().any(|t| t.is_nullable()),
            _ => false,
        }
    }

    pub fn is_optional(&self) -> bool {
        match self {
            LuaType::Nil | LuaType::Any | LuaType::Unknown => true,
            LuaType::Union(u) => u.types.iter().any(|t| t.is_optional()),
            LuaType::Variadic(_) => true,
            _ => false,
        }
    }

    pub fn is_always_truthy(&self) -> bool {
        match self {
            LuaType::Nil | LuaType::Boolean | LuaType::Any | LuaType::Unknown => false,
            LuaType::BooleanConst(boolean) | LuaType::DocBooleanConst(boolean) => boolean.clone(),
            LuaType::Union(u) => u.types.iter().all(|t| t.is_always_truthy()),
            _ => true,
        }
    }

    pub fn is_always_falsy(&self) -> bool {
        match self {
            LuaType::Nil | LuaType::BooleanConst(false) | LuaType::DocBooleanConst(false) => true,
            LuaType::Union(u) => u.types.iter().all(|t| t.is_always_falsy()),
            _ => false,
        }
    }

    pub fn is_tuple(&self) -> bool {
        matches!(self, LuaType::Tuple(_))
    }

    pub fn is_function(&self) -> bool {
        matches!(
            self,
            LuaType::DocFunction(_) | LuaType::Function | LuaType::Signature(_)
        )
    }

    pub fn is_signature(&self) -> bool {
        matches!(self, LuaType::Signature(_))
    }

    pub fn is_object(&self) -> bool {
        matches!(self, LuaType::Object(_))
    }

    pub fn is_union(&self) -> bool {
        matches!(self, LuaType::Union(_))
    }

    pub fn is_intersection(&self) -> bool {
        matches!(self, LuaType::Intersection(_))
    }

    pub fn is_call(&self) -> bool {
        matches!(self, LuaType::Call(_))
    }

    pub fn is_generic(&self) -> bool {
        matches!(self, LuaType::Generic(_) | LuaType::TableGeneric(_))
    }

    pub fn is_table_generic(&self) -> bool {
        matches!(self, LuaType::TableGeneric(_))
    }

    pub fn is_class_tpl(&self) -> bool {
        matches!(self, LuaType::TplRef(_))
    }

    pub fn is_str_tpl_ref(&self) -> bool {
        matches!(self, LuaType::StrTplRef(_))
    }

    pub fn is_tpl(&self) -> bool {
        matches!(self, LuaType::TplRef(_) | LuaType::StrTplRef(_))
    }

    pub fn is_self_infer(&self) -> bool {
        matches!(self, LuaType::SelfInfer)
    }

    pub fn is_any(&self) -> bool {
        matches!(self, LuaType::Any)
    }

    pub fn is_const(&self) -> bool {
        matches!(
            self,
            LuaType::BooleanConst(_)
                | LuaType::StringConst(_)
                | LuaType::IntegerConst(_)
                | LuaType::FloatConst(_)
                | LuaType::TableConst(_)
                | LuaType::DocStringConst(_)
                | LuaType::DocIntegerConst(_)
        )
    }

    pub fn is_multi_return(&self) -> bool {
        matches!(self, LuaType::Variadic(_))
    }

    pub fn is_global(&self) -> bool {
        matches!(self, LuaType::Global)
    }

    pub fn contain_tpl(&self) -> bool {
        match self {
            LuaType::Array(base) => base.contain_tpl(),
            LuaType::Call(base) => base.contain_tpl(),
            LuaType::Tuple(base) => base.contain_tpl(),
            LuaType::DocFunction(base) => base.contain_tpl(),
            LuaType::Object(base) => base.contain_tpl(),
            LuaType::Union(base) => base.contain_tpl(),
            LuaType::Intersection(base) => base.contain_tpl(),
            LuaType::Generic(base) => base.contain_tpl(),
            LuaType::Variadic(multi) => multi.contain_tpl(),
            LuaType::TableGeneric(params) => params.iter().any(|p| p.contain_tpl()),
            LuaType::Variadic(inner) => inner.contain_tpl(),
            LuaType::TplRef(_) => true,
            LuaType::StrTplRef(_) => true,
            LuaType::SelfInfer => true,
            LuaType::MultiLineUnion(inner) => inner.contain_tpl(),
            _ => false,
        }
    }

    pub fn is_namespace(&self) -> bool {
        matches!(self, LuaType::Namespace(_))
    }

    pub fn is_variadic(&self) -> bool {
        matches!(self, LuaType::Variadic(_))
    }

    pub fn is_member_owner(&self) -> bool {
        matches!(self, LuaType::Ref(_) | LuaType::TableConst(_))
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaTupleType {
    types: Vec<LuaType>,
}

impl LuaTupleType {
    pub fn new(types: Vec<LuaType>) -> Self {
        Self { types }
    }

    pub fn get_types(&self) -> &[LuaType] {
        &self.types
    }

    pub fn get_type(&self, idx: usize) -> Option<&LuaType> {
        self.types.get(idx)
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn contain_tpl(&self) -> bool {
        self.types.iter().any(|t| t.contain_tpl())
    }

    pub fn cast_down_array_base(&self, db: &DbIndex) -> LuaType {
        let mut ty = LuaType::Unknown;
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

        ty
    }
}

impl From<LuaTupleType> for LuaType {
    fn from(t: LuaTupleType) -> Self {
        LuaType::Tuple(t.into())
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaFunctionType {
    is_async: bool,
    is_colon_define: bool,
    params: Vec<(String, Option<LuaType>)>,
    ret: LuaType,
}

impl LuaFunctionType {
    pub fn new(
        is_async: bool,
        is_colon_define: bool,
        params: Vec<(String, Option<LuaType>)>,
        ret: LuaType,
    ) -> Self {
        Self {
            is_async,
            is_colon_define,
            params,
            ret,
        }
    }

    pub fn is_async(&self) -> bool {
        self.is_async
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

    pub fn get_variadic_ret(&self) -> VariadicType {
        if let LuaType::Variadic(variadic) = &self.ret {
            return variadic.deref().clone();
        }

        VariadicType::Base(self.ret.clone())
    }

    pub fn contain_tpl(&self) -> bool {
        self.params
            .iter()
            .any(|(_, t)| t.as_ref().map_or(false, |t| t.contain_tpl()))
            || self.ret.contain_tpl()
    }

    pub fn contain_self(&self) -> bool {
        self.is_colon_define
            || self
                .params
                .iter()
                .any(|(name, t)| name == "self" || t.as_ref().map_or(false, |t| t.is_self_infer()))
            || self.ret.is_self_infer()
    }

    pub fn first_param_is_self(&self) -> bool {
        if let Some((name, t)) = self.params.first() {
            name == "self" || t.as_ref().map_or(false, |t| t.is_self_infer())
        } else {
            false
        }
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
        for (key, value_type) in object_fields.into_iter() {
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
        self.fields.values().any(|t| t.contain_tpl())
            || self
                .index_access
                .iter()
                .any(|(k, v)| k.contain_tpl() || v.contain_tpl())
    }

    pub fn cast_down_array_base(&self, db: &DbIndex) -> Option<LuaType> {
        if self.index_access.len() != 0 {
            let mut ty = None;
            for (key, value_type) in self.index_access.iter() {
                if matches!(key, LuaType::Integer) {
                    if ty.is_none() {
                        ty = Some(LuaType::Unknown);
                    }
                    if let Some(t) = ty {
                        ty = Some(TypeOps::Union.apply(db, &t, value_type));
                    }
                }
            }
            return ty;
        }

        let mut ty = LuaType::Unknown;
        let mut count = 1;
        let mut fields = self.fields.iter().collect::<Vec<_>>();

        fields.sort_by(|(a, _), (b, _)| a.cmp(b));

        for (key, value_type) in fields {
            let idx = match key {
                LuaMemberKey::Integer(i) => i,
                _ => {
                    return None;
                }
            };

            if *idx != count {
                return None;
            }

            count += 1;

            ty = TypeOps::Union.apply(db, &ty, value_type);
        }

        Some(ty)
    }
}

impl From<LuaObjectType> for LuaType {
    fn from(t: LuaObjectType) -> Self {
        LuaType::Object(t.into())
    }
}
#[derive(Debug, Clone)]
pub struct LuaUnionType {
    types: Vec<LuaType>,
}

impl LuaUnionType {
    pub fn new(types: Vec<LuaType>) -> Self {
        Self { types }
    }

    pub fn get_types(&self) -> &[LuaType] {
        &self.types
    }

    pub(crate) fn into_types(&self) -> Vec<LuaType> {
        self.types.clone()
    }

    pub fn contain_tpl(&self) -> bool {
        self.types.iter().any(|t| t.contain_tpl())
    }
}

impl PartialEq for LuaUnionType {
    fn eq(&self, other: &Self) -> bool {
        if self.types.len() != other.types.len() {
            return false;
        }
        let mut counts = HashMap::new();
        // Count occurrences in self.types
        for t in &self.types {
            *counts.entry(t).or_insert(0) += 1;
        }
        // Decrease counts for other.types
        for t in &other.types {
            match counts.get_mut(t) {
                Some(count) if *count > 0 => *count -= 1,
                _ => return false,
            }
        }
        true
    }
}

impl Eq for LuaUnionType {}

impl std::hash::Hash for LuaUnionType {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // To get an order-insensitive hash, combine:
        // - the number of elements
        // - the sum and product of the hashes of individual elements.
        // This is a simple and fast commutative hash.
        let mut sum: u64 = 0;
        let mut prod: u64 = 1;
        for t in &self.types {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            t.hash(&mut hasher);
            let h = hasher.finish();
            sum = sum.wrapping_add(h);
            prod = prod.wrapping_mul(h.wrapping_add(1));
        }
        self.types.len().hash(state);
        sum.hash(state);
        prod.hash(state);
    }
}

impl From<LuaUnionType> for LuaType {
    fn from(t: LuaUnionType) -> Self {
        LuaType::Union(t.into())
    }
}

pub fn make_union(left_type: LuaType, right_type: LuaType) -> LuaType {
    match (left_type, right_type) {
        (left, right) if left == right => left,
        (LuaType::Union(left_type_union), LuaType::Union(right_type_union)) => {
            let mut left_types = left_type_union.into_types();
            let right_types = right_type_union.into_types();
            left_types.extend(right_types);
            LuaType::Union(LuaUnionType::new(left_types).into())
        }
        (LuaType::Union(left_type_union), right) => {
            let mut left_types = (*left_type_union).into_types();
            left_types.push(right);
            LuaType::Union(LuaUnionType::new(left_types).into())
        }
        (left, LuaType::Union(right_type_union)) => {
            let mut right_types = (*right_type_union).into_types();
            right_types.push(left);
            LuaType::Union(LuaUnionType::new(right_types).into())
        }
        (left, right) => LuaType::Union(LuaUnionType::new(vec![left, right]).into()),
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

    pub(crate) fn into_types(&self) -> Vec<LuaType> {
        self.types.clone()
    }

    pub fn contain_tpl(&self) -> bool {
        self.types.iter().any(|t| t.contain_tpl())
    }
}

impl From<LuaIntersectionType> for LuaType {
    fn from(t: LuaIntersectionType) -> Self {
        LuaType::Intersection(t.into())
    }
}

pub fn make_intersection(left_type: LuaType, right_type: LuaType) -> LuaType {
    match (left_type, right_type) {
        (left, right) if left == right => left,
        (LuaType::Intersection(left_type_union), LuaType::Intersection(right_type_union)) => {
            let mut left_types = left_type_union.into_types();
            let right_types = right_type_union.into_types();
            left_types.extend(right_types);
            LuaType::Intersection(LuaIntersectionType::new(left_types).into())
        }
        (LuaType::Intersection(left_type_union), right) => {
            let mut left_types = left_type_union.into_types();
            left_types.push(right);
            LuaType::Intersection(LuaIntersectionType::new(left_types).into())
        }
        (left, LuaType::Intersection(right_type_union)) => {
            let mut right_types = right_type_union.into_types();
            right_types.push(left);
            LuaType::Intersection(LuaIntersectionType::new(right_types).into())
        }
        (left, right) => LuaType::Intersection(LuaIntersectionType::new(vec![left, right]).into()),
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
        self.operand.iter().any(|t| t.contain_tpl())
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

    pub fn get_params(&self) -> &Vec<LuaType> {
        &self.params
    }

    pub fn contain_tpl(&self) -> bool {
        self.params.iter().any(|t| t.contain_tpl())
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

                // If the index exceeds the range, return the last element
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
                if types.len() == 0 {
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
        match self {
            VariadicType::Multi(types) => types.iter().any(|t| t.contain_tpl()),
            VariadicType::Base(t) => t.contain_tpl(),
        }
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

impl From<SmolStr> for LuaType {
    fn from(s: SmolStr) -> Self {
        let str: &str = s.as_ref();
        match str {
            "nil" => LuaType::Nil,
            "table" => LuaType::Table,
            "userdata" => LuaType::Userdata,
            "function" => LuaType::Function,
            "thread" => LuaType::Thread,
            "boolean" => LuaType::Boolean,
            "string" => LuaType::String,
            "integer" => LuaType::Integer,
            "number" => LuaType::Number,
            "io" => LuaType::Io,
            "global" => LuaType::Global,
            "self" => LuaType::SelfInfer,
            _ => LuaType::Ref(LuaTypeDeclId::new_by_id(s.into())),
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
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GenericTpl {
    tpl_id: GenericTplId,
    name: ArcIntern<SmolStr>,
}

impl GenericTpl {
    pub fn new(tpl_id: GenericTplId, name: ArcIntern<SmolStr>) -> Self {
        Self { tpl_id, name }
    }

    pub fn get_tpl_id(&self) -> GenericTplId {
        self.tpl_id
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaStringTplType {
    prefix: ArcIntern<String>,
    tpl_id: GenericTplId,
    name: ArcIntern<String>,
    suffix: ArcIntern<String>,
}

impl LuaStringTplType {
    pub fn new(prefix: &str, name: &str, tpl_id: GenericTplId, suffix: &str) -> Self {
        Self {
            prefix: ArcIntern::new(prefix.to_string()),
            tpl_id,
            name: ArcIntern::new(name.to_string()),
            suffix: ArcIntern::new(suffix.to_string()),
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

        LuaType::Union(Arc::new(LuaUnionType::new(types)))
    }

    pub fn contain_tpl(&self) -> bool {
        self.unions.iter().any(|(t, _)| t.contain_tpl())
    }
}
