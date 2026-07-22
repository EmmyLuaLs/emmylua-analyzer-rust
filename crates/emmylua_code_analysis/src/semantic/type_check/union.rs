use crate::{LuaType, LuaUnionType};

use super::{
    mismatch::{TypeMismatch, TypePathSegment},
    relation::{
        IntersectionState, Relater, RelationFailure, RelationKind, RelationOutcome, RelationResult,
    },
};

pub(crate) fn relate_union(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Option<RelationResult> {
    // source union 必须先分派, target union 只在普通 source 下作为候选义务.
    if let LuaType::Union(source_union) = source {
        return Some(relate_source_union(
            relater,
            source,
            source_union,
            target,
            intersection_state,
        ));
    }
    if let LuaType::Union(target_union) = target {
        return Some(relate_to_target_union(
            relater,
            source,
            target,
            target_union,
            intersection_state,
        ));
    }
    None
}

fn relate_source_union(
    relater: &mut Relater,
    source: &LuaType,
    source_union: &LuaUnionType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let members = source_union.into_vec();
    let conditional_extends = relater.kind() == RelationKind::ConditionalExtends;
    let mut first_indeterminate = None;
    for (index, member) in members.iter().enumerate() {
        match relater.probe_relation(member, target, intersection_state).0 {
            RelationOutcome::Related if conditional_extends => return Ok(()),
            RelationOutcome::Related => {}
            RelationOutcome::Indeterminate(kind) => {
                first_indeterminate.get_or_insert(kind);
            }
            RelationOutcome::Unrelated if conditional_extends => {}
            RelationOutcome::Unrelated => {
                if !relater.is_explain() {
                    return Err(RelationFailure::Unrelated(None));
                }
                return explain_union_constituent(
                    relater,
                    source,
                    target,
                    member,
                    target,
                    intersection_state,
                    TypePathSegment::SourceUnionMember(index),
                );
            }
        }
    }
    if let Some(kind) = first_indeterminate {
        return Err(relater.indeterminate_failure(kind, source, target));
    }
    if conditional_extends {
        relater.unrelated(|| TypeMismatch::incompatible(source, target))
    } else {
        Ok(())
    }
}

fn relate_to_target_union(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    target_union: &LuaUnionType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let candidates = target_union.into_vec();
    let mut best = None;
    let mut indeterminate = None;
    for (index, candidate) in candidates.iter().enumerate() {
        let (outcome, progress) = relater.probe_relation(source, candidate, intersection_state);
        match outcome {
            RelationOutcome::Related => return Ok(()),
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
    if let Some(kind) = indeterminate {
        return Err(relater.indeterminate_failure(kind, source, target));
    }

    let Some((best_index, _)) = best else {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    };
    if !relater.is_explain() {
        return Err(RelationFailure::Unrelated(None));
    }
    explain_union_constituent(
        relater,
        source,
        target,
        source,
        &candidates[best_index],
        intersection_state,
        TypePathSegment::TargetUnionCandidate(best_index),
    )
}

fn explain_union_constituent(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    constituent_source: &LuaType,
    constituent_target: &LuaType,
    intersection_state: IntersectionState,
    path: TypePathSegment,
) -> RelationResult {
    relater
        .relate(constituent_source, constituent_target, intersection_state)
        .map_err(|failure| failure.map_mismatch(|mismatch| mismatch.at(path, source, target)))
}
