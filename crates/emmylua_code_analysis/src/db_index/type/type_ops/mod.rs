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
}

impl TypeOps {
    pub fn apply(&self, db: &DbIndex, source: &LuaType, target: &LuaType) -> LuaType {
        match self {
            TypeOps::Union => union_type::union_type(db, source.clone(), target.clone()),
            TypeOps::Remove => {
                let result = remove_type::remove_type(db, source.clone(), target.clone());
                if let Some(result) = result {
                    return result;
                }

                match &source {
                    LuaType::Nil => LuaType::Never,
                    _ => source.clone(),
                }
            }
        }
    }
}
