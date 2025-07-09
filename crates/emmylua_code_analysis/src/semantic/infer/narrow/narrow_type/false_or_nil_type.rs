use crate::{
    semantic::infer::narrow::narrow_type::narrow_down_type, DbIndex, LuaType, LuaUnionType,
};

pub fn narrow_false_or_nil(db: &DbIndex, t: LuaType) -> LuaType {
    if t.is_boolean() {
        return LuaType::BooleanConst(false);
    }

    return narrow_down_type(db, t.clone(), LuaType::Nil).unwrap_or(t);
}

pub fn remove_false_or_nil(t: LuaType) -> LuaType {
    match t {
        LuaType::Nil => LuaType::Unknown,
        LuaType::BooleanConst(false) => LuaType::Unknown,
        LuaType::DocBooleanConst(false) => LuaType::Unknown,
        LuaType::Boolean => LuaType::BooleanConst(true),
        LuaType::Union(u) => {
            let types = u.into_vec();
            let mut new_types = Vec::new();
            for it in types.iter() {
                match it {
                    LuaType::Nil => {}
                    LuaType::BooleanConst(false) => {}
                    LuaType::DocBooleanConst(false) => {}
                    LuaType::Boolean => {
                        new_types.push(LuaType::BooleanConst(true));
                    }
                    _ => {
                        new_types.push(it.clone());
                    }
                }
            }

            if new_types.is_empty() {
                return LuaType::Unknown;
            } else if new_types.len() == 1 {
                return new_types[0].clone();
            } else {
                return LuaType::Union(LuaUnionType::from_vec(new_types).into());
            }
        }
        _ => t,
    }
}
