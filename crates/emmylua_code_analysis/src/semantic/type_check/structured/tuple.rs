use crate::{
    LuaArrayType, LuaMemberKey, LuaTupleType, LuaType, VariadicType, find_members_with_key,
};

use super::super::{
    mismatch::{TypeMismatch, TypeMismatchKind, TypePathSegment},
    relation::{IntersectionState, Relater, RelationResult},
};
use super::{
    array::effective_array_base, declared::relate_structural_source_to_declared_target,
    object_type::relate_object_members, table_const::relate_to_table_const_target,
};

pub(super) fn relate_tuple_source(
    relater: &mut Relater,
    source: &LuaType,
    source_tuple: &LuaTupleType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Option<RelationResult> {
    match target {
        LuaType::Table => {
            relater.note_progress();
            Some(Ok(()))
        }
        LuaType::Tuple(target_tuple) => Some(relate_tuple_to_tuple(
            relater,
            source,
            target,
            source_tuple,
            target_tuple,
            intersection_state,
        )),
        LuaType::Array(target_array) => Some(relate_tuple_to_array(
            relater,
            source,
            target,
            source_tuple,
            target_array,
            intersection_state,
        )),
        LuaType::TableGeneric(target_params) => Some(relate_tuple_to_table_generic(
            relater,
            source,
            target,
            source_tuple,
            target_params,
            intersection_state,
        )),
        LuaType::Object(target_object) => Some(relate_object_members(
            relater,
            source,
            target,
            target_object,
            intersection_state,
        )),
        LuaType::TableConst(target_range) => Some(relate_to_table_const_target(
            relater,
            source,
            target,
            target_range,
            intersection_state,
        )),
        LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_) => {
            Some(relate_structural_source_to_declared_target(
                relater,
                source,
                target,
                intersection_state,
            ))
        }
        _ => None,
    }
}

pub(super) fn relate_tuple_to_tuple(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_tuple: &LuaTupleType,
    target_tuple: &LuaTupleType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let source_check_len = match source_tuple.get_types().last() {
        Some(LuaType::Variadic(variadic)) => {
            let prefix_len = source_tuple.len() - 1;
            prefix_len
                + variadic
                    .get_max_len()
                    .unwrap_or_else(|| variadic.get_min_len().map_or(1, |len| len + 1))
        }
        _ => source_tuple.len(),
    };
    let (target_required_len, target_check_len) = match target_tuple.get_types().last() {
        Some(LuaType::Variadic(variadic)) => {
            let prefix_len = target_tuple.len() - 1;
            let required_len = prefix_len + variadic.get_min_len().unwrap_or(0);
            let check_len = variadic
                .get_max_len()
                .map(|len| prefix_len + len)
                .unwrap_or_else(|| source_check_len.max(required_len));
            (required_len, check_len)
        }
        _ => (target_tuple.len(), target_tuple.len()),
    };

    for index in 0..target_check_len {
        relater.consume_relation_budget()?;
        let Some(target_type) = target_tuple.get_type(index).and_then(|target_type| {
            if let LuaType::Variadic(variadic) = target_type {
                variadic.get_type(0)
            } else {
                Some(target_type)
            }
        }) else {
            continue;
        };
        let source_type = source_tuple.get_type(index).and_then(|source_type| {
            if let LuaType::Variadic(variadic) = source_type {
                variadic.get_type(0)
            } else {
                Some(source_type)
            }
        });
        let Some(source_type) = source_type else {
            if index >= target_required_len || target_type.is_optional() {
                continue;
            }
            return relater.unrelated(|| {
                TypeMismatch::new(
                    source,
                    target,
                    TypeMismatchKind::MissingTupleElement { index },
                )
            });
        };

        relater
            .relate(source_type, target_type, intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| {
                    mismatch.at(TypePathSegment::TupleElement(index), source, target)
                })
            })?;
        relater.note_progress();
    }

    Ok(())
}

pub(super) fn relate_tuple_to_array(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_tuple: &LuaTupleType,
    target_array: &LuaArrayType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let target_base = effective_array_base(relater, target_array.get_base());
    for (index, source_type) in source_tuple.get_types().iter().enumerate() {
        relater.consume_relation_budget()?;
        let source_type = match source_type {
            LuaType::Variadic(variadic) => match variadic.as_ref() {
                VariadicType::Base(base) => base,
                VariadicType::Multi(types) => {
                    for (offset, source_type) in types.iter().enumerate() {
                        relater
                            .relate(source_type, &target_base, intersection_state)
                            .map_err(|failure| {
                                failure.map_mismatch(|mismatch| {
                                    mismatch.at(
                                        TypePathSegment::TupleElement(index + offset),
                                        source,
                                        target,
                                    )
                                })
                            })?;
                        relater.note_progress();
                    }
                    continue;
                }
            },
            source_type => source_type,
        };
        relater
            .relate(source_type, &target_base, intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| {
                    mismatch.at(TypePathSegment::TupleElement(index), source, target)
                })
            })?;
        relater.note_progress();
    }
    Ok(())
}

pub(super) fn relate_tuple_to_table_generic(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_tuple: &LuaTupleType,
    target_params: &[LuaType],
    intersection_state: IntersectionState,
) -> RelationResult {
    if target_params.len() != 2 {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    }

    visit_tuple_index_entries(source_tuple, |key_type, source_type, index| {
        relater.consume_relation_budget()?;
        relater
            .relate(key_type, &target_params[0], intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| {
                    mismatch.at(TypePathSegment::GenericArgument(0), source, target)
                })
            })?;
        relater
            .relate(source_type, &target_params[1], intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| {
                    mismatch.at(TypePathSegment::TupleElement(index), source, target)
                })
            })?;
        relater.note_progress();
        Ok(())
    })
}

/// 按真实索引遍历元组元素, 无界可变尾部使用抽象整数键.
pub(super) fn visit_tuple_index_entries<E>(
    tuple: &LuaTupleType,
    mut visitor: impl FnMut(&LuaType, &LuaType, usize) -> Result<(), E>,
) -> Result<(), E> {
    let mut index = 0;
    for typ in tuple.get_types() {
        if let LuaType::Variadic(variadic) = typ {
            if visit_variadic_index_entries(variadic, &mut index, &mut visitor)? {
                break;
            }
        } else {
            let key = LuaType::IntegerConst(index as i64 + 1);
            visitor(&key, typ, index)?;
            index += 1;
        }
    }
    Ok(())
}

fn visit_variadic_index_entries<E>(
    variadic: &VariadicType,
    index: &mut usize,
    visitor: &mut impl FnMut(&LuaType, &LuaType, usize) -> Result<(), E>,
) -> Result<bool, E> {
    match variadic {
        VariadicType::Base(base) => {
            visitor(&LuaType::Integer, base, *index)?;
            Ok(true)
        }
        VariadicType::Multi(types) => {
            for typ in types {
                if let LuaType::Variadic(inner) = typ {
                    if visit_variadic_index_entries(inner, index, visitor)? {
                        return Ok(true);
                    }
                } else {
                    let key = LuaType::IntegerConst(*index as i64 + 1);
                    visitor(&key, typ, *index)?;
                    *index += 1;
                }
            }
            Ok(false)
        }
    }
}

pub(super) fn relate_keyed_source_to_tuple(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    target_tuple: &LuaTupleType,
    intersection_state: IntersectionState,
) -> RelationResult {
    for (index, target_type) in target_tuple.get_types().iter().enumerate() {
        relater.consume_relation_budget()?;
        let key = LuaMemberKey::Integer(index as i64 + 1);
        let source_type = find_members_with_key(relater.db(), source, key, false)
            .and_then(|members| members.into_iter().next())
            .map(|member| member.typ);
        let Some(source_type) = source_type else {
            if target_type.is_optional() {
                continue;
            }
            return relater.unrelated(|| {
                TypeMismatch::new(
                    source,
                    target,
                    TypeMismatchKind::MissingTupleElement { index },
                )
            });
        };
        relater
            .relate(&source_type, target_type, intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| {
                    mismatch.at(TypePathSegment::TupleElement(index), source, target)
                })
            })?;
        relater.note_progress();
    }
    Ok(())
}
