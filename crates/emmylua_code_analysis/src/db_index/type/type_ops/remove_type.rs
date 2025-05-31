use crate::{DbIndex, LuaType, LuaUnionType};

use super::get_real_type;

pub fn remove_type(db: &DbIndex, source: LuaType, removed_type: LuaType) -> Option<LuaType> {
    if source == removed_type {
        match source {
            LuaType::IntegerConst(_) => return Some(LuaType::Integer),
            LuaType::FloatConst(_) => return Some(LuaType::Number),
            _ => return None,
        }
    }

    match &removed_type {
        LuaType::Nil => {
            if source.is_nil() {
                return None;
            }
        }
        LuaType::Boolean => {
            if source.is_boolean() {
                return None;
            }
        }
        LuaType::Integer => {
            if source.is_integer() {
                return None;
            }
        }
        LuaType::Number => {
            if source.is_number() {
                return None;
            }
        }
        LuaType::String => {
            if source.is_string() {
                return None;
            }
        }
        LuaType::Io => {
            if source.is_io() {
                return None;
            }
        }
        LuaType::Function => {
            if source.is_function() {
                return None;
            }
        }
        LuaType::Thread => {
            if source.is_thread() {
                return None;
            }
        }
        LuaType::Userdata => {
            if source.is_userdata() {
                return None;
            }
        }
        LuaType::Table => match &source {
            LuaType::TableConst(_)
            | LuaType::Table
            | LuaType::Userdata
            | LuaType::Global
            | LuaType::Array(_)
            | LuaType::Tuple(_)
            | LuaType::Generic(_)
            | LuaType::Object(_)
            | LuaType::TableGeneric(_) => return None,
            LuaType::Ref(type_decl_id) | LuaType::Def(type_decl_id) => {
                let type_decl = db.get_type_index().get_type_decl(type_decl_id)?;
                // enum 在实际使用时实际上是 enum.field, 并不等于 table
                if type_decl.is_enum() {
                    return Some(source);
                }
                if type_decl.is_alias() {
                    if let Some(alias_ref) = get_real_type(db, &source) {
                        return remove_type(db, alias_ref.clone(), removed_type);
                    }
                }

                // 需要对`userdata`进行特殊处理
                if let Some(super_types) = db.get_type_index().get_super_types_iter(type_decl_id) {
                    for super_type in super_types {
                        if super_type.is_userdata() {
                            return Some(source);
                        }
                    }
                }
                return None;
            }
            _ => {}
        },
        LuaType::DocStringConst(s) => match &source {
            LuaType::DocStringConst(s2) => {
                if s == s2 {
                    return None;
                }
            }
            LuaType::StringConst(s2) => {
                if s == s2 {
                    return None;
                }
            }
            _ => {}
        },
        LuaType::StringConst(s) => match &source {
            LuaType::DocStringConst(s2) => {
                if s == s2 {
                    return None;
                }
            }
            LuaType::StringConst(s2) => {
                if s == s2 {
                    return None;
                }
            }
            _ => {}
        },
        LuaType::DocIntegerConst(i) => match &source {
            LuaType::DocIntegerConst(i2) => {
                if i == i2 {
                    return None;
                }
            }
            LuaType::IntegerConst(i2) => {
                if i == i2 {
                    return None;
                }
            }
            _ => {}
        },
        LuaType::IntegerConst(i) => match &source {
            LuaType::DocIntegerConst(i2) => {
                if i == i2 {
                    return None;
                }
            }
            LuaType::IntegerConst(i2) => {
                if i == i2 {
                    return None;
                }
            }
            _ => {}
        },
        _ => {}
    }

    if let LuaType::Union(u) = &source {
        let mut types = u
            .get_types()
            .iter()
            .filter_map(|t| remove_type(db, t.clone(), removed_type.clone()))
            .collect::<Vec<_>>();
        types.dedup();
        match types.len() {
            0 => return Some(LuaType::Nil),
            1 => return types.pop(),
            _ => return Some(LuaType::Union(LuaUnionType::new(types).into())),
        }
    } else if let LuaType::Union(u) = &removed_type {
        let mut types = u
            .get_types()
            .iter()
            .filter_map(|t| remove_type(db, source.clone(), t.clone()))
            .collect::<Vec<_>>();
        types.dedup();
        return match types.len() {
            0 => None,
            1 => Some(types[0].clone()),
            _ => Some(LuaType::Union(LuaUnionType::new(types).into())),
        };
    }

    Some(source)
}
