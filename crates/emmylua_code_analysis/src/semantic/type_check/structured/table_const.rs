use crate::{
    InFiled, LuaArrayType, LuaMemberKey, LuaMemberOwner, LuaTupleType, LuaType,
    semantic::type_check::error_chain::{ChainMessage, not_assignable_message, property_message},
};

use super::super::{
    is_optional,
    relation::{IntersectionState, Relater, RelationFailure, RelationResult},
};

use super::{
    array::effective_array_base,
    declared::relate_structural_source_to_declared_target,
    member::{
        collect_missing_members, relate_index_member, relate_keyed_member,
        unrelated_missing_members, visit_member_items,
    },
    object_type::{relate_member_to_table_generic, relate_to_object_target},
};
use crate::semantic::type_check::OverflowKind;

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
        LuaType::Object(target_object) => Some(relate_to_object_target(
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
            if is_optional(relater.db(), target_type) {
                continue;
            }
            return relater.fail(|_| ChainMessage::MissingTupleElement { index });
        };
        let result = relater.relate(&source_type, target_type, intersection_state);
        relater.on_unrelated(result, |_| ChainMessage::TupleElement { index })?;
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
    if member_len == 0 {
        return Ok(());
    }
    if member_len > relater.remaining_relation_budget() {
        return Err(RelationFailure::Indeterminate(OverflowKind::Budget));
    }

    let db = relater.db();
    let mut checked = false;
    visit_member_items(db, &owner, |key, item| {
        if !matches!(key, LuaMemberKey::Integer(index) if *index > 0) {
            return Ok(());
        }
        relater.consume_relation_budget()?;
        let source_type = item.resolve_type(db).unwrap_or(LuaType::Any);
        let result = relater.relate(&source_type, &target_base, intersection_state);
        relater.on_unrelated(result, |_| property_message(key))?;
        checked = true;
        relater.note_progress();
        Ok(())
    })?;

    if checked {
        Ok(())
    } else {
        relater.fail(|db| not_assignable_message(db, source, target))
    }
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
        return relater.fail(|db| not_assignable_message(db, source, target));
    }

    let db = relater.db();
    let owner = LuaMemberOwner::Element(range.clone());
    visit_member_items(db, &owner, |key, item| {
        let source_value_type = item.resolve_type(db).unwrap_or(LuaType::Any);
        relate_member_to_table_generic(
            relater,
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

    if relater.is_explain() {
        let (missing_keys, _) =
            collect_missing_members(relater, source, target, intersection_state)?;
        if !missing_keys.is_empty() {
            return unrelated_missing_members(relater, source, target, missing_keys);
        }
    }

    visit_member_items(db, &owner, |key, item| {
        let target_member_type = item.resolve_type(db).unwrap_or(LuaType::Any);
        if let LuaMemberKey::TypeKey(target_key_type) = key {
            if intersection_state.contains(IntersectionState::TARGET) {
                return Ok(());
            }
            return relate_index_member(
                relater,
                source,
                target,
                target_key_type,
                &target_member_type,
                intersection_state,
            );
        }

        relate_keyed_member(
            relater,
            source,
            key,
            &target_member_type,
            intersection_state,
        )
    })
}
