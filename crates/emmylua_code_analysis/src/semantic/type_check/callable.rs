use crate::{LuaFunctionType, LuaType, collect_callable_overload_groups};

use super::{
    mismatch::{TypeMismatch, TypePathSegment},
    normalize_type,
    relation::{IntersectionState, Relater, RelationFailure, RelationOutcome, RelationResult},
};

pub(crate) fn relate_callable(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Option<RelationResult> {
    match source {
        LuaType::Function => {
            return callable_candidates(relater, target).map(|candidates| {
                if candidates.is_empty() {
                    relater.unrelated(|| TypeMismatch::incompatible(source, target))
                } else {
                    relater.note_progress();
                    Ok(())
                }
            });
        }
        LuaType::DocFunction(source_func) => {
            if let LuaType::DocFunction(target_func) = target {
                return Some(relate_function(
                    relater,
                    source,
                    source_func,
                    target,
                    target_func,
                    intersection_state,
                ));
            }
        }
        LuaType::Signature(_)
        | LuaType::Ref(_)
        | LuaType::Def(_)
        | LuaType::Generic(_)
        | LuaType::TableConst(_) => {}
        _ => return None,
    }

    let source_candidates = callable_candidates(relater, source)?;
    if source_candidates.is_empty() {
        return Some(relater.unrelated(|| TypeMismatch::incompatible(source, target)));
    }

    if matches!(target, LuaType::Function) {
        relater.note_progress();
        return Some(Ok(()));
    }

    let target_candidates = callable_candidates(relater, target)?;
    if target_candidates.is_empty() {
        return Some(relater.unrelated(|| TypeMismatch::incompatible(source, target)));
    }

    Some(relate_to_callable_targets(
        relater,
        source,
        target,
        &source_candidates,
        &target_candidates,
        intersection_state,
    ))
}

fn relate_to_callable_targets(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_candidates: &[LuaType],
    target_candidates: &[LuaType],
    intersection_state: IntersectionState,
) -> RelationResult {
    let mut best = None;
    let mut indeterminate = None;
    let mut has_unrelated_target = false;
    for (target_index, target_candidate) in target_candidates.iter().enumerate() {
        let mut target_related = false;
        let mut target_indeterminate = None;
        let mut target_best = None;
        for (source_index, source_candidate) in source_candidates.iter().enumerate() {
            let (outcome, progress) =
                relater.probe_relation(source_candidate, target_candidate, intersection_state);
            match outcome {
                RelationOutcome::Related => {
                    target_related = true;
                    break;
                }
                RelationOutcome::Indeterminate(kind) => {
                    target_indeterminate.get_or_insert(kind);
                }
                RelationOutcome::Unrelated => {
                    if target_best
                        .map(|(_, best_progress)| progress > best_progress)
                        .unwrap_or(true)
                    {
                        target_best = Some((source_index, progress));
                    }
                }
            }
        }

        if target_related {
            continue;
        }
        if let Some(kind) = target_indeterminate {
            indeterminate.get_or_insert(kind);
            continue;
        }

        has_unrelated_target = true;
        if let Some((source_index, progress)) = target_best
            && best
                .map(|(_, _, best_progress)| progress > best_progress)
                .unwrap_or(true)
        {
            best = Some((source_index, target_index, progress));
        }
        if !relater.is_explain() {
            return Err(RelationFailure::Unrelated(None));
        }
    }

    if has_unrelated_target {
        let Some((source_index, target_index, _)) = best else {
            return relater.unrelated(|| TypeMismatch::incompatible(source, target));
        };
        return relater
            .relate(
                &source_candidates[source_index],
                &target_candidates[target_index],
                intersection_state,
            )
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| {
                    mismatch.at(
                        TypePathSegment::TargetUnionCandidate(target_index),
                        source,
                        target,
                    )
                })
            });
    }
    if let Some(kind) = indeterminate {
        return Err(RelationFailure::Indeterminate(kind));
    }
    Ok(())
}

fn relate_function(
    relater: &mut Relater,
    source: &LuaType,
    source_func: &LuaFunctionType,
    target: &LuaType,
    target_func: &LuaFunctionType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let target_self_offset = usize::from(target_func.is_colon_define());
    let source_self_offset = usize::from(
        source_func.is_colon_define()
            && (target_func.is_colon_define()
                || target_func
                    .get_params()
                    .first()
                    .is_some_and(|(name, _)| name == "self")),
    );
    let target_len = target_func.get_params().len() + target_self_offset;
    for index in 0..target_len {
        let (target_name, target_type) = if index < target_self_offset {
            ("self", None)
        } else {
            let (name, typ) = &target_func.get_params()[index - target_self_offset];
            (name.as_str(), typ.as_ref())
        };
        let (source_name, source_type) = if index < source_self_offset {
            ("self", None)
        } else if let Some((name, typ)) = source_func.get_params().get(index - source_self_offset) {
            (name.as_str(), typ.as_ref())
        } else {
            break;
        };
        if source_name == "..." {
            if let Some(source_vararg) = source_type {
                for remaining in index..target_len {
                    let remaining_target = if remaining < target_self_offset {
                        None
                    } else {
                        target_func.get_params()[remaining - target_self_offset]
                            .1
                            .as_ref()
                    };
                    if let Some(remaining_target) = remaining_target {
                        relater
                            .relate_with_directional_policy(
                                remaining_target,
                                source_vararg,
                                intersection_state,
                            )
                            .map_err(|failure| {
                                failure.map_mismatch(|mismatch| {
                                    mismatch.at(
                                        TypePathSegment::FunctionParameter(remaining),
                                        source,
                                        target,
                                    )
                                })
                            })?;
                    }
                }
            }
            break;
        }
        if target_name == "..." {
            break;
        }
        if let (Some(source_type), Some(target_type)) = (source_type, target_type) {
            if source_type.is_self_infer() || target_type.is_self_infer() {
                relater.note_progress();
                continue;
            }
            // 函数参数是逆变的.
            relater
                .relate_with_directional_policy(target_type, source_type, intersection_state)
                .map_err(|failure| {
                    failure.map_mismatch(|mismatch| {
                        mismatch.at(TypePathSegment::FunctionParameter(index), source, target)
                    })
                })?;
            relater.note_progress();
        }
    }

    Ok(())
}

fn callable_candidates(relater: &Relater, typ: &LuaType) -> Option<Vec<LuaType>> {
    if let Some(normalized) = normalize_type(relater.db(), typ)
        && normalized != *typ
    {
        return callable_candidates(relater, &normalized);
    }
    if matches!(typ, LuaType::Function) {
        return Some(vec![LuaType::Function]);
    }

    let mut overload_groups = Vec::new();
    collect_callable_overload_groups(relater.db(), typ, &mut overload_groups).ok()?;
    let candidates = overload_groups
        .into_iter()
        .flatten()
        .map(LuaType::DocFunction)
        .collect::<Vec<_>>();
    (!candidates.is_empty()).then_some(candidates)
}
