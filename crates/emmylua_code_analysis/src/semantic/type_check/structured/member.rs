use crate::{
    DbIndex, LuaIntersectionType, LuaMemberIndexItem, LuaMemberKey, LuaMemberOwner, LuaType,
    semantic::member::find_members_with_key,
};

use super::super::{
    mismatch::{TypeMismatch, TypeMismatchKind, TypePathSegment},
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
        if target_member_type.is_optional() {
            return Ok(());
        }
        return relater
            .unrelated(|| TypeMismatch::new(TypeMismatchKind::MissingMember { key: key.clone() }));
    };

    let field_result =
        relater.relate_field_types(&source_member_type, target_member_type, intersection_state);
    if let Err(failure) = field_result {
        if relater.is_explain() {
            return Err(
                failure.map_mismatch(|mismatch| mismatch.at(TypePathSegment::Member(key.clone())))
            );
        }
        return Err(failure);
    }
    relater.note_progress();
    Ok(())
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
                relater
                    .relate(source_value_type, target_value_type, intersection_state)
                    .map_err(|failure| {
                        failure.map_mismatch(|mismatch| {
                            mismatch.at(TypePathSegment::Index(source_key_type.clone()))
                        })
                    })?;
                relater.note_progress();
                Ok(())
            }
            RelationOutcome::Unrelated if !require_compatible_key => Ok(()),
            RelationOutcome::Unrelated => {
                relater.unrelated(|| TypeMismatch::incompatible(source, target))
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
