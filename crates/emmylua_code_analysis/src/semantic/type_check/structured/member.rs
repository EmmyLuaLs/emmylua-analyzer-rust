use crate::{
    DbIndex, LuaIntersectionType, LuaMemberIndexItem, LuaMemberKey, LuaMemberOwner, LuaType,
    semantic::member::find_members_with_key,
    semantic::type_check::error_chain::{
        index_message, missing_members_message, not_assignable_message, property_message,
    },
};

use super::super::{
    is_optional,
    relation::{IntersectionState, Relater, RelationFailure, RelationOutcome, RelationResult},
};
use super::{declared::visit_declared_members, tuple::visit_tuple_index_entries};

pub(super) fn visit_member_items<E>(
    db: &DbIndex,
    owner: &LuaMemberOwner,
    mut visitor: impl FnMut(&LuaMemberKey, &LuaMemberIndexItem) -> Result<(), E>,
) -> Result<(), E> {
    let Some(mut member_items) = db.get_member_index().get_member_items(owner) else {
        return Ok(());
    };
    member_items.try_for_each(|(key, item)| visitor(key, item))
}

pub(super) fn relate_keyed_member(
    relater: &mut Relater,
    source: &LuaType,
    key: &LuaMemberKey,
    target_member_type: &LuaType,
    intersection_state: IntersectionState,
) -> RelationResult {
    relater.consume_relation_budget()?;
    let source_member_type = find_source_member_type(relater, source, key, intersection_state)?;
    let Some(source_member_type) = source_member_type else {
        // Explain 模式在此时必然经过了缺失字段判断, 因此可以直接跳过
        if relater.is_explain() || is_optional(relater.db(), target_member_type) {
            return Ok(());
        }
        return relater.fail(|db| {
            missing_members_message(db, source, target_member_type, std::slice::from_ref(key))
        });
    };

    let field_result =
        relater.relate_field_types(&source_member_type, target_member_type, intersection_state);
    let result = relater.on_unrelated(field_result, |_| property_message(key));
    if result.is_ok() {
        relater.note_progress();
    }
    result
}

pub(super) fn find_source_member_type(
    relater: &mut Relater,
    source: &LuaType,
    key: &LuaMemberKey,
    intersection_state: IntersectionState,
) -> Result<Option<LuaType>, RelationFailure> {
    let member_type = match source {
        LuaType::Object(source_object) => source_object.get_member_type(key),
        LuaType::TableConst(range) => relater
            .db()
            .get_member_index()
            .get_member_item(&LuaMemberOwner::Element(range.clone()), key)
            .map(|item| item.resolve_type(relater.db()).unwrap_or(LuaType::Any)),
        LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_) | LuaType::Intersection(_) => {
            find_members_with_key(relater.db(), source, key.clone(), false)
                .and_then(|members| members.into_iter().next())
                .map(|member| member.typ)
        }
        LuaType::Tuple(source_tuple) => match key {
            LuaMemberKey::Integer(index) if *index > 0 => {
                source_tuple.get_type(*index as usize - 1).cloned()
            }
            _ => None,
        },
        LuaType::Array(source_array) => match key {
            LuaMemberKey::Integer(index) if *index > 0 => Some(source_array.get_base().clone()),
            LuaMemberKey::TypeKey(key_type) if key_type.is_integer() => {
                Some(source_array.get_base().clone())
            }
            _ => None,
        },
        LuaType::TableGeneric(source_params) if source_params.len() == 2 => {
            let Some(source_key_type) = key.to_index_type() else {
                return Ok(None);
            };
            match relater
                .probe_relation(&source_key_type, &source_params[0], intersection_state)
                .0
            {
                RelationOutcome::Related => Some(source_params[1].clone()),
                RelationOutcome::Unrelated => None,
                RelationOutcome::Indeterminate(kind) => {
                    return Err(RelationFailure::Indeterminate(kind));
                }
            }
        }
        _ => None,
    };
    Ok(member_type)
}

pub(super) fn relate_index_member(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    target_key_type: &LuaType,
    target_value_type: &LuaType,
    intersection_state: IntersectionState,
) -> RelationResult {
    // SOURCE 表示当前关系位于源交集的组成类型分支, 目标索引签名不参与该分支的关系判定.
    if intersection_state.contains(IntersectionState::SOURCE) {
        return Ok(());
    }

    let relate_entry = |relater: &mut Relater,
                        source_key_type: &LuaType,
                        source_value_type: &LuaType,
                        require_compatible_key: bool|
     -> RelationResult {
        relater.consume_relation_budget()?;
        match relater
            .probe_relation(source_key_type, target_key_type, intersection_state)
            .0
        {
            RelationOutcome::Related => {
                let result =
                    relater.relate(source_value_type, target_value_type, intersection_state);
                relater.on_unrelated(result, |db| index_message(db, source_key_type))?;
                relater.note_progress();
                Ok(())
            }
            RelationOutcome::Unrelated if !require_compatible_key => Ok(()),
            RelationOutcome::Unrelated => {
                relater.fail(|db| not_assignable_message(db, source, target))
            }
            RelationOutcome::Indeterminate(kind) => Err(RelationFailure::Indeterminate(kind)),
        }
    };

    match source {
        LuaType::Object(source_object) => {
            for (key, source_value_type) in source_object.get_fields() {
                let Some(source_key_type) = key.to_index_type() else {
                    continue;
                };
                relate_entry(relater, &source_key_type, source_value_type, false)?;
            }
            for (source_key_type, source_value_type) in source_object.get_index_access() {
                relate_entry(relater, source_key_type, source_value_type, false)?;
            }
        }
        LuaType::TableConst(range) => {
            let owner = LuaMemberOwner::Element(range.clone());
            let db = relater.db();
            visit_member_items(db, &owner, |key, item| {
                let Some(source_key_type) = key.to_index_type() else {
                    return Ok(());
                };
                let source_value_type = item.resolve_type(db).unwrap_or(LuaType::Any);
                relate_entry(relater, &source_key_type, &source_value_type, false)
            })?;
        }
        LuaType::Tuple(source_tuple) => {
            visit_tuple_index_entries(source_tuple, |key_type, source_value_type, _| {
                relate_entry(
                    relater,
                    key_type,
                    source_value_type,
                    matches!(key_type, LuaType::Integer),
                )
            })?;
        }
        LuaType::Array(source_array) => {
            relate_entry(relater, &LuaType::Integer, source_array.get_base(), false)?;
        }
        LuaType::TableGeneric(source_params) if source_params.len() == 2 => {
            relate_entry(relater, &source_params[0], &source_params[1], false)?;
        }
        LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_) => {
            visit_declared_members(relater, source, |relater, key, source_value_type| {
                let Some(source_key_type) = key.to_index_type() else {
                    return Ok(());
                };
                relate_entry(relater, &source_key_type, source_value_type, false)
            })?;
        }
        LuaType::Intersection(intersection) => {
            for member in intersection.get_types() {
                relate_index_member(
                    relater,
                    member,
                    target,
                    target_key_type,
                    target_value_type,
                    intersection_state,
                )?;
            }
        }
        _ => {}
    }

    Ok(())
}

pub(in crate::semantic::type_check) fn relate_target_intersection_index_members(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    intersection: &LuaIntersectionType,
) -> RelationResult {
    for member in intersection.get_types() {
        match member {
            LuaType::Object(object) => {
                for (key_type, value_type) in object.get_index_access() {
                    relate_index_member(
                        relater,
                        source,
                        target,
                        key_type,
                        value_type,
                        IntersectionState::NONE,
                    )?;
                }
            }
            LuaType::TableConst(range) => {
                let owner = LuaMemberOwner::Element(range.clone());
                let db = relater.db();
                visit_member_items(db, &owner, |key, item| {
                    let LuaMemberKey::TypeKey(key_type) = key else {
                        return Ok(());
                    };
                    let value_type = item.resolve_type(db).unwrap_or(LuaType::Any);
                    relate_index_member(
                        relater,
                        source,
                        target,
                        key_type,
                        &value_type,
                        IntersectionState::NONE,
                    )
                })?;
            }
            LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_) => {
                visit_declared_members(relater, member, |relater, key, value_type| {
                    let LuaMemberKey::TypeKey(key_type) = key else {
                        return Ok(());
                    };
                    relate_index_member(
                        relater,
                        source,
                        target,
                        key_type,
                        value_type,
                        IntersectionState::NONE,
                    )
                })?;
            }
            LuaType::Intersection(nested) => {
                relate_target_intersection_index_members(relater, source, target, nested)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// 收集 target 中 source 缺失且不可空的 keyed 成员
pub(in crate::semantic::type_check) fn collect_missing_members(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Result<(Vec<LuaMemberKey>, bool), RelationFailure> {
    let mut missing_keys = Vec::new();
    let mut has_shared_key = false;
    match target {
        LuaType::Object(object) => {
            for (key, member_type) in object.get_fields() {
                if find_source_member_type(relater, source, key, intersection_state)?.is_some() {
                    has_shared_key = true;
                } else if !is_optional(relater.db(), member_type) {
                    missing_keys.push(key.clone());
                }
            }
        }
        LuaType::TableConst(range) => {
            let owner = LuaMemberOwner::Element(range.clone());
            let db = relater.db();
            visit_member_items(db, &owner, |key, item| {
                if matches!(key, LuaMemberKey::TypeKey(_)) {
                    return Ok(());
                }
                if find_source_member_type(relater, source, key, intersection_state)?.is_some() {
                    has_shared_key = true;
                } else if !is_optional(db, &item.resolve_type(db).unwrap_or(LuaType::Any)) {
                    missing_keys.push(key.clone());
                }
                Ok(())
            })?;
        }
        LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_) => {
            visit_declared_members(relater, target, |relater, key, member_type| {
                if matches!(key, LuaMemberKey::TypeKey(_)) {
                    return Ok(());
                }
                if find_source_member_type(relater, source, key, intersection_state)?.is_some() {
                    has_shared_key = true;
                } else if !is_optional(relater.db(), member_type) {
                    missing_keys.push(key.clone());
                }
                Ok(())
            })?;
        }
        _ => {}
    }
    Ok((missing_keys, has_shared_key))
}

/// 探测目标成员在 source 中是否缺失且不可空.
pub(super) fn probe_missing_member(
    relater: &mut Relater,
    source: &LuaType,
    key: &LuaMemberKey,
    target_member_type: &LuaType,
    intersection_state: IntersectionState,
) -> Result<bool, RelationFailure> {
    if find_source_member_type(relater, source, key, intersection_state)?.is_some() {
        return Ok(false);
    }
    Ok(!is_optional(relater.db(), target_member_type))
}

/// 用收集到的全部缺失字段构建整体失败.
pub(in crate::semantic::type_check) fn unrelated_missing_members(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    keys: Vec<LuaMemberKey>,
) -> RelationResult {
    relater.fail(|db| missing_members_message(db, source, target, &keys))
}
