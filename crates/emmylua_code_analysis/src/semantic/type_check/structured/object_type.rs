use crate::{LuaArrayType, LuaMemberKey, LuaObjectType, LuaTupleType, LuaType};

use super::super::{
    is_optional,
    mismatch::{TypeMismatch, TypeMismatchKind, TypePathSegment},
    relation::{IntersectionState, Relater, RelationResult},
};
use super::{
    array::effective_array_base,
    declared::relate_structural_source_to_declared_target,
    member::{
        collect_missing_members, relate_index_member, relate_keyed_member,
        unrelated_missing_members,
    },
    table_const::relate_to_table_const_target,
};

pub(super) fn relate_object_source(
    relater: &mut Relater,
    source: &LuaType,
    source_object: &LuaObjectType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Option<RelationResult> {
    match target {
        LuaType::Table => {
            relater.note_progress();
            Some(Ok(()))
        }
        LuaType::Object(target_object) => Some(relate_object_to_object(
            relater,
            source,
            target,
            source_object,
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
        LuaType::Tuple(target_tuple) => Some(relate_object_to_tuple(
            relater,
            source_object,
            target_tuple,
            intersection_state,
        )),
        LuaType::Array(target_array) => Some(relate_object_to_array(
            relater,
            source,
            target,
            source_object,
            target_array,
            intersection_state,
        )),
        LuaType::TableGeneric(target_params) => Some(relate_object_to_table_generic(
            relater,
            source,
            target,
            source_object,
            target_params,
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

pub(super) fn relate_object_to_tuple(
    relater: &mut Relater,
    source_object: &LuaObjectType,
    target_tuple: &LuaTupleType,
    intersection_state: IntersectionState,
) -> RelationResult {
    for (index, target_type) in target_tuple.get_types().iter().enumerate() {
        relater.consume_relation_budget()?;
        let key = LuaMemberKey::Integer(index as i64 + 1);
        let Some(source_type) = source_object.get_field(&key) else {
            if is_optional(relater.db(), target_type) {
                continue;
            }
            return relater
                .unrelated(|| TypeMismatch::new(TypeMismatchKind::MissingTupleElement { index }));
        };
        relater
            .relate(source_type, target_type, intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| mismatch.at(TypePathSegment::TupleElement(index)))
            })?;
        relater.note_progress();
    }
    Ok(())
}

pub(super) fn relate_object_to_array(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_object: &LuaObjectType,
    target_array: &LuaArrayType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let target_base = effective_array_base(relater, target_array.get_base());
    let mut checked = false;
    for (key, source_type) in source_object.get_fields() {
        if !matches!(key, LuaMemberKey::Integer(index) if *index > 0) {
            continue;
        }
        relater.consume_relation_budget()?;
        relater
            .relate(source_type, &target_base, intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| mismatch.at(TypePathSegment::Member(key.clone())))
            })?;
        checked = true;
        relater.note_progress();
    }
    for (source_key, source_type) in source_object.get_index_access() {
        if !source_key.is_integer() {
            continue;
        }
        relater.consume_relation_budget()?;
        relater
            .relate(source_type, &target_base, intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| {
                    mismatch.at(TypePathSegment::Index(source_key.clone()))
                })
            })?;
        checked = true;
        relater.note_progress();
    }

    if checked {
        Ok(())
    } else {
        relater.unrelated(|| TypeMismatch::incompatible(source, target))
    }
}

pub(super) fn relate_object_to_table_generic(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_object: &LuaObjectType,
    target_params: &[LuaType],
    intersection_state: IntersectionState,
) -> RelationResult {
    if target_params.len() != 2 {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    }

    for (key, source_type) in source_object.get_fields() {
        relate_member_to_table_generic(
            relater,
            key,
            source_type,
            target_params,
            intersection_state,
        )?;
    }
    for (source_key_type, source_type) in source_object.get_index_access() {
        relater.consume_relation_budget()?;
        relater
            .relate(source_key_type, &target_params[0], intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| {
                    mismatch.at(TypePathSegment::Index(source_key_type.clone()))
                })
            })?;
        relater
            .relate(source_type, &target_params[1], intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| {
                    mismatch.at(TypePathSegment::Index(source_key_type.clone()))
                })
            })?;
        relater.note_progress();
    }
    Ok(())
}

pub(super) fn relate_member_to_table_generic(
    relater: &mut Relater,
    key: &LuaMemberKey,
    source_value_type: &LuaType,
    target_params: &[LuaType],
    intersection_state: IntersectionState,
) -> RelationResult {
    relater.consume_relation_budget()?;
    let Some(source_key_type) = key.to_index_type() else {
        return Ok(());
    };
    relater
        .relate(&source_key_type, &target_params[0], intersection_state)
        .map_err(|failure| {
            failure.map_mismatch(|mismatch| {
                mismatch.at(TypePathSegment::Index(source_key_type.clone()))
            })
        })?;
    relater
        .relate(source_value_type, &target_params[1], intersection_state)
        .map_err(|failure| {
            failure.map_mismatch(|mismatch| mismatch.at(TypePathSegment::Member(key.clone())))
        })?;
    relater.note_progress();
    Ok(())
}

pub(in crate::semantic::type_check) fn relate_to_object_target(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    target_object: &LuaObjectType,
    intersection_state: IntersectionState,
) -> RelationResult {
    if let LuaType::Object(source_object) = source {
        return relate_object_to_object(
            relater,
            source,
            target,
            source_object,
            target_object,
            intersection_state,
        );
    }

    if relater.is_explain() {
        let (missing_keys, _) =
            collect_missing_members(relater, source, target, intersection_state)?;
        if !missing_keys.is_empty() {
            return unrelated_missing_members(relater, missing_keys);
        }
    }

    for (key, target_member_type) in target_object.get_fields() {
        relate_keyed_member(relater, source, key, target_member_type, intersection_state)?;
    }

    if !intersection_state.contains(IntersectionState::TARGET) {
        for (target_key_type, target_value_type) in target_object.get_index_access() {
            relate_index_member(
                relater,
                source,
                target,
                target_key_type,
                target_value_type,
                intersection_state,
            )?;
        }
    }

    Ok(())
}

#[inline(always)]
pub(super) fn relate_object_to_object(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_object: &LuaObjectType,
    target_object: &LuaObjectType,
    intersection_state: IntersectionState,
) -> RelationResult {
    // 对于解释模式, 我们应先做可空判断.
    if relater.is_explain() {
        let mut missing_keys = Vec::new();
        for (target_key, target_member_type) in target_object.get_fields() {
            if source_object.get_field(target_key).is_none()
                && !is_optional(relater.db(), target_member_type)
            {
                missing_keys.push(target_key.clone());
            }
        }
        if !missing_keys.is_empty() {
            return unrelated_missing_members(relater, missing_keys);
        }
    }

    for (target_key, target_member_type) in target_object.get_fields() {
        relater.consume_relation_budget()?;
        let source_member_type = source_object.get_field(target_key);
        let Some(source_member_type) = source_member_type else {
            if relater.is_explain() || is_optional(relater.db(), target_member_type) {
                continue;
            }
            return relater.unrelated(|| {
                TypeMismatch::new(TypeMismatchKind::MissingMembers {
                    keys: vec![target_key.clone()],
                })
            });
        };

        let field_result =
            relater.relate_field_types(source_member_type, target_member_type, intersection_state);
        if let Err(failure) = field_result {
            if relater.is_explain() {
                return Err(failure.map_mismatch(|mismatch| {
                    mismatch.at(TypePathSegment::Member(target_key.clone()))
                }));
            }
            return Err(failure);
        }
        relater.note_progress();
    }

    if !intersection_state.contains(IntersectionState::TARGET) {
        for (target_key_type, target_value_type) in target_object.get_index_access() {
            relate_index_member(
                relater,
                source,
                target,
                target_key_type,
                target_value_type,
                intersection_state,
            )?;
        }
    }

    Ok(())
}
