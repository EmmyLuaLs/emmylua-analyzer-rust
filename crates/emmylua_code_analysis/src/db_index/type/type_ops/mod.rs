mod and_type;
mod false_or_nil_type;
mod narrow_type;
mod remove_type;
mod test;
mod union_type;

use crate::DbIndex;

use super::LuaType;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum TypeOps {
    /// Add a type to the source type
    Union,
    /// Remove a type from the source type
    Remove,
    /// Remove a type from the source type, but keep the source type
    RemoveNilOrFalse,
    /// Force a type to the source type
    Narrow,
    /// Only keep the false or nil type
    NarrowFalseOrNil,
    /// And operation
    And,
}

impl TypeOps {
    pub fn apply(&self, db: &DbIndex, source: &LuaType, target: &LuaType) -> LuaType {
        match self {
            TypeOps::Union => union_type::union_type(source.clone(), target.clone()),
            TypeOps::Remove => {
                remove_type::remove_type(db, source.clone(), target.clone()).unwrap_or(LuaType::Any)
            }
            TypeOps::Narrow => narrow_type::narrow_down_type(source.clone(), target.clone())
                .unwrap_or(target.clone()),
            TypeOps::And => and_type::and_type(source.clone(), target.clone()),
            _ => source.clone(),
        }
    }

    pub fn apply_source(&self, db: &DbIndex, source: &LuaType) -> LuaType {
        match self {
            TypeOps::NarrowFalseOrNil => false_or_nil_type::narrow_false_or_nil(db, source.clone()),
            TypeOps::RemoveNilOrFalse => false_or_nil_type::remove_false_or_nil(source.clone()),
            _ => source.clone(),
        }
    }
}
