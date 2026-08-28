use crate::{
    LuaArrayLen, LuaArrayType, LuaMemberKey, LuaObjectType, LuaTupleType, LuaType, LuaUnionType,
    find_members_with_key,
};

use super::super::{
    mismatch::{TypeMismatch, TypeMismatchKind, TypePathInfo, TypePathSegment},
    relation::{IntersectionState, Relater, RelationFailure, RelationResult},
};
use super::{
    declared::{resolve_declared_target_alias_or_enum, visit_declared_members},
    member::relate_index_member,
    table_const::relate_to_table_const_target,
};

pub(super) fn relate_array_source(
    relater: &mut Relater,
    source: &LuaType,
    source_array: &LuaArrayType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Option<RelationResult> {
    match target {
        LuaType::Table => {
            relater.note_progress();
            Some(Ok(()))
        }
        LuaType::Array(target_array) => Some(relate_array_to_array(
            relater,
            source,
            target,
            source_array,
            target_array,
            intersection_state,
        )),
        LuaType::Tuple(target_tuple) => Some(relate_array_to_tuple(
            relater,
            source_array,
            target_tuple,
            intersection_state,
        )),
        LuaType::TableGeneric(target_params) => Some(relate_array_to_table_generic(
            relater,
            source,
            target,
            source_array,
            target_params,
            intersection_state,
        )),
        LuaType::Object(target_object) => Some(relate_array_to_object(
            relater,
            source,
            target,
            source_array,
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
            Some(relate_array_to_declared_target(
                relater,
                source,
                target,
                source_array,
                intersection_state,
            ))
        }
        _ => None,
    }
}

#[inline(always)]
pub(in crate::semantic::type_check) fn relate_array_to_array(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_array: &LuaArrayType,
    target_array: &LuaArrayType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let target_base = effective_array_base(relater, target_array.get_base());
    relater
        .relate(source_array.get_base(), &target_base, intersection_state)
        .map_err(|failure| {
            failure.map_mismatch(|mismatch| {
                append_array_element_path(
                    mismatch,
                    source,
                    target,
                    source_array.get_base(),
                    target_array.get_base(),
                )
            })
        })
}

pub(super) fn effective_array_base(relater: &Relater, base: &LuaType) -> LuaType {
    if !relater.db().get_emmyrc().strict.array_index || base.is_optional() {
        base.clone()
    } else {
        LuaUnionType::Nullable(base.clone()).into()
    }
}

pub(super) fn relate_array_to_tuple(
    relater: &mut Relater,
    source_array: &LuaArrayType,
    target_tuple: &LuaTupleType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let target_tuple_types = target_tuple.get_types();
    let mut target_required_len = 0;
    for (index, target_type) in target_tuple_types.iter().enumerate() {
        match target_type {
            LuaType::Variadic(variadic) => {
                if let Some(min_len) = variadic.get_min_len() {
                    target_required_len = target_required_len.max(index + min_len);
                }
            }
            _ if !target_type.is_optional() => target_required_len = index + 1,
            _ => {}
        }
    }

    let source_min_len = match source_array.get_len() {
        LuaArrayLen::None => 0,
        LuaArrayLen::Max(len) => usize::try_from(*len).unwrap_or(0),
    };
    if source_min_len < target_required_len {
        return relater.unrelated(|| {
            TypeMismatch::new(TypeMismatchKind::Message(
                t!(
                    "The target requires at least %{count} element(s) but source may have fewer.",
                    count = target_required_len
                )
                .to_string(),
            ))
        });
    }

    let target_tuple_check_len = match target_tuple_types.last() {
        Some(LuaType::Variadic(variadic)) => {
            let prefix_len = target_tuple_types.len() - 1;
            prefix_len
                + variadic
                    .get_max_len()
                    .unwrap_or_else(|| variadic.get_min_len().map_or(1, |len| len + 1))
        }
        _ => target_tuple_types.len(),
    };
    for index in 0..target_tuple_check_len {
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
        relater
            .relate(source_array.get_base(), target_type, intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| mismatch.at(TypePathSegment::TupleElement(index)))
            })?;
        relater.note_progress();
    }
    Ok(())
}

pub(super) fn relate_array_to_table_generic(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_array: &LuaArrayType,
    target_params: &[LuaType],
    intersection_state: IntersectionState,
) -> RelationResult {
    if target_params.len() != 2 {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    }

    relater
        .relate(&LuaType::Integer, &target_params[0], intersection_state)
        .map_err(|failure| {
            failure.map_mismatch(|mismatch| mismatch.at(TypePathSegment::GenericArgument(0)))
        })?;
    relater.note_progress();
    relater
        .relate(
            source_array.get_base(),
            &target_params[1],
            intersection_state,
        )
        .map_err(|failure| {
            failure.map_mismatch(|mismatch| mismatch.at(TypePathSegment::GenericArgument(1)))
        })?;
    relater.note_progress();
    Ok(())
}

pub(super) fn relate_keyed_source_to_array(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    target_array: &LuaArrayType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let target_base = effective_array_base(relater, target_array.get_base());
    relater.consume_relation_budget()?;
    let source_type = find_members_with_key(
        relater.db(),
        source,
        LuaMemberKey::TypeKey(LuaType::Integer),
        false,
    )
    .and_then(|members| members.into_iter().next())
    .map(|member| member.typ);
    let Some(source_type) = source_type else {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    };
    relater
        .relate(&source_type, &target_base, intersection_state)
        .map_err(|failure| {
            failure.map_mismatch(|mismatch| {
                append_array_element_path(
                    mismatch,
                    source,
                    target,
                    &source_type,
                    target_array.get_base(),
                )
            })
        })?;
    relater.note_progress();
    Ok(())
}

fn relate_array_to_object(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_array: &LuaArrayType,
    target_object: &LuaObjectType,
    intersection_state: IntersectionState,
) -> RelationResult {
    // 如果目标含有必需的命名字段, 数组没有命名形状, 直接不兼容
    for (key, member_type) in target_object.get_fields() {
        match key {
            LuaMemberKey::Integer(index) if *index > 0 => {
                // 已知长度覆盖该索引时成员必然存在, 否则保留数组越界产生的 nil.
                let source_member_type = if matches!(source_array.get_len(), LuaArrayLen::Max(max_len) if index <= max_len)
                {
                    source_array.get_base().clone()
                } else {
                    effective_array_base(relater, source_array.get_base())
                };
                relater.consume_relation_budget()?;
                relater
                    .relate(&source_member_type, member_type, intersection_state)
                    .map_err(|failure| {
                        failure.map_mismatch(|mismatch| {
                            append_array_element_path(
                                mismatch,
                                source,
                                target,
                                &source_member_type,
                                member_type,
                            )
                        })
                    })?;
                relater.note_progress();
            }
            _ => {
                if !member_type.is_optional() {
                    return relater.unrelated(|| TypeMismatch::incompatible(source, target));
                }
            }
        }
    }

    if !intersection_state.contains(IntersectionState::TARGET) {
        for (target_key_type, target_value_type) in target_object.get_index_access() {
            relater.consume_relation_budget()?;
            relater
                .relate(&LuaType::Integer, target_key_type, intersection_state)
                .map_err(|failure| {
                    failure.map_mismatch(|mismatch| {
                        mismatch.at(TypePathSegment::Index(LuaType::Integer))
                    })
                })?;
            relater
                .relate(
                    source_array.get_base(),
                    target_value_type,
                    intersection_state,
                )
                .map_err(|failure| {
                    failure.map_mismatch(|mismatch| {
                        append_array_element_path(
                            mismatch,
                            source,
                            target,
                            source_array.get_base(),
                            target_value_type,
                        )
                    })
                })?;
            relater.note_progress();
        }
    }

    Ok(())
}

fn relate_array_to_declared_target(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_array: &LuaArrayType,
    intersection_state: IntersectionState,
) -> RelationResult {
    if let Some(result) =
        resolve_declared_target_alias_or_enum(relater, source, target, intersection_state)
    {
        return result;
    }

    // 检查是否有整数索引或索引访问, 如果没有, 则不兼容
    let mut has_integer_or_index = false;
    let mut mismatch = None;
    let result =
        visit_declared_members(
            relater,
            target,
            |relater, key, target_member_type| match key {
                LuaMemberKey::Integer(idx) if *idx > 0 => {
                    has_integer_or_index = true;
                    // 已知长度覆盖该索引时成员必然存在, 否则保留数组越界产生的 nil.
                    let source_member_type = if matches!(
                        source_array.get_len(),
                        LuaArrayLen::Max(max_len) if idx <= max_len
                    ) {
                        source_array.get_base().clone()
                    } else {
                        effective_array_base(relater, source_array.get_base())
                    };
                    relater.consume_relation_budget()?;
                    relater
                        .relate(&source_member_type, target_member_type, intersection_state)
                        .map_err(|failure| {
                            failure.map_mismatch(|mismatch| {
                                append_array_element_path(
                                    mismatch,
                                    source,
                                    target,
                                    &source_member_type,
                                    target_member_type,
                                )
                            })
                        })?;
                    relater.note_progress();
                    Ok(())
                }
                LuaMemberKey::TypeKey(target_key_type) => {
                    has_integer_or_index = true;
                    if intersection_state.contains(IntersectionState::TARGET) {
                        return Ok(());
                    }
                    relate_index_member(
                        relater,
                        source,
                        target,
                        target_key_type,
                        target_member_type,
                        intersection_state,
                    )
                }
                _ => {
                    if !target_member_type.is_optional() {
                        mismatch = Some(TypeMismatch::incompatible(source, target));
                        return Err(RelationFailure::Unrelated(mismatch.clone()));
                    }
                    Ok(())
                }
            },
        );

    if let Some(m) = mismatch {
        return relater.unrelated(|| m);
    }
    if result.is_ok() && !has_integer_or_index {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    }
    result
}

pub(super) fn append_array_element_path(
    mismatch: TypeMismatch,
    source: &LuaType,
    target: &LuaType,
    source_element: &LuaType,
    target_element: &LuaType,
) -> TypeMismatch {
    let include_element =
        mismatch.has_path() || !matches!(mismatch.reason(), TypeMismatchKind::Incompatible { .. });
    let outer_relation = std::iter::once(TypePathInfo::relation(source, target));
    let element_relation = include_element
        .then(|| TypePathInfo::relation(source_element, target_element))
        .into_iter();
    mismatch.at_with_info(
        TypePathSegment::ArrayElement,
        outer_relation.chain(element_relation),
    )
}
