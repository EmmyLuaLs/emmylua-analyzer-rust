use crate::{LuaType, LuaUnionType};

// need to be optimized
pub fn narrow_down_type(source: LuaType, target: LuaType) -> Option<LuaType> {
    match &target {
        LuaType::Number => {
            if source.is_number() {
                return Some(source);
            }
        }
        LuaType::Integer => {
            if source.is_integer() {
                return Some(source);
            }
        }
        LuaType::String => {
            if source.is_string() {
                return Some(source);
            }
        }
        LuaType::Boolean => {
            if source.is_boolean() {
                return Some(source);
            }
        }
        LuaType::Table => {
            if source.is_table() {
                return Some(source);
            }
        }
        LuaType::Function => {
            if source.is_function() {
                return Some(source);
            }
        }
        LuaType::Thread => {
            if source.is_thread() {
                return Some(source);
            }
        }
        LuaType::Userdata => {
            if source.is_userdata() {
                return Some(source);
            }
        }
        LuaType::Nil => {
            if source.is_nil() {
                return Some(source);
            }
        }
        LuaType::Any => {
            return Some(source);
        }
        LuaType::FloatConst(f) => {
            if source.is_number() {
                return Some(LuaType::Number);
            } else if source.is_unknown() {
                return Some(LuaType::FloatConst(*f));
            }
        }
        LuaType::IntegerConst(i) => {
            if source.is_integer() {
                return Some(LuaType::Integer);
            } else if source.is_unknown() {
                return Some(LuaType::IntegerConst(*i));
            }
        }
        LuaType::StringConst(s) => {
            if source.is_string() {
                return Some(LuaType::String);
            } else if source.is_unknown() {
                return Some(LuaType::StringConst(s.clone()));
            }
        }
        LuaType::TableConst(t) => match &source {
            LuaType::TableConst(s) => {
                return Some(LuaType::TableConst(s.clone()));
            }
            LuaType::Table | LuaType::Userdata | LuaType::Any | LuaType::Unknown => {
                return Some(LuaType::TableConst(t.clone()));
            }
            LuaType::Ref(_)
            | LuaType::Def(_)
            | LuaType::Global
            | LuaType::Array(_)
            | LuaType::Tuple(_)
            | LuaType::Generic(_)
            | LuaType::TableGeneric(_) => return Some(source),
            _ => {}
        },
        LuaType::Unknown => return Some(source),
        _ => return Some(target),
    }

    match &source {
        LuaType::Union(union) => {
            let union_types = union
                .get_types()
                .iter()
                .filter_map(|t| narrow_down_type(t.clone(), target.clone()))
                .collect::<Vec<_>>();

            return match union_types.len() {
                0 => Some(target),
                1 => Some(union_types[0].clone()),
                _ => Some(LuaType::Union(LuaUnionType::new(union_types).into())),
            };
        }
        LuaType::Nullable(inner) => {
            let union_types = vec![LuaType::Nil, (**inner).clone()]
                .iter()
                .filter_map(|t| narrow_down_type(t.clone(), target.clone()))
                .collect::<Vec<_>>();

            return match union_types.len() {
                0 => Some(target),
                1 => Some(union_types[0].clone()),
                _ => Some(LuaType::Union(LuaUnionType::new(union_types).into())),
            };
        }
        _ => {}
    }

    None
}
