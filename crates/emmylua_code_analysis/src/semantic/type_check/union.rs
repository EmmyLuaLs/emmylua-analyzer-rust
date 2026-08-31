use std::borrow::Cow;

use crate::{
    BasicTypeKind, LuaMemberKey, LuaType, LuaUnionType,
    semantic::type_check::error_chain::not_assignable_message,
};

use super::{
    relation::{IntersectionState, Relater, RelationFailure, RelationOutcome, RelationResult},
    structured::{collect_missing_members, unrelated_missing_members},
};

pub(crate) fn relate_union(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Option<RelationResult> {
    // 先分派源类型
    match source {
        LuaType::Union(source_union) => {
            let result = relate_source_union_members(
                relater,
                source_union.iter(),
                target,
                intersection_state,
            );
            return Some(
                relater.on_unrelated(result, |db| not_assignable_message(db, source, target)),
            );
        }
        LuaType::MultiLineUnion(source_multi) => {
            let result = relate_source_union_members(
                relater,
                source_multi.iter().map(Cow::Borrowed),
                target,
                intersection_state,
            );
            return Some(
                relater.on_unrelated(result, |db| not_assignable_message(db, source, target)),
            );
        }
        _ => {}
    }

    // 目标不是联合类型时交由其他关系处理.
    if !matches!(target, LuaType::Union(_) | LuaType::MultiLineUnion(_)) {
        return None;
    }

    // 源确定非空时, 单一可空目标可直接剥离为其非 Nil 成员进行比对.
    if !source.is_nullable() {
        if let Some(non_nil_target) = get_single_non_nil_candidate(target) {
            return Some(relater.relate(source, &non_nil_target, intersection_state));
        }
    }

    match target {
        LuaType::Union(target_union) => Some(relate_to_target_union_candidates(
            relater,
            source,
            target,
            target_union.iter(),
            intersection_state,
        )),
        LuaType::MultiLineUnion(target_multi) => Some(relate_to_target_union_candidates(
            relater,
            source,
            target,
            target_multi.iter().map(Cow::Borrowed),
            intersection_state,
        )),
        _ => None,
    }
}

fn get_single_non_nil_candidate<'a>(target: &'a LuaType) -> Option<Cow<'a, LuaType>> {
    match target {
        LuaType::Union(target_union) => match target_union.as_ref() {
            LuaUnionType::Nullable(non_nil) => {
                if !matches!(non_nil, LuaType::Union(_) | LuaType::MultiLineUnion(_)) {
                    Some(Cow::Borrowed(non_nil))
                } else {
                    None
                }
            }
            LuaUnionType::Basic(basic) if basic.contains(BasicTypeKind::Nil) => {
                let mut non_nil_basic = *basic;
                non_nil_basic.remove(BasicTypeKind::Nil);
                let mut iter = non_nil_basic.iter_kinds();
                let first = iter.next()?;
                if iter.next().is_none() {
                    Some(Cow::Owned(first.into()))
                } else {
                    None
                }
            }
            LuaUnionType::Multi(candidates) if candidates.contains(&LuaType::Nil) => {
                let mut non_nil = candidates.iter().filter(|t| !t.is_nil());
                let first = non_nil.next()?;
                if non_nil.next().is_none()
                    && !matches!(first, LuaType::Union(_) | LuaType::MultiLineUnion(_))
                {
                    Some(Cow::Borrowed(first))
                } else {
                    None
                }
            }
            _ => None,
        },
        LuaType::MultiLineUnion(target_multi) => {
            if target_multi.iter().any(|t| t.is_nil()) {
                let mut non_nil = target_multi.iter().filter(|t| !t.is_nil());
                let first = non_nil.next()?;
                if non_nil.next().is_none()
                    && !matches!(first, LuaType::Union(_) | LuaType::MultiLineUnion(_))
                {
                    Some(Cow::Borrowed(first))
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn relate_source_union_members<'a>(
    relater: &mut Relater,
    members: impl Iterator<Item = Cow<'a, LuaType>>,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let mut first_indeterminate = None;
    for member in members {
        match relater
            .probe_relation(&member, target, intersection_state)
            .0
        {
            RelationOutcome::Related => {}
            RelationOutcome::Indeterminate(kind) => {
                first_indeterminate.get_or_insert(kind);
            }
            RelationOutcome::Unrelated => {
                if !relater.is_explain() {
                    return Err(RelationFailure::Unrelated);
                }
                return relater.relate(&member, target, intersection_state);
            }
        }
    }
    if let Some(kind) = first_indeterminate {
        return Err(RelationFailure::Indeterminate(kind));
    }
    Ok(())
}

fn relate_to_target_union_candidates<'a>(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    candidates: impl Iterator<Item = Cow<'a, LuaType>>,
    intersection_state: IntersectionState,
) -> RelationResult {
    let mut indeterminate = None;
    let mut failed_candidates = Vec::new();

    for candidate in candidates {
        match relater
            .probe_relation(source, &candidate, intersection_state)
            .0
        {
            RelationOutcome::Related => return Ok(()),
            RelationOutcome::Indeterminate(kind) => {
                indeterminate.get_or_insert(kind);
            }
            RelationOutcome::Unrelated => {}
        }
        if relater.is_explain() {
            failed_candidates.push(candidate.into_owned());
        }
    }

    if let Some(kind) = indeterminate {
        return Err(RelationFailure::Indeterminate(kind));
    }

    if !relater.is_explain() {
        return Err(RelationFailure::Unrelated);
    }

    // probe_relation 有早退行为, 因此得到的结果并不一定是最匹配的, 我们必须独立处理缺失字段判别.
    let mut evidence: Option<(usize, Vec<LuaMemberKey>)> = None;
    for (index, candidate) in failed_candidates.iter().enumerate() {
        let (missing_keys, has_shared_key) =
            collect_missing_members(relater, source, candidate, intersection_state)?;
        if !has_shared_key {
            continue;
        }
        if missing_keys.is_empty() {
            // 必填字段全部在场: 失败必然是字段类型不匹配, 重放该分支取路径化证据.
            evidence = Some((index, missing_keys));
            break;
        }
        if evidence
            .as_ref()
            .is_none_or(|(_, best_missing)| missing_keys.len() < best_missing.len())
        {
            evidence = Some((index, missing_keys));
        }
    }

    let Some((best_index, missing_keys)) = evidence else {
        return relater.fail(|db| not_assignable_message(db, source, target));
    };
    if !missing_keys.is_empty() {
        return unrelated_missing_members(
            relater,
            source,
            &failed_candidates[best_index],
            missing_keys,
        );
    }
    relater.relate(source, &failed_candidates[best_index], intersection_state)
}
