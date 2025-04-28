use crate::{DbIndex, LuaType, LuaUnionType};

use super::TypeOps;

pub fn narrow_false_or_nil(db: &DbIndex, t: LuaType) -> LuaType {
    if t.is_boolean() {
        return LuaType::BooleanConst(false);
    }

    return TypeOps::Narrow.apply(db, &t, &LuaType::Nil);
}

pub fn remove_false_or_nil(t: LuaType) -> LuaType {
    match t {
        LuaType::Nil => LuaType::Unknown,
        LuaType::BooleanConst(false) => LuaType::Unknown,
        LuaType::DocBooleanConst(false) => LuaType::Unknown,
        LuaType::Boolean => LuaType::BooleanConst(true),
        LuaType::Union(u) => {
            let types = u.get_types();
            let mut new_types = vec![];
            for it in types.iter() {
                match it {
                    LuaType::Nil => {}
                    LuaType::BooleanConst(false) => {}
                    LuaType::DocBooleanConst(false) => {}
                    LuaType::Boolean => new_types.push(LuaType::BooleanConst(true)),
                    _ => new_types.push(it.clone()),
                }
            }

            if new_types.is_empty() {
                return LuaType::Unknown;
            } else if new_types.len() == 1 {
                return new_types[0].clone();
            } else {
                return LuaType::Union(LuaUnionType::new(new_types).into());
            }
        }
        _ => t,
    }
}
