use crate::{InFiled, LuaArrayType, LuaMemberKey, LuaMemberOwner, LuaTupleType, LuaType};

use super::super::{
    mismatch::{OverflowKind, TypeMismatch, TypeMismatchKind, TypePathSegment},
    relation::{IntersectionState, Relater, RelationFailure, RelationResult},
};
use super::{
    array::effective_array_base,
    declared::relate_structural_source_to_declared_target,
    object_type::{
        relate_index_obligation, relate_member_to_table_generic, relate_named_member_obligation,
        relate_object_members, visit_member_items,
    },
};

pub(super) fn relate_table_const_source(
    relater: &mut Relater,
    source: &LuaType,
    source_range: &InFiled<rowan::TextRange>,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Option<RelationResult> {
    match target {
        LuaType::Table => {
            relater.note_progress();
            Some(Ok(()))
        }
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
        LuaType::Tuple(target_tuple) => Some(relate_table_const_to_tuple(
            relater,
            source,
            target,
            source_range,
            target_tuple,
            intersection_state,
        )),
        LuaType::Array(target_array) => Some(relate_table_const_to_array(
            relater,
            source,
            target,
            source_range,
            target_array,
            intersection_state,
        )),
        LuaType::TableGeneric(target_params) => Some(relate_table_const_to_table_generic(
            relater,
            source,
            target,
            source_range,
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

pub(super) fn relate_table_const_to_tuple(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    range: &InFiled<rowan::TextRange>,
    target_tuple: &LuaTupleType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let owner = LuaMemberOwner::Element(range.clone());
    for (index, target_type) in target_tuple.get_types().iter().enumerate() {
        relater.consume_relation_budget()?;
        let key = LuaMemberKey::Integer(index as i64 + 1);
        let source_type = relater
            .db()
            .get_member_index()
            .get_member_item(&owner, &key)
            .map(|item| item.resolve_type(relater.db()).unwrap_or(LuaType::Any));
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

pub(super) fn relate_table_const_to_array(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    range: &InFiled<rowan::TextRange>,
    target_array: &LuaArrayType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let target_base = effective_array_base(relater, target_array.get_base());
    let owner = LuaMemberOwner::Element(range.clone());
    let member_len = relater.db().get_member_index().get_member_len(&owner);
    if member_len > relater.remaining_relation_budget() {
        return Err(RelationFailure::Indeterminate(OverflowKind::Budget));
    }

    for index in 0..member_len {
        relater.consume_relation_budget()?;
        let key = LuaMemberKey::Integer(index as i64 + 1);
        let Some(source_type) = relater
            .db()
            .get_member_index()
            .get_member_item(&owner, &key)
            .map(|item| item.resolve_type(relater.db()).unwrap_or(LuaType::Any))
        else {
            return relater.unrelated(|| TypeMismatch::incompatible(source, target));
        };
        relater
            .relate(&source_type, &target_base, intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| {
                    mismatch.at(TypePathSegment::TupleElement(index), source, target)
                })
            })?;
        relater.note_progress();
    }
    Ok(())
}

pub(super) fn relate_table_const_to_table_generic(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    range: &InFiled<rowan::TextRange>,
    target_params: &[LuaType],
    intersection_state: IntersectionState,
) -> RelationResult {
    if target_params.len() != 2 {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    }

    let db = relater.db();
    let owner = LuaMemberOwner::Element(range.clone());
    visit_member_items(db, &owner, |key, item| {
        let source_value_type = item.resolve_type(db).unwrap_or(LuaType::Any);
        relate_member_to_table_generic(
            relater,
            source,
            target,
            key,
            &source_value_type,
            target_params,
            intersection_state,
        )
    })
}

pub(super) fn relate_to_table_const_target(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    range: &InFiled<rowan::TextRange>,
    intersection_state: IntersectionState,
) -> RelationResult {
    let owner = LuaMemberOwner::Element(range.clone());
    let db = relater.db();
    visit_member_items(db, &owner, |key, item| {
        let target_member_type = item.resolve_type(db).unwrap_or(LuaType::Any);
        if let LuaMemberKey::TypeKey(target_key_type) = key {
            if intersection_state.contains(IntersectionState::TARGET) {
                return Ok(());
            }
            return relate_index_obligation(
                relater,
                source,
                target,
                target_key_type,
                &target_member_type,
                intersection_state,
            );
        }

        relate_named_member_obligation(
            relater,
            source,
            target,
            key,
            &target_member_type,
            intersection_state,
        )
    })
}
