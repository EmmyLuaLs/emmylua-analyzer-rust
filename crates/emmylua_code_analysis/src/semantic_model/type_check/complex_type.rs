//! Complex type checks: Array/TableGeneric/Tuple/Object/Call/MultiLineUnion.

use crate::semantic_model::type_eval::expand_alias_branches;
use crate::{IndexInfo, LuaMemberKey, LuaObjectType, LuaType};

use super::context::TypeCheckContext;
use super::{
    RequiredChecks, TypeCheckFailReason, TypeCheckResult, check_all, check_alternatives,
    check_general_type_compact,
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
                LuaType::Object(object) => check_sequence_object(context, source, object),
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
            LuaType::Array(array) => check_all(
                source_tuple
                    .get_types()
                    .iter()
                    .map(|actual| check_general_type_compact(context, actual, array.get_base())),
            ),
            LuaType::Object(object) => check_sequence_object(context, source, object),
            LuaType::Table | LuaType::TableGeneric(_) | LuaType::Ref(_) | LuaType::Def(_) => Ok(()),
            _ => Err(context.mismatch(source, compact_type)),
        },
        LuaType::Object(source_object) if context.strict_subtype => match compact_type {
            LuaType::Object(compact_object) => {
                let mut checks = RequiredChecks::default();
                for (key, expected) in compact_object.get_fields() {
                    let actual = source_object.get_field(key).unwrap_or(&LuaType::Nil);
                    checks.require(check_general_type_compact(context, actual, expected))?;
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

/// Arrays and tuples must satisfy both the fields and the index signatures of
/// an object. Extra keys are allowed; missing required fields are checked as nil.
fn check_sequence_object(
    context: &mut TypeCheckContext,
    source: &LuaType,
    object: &LuaObjectType,
) -> TypeCheckResult {
    let mut checks = RequiredChecks::default();
    for (key, expected) in object.get_fields() {
        let actual = match (source, key) {
            (LuaType::Array(array), LuaMemberKey::Integer(index)) if *index > 0 => {
                Some(array.get_base())
            }
            (LuaType::Tuple(tuple), LuaMemberKey::Integer(index)) if *index > 0 => {
                tuple.get_types().get(*index as usize - 1)
            }
            _ => None,
        }
        .unwrap_or(&LuaType::Nil);
        checks.require(check_general_type_compact(context, actual, expected))?;
    }

    for (key_type, expected) in object.get_index_access() {
        match source {
            LuaType::Array(array) => match integer_index_applies(context, key_type) {
                Ok(true) => checks.require(check_general_type_compact(
                    context,
                    array.get_base(),
                    expected,
                ))?,
                Ok(false) => {}
                Err(err) => checks.require(Err(err))?,
            },
            LuaType::Tuple(tuple) => {
                for (index, actual) in tuple.get_types().iter().enumerate() {
                    let key = LuaType::IntegerConst(index as i64 + 1);
                    match check_general_type_compact(context, &key, key_type) {
                        Ok(()) => {
                            checks.require(check_general_type_compact(context, actual, expected))?
                        }
                        Err(err) if err.is_type_not_match() => {}
                        Err(err) => checks.require(Err(err))?,
                    }
                }
            }
            _ => unreachable!("only arrays and tuples use sequence object checks"),
        }
    }
    checks.finish()
}

pub(crate) fn integer_index_applies(
    context: &mut TypeCheckContext,
    key_type: &LuaType,
) -> Result<bool, TypeCheckFailReason> {
    // A whole-type match can prove overlap without expanding recursive branches.
    if check_alternatives(
        context.mismatch(&LuaType::Integer, key_type),
        [
            check_general_type_compact(context, &LuaType::Integer, key_type),
            check_general_type_compact(context, key_type, &LuaType::Integer),
        ],
    )
    .is_ok()
    {
        return Ok(true);
    }
    // One numeric alternative is enough to constrain the array's values.
    // Use the shared alias expansion so named unions follow the same rule.
    let key_type = context
        .require_complete(|| expand_alias_branches(context.model, key_type))?
        .ok_or(TypeCheckFailReason::Unresolved)?;
    let failure = context.mismatch(&LuaType::Integer, &key_type);
    let keys = match key_type {
        LuaType::Union(union) => union.into_vec(),
        key => vec![key],
    };
    match check_alternatives(
        failure,
        keys.iter().flat_map(|key| {
            [
                check_general_type_compact(context, &LuaType::Integer, key),
                check_general_type_compact(context, key, &LuaType::Integer),
            ]
        }),
    ) {
        Ok(()) => Ok(true),
        Err(err) if err.is_type_not_match() => Ok(false),
        Err(err) => Err(err),
    }
}

/// Index signatures of `typ` whose key type accepts `key_type`.
pub(super) fn applicable_index_infos(
    context: &mut TypeCheckContext,
    typ: &LuaType,
    key_type: &LuaType,
) -> Result<Vec<IndexInfo>, TypeCheckFailReason> {
    let infos = context.require_complete(|| context.model.index_infos(typ))?;
    let mut applicable = Vec::new();
    for info in infos {
        match check_general_type_compact(context, key_type, &info.key_type) {
            Ok(()) => applicable.push(info),
            Err(err) if err.is_type_not_match() => {}
            Err(err) => return Err(err),
        }
    }
    Ok(applicable)
}
