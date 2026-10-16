//! Complex type checks: Union/Intersection/Array/TableGeneric/Tuple/Object/Call/MultiLineUnion.

use crate::{LuaMemberKey, LuaType};

use super::context::TypeCheckContext;
use super::{
    RequiredChecks, TypeCheckResult, check_all, check_alternatives, check_general_type_compact,
};

pub fn check_complex_type_compact(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_type: &LuaType,
) -> TypeCheckResult {
    match source {
        LuaType::Array(source_array) => {
            let base = source_array.get_base().clone();
            match compact_type {
                LuaType::Array(compact_array) => {
                    check_general_type_compact(context, &base, compact_array.get_base())
                }
                LuaType::Object(object) if context.mode.uses_assignment_rules() => {
                    let array_base = source_array.get_base();
                    check_alternatives(
                        context.mismatch(source, compact_type),
                        object
                            .get_index_access()
                            .iter()
                            .filter(|(key_ty, _)| {
                                matches!(
                                    key_ty,
                                    LuaType::Integer | LuaType::Number | LuaType::IntegerConst(_)
                                )
                            })
                            .map(|(_, expected)| {
                                check_general_type_compact(context, array_base, expected)
                            }),
                    )
                }
                LuaType::Tuple(tuple) => check_all(
                    tuple
                        .get_types()
                        .iter()
                        .map(|element| check_general_type_compact(context, &base, element)),
                ),
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
                check_all(
                    source_types
                        .iter()
                        .zip(compact_types)
                        .map(|(s, t)| check_general_type_compact(context, s, t)),
                )
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
                check_all(
                    source_types
                        .iter()
                        .zip(compact_types)
                        .map(|(s, t)| check_general_type_compact(context, s, t)),
                )
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
                let mut checks = RequiredChecks::default();
                for (key, expected) in source_object.get_fields() {
                    if let Some(actual) = compact_object.get_field(key) {
                        checks.require(check_general_type_compact(context, expected, actual))?;
                    }
                }
                checks.finish()
            }
            LuaType::Tuple(tuple) => {
                let mut checks = RequiredChecks::default();
                for (index, element_ty) in tuple.get_types().iter().enumerate() {
                    let key = LuaMemberKey::Integer(index as i64 + 1);
                    let Some(expected) = source_object.get_fields().get(&key) else {
                        return Err(context.mismatch(source, compact_type));
                    };
                    checks.require(check_general_type_compact(context, expected, element_ty))?;
                }
                checks.finish()
            }
            LuaType::TableConst(_) => {
                let table_infos =
                    context.require_complete(|| context.model.member_infos(compact_type))?;
                let mut checks = RequiredChecks::default();
                for (key, expected) in source_object.get_fields() {
                    if let Some(actual) = table_infos
                        .iter()
                        .find(|info| &info.key == key)
                        .map(|info| &info.typ)
                    {
                        checks.require(check_general_type_compact(context, expected, actual))?;
                    }
                }
                checks.finish()
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
            // M0: call types such as keyof are not expanded (semantic has no member-key union).
            if let LuaType::Call(compact_call) = compact_type
                && compact_call.as_ref() == source_call.as_ref()
            {
                return Ok(());
            }
            Err(context.mismatch(source, compact_type))
        }
        LuaType::MultiLineUnion(source_multi) => {
            let union = source_multi.to_union();
            check_general_type_compact(context, &union, compact_type)
        }
        _ => Err(context.mismatch(source, compact_type)),
    }
}
