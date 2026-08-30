//! Complex type checks: Union/Intersection/Array/TableGeneric/Tuple/Object/Call/MultiLineUnion.

use crate::{LuaMemberKey, LuaType};

use super::context::TypeCheckContext;
use super::guard::TypeCheckGuard;
use super::{TypeCheckResult, check_general_type_compact};

pub fn check_complex_type_compact(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    match source {
        LuaType::Array(source_array) => {
            let base = source_array.get_base().clone();
            match compact_type {
                LuaType::Array(compact_array) => check_general_type_compact(
                    context,
                    &base,
                    compact_array.get_base(),
                    check_guard.next_level()?,
                ),
                LuaType::Object(object) if context.assign_mode => {
                    let array_base = source_array.get_base();
                    for (key_ty, expected) in object.get_index_access() {
                        if matches!(
                            key_ty,
                            LuaType::Integer | LuaType::Number | LuaType::IntegerConst(_)
                        ) && (matches!(expected, LuaType::Any | LuaType::Unknown)
                            || check_general_type_compact(
                                context,
                                array_base,
                                expected,
                                check_guard.next_level()?,
                            )
                            .is_ok())
                        {
                            return Ok(());
                        }
                    }
                    Err(context.mismatch(source, compact_type))
                }
                LuaType::Tuple(tuple) => {
                    let next_guard = check_guard.next_level()?;
                    for element in tuple.get_types() {
                        check_general_type_compact(context, &base, element, next_guard)?;
                    }
                    Ok(())
                }
                LuaType::Table
                | LuaType::TableGeneric(_)
                | LuaType::Ref(_)
                | LuaType::Def(_)
                | LuaType::Generic(_) => Ok(()),
                _ => Err(context.mismatch(source, compact_type)),
            }
        }
        LuaType::TableGeneric(source_generic) => match compact_type {
            LuaType::TableGeneric(compact_generic) => {
                let source_types = source_generic.as_ref();
                let compact_types = compact_generic.as_ref();
                if source_types.len() != compact_types.len() {
                    return Err(context.mismatch(source, compact_type));
                }
                let next_guard = check_guard.next_level()?;
                for (s, t) in source_types.iter().zip(compact_types.iter()) {
                    check_general_type_compact(context, s, t, next_guard)?;
                }
                Ok(())
            }
            LuaType::Table
            | LuaType::Array(_)
            | LuaType::Tuple(_)
            | LuaType::Object(_)
            | LuaType::Ref(_)
            | LuaType::Def(_) => Ok(()),
            _ => Err(context.mismatch(source, compact_type)),
        },
        LuaType::Tuple(source_tuple) => match compact_type {
            LuaType::Tuple(compact_tuple) => {
                let source_types = source_tuple.get_types();
                let compact_types = compact_tuple.get_types();
                if source_types.len() != compact_types.len() {
                    return Err(context.mismatch(source, compact_type));
                }
                let next_guard = check_guard.next_level()?;
                for (s, t) in source_types.iter().zip(compact_types.iter()) {
                    check_general_type_compact(context, s, t, next_guard)?;
                }
                Ok(())
            }
            LuaType::Table
            | LuaType::Array(_)
            | LuaType::TableGeneric(_)
            | LuaType::Ref(_)
            | LuaType::Def(_) => Ok(()),
            _ => Err(context.mismatch(source, compact_type)),
        },
        LuaType::Object(source_object) if context.strict_object => match compact_type {
            LuaType::Object(compact_object) => {
                let next_guard = check_guard.next_level()?;
                for (key, expected) in source_object.get_fields() {
                    if let Some(actual) = compact_object.get_field(key) {
                        check_general_type_compact(context, expected, actual, next_guard)?;
                    }
                }
                Ok(())
            }
            LuaType::Tuple(tuple) => {
                let next_guard = check_guard.next_level()?;
                for (index, element_ty) in tuple.get_types().iter().enumerate() {
                    let key = LuaMemberKey::Integer(index as i64 + 1);
                    let Some(expected) = source_object.get_fields().get(&key) else {
                        return Err(context.mismatch(source, compact_type));
                    };
                    check_general_type_compact(context, expected, element_ty, next_guard)?;
                }
                Ok(())
            }
            LuaType::TableConst(_) => {
                let table_infos = context.model.member_infos(compact_type);
                let next_guard = check_guard.next_level()?;
                for (key, expected) in source_object.get_fields() {
                    let Some(actual) = table_infos
                        .iter()
                        .find(|info| &info.key == key)
                        .map(|info| info.typ.clone())
                    else {
                        continue;
                    };
                    check_general_type_compact(context, expected, &actual, next_guard)?;
                }
                Ok(())
            }
            LuaType::Table | LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_) => Ok(()),
            _ => Err(context.mismatch(source, compact_type)),
        },
        LuaType::Object(_) => match compact_type {
            LuaType::Object(_)
            | LuaType::Table
            | LuaType::Ref(_)
            | LuaType::Def(_)
            | LuaType::Generic(_) => Ok(()),
            _ => Err(context.mismatch(source, compact_type)),
        },
        LuaType::Union(_) => {
            // Source union: every member must be assignable to the target.
            Err(context.mismatch(source, compact_type))
        }
        LuaType::Intersection(_) => {
            // Source intersection: any member matching is enough (handled at the dispatch layer).
            Err(context.mismatch(source, compact_type))
        }
        LuaType::Call(source_call) => {
            // M0: call types such as keyof are not expanded (salsa has no member-key union).
            if let LuaType::Call(compact_call) = compact_type
                && compact_call.as_ref() == source_call.as_ref()
            {
                return Ok(());
            }
            Err(context.mismatch(source, compact_type))
        }
        LuaType::MultiLineUnion(source_multi) => {
            let union = source_multi.to_union();
            check_general_type_compact(context, &union, compact_type, check_guard.next_level()?)
        }
        _ => Err(context.mismatch(source, compact_type)),
    }
}
