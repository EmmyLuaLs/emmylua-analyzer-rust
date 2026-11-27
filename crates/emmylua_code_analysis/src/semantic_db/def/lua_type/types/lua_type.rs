use crate::signature::LuaSignatureId;
use crate::{FileId, InFiled};
use internment::ArcIntern;
use rowan::TextRange;
use smol_str::SmolStr;
use std::{hash::Hash, rc::Rc};

use super::complex::{
    GenericTpl, LuaAliasCallType, LuaArrayType, LuaConditionalType, LuaFunctionType,
    LuaGenericType, LuaInstanceType, LuaIntersectionType, LuaMappedType, LuaMultiLineUnion,
    LuaObjectType, LuaStringTplType, LuaTupleType, LuaUnionType, VariadicType,
};
use crate::semantic_db::def::lua_type::type_decl::LuaTypeDeclId;

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
    Never,
    BooleanConst(bool),
    StringConst(ArcIntern<SmolStr>),
    IntegerConst(i64),
    FloatConst(f64),
    TableConst(InFiled<TextRange>),
    Ref(LuaTypeDeclId),
    Def(LuaTypeDeclId),
    Array(Rc<LuaArrayType>),
    Tuple(Rc<LuaTupleType>),
    DocFunction(Rc<LuaFunctionType>),
    Object(Rc<LuaObjectType>),
    Union(Rc<LuaUnionType>),
    Intersection(Rc<LuaIntersectionType>),
    Generic(Rc<LuaGenericType>),
    TableGeneric(Rc<Vec<LuaType>>),
    TplRef(Rc<GenericTpl>),
    StrTplRef(Rc<LuaStringTplType>),
    Variadic(Rc<VariadicType>),
    Signature(LuaSignatureId),
    Instance(Rc<LuaInstanceType>),
    DocStringConst(ArcIntern<SmolStr>),
    DocIntegerConst(i64),
    DocBooleanConst(bool),
    Namespace(ArcIntern<SmolStr>),
    Call(Rc<LuaAliasCallType>),
    MultiLineUnion(Rc<LuaMultiLineUnion>),
    TypeGuard(Rc<LuaType>),
    Language(ArcIntern<SmolStr>),
    ModuleRef(FileId),
    Conditional(Rc<LuaConditionalType>),
    Mapped(Rc<LuaMappedType>),
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
            (LuaType::FloatConst(a), LuaType::FloatConst(b)) => float_bits_eq(*a, *b),
            (LuaType::TableConst(a), LuaType::TableConst(b)) => a == b,
            (LuaType::Ref(a), LuaType::Ref(b)) => a == b,
            (LuaType::Def(a), LuaType::Def(b)) => a == b,
            // Rc-backed payloads are frequently shared. The pointer check is a
            // cheap fast path for cache lookups; fall back to structural equality.
            (LuaType::Array(a), LuaType::Array(b)) => Rc::ptr_eq(a, b) || a == b,
            (LuaType::Call(a), LuaType::Call(b)) => Rc::ptr_eq(a, b) || a == b,
            (LuaType::Tuple(a), LuaType::Tuple(b)) => Rc::ptr_eq(a, b) || a == b,
            (LuaType::DocFunction(a), LuaType::DocFunction(b)) => Rc::ptr_eq(a, b) || a == b,
            (LuaType::Object(a), LuaType::Object(b)) => Rc::ptr_eq(a, b) || a == b,
            (LuaType::Union(a), LuaType::Union(b)) => Rc::ptr_eq(a, b) || a == b,
            (LuaType::Intersection(a), LuaType::Intersection(b)) => Rc::ptr_eq(a, b) || a == b,
            (LuaType::Generic(a), LuaType::Generic(b)) => Rc::ptr_eq(a, b) || a == b,
            (LuaType::TableGeneric(a), LuaType::TableGeneric(b)) => Rc::ptr_eq(a, b) || a == b,
            (LuaType::TplRef(a), LuaType::TplRef(b)) => Rc::ptr_eq(a, b) || a == b,
            (LuaType::StrTplRef(a), LuaType::StrTplRef(b)) => Rc::ptr_eq(a, b) || a == b,
            (LuaType::Variadic(a), LuaType::Variadic(b)) => Rc::ptr_eq(a, b) || a == b,
            (LuaType::DocBooleanConst(a), LuaType::DocBooleanConst(b)) => a == b,
            (LuaType::Signature(a), LuaType::Signature(b)) => a == b,
            (LuaType::Instance(a), LuaType::Instance(b)) => Rc::ptr_eq(a, b) || a == b,
            (LuaType::DocStringConst(a), LuaType::DocStringConst(b)) => a == b,
            (LuaType::DocIntegerConst(a), LuaType::DocIntegerConst(b)) => a == b,
            (LuaType::Namespace(a), LuaType::Namespace(b)) => a == b,
            (LuaType::MultiLineUnion(a), LuaType::MultiLineUnion(b)) => Rc::ptr_eq(a, b) || a == b,
            (LuaType::TypeGuard(a), LuaType::TypeGuard(b)) => Rc::ptr_eq(a, b) || a == b,
            (LuaType::Never, LuaType::Never) => true,
            (LuaType::Language(a), LuaType::Language(b)) => a == b,
            (LuaType::ModuleRef(a), LuaType::ModuleRef(b)) => a == b,
            (LuaType::Conditional(a), LuaType::Conditional(b)) => Rc::ptr_eq(a, b) || a == b,
            (LuaType::Mapped(a), LuaType::Mapped(b)) => Rc::ptr_eq(a, b) || a == b,
            _ => false,
        }
    }
}

impl Eq for LuaType {}

/// `-0.0 == 0.0` and `NaN != NaN` under `f64::eq`, but `LuaType` is used as a
/// hash-map key and implements `Eq`. Compare float constants by bit pattern,
/// normalizing both zeros to the same value so `Eq` and `Hash` agree.
fn float_bits(value: f64) -> u64 {
    if value == 0.0 { 0 } else { value.to_bits() }
}

fn float_bits_eq(left: f64, right: f64) -> bool {
    float_bits(left) == float_bits(right)
}

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
            LuaType::FloatConst(a) => (17, float_bits(*a)).hash(state),
            LuaType::TableConst(a) => (18, a).hash(state),
            LuaType::Ref(a) => (19, a).hash(state),
            LuaType::Def(a) => (20, a).hash(state),
            LuaType::Array(a) => (22, a.as_ref()).hash(state),
            LuaType::Call(a) => (23, a.as_ref()).hash(state),
            LuaType::Tuple(a) => (25, a.as_ref()).hash(state),
            LuaType::DocFunction(a) => (26, a.as_ref()).hash(state),
            LuaType::Object(a) => (27, a.as_ref()).hash(state),
            LuaType::Union(a) => (28, a.as_ref()).hash(state),
            LuaType::Intersection(a) => (29, a.as_ref()).hash(state),
            LuaType::Generic(a) => (30, a.as_ref()).hash(state),
            LuaType::TableGeneric(a) => (31, a.as_ref()).hash(state),
            LuaType::TplRef(a) => (32, a.as_ref()).hash(state),
            LuaType::StrTplRef(a) => (33, a.as_ref()).hash(state),
            LuaType::Variadic(a) => (34, a.as_ref()).hash(state),
            LuaType::DocBooleanConst(a) => (35, a).hash(state),
            LuaType::Signature(a) => (36, a).hash(state),
            LuaType::Instance(a) => (37, a).hash(state),
            LuaType::DocStringConst(a) => (38, a).hash(state),
            LuaType::DocIntegerConst(a) => (39, a).hash(state),
            LuaType::Namespace(a) => (40, a).hash(state),
            LuaType::MultiLineUnion(a) => (43, a.as_ref()).hash(state),
            LuaType::TypeGuard(a) => (44, a.as_ref()).hash(state),
            LuaType::Never => 45.hash(state),
            LuaType::Language(a) => (46, a).hash(state),
            LuaType::ModuleRef(a) => (47, a).hash(state),
            LuaType::Conditional(a) => (48, a.as_ref()).hash(state),
            LuaType::Mapped(a) => (49, a.as_ref()).hash(state),
        }
    }
}
