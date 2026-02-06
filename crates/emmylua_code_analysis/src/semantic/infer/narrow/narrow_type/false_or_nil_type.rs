use crate::{DbIndex, LuaType, semantic::infer::narrow::narrow_type::narrow_down_type};

/// Narrows a type to only its falsy parts (false or nil).
///
/// In Lua, only `false` and `nil` are falsy values. This function extracts the subset
/// of a type that could be falsy, used for type narrowing in the false branch of
/// conditional expressions.
///
/// # Examples
///
/// - `Boolean` → `BooleanConst(false)` (only the false possibility)
/// - `Nil` → `Nil` (already falsy)
/// - `string | nil | boolean` → `nil | false` (only the falsy parts)
/// - `string` → `Never` (strings cannot be falsy)
pub fn narrow_false_or_nil(db: &DbIndex, t: LuaType) -> LuaType {
    match &t {
        LuaType::Boolean => {
            return LuaType::BooleanConst(false);
        }
        LuaType::Union(u) => {
            // For unions, collect all the falsy parts from each member
            let falsy_types: Vec<_> = u
                .into_vec()
                .iter()
                .map(|member| narrow_false_or_nil(db, member.clone()))
                .filter(|falsy| !falsy.is_never())
                .collect();
            return LuaType::from_vec(falsy_types);
        }
        LuaType::Nil | LuaType::BooleanConst(false) | LuaType::DocBooleanConst(false) => {
            return t;
        }
        _ => {}
    }

    narrow_down_type(db, t.clone(), LuaType::Nil, None).unwrap_or(LuaType::Never)
}

/// Removes falsy values (false and nil) from a type.
///
/// This function filters out `nil` and `false` from a type, leaving only the truthy
/// possibilities. Used for type narrowing in the true branch of conditional expressions.
///
/// # Examples
///
/// - `Nil` → `Unknown` (removes nil entirely)
/// - `BooleanConst(false)` → `Unknown` (removes false)
/// - `Boolean` → `BooleanConst(true)` (removes false, keeps true)
/// - `string | nil | boolean` → `string | true`
/// - `string` → `string` (already truthy, unchanged)
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

            LuaType::from_vec(new_types)
        }
        _ => t,
    }
}
