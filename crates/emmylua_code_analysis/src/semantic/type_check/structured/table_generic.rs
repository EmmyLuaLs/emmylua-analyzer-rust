//! TableGeneric source 的具体结构关系.

use crate::{
    LuaArrayType, LuaType,
    semantic::type_check::error_chain::{ChainMessage, not_assignable_message},
};

use super::super::relation::{IntersectionState, Relater, RelationResult};
use super::{
    array::effective_array_base, declared::relate_structural_source_to_declared_target,
    object_type::relate_to_object_target, table_const::relate_to_table_const_target,
};

pub(super) fn relate_table_generic_source(
    relater: &mut Relater,
    source: &LuaType,
    source_params: &[LuaType],
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Option<RelationResult> {
    match target {
        LuaType::Table => {
            relater.note_progress();
            Some(Ok(()))
        }
        LuaType::TableGeneric(target_params) => Some(relate_table_generic_to_table_generic(
            relater,
            source,
            target,
            source_params,
            target_params,
            intersection_state,
        )),
        LuaType::Array(target_array) => Some(relate_table_generic_to_array(
            relater,
            source,
            target,
            source_params,
            target_array,
            intersection_state,
        )),
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

pub(super) fn relate_table_generic_to_table_generic(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_params: &[LuaType],
    target_params: &[LuaType],
    intersection_state: IntersectionState,
) -> RelationResult {
    if source_params.len() != 2 || target_params.len() != 2 {
        return relater.fail(|db| not_assignable_message(db, source, target));
    }

    let key_result = relater.relate(&source_params[0], &target_params[0], intersection_state);
    relater.on_unrelated(key_result, |_| ChainMessage::GenericArgument { index: 0 })?;
    relater.note_progress();
    let value_result = relater.relate(&source_params[1], &target_params[1], intersection_state);
    relater.on_unrelated(value_result, |_| ChainMessage::GenericArgument { index: 1 })?;
    relater.note_progress();
    Ok(())
}

pub(super) fn relate_table_generic_to_array(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_params: &[LuaType],
    target_array: &LuaArrayType,
    intersection_state: IntersectionState,
) -> RelationResult {
    if source_params.len() != 2 || (!source_params[0].is_integer() && !source_params[0].is_any()) {
        return relater.fail(|db| not_assignable_message(db, source, target));
    }
    let target_base = effective_array_base(relater, target_array.get_base());
    let result = relater.relate(&source_params[1], &target_base, intersection_state);
    relater.on_unrelated(result, |_| ChainMessage::ArrayElement)
}
