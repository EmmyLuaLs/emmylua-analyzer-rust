//! TableGeneric source 的具体结构关系.

use crate::{LuaArrayType, LuaType};

use super::super::{
    mismatch::{TypeMismatch, TypePathSegment},
    relation::{IntersectionState, Relater, RelationResult},
};
use super::{
    array::{append_array_element_path, effective_array_base},
    declared::relate_structural_source_to_declared_target,
    object_type::relate_object_members,
    table_const::relate_to_table_const_target,
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

pub(super) fn relate_table_generic_to_table_generic(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_params: &[LuaType],
    target_params: &[LuaType],
    intersection_state: IntersectionState,
) -> RelationResult {
    if source_params.len() != 2 || target_params.len() != 2 {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    }

    relater
        .relate(&source_params[0], &target_params[0], intersection_state)
        .map_err(|failure| {
            failure.map_mismatch(|mismatch| mismatch.at(TypePathSegment::GenericArgument(0)))
        })?;
    relater.note_progress();
    relater
        .relate(&source_params[1], &target_params[1], intersection_state)
        .map_err(|failure| {
            failure.map_mismatch(|mismatch| mismatch.at(TypePathSegment::GenericArgument(1)))
        })?;
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
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    }
    let target_base = effective_array_base(relater, target_array.get_base());
    relater
        .relate(&source_params[1], &target_base, intersection_state)
        .map_err(|failure| {
            failure.map_mismatch(|mismatch| {
                append_array_element_path(
                    mismatch,
                    source,
                    target,
                    &source_params[1],
                    target_array.get_base(),
                )
            })
        })
}
