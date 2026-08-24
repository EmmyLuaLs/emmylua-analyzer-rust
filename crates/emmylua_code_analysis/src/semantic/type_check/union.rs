use crate::{BasicTypeKind, LuaType, LuaUnionType};

use super::{
    mismatch::{TypeMismatch, TypeMismatchKind},
    relation::{IntersectionState, Relater, RelationFailure, RelationOutcome, RelationResult},
};

pub(crate) fn relate_union(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Option<RelationResult> {
    // source union 必须先分派, target union 只在普通 source 下作为候选义务.
    if let LuaType::Union(source_union) = source {
        let result = match source_union.as_ref() {
            LuaUnionType::Multi(members) => {
                relate_source_union_members(relater, members, target, intersection_state)
            }
            _ => {
                let members = source_union.into_vec();
                relate_source_union_members(relater, &members, target, intersection_state)
            }
        };
        return Some(result);
    }
    if let LuaType::Union(target_union) = target {
        let result = match target_union.as_ref() {
            LuaUnionType::Nullable(non_nil_target) => relate_to_nullable_target(
                relater,
                source,
                target,
                non_nil_target,
                intersection_state,
            ),
            LuaUnionType::Basic(basic) if basic.contains(BasicTypeKind::Nil) => {
                let mut non_nil_basic = *basic;
                non_nil_basic.remove(BasicTypeKind::Nil);
                if non_nil_basic.is_empty() {
                    relater.relate(source, &LuaType::Nil, intersection_state)
                } else {
                    relate_to_nullable_target(
                        relater,
                        source,
                        target,
                        &LuaUnionType::Basic(non_nil_basic).into(),
                        intersection_state,
                    )
                }
            }
            LuaUnionType::Multi(candidates) => relate_to_target_union_candidates(
                relater,
                source,
                target,
                candidates,
                intersection_state,
            ),
            _ => {
                let candidates = target_union.into_vec();
                relate_to_target_union_candidates(
                    relater,
                    source,
                    target,
                    &candidates,
                    intersection_state,
                )
            }
        };
        return Some(result);
    }
    None
}

#[inline(always)]
pub(super) fn relate_to_nullable_target(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    non_nil_target: &LuaType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let non_nil_failure = match relater.relate(source, non_nil_target, intersection_state) {
        Ok(()) => return Ok(()),
        Err(failure) => failure,
    };
    let nil_outcome = relater
        .probe_relation(source, &LuaType::Nil, intersection_state)
        .0;
    match (non_nil_failure, nil_outcome) {
        (_, RelationOutcome::Related) => Ok(()),
        (RelationFailure::Indeterminate(kind), _) => Err(RelationFailure::Indeterminate(kind)),
        (RelationFailure::Unrelated(_), RelationOutcome::Indeterminate(kind)) => {
            Err(RelationFailure::Indeterminate(kind))
        }
        (RelationFailure::Unrelated(mismatch), RelationOutcome::Unrelated) => {
            if mismatch.as_ref().is_some_and(|m| {
                m.has_path() || !matches!(m.reason(), TypeMismatchKind::Incompatible { .. })
            }) {
                Err(RelationFailure::Unrelated(mismatch))
            } else {
                relater.unrelated(|| TypeMismatch::incompatible(source, target))
            }
        }
    }
}

fn relate_source_union_members(
    relater: &mut Relater,
    members: &[LuaType],
    target: &LuaType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let mut first_indeterminate = None;
    for member in members {
        match relater.probe_relation(member, target, intersection_state).0 {
            RelationOutcome::Related => {}
            RelationOutcome::Indeterminate(kind) => {
                first_indeterminate.get_or_insert(kind);
            }
            RelationOutcome::Unrelated => {
                if !relater.is_explain() {
                    return Err(RelationFailure::Unrelated(None));
                }
                return relater.relate(member, target, intersection_state);
            }
        }
    }
    if let Some(kind) = first_indeterminate {
        return Err(RelationFailure::Indeterminate(kind));
    }
    Ok(())
}

fn relate_to_target_union_candidates(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    candidates: &[LuaType],
    intersection_state: IntersectionState,
) -> RelationResult {
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
        return Err(RelationFailure::Indeterminate(kind));
    }

    let Some((best_index, best_progress)) = best else {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    };
    if !relater.is_explain() {
        return Err(RelationFailure::Unrelated(None));
    }
    if best_progress == 0 {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    }
    relater.relate(source, &candidates[best_index], intersection_state)
}
