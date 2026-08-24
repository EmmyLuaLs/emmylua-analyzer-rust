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

    // 基类型 id 相同时先按位置比较类型实参
    let fast_failure = if let LuaType::Generic(target_generic) = target
        && source_generic.get_base_type_id_ref() == target_generic.get_base_type_id_ref()
        && !source_decl.is_enum()
    {
        match relate_same_family_generic_args(
            relater,
            source_generic,
            target_generic,
            intersection_state,
        ) {
            SameFamilyArgsOutcome::Related => return Some(Ok(())),
            SameFamilyArgsOutcome::Proceed(failure) => failure,
        }
    } else {
        None
    };

    let result = if source_decl.is_alias() {
        let substitutor = TypeSubstitutor::from_alias(
            source_generic.get_params().clone(),
            source_generic.get_base_type_id(),
        );
        match source_decl.get_alias_origin(relater.db(), Some(&substitutor)) {
            Some(alias_origin) => Some(relater.relate(&alias_origin, target, intersection_state)),
            None => return Some(relater.unrelated(|| TypeMismatch::incompatible(source, target))),
        }
    } else if source_decl.is_class() {
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
    } else {
        return Some(relater.unrelated(|| TypeMismatch::incompatible(source, target)));
    };

    match (fast_failure, result) {
        // 多实参别名可能让不同参数处于不同方差位置, 首个快捷失败未必是真正的失败原因.
        // 单实参时不存在参数归因歧义, 可以用快捷失败压缩诊断路径.
        (Some(fast_failure), Some(Err(RelationFailure::Unrelated(_))))
            if source_decl.is_alias() && source_generic.get_params().len() == 1 =>
        {
            Some(Err(fast_failure))
        }
        (_, result) => result,
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
    let nominal_relation =
        classify_declared_type_relation(relater.db(), source_id, target_id, relater.policy());

    let same_family = source_id == target_id;
    let direct_result = 'direct: {
        if !same_family || source_generic.get_params().len() != target_generic.get_params().len() {
            break 'direct relater.unrelated(|| TypeMismatch::incompatible(source, target));
        }

        match relate_same_family_generic_args(
            relater,
            source_generic,
            target_generic,
            intersection_state,
        ) {
            SameFamilyArgsOutcome::Related | SameFamilyArgsOutcome::Proceed(None) => {
                // 此时无失配报告且参数量相等, 那么认为是成功的
                relater.note_progress();
                Ok(())
            }
            SameFamilyArgsOutcome::Proceed(Some(failure)) => break 'direct Err(failure),
        }
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

/// 同族泛型实参快捷比较的结果
enum SameFamilyArgsOutcome {
    Related,
    /// 需完整流程裁决
    Proceed(Option<RelationFailure>),
}

/// 基类型 id 相同时直接按位置比较类型实参, 不做结构展开.
fn relate_same_family_generic_args(
    relater: &mut Relater,
    source_generic: &LuaGenericType,
    target_generic: &LuaGenericType,
    intersection_state: IntersectionState,
) -> SameFamilyArgsOutcome {
    let source_params = source_generic.get_params();
    let target_params = target_generic.get_params();
    if source_params.len() != target_params.len() {
        return SameFamilyArgsOutcome::Proceed(None);
    }
    // 单实参失败时保留空 path, 诊断直接展示实参对比; 多实参需标注失配位置.
    let locate_argument = source_params.len() > 1;
    let mut all_trivial = true;
    for (index, (source_param, target_param)) in source_params.iter().zip(target_params).enumerate()
    {
        // 判定一对类型是否在任意方差位置都可互换, 不能用 `fast_eq_check`, 其在逆变位置会误放行.
        let trivial = source_param == target_param
            || matches!(source_param, LuaType::Any | LuaType::SelfInfer)
            || matches!(
                target_param,
                LuaType::Any | LuaType::Unknown | LuaType::SelfInfer
            )
            || matches!(source_param, LuaType::TplRef(tpl) if tpl.get_constraint().is_none())
            || matches!(target_param, LuaType::TplRef(tpl) if tpl.get_constraint().is_none());
        all_trivial &= trivial;
        if !trivial
            && let Err(failure) = relater.relate(source_param, target_param, intersection_state)
        {
            let failure = if locate_argument {
                failure
                    .map_mismatch(|mismatch| mismatch.at(TypePathSegment::GenericArgument(index)))
            } else {
                failure
            };
            return SameFamilyArgsOutcome::Proceed(Some(failure));
        }
    }
    if all_trivial {
        relater.note_progress();
        SameFamilyArgsOutcome::Related
    } else {
        SameFamilyArgsOutcome::Proceed(None)
    }
}
