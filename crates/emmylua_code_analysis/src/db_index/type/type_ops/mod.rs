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
    // /// And operation
    // And,
}

impl TypeOps {
    pub fn apply(&self, db: &DbIndex, source: &LuaType, target: &LuaType) -> LuaType {
        match self {
            TypeOps::Union => union_type::union_type(source.clone(), target.clone()),
            TypeOps::Remove => {
                remove_type::remove_type(db, source.clone(), target.clone()).unwrap_or(LuaType::Any)
            }
            TypeOps::Narrow => narrow_type::narrow_down_type(db, source.clone(), target.clone())
                .unwrap_or(target.clone()),
            // TypeOps::And => and_type::and_type(source.clone(), target.clone()),
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

pub fn get_real_type<'a>(db: &'a DbIndex, typ: &'a LuaType) -> Option<&'a LuaType> {
    get_real_type_with_depth(db, typ, 0)
}

fn get_real_type_with_depth<'a>(
    db: &'a DbIndex,
    typ: &'a LuaType,
    depth: u32,
) -> Option<&'a LuaType> {
    const MAX_RECURSION_DEPTH: u32 = 50;

    if depth >= MAX_RECURSION_DEPTH {
        return Some(typ);
    }

    match typ {
        LuaType::Ref(type_decl_id) => {
            let type_decl = db.get_type_index().get_type_decl(type_decl_id)?;
            if type_decl.is_alias() {
                return get_real_type_with_depth(db, type_decl.get_alias_ref()?, depth + 1);
            }
            Some(typ)
        }
        _ => Some(typ),
    }
}
