use crate::{
    LuaArrayLen, LuaArrayType, LuaMemberKey, LuaTupleType, LuaType, LuaUnionType,
    find_members_with_key,
};

use super::super::{
    mismatch::{TypeMismatch, TypeMismatchKind, TypePathSegment},
    relation::{IntersectionState, Relater, RelationResult},
};
use super::{
    declared::relate_structural_source_to_declared_target, object_type::relate_object_members,
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
            source,
            target,
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
            failure
                .map_mismatch(|mismatch| mismatch.at(TypePathSegment::ArrayElement, source, target))
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
    source: &LuaType,
    target: &LuaType,
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
            TypeMismatch::new(
                source,
                target,
                TypeMismatchKind::Message(
                    t!(
                        "The target requires at least %{count} element(s) but source may have fewer.",
                        count = target_required_len
                    )
                    .to_string(),
                ),
            )
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
                failure.map_mismatch(|mismatch| {
                    mismatch.at(TypePathSegment::TupleElement(index), source, target)
                })
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
            failure.map_mismatch(|mismatch| {
                mismatch.at(TypePathSegment::GenericArgument(0), source, target)
            })
        })?;
    relater.note_progress();
    relater
        .relate(
            source_array.get_base(),
            &target_params[1],
            intersection_state,
        )
        .map_err(|failure| {
            failure.map_mismatch(|mismatch| {
                mismatch.at(TypePathSegment::GenericArgument(1), source, target)
            })
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
            failure
                .map_mismatch(|mismatch| mismatch.at(TypePathSegment::ArrayElement, source, target))
        })?;
    relater.note_progress();
    Ok(())
}
