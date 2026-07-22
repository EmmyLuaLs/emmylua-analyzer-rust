use crate::{LuaIntersectionType, LuaType};

use super::{
    mismatch::{TypeMismatch, TypePathSegment},
    relation::{IntersectionState, Relater, RelationFailure, RelationOutcome, RelationResult},
    structured::{relate_structured, relate_target_intersection_index_obligations},
};

pub(crate) fn relate_intersection(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    outer_intersection_state: IntersectionState,
) -> Option<RelationResult> {
    // target intersection 是 union 分解后的目标义务, 必须先于 source intersection 执行.
    if let LuaType::Intersection(target_intersection) = target {
        return Some(relate_to_target_intersection(
            relater,
            source,
            target,
            target_intersection,
            outer_intersection_state,
        ));
    }
    let LuaType::Intersection(source_intersection) = source else {
        return None;
    };
    Some(relate_source_intersection(
        relater,
        source,
        source_intersection,
        target,
        outer_intersection_state,
    ))
}

fn relate_to_target_intersection(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    target_intersection: &LuaIntersectionType,
    outer_intersection_state: IntersectionState,
) -> RelationResult {
    let mut indeterminate = None;
    for (index, member) in target_intersection.get_types().iter().enumerate() {
        match relater.relate(source, member, IntersectionState::TARGET) {
            Ok(()) => {}
            // 遇到 Indeterminate 仅先暂存, 因为可能存在更精确的报错信息.
            Err(failure @ RelationFailure::Indeterminate(_, _)) => {
                indeterminate.get_or_insert_with(|| {
                    failure.map_mismatch(|mismatch| {
                        mismatch.at(TypePathSegment::IntersectionMember(index), source, target)
                    })
                });
            }
            Err(failure @ RelationFailure::Unrelated(_)) => {
                return Err(failure.map_mismatch(|mismatch| {
                    mismatch.at(TypePathSegment::IntersectionMember(index), source, target)
                }));
            }
        }
    }

    if !outer_intersection_state.contains(IntersectionState::TARGET) {
        match relate_target_intersection_index_obligations(
            relater,
            source,
            target,
            target_intersection,
        ) {
            Ok(()) => {}
            Err(failure @ RelationFailure::Unrelated(_)) => return Err(failure),
            Err(failure @ RelationFailure::Indeterminate(_, _)) => {
                indeterminate.get_or_insert(failure);
            }
        }
    }

    if let Some(failure) = indeterminate {
        return Err(failure);
    }
    Ok(())
}

fn relate_source_intersection(
    relater: &mut Relater,
    source: &LuaType,
    source_intersection: &LuaIntersectionType,
    target: &LuaType,
    outer_intersection_state: IntersectionState,
) -> RelationResult {
    let constituent_state = IntersectionState::SOURCE;

    let mut best = None;
    let mut indeterminate = None;
    let mut related = false;
    for (index, member) in source_intersection.get_types().iter().enumerate() {
        let (outcome, progress) = relater.probe_relation(member, target, constituent_state);
        match outcome {
            RelationOutcome::Related => related = true,
            RelationOutcome::Indeterminate(kind) => {
                indeterminate.get_or_insert(kind);
            }
            RelationOutcome::Unrelated => {
                if best
                    .map(|(_, current_progress)| progress > current_progress)
                    .unwrap_or(true)
                {
                    best = Some((index, progress));
                }
            }
        }
    }

    // 结构 target 的成员与索引义务必须先检查完整 intersection, 不能由单个 constituent 跳过.
    if let Some(result) = relate_structured(relater, source, target, outer_intersection_state) {
        return result;
    }
    if related {
        return Ok(());
    }
    if let Some(kind) = indeterminate {
        return Err(relater.indeterminate_failure(kind, source, target));
    }
    let Some((best_index, _)) = best else {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    };
    if !relater.is_explain() {
        return Err(RelationFailure::Unrelated(None));
    }
    relater
        .relate(
            &source_intersection.get_types()[best_index],
            target,
            constituent_state,
        )
        .map_err(|failure| {
            failure.map_mismatch(|mismatch| {
                mismatch.at(
                    TypePathSegment::IntersectionMember(best_index),
                    source,
                    target,
                )
            })
        })
}
