use hashbrown::HashSet;

use crate::{
    DbIndex, LuaGenericType, LuaMemberOwner, LuaType, LuaTypeDecl, LuaTypeDeclId, TypeSubstitutor,
    complete_type_generic_args_in_type, instantiate_type_generic,
    semantic::type_check::structured::object_type::{
        relate_member_to_table_generic, visit_declared_members,
    },
};

use super::super::{
    mismatch::TypeMismatch,
    relation::{
        DeclaredRelationPolicy, IntersectionState, Relater, RelationOutcome, RelationResult,
    },
    sub_type::{get_base_type_id, is_sub_type_of},
};
use super::{
    array::relate_keyed_source_to_array,
    object_type::{relate_object_members, relate_to_declared_target_members},
    table_const::relate_to_table_const_target,
    tuple::relate_keyed_source_to_tuple,
};

pub(super) fn relate_declared_source(
    relater: &mut Relater,
    source: &LuaType,
    source_id: &LuaTypeDeclId,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Option<RelationResult> {
    let Some(source_decl) = relater.db().get_type_index().get_type_decl(source_id) else {
        return Some(relater.unrelated(|| TypeMismatch::incompatible(source, target)));
    };

    if source_decl.is_alias() {
        let Some(alias_origin) = source_decl.get_alias_origin(relater.db(), None) else {
            return Some(relater.unrelated(|| TypeMismatch::incompatible(source, target)));
        };
        return Some(relater.relate(&alias_origin, target, intersection_state));
    }

    if source_decl.is_enum() {
        Some(relate_enum_source(
            relater,
            source,
            source_id,
            source_decl,
            target,
            intersection_state,
        ))
    } else {
        relate_class_source(relater, source, source_id, target, intersection_state)
    }
}

fn relate_enum_source(
    relater: &mut Relater,
    source: &LuaType,
    source_id: &LuaTypeDeclId,
    source_decl: &LuaTypeDecl,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> RelationResult {
    if matches!(target, LuaType::Ref(target_id) | LuaType::Def(target_id) if target_id == source_id)
    {
        relater.note_progress();
        return Ok(());
    }

    // enum Def 表示运行时声明表, 只额外保留宽 Table 与 TableGeneric 关系.
    if matches!(source, LuaType::Def(_)) {
        match target {
            LuaType::Table => {
                relater.note_progress();
                return Ok(());
            }
            LuaType::TableGeneric(target_params) => {
                return relate_declared_to_table_generic(
                    relater,
                    source,
                    target,
                    target_params,
                    intersection_state,
                );
            }
            _ => {}
        }
    }

    let Some(enum_fields) = source_decl.get_enum_field_type(relater.db()) else {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    };

    relater.relate(&enum_fields, target, intersection_state)
}

fn relate_class_source(
    relater: &mut Relater,
    source: &LuaType,
    source_id: &LuaTypeDeclId,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Option<RelationResult> {
    match target {
        LuaType::Tuple(target_tuple) => Some(relate_keyed_source_to_tuple(
            relater,
            source,
            target,
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
                source_id,
                target,
                target_id,
                intersection_state,
            ))
        }
        LuaType::Generic(target_generic) => Some(relate_class_source_to_generic_target(
            relater,
            source,
            source_id,
            target,
            target_generic,
            intersection_state,
        )),
        LuaType::Table | LuaType::Userdata => {
            relater.note_progress();
            Some(Ok(()))
        }
        _ => relate_class_source_to_simple_target(relater, source, source_id, target),
    }
}

pub(super) fn relate_base_source_to_declared_target(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Option<RelationResult> {
    let source_id = get_base_type_id(source)?;
    let target_id = match target {
        LuaType::Ref(target_id) | LuaType::Def(target_id) => target_id,
        LuaType::Generic(target_generic) => target_generic.get_base_type_id_ref(),
        _ => return None,
    };
    let Some(target_decl) = relater.db().get_type_index().get_type_decl(target_id) else {
        return Some(relater.unrelated(|| TypeMismatch::incompatible(source, target)));
    };
    if target_decl.is_alias() || target_decl.is_enum() {
        return Some(relate_structural_source_to_declared_target(
            relater,
            source,
            target,
            intersection_state,
        ));
    }

    let nominal_relation =
        classify_declared_type_relation(relater.db(), &source_id, target_id, relater.policy());
    let target_contains_tpl =
        matches!(target, LuaType::Generic(target_generic) if target_generic.contain_tpl());
    if nominal_relation == DeclaredTypeRelation::Forward
        || nominal_relation == DeclaredTypeRelation::LegacyReverse && !target_contains_tpl
    {
        relater.note_progress();
        Some(Ok(()))
    } else {
        Some(relater.unrelated(|| TypeMismatch::incompatible(source, target)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeclaredTypeRelation {
    Forward,
    LegacyReverse,
    Unrelated,
}

/// 统一判断声明类型的名义方向
pub(super) fn classify_declared_type_relation(
    db: &DbIndex,
    source_id: &LuaTypeDeclId,
    target_id: &LuaTypeDeclId,
    policy: DeclaredRelationPolicy,
) -> DeclaredTypeRelation {
    if source_id == target_id || is_sub_type_of(db, source_id, target_id) {
        DeclaredTypeRelation::Forward
    } else if policy == DeclaredRelationPolicy::LegacyAssignable
        && is_sub_type_of(db, target_id, source_id)
    {
        DeclaredTypeRelation::LegacyReverse
    } else {
        DeclaredTypeRelation::Unrelated
    }
}

pub(super) fn relate_nominal_source_to_declared_target(
    relater: &mut Relater,
    source: &LuaType,
    source_id: &LuaTypeDeclId,
    target: &LuaType,
    target_id: &LuaTypeDeclId,
    intersection_state: IntersectionState,
) -> RelationResult {
    let Some(target_decl) = relater.db().get_type_index().get_type_decl(target_id) else {
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

    if classify_declared_type_relation(relater.db(), source_id, target_id, relater.policy())
        != DeclaredTypeRelation::Unrelated
    {
        relater.note_progress();
        return Ok(());
    }

    if declared_type_has_members(relater.db(), target) {
        return relate_to_declared_target_members(relater, source, target, intersection_state);
    }

    relater.unrelated(|| TypeMismatch::incompatible(source, target))
}

fn relate_class_source_to_generic_target(
    relater: &mut Relater,
    source: &LuaType,
    source_id: &LuaTypeDeclId,
    target: &LuaType,
    target_generic: &LuaGenericType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let target_id = target_generic.get_base_type_id_ref();
    let Some(target_decl) = relater.db().get_type_index().get_type_decl(target_id) else {
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

    let completed_source = complete_type_generic_args_in_type(relater.db(), source);
    if completed_source != *source && matches!(completed_source, LuaType::Generic(_)) {
        return relater.relate(&completed_source, target, intersection_state);
    }

    match classify_declared_type_relation(relater.db(), source_id, target_id, relater.policy()) {
        DeclaredTypeRelation::Forward => {
            if source_id == target_id
                && target_generic.get_params().iter().all(|param| {
                    param.is_any() || matches!(param, LuaType::TplRef(_) | LuaType::StrTplRef(_))
                })
            {
                relater.note_progress();
                return Ok(());
            }

            let mut indeterminate = None;
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
                return Err(relater.indeterminate_failure(kind, source, target));
            }
            return relater.unrelated(|| TypeMismatch::incompatible(source, target));
        }
        DeclaredTypeRelation::LegacyReverse if !target_generic.contain_tpl() => {
            relater.note_progress();
            return Ok(());
        }
        DeclaredTypeRelation::LegacyReverse | DeclaredTypeRelation::Unrelated => {}
    }

    if declared_type_has_members(relater.db(), target) {
        return relate_to_declared_target_members(relater, source, target, intersection_state);
    }

    relater.unrelated(|| TypeMismatch::incompatible(source, target))
}

pub(super) fn relate_structural_source_to_declared_target(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let (target_id, substitutor) = match target {
        LuaType::Ref(target_id) | LuaType::Def(target_id) => (target_id.clone(), None),
        LuaType::Generic(target_generic) => (
            target_generic.get_base_type_id(),
            Some(TypeSubstitutor::from_alias(
                target_generic.get_params().clone(),
                target_generic.get_base_type_id(),
            )),
        ),
        _ => return relater.unrelated(|| TypeMismatch::incompatible(source, target)),
    };
    let Some(target_decl) = relater.db().get_type_index().get_type_decl(&target_id) else {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    };

    if target_decl.is_alias() {
        let Some(origin_type) = target_decl.get_alias_origin(relater.db(), substitutor.as_ref())
        else {
            return relater.unrelated(|| TypeMismatch::incompatible(source, target));
        };
        let origin_contains_source = match &origin_type {
            LuaType::Union(origin_union) => origin_union.into_vec().contains(&source),
            _ => origin_type == *source,
        };
        if origin_contains_source {
            relater.note_progress();
            return Ok(());
        }
        return relater.relate(source, &origin_type, intersection_state);
    }

    if target_decl.is_enum() {
        let Some(enum_fields) = target_decl.get_enum_field_type(relater.db()) else {
            return relater.unrelated(|| TypeMismatch::incompatible(source, target));
        };

        // enum 参与位运算时结果会被推断为 Integer, 但直接写入整数常量仍需匹配 enum 字段.
        if let LuaType::Union(enum_types) = &enum_fields
            && enum_types
                .into_vec()
                .iter()
                .all(|typ| matches!(typ, LuaType::DocIntegerConst(_) | LuaType::IntegerConst(_)))
            && matches!(source, LuaType::Integer)
        {
            relater.note_progress();
            return Ok(());
        }

        return relater.relate(source, &enum_fields, intersection_state);
    }

    relate_to_declared_target_members(relater, source, target, intersection_state)
}

fn relate_class_source_to_simple_target(
    relater: &mut Relater,
    source: &LuaType,
    source_id: &LuaTypeDeclId,
    target: &LuaType,
) -> Option<RelationResult> {
    let conditional_extends = false;
    match target {
        LuaType::String | LuaType::StringConst(_) | LuaType::Integer | LuaType::IntegerConst(_) => {
        }
        LuaType::DocStringConst(_) | LuaType::DocIntegerConst(_) if conditional_extends => {}
        _ => return None,
    }

    let target_is_literal = matches!(
        target,
        LuaType::StringConst(_)
            | LuaType::IntegerConst(_)
            | LuaType::DocStringConst(_)
            | LuaType::DocIntegerConst(_)
    );
    if !(conditional_extends && target_is_literal)
        && let Some(target_base_id) = get_base_type_id(target)
        && classify_declared_type_relation(
            relater.db(),
            source_id,
            &target_base_id,
            relater.policy(),
        ) != DeclaredTypeRelation::Unrelated
    {
        relater.note_progress();
        return Some(Ok(()));
    }

    Some(relater.unrelated(|| TypeMismatch::incompatible(source, target)))
}

pub(super) fn declared_type_has_members(db: &DbIndex, typ: &LuaType) -> bool {
    let mut pending = vec![typ.clone()];
    let mut visited = HashSet::new();
    while let Some(current) = pending.pop() {
        let type_id = match &current {
            LuaType::Ref(type_id) | LuaType::Def(type_id) => type_id.clone(),
            LuaType::Generic(generic) => generic.get_base_type_id(),
            _ => continue,
        };
        if !visited.insert(type_id.clone()) {
            continue;
        }
        if db
            .get_member_index()
            .get_member_len(&LuaMemberOwner::Type(type_id))
            > 0
        {
            return true;
        }
        pending.extend(declared_super_types(db, &current));
    }
    false
}

pub(super) fn declared_super_types(db: &DbIndex, typ: &LuaType) -> Vec<LuaType> {
    let (type_id, substitutor) = match typ {
        LuaType::Ref(type_id) | LuaType::Def(type_id) => (type_id.clone(), None),
        LuaType::Generic(generic) => (
            generic.get_base_type_id(),
            Some(TypeSubstitutor::from_type_array(
                generic.get_params().clone(),
            )),
        ),
        _ => return Vec::new(),
    };
    db.get_type_index()
        .get_super_types_iter(&type_id)
        .map(|supers| {
            supers
                .map(|super_type| {
                    substitutor
                        .as_ref()
                        .map(|substitutor| instantiate_type_generic(db, super_type, substitutor))
                        .unwrap_or_else(|| super_type.clone())
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn relate_declared_to_table_generic(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    target_params: &[LuaType],
    intersection_state: IntersectionState,
) -> RelationResult {
    if target_params.len() != 2 {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    }

    visit_declared_members(relater, source, |relater, key, source_value_type| {
        relate_member_to_table_generic(
            relater,
            source,
            target,
            key,
            source_value_type,
            target_params,
            intersection_state,
        )
    })
}
