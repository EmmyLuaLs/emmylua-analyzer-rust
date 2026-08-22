use crate::{
    LuaGenericType, LuaType, TypeSubstitutor,
    semantic::type_check::structured::declared::relate_declared_to_table_generic,
};

use super::super::{
    mismatch::{TypeMismatch, TypePathSegment},
    relation::{IntersectionState, Relater, RelationFailure, RelationOutcome, RelationResult},
};
use super::{
    array::relate_keyed_source_to_array,
    declared::{
        DeclaredTypeRelation, classify_declared_type_relation, declared_super_types,
        declared_type_has_members, relate_nominal_source_to_declared_target,
        relate_structural_source_to_declared_target,
    },
    object_type::{relate_object_members, relate_to_declared_target_members},
    table_const::relate_to_table_const_target,
    tuple::relate_keyed_source_to_tuple,
};

pub(super) fn relate_generic_source(
    relater: &mut Relater,
    source: &LuaType,
    source_generic: &LuaGenericType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Option<RelationResult> {
    let Some(source_decl) = relater
        .db()
        .get_type_index()
        .get_type_decl(source_generic.get_base_type_id_ref())
    else {
        return Some(relater.unrelated(|| TypeMismatch::incompatible(source, target)));
    };

    if source_decl.is_alias() {
        let substitutor = TypeSubstitutor::from_alias(
            source_generic.get_params().clone(),
            source_generic.get_base_type_id(),
        );
        let Some(alias_origin) = source_decl.get_alias_origin(relater.db(), Some(&substitutor))
        else {
            return Some(relater.unrelated(|| TypeMismatch::incompatible(source, target)));
        };
        return Some(relater.relate(&alias_origin, target, intersection_state));
    }
    if !source_decl.is_class() {
        return Some(relater.unrelated(|| TypeMismatch::incompatible(source, target)));
    }

    match target {
        LuaType::Tuple(target_tuple) => Some(relate_keyed_source_to_tuple(
            relater,
            source,
            target_tuple,
            intersection_state,
        )),
        LuaType::Array(target_array) => Some(relate_keyed_source_to_array(
            relater,
            source,
            target,
            target_array,
            intersection_state,
        )),
        LuaType::TableGeneric(target_params) => Some(relate_declared_to_table_generic(
            relater,
            source,
            target,
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
        LuaType::Ref(target_id) | LuaType::Def(target_id) => {
            Some(relate_nominal_source_to_declared_target(
                relater,
                source,
                source_generic.get_base_type_id_ref(),
                target,
                target_id,
                intersection_state,
            ))
        }
        LuaType::Generic(target_generic) => Some(relate_generic_source_to_generic_target(
            relater,
            source,
            source_generic,
            target,
            target_generic,
            intersection_state,
        )),
        LuaType::Table | LuaType::Userdata => {
            relater.note_progress();
            Some(Ok(()))
        }
        _ => None,
    }
}

fn relate_generic_source_to_generic_target(
    relater: &mut Relater,
    source: &LuaType,
    source_generic: &LuaGenericType,
    target: &LuaType,
    target_generic: &LuaGenericType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let Some(target_decl) = relater
        .db()
        .get_type_index()
        .get_type_decl(target_generic.get_base_type_id_ref())
    else {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    };
    if target_decl.is_alias() || target_decl.is_enum() {
        return relate_structural_source_to_declared_target(
            relater,
            source,
            target,
            intersection_state,
        );
    }

    let source_id = source_generic.get_base_type_id_ref();
    let target_id = target_generic.get_base_type_id_ref();
    let same_family = source_id == target_id;
    let nominal_relation =
        classify_declared_type_relation(relater.db(), source_id, target_id, relater.policy());

    let direct_result = 'direct: {
        if !same_family || source_generic.get_params().len() != target_generic.get_params().len() {
            break 'direct relater.unrelated(|| TypeMismatch::incompatible(source, target));
        }

        for (index, (source_param, target_param)) in source_generic
            .get_params()
            .iter()
            .zip(target_generic.get_params())
            .enumerate()
        {
            if let Err(failure) = relater.relate(source_param, target_param, intersection_state) {
                break 'direct Err(failure.map_mismatch(|mismatch| {
                    mismatch.at(TypePathSegment::GenericArgument(index))
                }));
            }
        }
        relater.note_progress();
        Ok(())
    };

    if same_family {
        return direct_result;
    }

    let mut indeterminate = match &direct_result {
        Err(RelationFailure::Indeterminate(kind)) => Some(*kind),
        _ => None,
    };
    for super_type in declared_super_types(relater.db(), source) {
        match relater
            .probe_relation(&super_type, target, intersection_state)
            .0
        {
            RelationOutcome::Related => {
                relater.note_progress();
                return Ok(());
            }
            RelationOutcome::Indeterminate(kind) => {
                indeterminate.get_or_insert(kind);
            }
            RelationOutcome::Unrelated => {}
        }
    }

    if let Some(kind) = indeterminate {
        return Err(RelationFailure::Indeterminate(kind));
    }

    if nominal_relation == DeclaredTypeRelation::LegacyReverse && !target_generic.contain_tpl() {
        relater.note_progress();
        return Ok(());
    }
    if nominal_relation == DeclaredTypeRelation::Forward {
        return direct_result;
    }
    if declared_type_has_members(relater.db(), target) {
        return relate_to_declared_target_members(relater, source, target, intersection_state);
    }

    direct_result
}
