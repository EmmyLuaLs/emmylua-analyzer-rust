use crate::{DbIndex, LuaType};

use super::{
    type_check_fail_reason::TypeCheckFailReason, type_check_guard::TypeCheckGuard, TypeCheckResult,
};

pub fn check_simple_type_compact(
    db: &DbIndex,
    source: &LuaType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    match source {
        LuaType::Unknown | LuaType::Any => return Ok(()),
        LuaType::Nil => {
            if let LuaType::Nil = compact_type {
                return Ok(());
            }
        }
        LuaType::Table | LuaType::TableConst(_) => {
            if matches!(
                compact_type,
                LuaType::Table
                    | LuaType::TableConst(_)
                    | LuaType::Tuple(_)
                    | LuaType::Array(_)
                    | LuaType::Object(_)
                    | LuaType::Ref(_)
                    | LuaType::Def(_)
                    | LuaType::TableGeneric(_)
                    | LuaType::Generic(_)
                    | LuaType::Global
                    | LuaType::Userdata
                    | LuaType::Instance(_)
            ) {
                return Ok(());
            }
        }
        LuaType::Userdata => {
            if matches!(
                compact_type,
                LuaType::Userdata | LuaType::Ref(_) | LuaType::Def(_)
            ) {
                return Ok(());
            }
        }
        LuaType::Function => {
            if matches!(
                compact_type,
                LuaType::Function | LuaType::DocFunction(_) | LuaType::Signature(_)
            ) {
                return Ok(());
            }
        }
        LuaType::Thread => {
            if let LuaType::Thread = compact_type {
                return Ok(());
            }
        }
        LuaType::Boolean | LuaType::BooleanConst(_) => {
            if matches!(compact_type, LuaType::Boolean | LuaType::BooleanConst(_)) {
                return Ok(());
            }
        }
        LuaType::String | LuaType::StringConst(_) => {
            if matches!(
                compact_type,
                LuaType::String | LuaType::StringConst(_) | LuaType::DocStringConst(_)
            ) {
                return Ok(());
            }
        }
        LuaType::Integer | LuaType::IntegerConst(_) => {
            if matches!(
                compact_type,
                LuaType::Integer | LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_)
            ) {
                return Ok(());
            }
        }
        LuaType::Number | LuaType::FloatConst(_) => {
            if matches!(
                compact_type,
                LuaType::Number
                    | LuaType::FloatConst(_)
                    | LuaType::Integer
                    | LuaType::IntegerConst(_)
                    | LuaType::DocIntegerConst(_)
            ) {
                return Ok(());
            }
        }
        LuaType::Io => {
            if let LuaType::Io = compact_type {
                return Ok(());
            }
        }
        LuaType::Global => {
            if let LuaType::Global = compact_type {
                return Ok(());
            }
        }
        LuaType::DocIntegerConst(i) => match compact_type {
            LuaType::IntegerConst(j) => {
                if i == j {
                    return Ok(());
                }

                return Err(TypeCheckFailReason::TypeNotMatch);
            }
            LuaType::Integer => return Err(TypeCheckFailReason::TypeNotMatch),
            LuaType::DocIntegerConst(j) => {
                if i == j {
                    return Ok(());
                }

                return Err(TypeCheckFailReason::TypeNotMatch);
            }
            _ => {}
        },
        LuaType::DocStringConst(s) => match compact_type {
            LuaType::StringConst(t) => {
                if s == t {
                    return Ok(());
                }

                return Err(TypeCheckFailReason::TypeNotMatch);
            }
            LuaType::String => return Err(TypeCheckFailReason::TypeNotMatch),
            LuaType::DocStringConst(t) => {
                if s == t {
                    return Ok(());
                }

                return Err(TypeCheckFailReason::TypeNotMatch);
            }
            _ => {}
        },
        LuaType::StrTplRef(_) => {
            if compact_type.is_string() {
                return Ok(());
            }
        }
        LuaType::TplRef(_) => return Ok(()),
        LuaType::Namespace(source_namespace) => {
            if let LuaType::Namespace(compact_namespace) = compact_type {
                if source_namespace == compact_namespace {
                    return Ok(());
                }
            }
        }
        LuaType::Variadic(source_type) => {
            if let LuaType::Variadic(compact_type) = compact_type {
                return check_simple_type_compact(
                    db,
                    source_type,
                    compact_type,
                    check_guard.next_level()?,
                );
            } else {
                return check_simple_type_compact(
                    db,
                    source_type,
                    compact_type,
                    check_guard.next_level()?,
                );
            }
        }
        _ => {}
    }

    if let LuaType::Union(union) = compact_type {
        for sub_compact in union.get_types() {
            if check_simple_type_compact(db, source, sub_compact, check_guard.next_level()?)
                .is_err()
            {
                return Err(TypeCheckFailReason::TypeNotMatch);
            }
        }

        return Ok(());
    }

    // complex infer
    Err(TypeCheckFailReason::TypeNotMatch)
}
