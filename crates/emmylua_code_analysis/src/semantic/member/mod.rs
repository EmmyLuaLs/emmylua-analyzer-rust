mod infer_members;

use crate::{
    db_index::{LuaType, LuaTypeDeclId},
    LuaMemberKey, LuaPropertyOwnerId,
};
pub use infer_members::infer_members;

pub fn without_members(type_: &LuaType) -> bool {
    match type_ {
        LuaType::Nil
        | LuaType::Boolean
        | LuaType::BooleanConst(_)
        | LuaType::Integer
        | LuaType::IntegerConst(_)
        | LuaType::Number
        | LuaType::FloatConst(_)
        | LuaType::Function
        | LuaType::DocFunction(_)
        | LuaType::Table
        | LuaType::TableGeneric(_)
        | LuaType::Userdata
        | LuaType::Thread
        | LuaType::Unknown
        | LuaType::Any
        | LuaType::SelfInfer
        | LuaType::StrTplRef(_)
        | LuaType::TplRef(_)
        | LuaType::Array(_)
        | LuaType::MuliReturn(_) => true,
        _ => false,
    }
}

pub fn without_index_operator(type_: &LuaType) -> bool {
    match type_ {
        LuaType::Nil
        | LuaType::Boolean
        | LuaType::BooleanConst(_)
        | LuaType::Integer
        | LuaType::IntegerConst(_)
        | LuaType::Number
        | LuaType::FloatConst(_)
        | LuaType::Function
        | LuaType::DocFunction(_)
        | LuaType::Table
        | LuaType::Userdata
        | LuaType::Thread
        | LuaType::Unknown
        | LuaType::String
        | LuaType::StringConst(_)
        | LuaType::Io
        | LuaType::Any
        | LuaType::StrTplRef(_)
        | LuaType::TplRef(_)
        | LuaType::MuliReturn(_) => true,
        _ => false,
    }
}

pub fn get_buildin_type_map_type_id(type_: &LuaType) -> Option<LuaTypeDeclId> {
    match type_ {
        LuaType::String | LuaType::StringConst(_) => Some(LuaTypeDeclId::new("string")),
        LuaType::Io => Some(LuaTypeDeclId::new("io")),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaMemberInfo {
    pub property_owner_id: Option<LuaPropertyOwnerId>,
    pub key: LuaMemberKey,
    pub typ: LuaType,
    pub origin_typ: Option<LuaType>,
}

impl LuaMemberInfo {
    pub fn get_origin_type(&self) -> &LuaType {
        self.origin_typ.as_ref().unwrap_or(&self.typ)
    }
}

type InferMembersResult = Option<Vec<LuaMemberInfo>>;
