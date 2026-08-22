use hashbrown::HashSet;

use crate::{
    DbIndex, LuaArrayType, LuaIntersectionType, LuaMemberIndexItem, LuaMemberKey, LuaMemberOwner,
    LuaObjectType, LuaTupleType, LuaType, LuaTypeDeclId, TypeSubstitutor, instantiate_type_generic,
    semantic::member::find_members_with_key,
};

use super::super::{
    mismatch::{TypeMismatch, TypeMismatchKind, TypePathSegment},
    relation::{IntersectionState, Relater, RelationFailure, RelationOutcome, RelationResult},
};
use super::{
    array::effective_array_base, declared::relate_structural_source_to_declared_target,
    table_const::relate_to_table_const_target, tuple::visit_tuple_index_entries,
};

pub(super) fn relate_object_source(
    relater: &mut Relater,
    source: &LuaType,
    source_object: &LuaObjectType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Option<RelationResult> {
    match target {
        LuaType::Table => {
            relater.note_progress();
            Some(Ok(()))
        }
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
        LuaType::Tuple(target_tuple) => Some(relate_object_to_tuple(
            relater,
            source_object,
            target_tuple,
            intersection_state,
        )),
        LuaType::Array(target_array) => Some(relate_object_to_array(
            relater,
            source,
            target,
            source_object,
            target_array,
            intersection_state,
        )),
        LuaType::TableGeneric(target_params) => Some(relate_object_to_table_generic(
            relater,
            source,
            target,
            source_object,
            target_params,
            intersection_state,
        )),
        LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_) => {
            Some(relate_structural_source_to_declared_target(
                relater,
                source,
                target,
                intersection_state,
            ))
        }
        _ => None,
    }
}

pub(super) fn relate_object_to_tuple(
    relater: &mut Relater,
    source_object: &LuaObjectType,
    target_tuple: &LuaTupleType,
    intersection_state: IntersectionState,
) -> RelationResult {
    for (index, target_type) in target_tuple.get_types().iter().enumerate() {
        relater.consume_relation_budget()?;
        let key = LuaMemberKey::Integer(index as i64 + 1);
        let Some(source_type) = source_object.get_field(&key) else {
            if target_type.is_optional() {
                continue;
            }
            return relater
                .unrelated(|| TypeMismatch::new(TypeMismatchKind::MissingTupleElement { index }));
        };
        relater
            .relate(source_type, target_type, intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| mismatch.at(TypePathSegment::TupleElement(index)))
            })?;
        relater.note_progress();
    }
    Ok(())
}

pub(super) fn relate_object_to_array(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_object: &LuaObjectType,
    target_array: &LuaArrayType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let target_base = effective_array_base(relater, target_array.get_base());
    let mut checked = false;
    for (key, source_type) in source_object.get_fields() {
        if !matches!(key, LuaMemberKey::Integer(index) if *index > 0) {
            continue;
        }
        relater.consume_relation_budget()?;
        relater
            .relate(source_type, &target_base, intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| mismatch.at(TypePathSegment::Member(key.clone())))
            })?;
        checked = true;
        relater.note_progress();
    }
    for (source_key, source_type) in source_object.get_index_access() {
        if !source_key.is_integer() {
            continue;
        }
        relater.consume_relation_budget()?;
        relater
            .relate(source_type, &target_base, intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| {
                    mismatch.at(TypePathSegment::Index(source_key.clone()))
                })
            })?;
        checked = true;
        relater.note_progress();
    }

    if checked {
        Ok(())
    } else {
        relater.unrelated(|| TypeMismatch::incompatible(source, target))
    }
}

pub(super) fn relate_object_to_table_generic(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_object: &LuaObjectType,
    target_params: &[LuaType],
    intersection_state: IntersectionState,
) -> RelationResult {
    if target_params.len() != 2 {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    }

    for (key, source_type) in source_object.get_fields() {
        relate_member_to_table_generic(
            relater,
            key,
            source_type,
            target_params,
            intersection_state,
        )?;
    }
    for (source_key_type, source_type) in source_object.get_index_access() {
        relater.consume_relation_budget()?;
        relater
            .relate(source_key_type, &target_params[0], intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| {
                    mismatch.at(TypePathSegment::Index(source_key_type.clone()))
                })
            })?;
        relater
            .relate(source_type, &target_params[1], intersection_state)
            .map_err(|failure| {
                failure.map_mismatch(|mismatch| {
                    mismatch.at(TypePathSegment::Index(source_key_type.clone()))
                })
            })?;
        relater.note_progress();
    }
    Ok(())
}

pub(super) fn relate_member_to_table_generic(
    relater: &mut Relater,
    key: &LuaMemberKey,
    source_value_type: &LuaType,
    target_params: &[LuaType],
    intersection_state: IntersectionState,
) -> RelationResult {
    relater.consume_relation_budget()?;
    let Some(source_key_type) = key.to_index_type() else {
        return Ok(());
    };
    relater
        .relate(&source_key_type, &target_params[0], intersection_state)
        .map_err(|failure| {
            failure.map_mismatch(|mismatch| {
                mismatch.at(TypePathSegment::Index(source_key_type.clone()))
            })
        })?;
    relater
        .relate(source_value_type, &target_params[1], intersection_state)
        .map_err(|failure| {
            failure.map_mismatch(|mismatch| mismatch.at(TypePathSegment::Member(key.clone())))
        })?;
    relater.note_progress();
    Ok(())
}

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

pub(in crate::semantic::type_check) fn relate_object_members(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    target_object: &LuaObjectType,
    intersection_state: IntersectionState,
) -> RelationResult {
    if let LuaType::Object(source_object) = source {
        for (target_key, target_member_type) in target_object.get_fields() {
            relater.consume_relation_budget()?;
            let source_member_type = source_object.get_field(target_key);
            let Some(source_member_type) = source_member_type else {
                if target_member_type.is_optional() {
                    continue;
                }
                return relater.unrelated(|| {
                    TypeMismatch::new(TypeMismatchKind::MissingMember {
                        key: target_key.clone(),
                    })
                });
            };

            let field_result = relater.relate_field_types(
                source_member_type,
                target_member_type,
                intersection_state,
            );
            if let Err(failure) = field_result {
                if relater.is_explain() {
                    return Err(failure.map_mismatch(|mismatch| {
                        mismatch.at(TypePathSegment::Member(target_key.clone()))
                    }));
                }
                return Err(failure);
            }
            relater.note_progress();
        }
    } else {
        for (key, target_member_type) in target_object.get_fields() {
            relate_named_member_obligation(
                relater,
                source,
                key,
                target_member_type,
                intersection_state,
            )?;
        }
    }

    if !intersection_state.contains(IntersectionState::TARGET) {
        for (target_key_type, target_value_type) in target_object.get_index_access() {
            relate_index_obligation(
                relater,
                source,
                target,
                target_key_type,
                target_value_type,
                intersection_state,
            )?;
        }
    }

    Ok(())
}

pub(super) fn relate_named_member_obligation(
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

pub(in crate::semantic::type_check) fn relate_to_declared_target_members(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let target_id = match target {
        LuaType::Ref(type_id) | LuaType::Def(type_id) => Some(type_id),
        LuaType::Generic(generic) => Some(generic.get_base_type_id_ref()),
        _ => None,
    };
    if target_id.is_some_and(|type_id| {
        relater
            .db()
            .get_type_index()
            .get_type_decl(type_id)
            .is_none()
    }) {
        return relater.unrelated(|| TypeMismatch::incompatible(source, target));
    }

    visit_declared_members(relater, target, |relater, key, target_member_type| {
        if let LuaMemberKey::TypeKey(target_key_type) = key {
            if intersection_state.contains(IntersectionState::TARGET) {
                return Ok(());
            }
            return relate_index_obligation(
                relater,
                source,
                target,
                target_key_type,
                target_member_type,
                intersection_state,
            );
        }

        relate_named_member_obligation(relater, source, key, target_member_type, intersection_state)
    })
}

fn find_source_member_type(
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

pub(super) fn relate_index_obligation(
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
                relate_index_obligation(
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

pub(in crate::semantic::type_check) fn relate_target_intersection_index_obligations(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    intersection: &LuaIntersectionType,
) -> RelationResult {
    for member in intersection.get_types() {
        match member {
            LuaType::Object(object) => {
                for (key_type, value_type) in object.get_index_access() {
                    relate_index_obligation(
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
                    relate_index_obligation(
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
                    relate_index_obligation(
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
                relate_target_intersection_index_obligations(relater, source, target, nested)?;
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn visit_declared_members(
    relater: &mut Relater,
    declared_type: &LuaType,
    mut visitor: impl FnMut(&mut Relater, &LuaMemberKey, &LuaType) -> RelationResult,
) -> RelationResult {
    let mut seen_keys = HashSet::new();
    let mut visited_types = HashSet::new();
    visit_declared_type_members(
        relater,
        declared_type,
        &mut seen_keys,
        &mut visited_types,
        &mut visitor,
    )
}

fn visit_declared_type_members(
    relater: &mut Relater,
    declared_type: &LuaType,
    seen_keys: &mut HashSet<LuaMemberKey>,
    visited_types: &mut HashSet<LuaTypeDeclId>,
    visitor: &mut impl FnMut(&mut Relater, &LuaMemberKey, &LuaType) -> RelationResult,
) -> RelationResult {
    // 实例化后的对象类型走快速路径
    match declared_type {
        LuaType::Object(object) => {
            for (key, member_type) in object.get_fields() {
                if !seen_keys.insert(key.clone()) {
                    continue;
                }
                visitor(relater, key, member_type)?;
            }
            for (key_type, member_type) in object.get_index_access() {
                let key = LuaMemberKey::TypeKey(key_type.clone());
                if !seen_keys.insert(key.clone()) {
                    continue;
                }
                visitor(relater, &key, member_type)?;
            }
            return Ok(());
        }
        LuaType::TableGeneric(params) if params.len() == 2 => {
            let key = LuaMemberKey::TypeKey(params[0].clone());
            if seen_keys.insert(key.clone()) {
                visitor(relater, &key, &params[1])?;
            }
            return Ok(());
        }
        LuaType::Tuple(tuple) => {
            for (index, member_type) in tuple.get_types().iter().enumerate() {
                let key = LuaMemberKey::Integer(index as i64 + 1);
                if !seen_keys.insert(key.clone()) {
                    continue;
                }
                visitor(relater, &key, member_type)?;
            }
            return Ok(());
        }
        LuaType::Array(array) => {
            let key = LuaMemberKey::TypeKey(LuaType::Integer);
            if seen_keys.insert(key.clone()) {
                visitor(relater, &key, array.get_base())?;
            }
            return Ok(());
        }
        _ => {}
    }

    let (type_id, substitutor, generic_params) = match declared_type {
        LuaType::Ref(type_id) | LuaType::Def(type_id) => (type_id.clone(), None, None),
        LuaType::Generic(generic) => (
            generic.get_base_type_id(),
            Some(TypeSubstitutor::from_type_array(
                generic.get_params().clone(),
            )),
            Some(generic.get_params()),
        ),
        _ => return Ok(()),
    };
    let db = relater.db();
    let owner = LuaMemberOwner::Type(type_id.clone());
    let type_decl = db.get_type_index().get_type_decl(&type_id);
    let is_alias = type_decl.as_ref().is_some_and(|decl| decl.is_alias());
    let has_supers = db
        .get_type_index()
        .get_super_types_iter(&type_id)
        .is_some_and(|mut supers| supers.next().is_some());

    // alias 的有效成员位于 origin, 需要落到慢路径的 alias 回退, 不进快路径.
    if !has_supers && !is_alias && seen_keys.is_empty() {
        // 非泛型成员直接复用索引键, 避免宽对象关系检查为每个字段复制键.
        let Some(substitutor) = substitutor.as_ref() else {
            visit_member_items(db, &owner, |key, item| {
                let Ok(member_type) = item.resolve_type(db) else {
                    return Ok(());
                };
                visitor(relater, key, &member_type)
            })?;
            return Ok(());
        };
        visit_member_items(db, &owner, |key, item| {
            let Some((key, member_type)) = resolve_instantiated_member(db, key, item, substitutor)
            else {
                return Ok(());
            };
            visitor(relater, &key, &member_type)
        })?;
        return Ok(());
    }

    if !visited_types.insert(type_id.clone()) {
        return Ok(());
    }

    if let Some(substitutor) = substitutor.as_ref() {
        visit_member_items(db, &owner, |key, item| {
            let Some((key, member_type)) = resolve_instantiated_member(db, key, item, substitutor)
            else {
                return Ok(());
            };
            if !seen_keys.insert(key.clone()) {
                return Ok(());
            }
            visitor(relater, &key, &member_type)
        })?;
    } else {
        visit_member_items(db, &owner, |key, item| {
            if !seen_keys.insert(key.clone()) {
                return Ok(());
            }
            let Ok(member_type) = item.resolve_type(db) else {
                return Ok(());
            };
            visitor(relater, key, &member_type)
        })?;
    }

    if let Some(super_types) = db.get_type_index().get_super_types_iter(&type_id) {
        for super_type in super_types {
            let super_type = substitutor
                .as_ref()
                .map(|substitutor| instantiate_type_generic(db, super_type, substitutor))
                .unwrap_or_else(|| super_type.clone());
            visit_declared_type_members(relater, &super_type, seen_keys, visited_types, visitor)?;
        }
    }

    // 无父类型且无自身成员的 alias: 有效成员位于 alias origin.
    // alias substitutor 只在此处需要, 确认 is_alias && !has_supers 后再构造.
    if !has_supers
        && let Some(type_decl) = type_decl.as_ref()
        && type_decl.is_alias()
    {
        let alias_substitutor = generic_params.map(|generic_params| {
            TypeSubstitutor::from_alias(generic_params.to_vec(), type_id.clone())
        });
        if let Some(origin) = type_decl.get_alias_origin(db, alias_substitutor.as_ref()) {
            return visit_declared_type_members(
                relater,
                &origin,
                seen_keys,
                visited_types,
                visitor,
            );
        }
    }

    Ok(())
}

fn resolve_instantiated_member(
    db: &DbIndex,
    key: &LuaMemberKey,
    item: &LuaMemberIndexItem,
    substitutor: &TypeSubstitutor,
) -> Option<(LuaMemberKey, LuaType)> {
    let Ok(member_type) = item.resolve_type(db) else {
        return None;
    };
    let member_type = instantiate_type_generic(db, &member_type, substitutor);
    let mut key = key.clone();
    // 索引成员的键类型同样需要实例化, 否则泛型父类型的 [T] 无法收敛为实际键.
    if let LuaMemberKey::TypeKey(key_type) = &key {
        let instantiated_key = instantiate_type_generic(db, key_type, substitutor);
        if instantiated_key != *key_type {
            key = LuaMemberKey::TypeKey(instantiated_key);
        }
    }
    Some((key, member_type))
}
