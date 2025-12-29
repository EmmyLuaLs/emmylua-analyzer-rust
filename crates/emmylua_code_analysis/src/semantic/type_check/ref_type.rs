use std::collections::HashMap;

use crate::{
    LuaMemberKey, LuaMemberOwner, LuaObjectType, LuaTupleType, LuaType, LuaTypeCache, LuaTypeDecl,
    LuaTypeDeclId, RenderLevel, humanize_type,
    semantic::{member::find_members, type_check::type_check_context::TypeCheckContext},
};

use super::{
    TypeCheckResult, check_general_type_compact, is_sub_type_of, sub_type::get_base_type_id,
    type_check_fail_reason::TypeCheckFailReason, type_check_guard::TypeCheckGuard,
};

pub fn check_ref_type_compact(
    context: &mut TypeCheckContext,
    source_id: &LuaTypeDeclId,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let type_decl = context
        .db
        .get_type_index()
        .get_type_decl(source_id)
        // unreachable!
        .ok_or(if context.detail {
            TypeCheckFailReason::TypeNotMatchWithReason(
                t!("type `%{name}` not found.", name = source_id.get_name()).to_string(),
            )
        } else {
            TypeCheckFailReason::TypeNotMatch
        })?;

    if type_decl.is_alias() {
        if let Some(origin_type) = type_decl.get_alias_origin(context.db, None) {
            let result = check_general_type_compact(
                context,
                &origin_type,
                compact_type,
                check_guard.next_level()?,
            );
            if result.is_err() && origin_type.is_function() {
                return check_ref_class(context, source_id, compact_type, check_guard);
            }
            return result;
        }

        return Err(TypeCheckFailReason::TypeNotMatch);
    }

    if type_decl.is_enum() {
        check_ref_enum(context, source_id, compact_type, check_guard, type_decl)
    } else {
        check_ref_class(context, source_id, compact_type, check_guard)
    }
}

fn check_ref_enum(
    context: &mut TypeCheckContext,
    source_id: &LuaTypeDeclId,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
    type_decl: &LuaTypeDecl,
) -> TypeCheckResult {
    // 直接匹配相同类型
    if matches!(compact_type, LuaType::Def(id) | LuaType::Ref(id) if id == source_id) {
        return Ok(());
    }

    let enum_fields = type_decl
        .get_enum_field_type(context.db)
        .ok_or(TypeCheckFailReason::TypeNotMatch)?;

    // 移除掉枚举类型本身
    let compact_type = match compact_type {
        LuaType::Union(union_types) => {
            let new_types: Vec<_> = union_types
                .into_vec()
                .iter()
                .filter(
                    |typ| !matches!(typ, LuaType::Def(id) | LuaType::Ref(id) if id == source_id),
                )
                .cloned()
                .collect();
            LuaType::from_vec(new_types)
        }
        LuaType::Ref(compact_id) => {
            if let Some(compact_decl) = context.db.get_type_index().get_type_decl(compact_id)
                && compact_decl.is_enum()
                && let Some(compact_enum_fields) = compact_decl.get_enum_field_type(context.db)
            {
                return check_general_type_compact(
                    context,
                    &enum_fields,
                    &compact_enum_fields,
                    check_guard.next_level()?,
                );
            }
            compact_type.clone()
        }
        _ => compact_type.clone(),
    };

    // 当 enum 的值全为整数常量时, 可能会用于位运算, 此时右值推断为整数
    if let LuaType::Union(union_types) = &enum_fields
        && union_types
            .into_vec()
            .iter()
            .all(|t| matches!(t, LuaType::DocIntegerConst(_) | LuaType::IntegerConst(_)))
        && matches!(
            compact_type,
            LuaType::Integer | LuaType::DocIntegerConst(_) | LuaType::IntegerConst(_)
        )
    {
        return Ok(());
    }

    check_general_type_compact(
        context,
        &enum_fields,
        &compact_type,
        check_guard.next_level()?,
    )
}

fn check_ref_class(
    context: &mut TypeCheckContext,
    source_id: &LuaTypeDeclId,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    match compact_type {
        LuaType::Def(id) | LuaType::Ref(id) => {
            if source_id == id {
                return Ok(());
            }

            // 检查子类型关系
            if is_sub_type_of(context.db, id, source_id) {
                return Ok(());
            }
            // 这不是正确的逻辑. 但不假设超类会自动转换为子类, 则会过于严格
            if is_sub_type_of(context.db, source_id, id) {
                return Ok(());
            }

            // `compact`为枚举时的额外处理
            if let Some(compact_decl) = context.db.get_type_index().get_type_decl(id)
                && compact_decl.is_enum()
                && let Some(LuaType::Union(enum_fields)) =
                    compact_decl.get_enum_field_type(context.db)
            {
                let source = LuaType::Ref(source_id.clone());
                for field in enum_fields.into_vec() {
                    check_general_type_compact(
                        context,
                        &source,
                        &field,
                        check_guard.next_level()?,
                    )?;
                }
                return Ok(());
            }

            Err(TypeCheckFailReason::TypeNotMatch)
        }
        LuaType::TableConst(range) => check_ref_type_compact_table(
            context,
            source_id,
            LuaMemberOwner::Element(range.clone()),
            check_guard.next_level()?,
        ),
        LuaType::Object(object_type) => check_ref_type_compact_object(
            context,
            object_type,
            source_id,
            check_guard.next_level()?,
        ),
        LuaType::Table => Ok(()),
        LuaType::Union(union_type) => {
            for typ in union_type.into_vec() {
                check_general_type_compact(
                    context,
                    &LuaType::Ref(source_id.clone()),
                    &typ,
                    check_guard.next_level()?,
                )?;
            }
            Ok(())
        }
        LuaType::Tuple(tuple_type) => {
            check_ref_type_compact_tuple(context, tuple_type, source_id, check_guard.next_level()?)
        }
        LuaType::Generic(generic) => {
            let base_type_id = generic.get_base_type_id();
            if source_id == &base_type_id
                || is_sub_type_of(context.db, &base_type_id, source_id)
                || is_sub_type_of(context.db, source_id, &base_type_id)
            {
                Ok(())
            } else {
                Err(TypeCheckFailReason::TypeNotMatch)
            }
        }
        _ => {
            if let Some(base_type_id) = get_base_type_id(compact_type) {
                if source_id == &base_type_id
                    || is_sub_type_of(context.db, &base_type_id, source_id)
                    || is_sub_type_of(context.db, source_id, &base_type_id)
                {
                    Ok(())
                } else {
                    Err(TypeCheckFailReason::TypeNotMatch)
                }
            } else {
                Err(TypeCheckFailReason::TypeNotMatch)
            }
        }
    }
}

fn check_ref_type_compact_table(
    context: &mut TypeCheckContext,
    source_type_id: &LuaTypeDeclId,
    table_owner: LuaMemberOwner,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let member_index = context.db.get_member_index();
    let table_member_map: HashMap<_, _> = member_index
        .get_members(&table_owner)
        .map(|members| {
            members
                .iter()
                .map(|m| (m.get_key().clone(), m.get_id()))
                .collect()
        })
        .unwrap_or_default();

    let source_type_members =
        member_index.get_members(&LuaMemberOwner::Type(source_type_id.clone()));
    let Some(source_type_members) = source_type_members else {
        return Ok(()); // empty member donot need check
    };

    for source_member in source_type_members {
        let source_member_type = context
            .db
            .get_type_index()
            .get_type_cache(&source_member.get_id().into())
            .unwrap_or(&LuaTypeCache::InferType(LuaType::Any))
            .as_type();
        let key = source_member.get_key();

        if context.is_key_checked(key) {
            continue;
        }

        match table_member_map.get(key) {
            Some(table_member_id) => {
                let table_member = member_index
                    .get_member(table_member_id)
                    .ok_or(TypeCheckFailReason::TypeNotMatch)?;
                let table_member_type = context
                    .db
                    .get_type_index()
                    .get_type_cache(&table_member.get_id().into())
                    .unwrap_or(&LuaTypeCache::InferType(LuaType::Any))
                    .as_type();

                if let Err(err) = check_general_type_compact(
                    context,
                    source_member_type,
                    table_member_type,
                    check_guard.next_level()?,
                ) && err.is_type_not_match()
                {
                    if !context.detail {
                        return Err(TypeCheckFailReason::TypeNotMatch);
                    }

                    return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                        t!(
                            "member %{name} type not match, expect %{expect}, got %{got}",
                            name = key.to_path(),
                            expect =
                                humanize_type(context.db, source_member_type, RenderLevel::Simple),
                            got = humanize_type(context.db, table_member_type, RenderLevel::Simple)
                        )
                        .to_string(),
                    ));
                }
            }
            None if !source_member_type.is_optional() => {
                if !context.detail {
                    return Err(TypeCheckFailReason::TypeNotMatch);
                }

                return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                    t!("missing member %{name}, in table", name = key.to_path()).to_string(),
                ));
            }
            _ => {} // Optional member not found, continue
        }

        context.mark_key_checked(key.clone());
    }

    // 检查超类型
    if let Some(supers) = context.db.get_type_index().get_super_types(source_type_id) {
        let table_type = LuaType::TableConst(
            table_owner
                .get_element_range()
                .ok_or(TypeCheckFailReason::TypeNotMatch)?
                .clone(),
        );
        for super_type in supers {
            check_general_type_compact(
                context,
                &super_type,
                &table_type,
                check_guard.next_level()?,
            )?;
        }
    }

    Ok(())
}

fn check_ref_type_compact_object(
    context: &mut TypeCheckContext,
    object_type: &LuaObjectType,
    source_type_id: &LuaTypeDeclId,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    // ref 可能继承自其他类型, 所以需要使用 infer_members 来获取所有成员
    let Some(source_type_members) = find_members(context.db, &LuaType::Ref(source_type_id.clone()))
    else {
        return Ok(());
    };

    for source_member in source_type_members {
        let source_member_type = source_member.typ;
        let key = source_member.key;
        if context.is_key_checked(&key) {
            continue;
        }

        match get_object_field_type(object_type, &key) {
            Some(field_type) => {
                if let Err(err) = check_general_type_compact(
                    context,
                    &source_member_type,
                    field_type,
                    check_guard.next_level()?,
                ) && err.is_type_not_match()
                {
                    if !context.detail {
                        return Err(TypeCheckFailReason::TypeNotMatch);
                    }

                    return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                        t!(
                            "member %{name} type not match, expect %{expect}, got %{got}",
                            name = key.to_path(),
                            expect =
                                humanize_type(context.db, &source_member_type, RenderLevel::Simple),
                            got = humanize_type(context.db, field_type, RenderLevel::Simple)
                        )
                        .to_string(),
                    ));
                }
            }
            None if !source_member_type.is_optional() => {
                if !context.detail {
                    return Err(TypeCheckFailReason::TypeNotMatch);
                }
                return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                    t!("missing member %{name}, in table", name = key.to_path()).to_string(),
                ));
            }
            _ => {} // Optional member not found, continue
        }

        context.mark_key_checked(key);
    }

    Ok(())
}

fn get_object_field_type<'a>(
    object_type: &'a LuaObjectType,
    key: &LuaMemberKey,
) -> Option<&'a LuaType> {
    object_type.get_field(key).or_else(|| {
        if let LuaMemberKey::ExprType(t) = key {
            object_type
                .get_index_access()
                .iter()
                .find_map(|(index_key, value)| (index_key == t).then_some(value))
        } else {
            None
        }
    })
}

fn check_ref_type_compact_tuple(
    context: &mut TypeCheckContext,
    tuple_type: &LuaTupleType,
    source_type_id: &LuaTypeDeclId,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let Some(source_type_members) = find_members(context.db, &LuaType::Ref(source_type_id.clone()))
    else {
        return Ok(());
    };

    let tuple_types = tuple_type.get_types();
    for member in source_type_members {
        let key = member.key;
        if context.is_key_checked(&key) {
            continue;
        }
        match &key {
            LuaMemberKey::Integer(index) => {
                // 在 lua 中数组索引从 1 开始, 当数组被解析为元组时也必然从 1 开始
                if *index <= 0 {
                    return Err(TypeCheckFailReason::TypeNotMatch);
                }

                let Some(tuple_type) = tuple_types.get(*index as usize - 1) else {
                    return Err(TypeCheckFailReason::TypeNotMatch);
                };

                check_general_type_compact(
                    context,
                    &member.typ,
                    tuple_type,
                    check_guard.next_level()?,
                )?;
            }
            LuaMemberKey::ExprType(LuaType::Integer) => {
                // 遍历元组确定所有内容是否匹配
                for tuple_type in tuple_types {
                    check_general_type_compact(
                        context,
                        &member.typ,
                        tuple_type,
                        check_guard.next_level()?,
                    )?;
                }
            }
            _ => {
                return Err(TypeCheckFailReason::TypeNotMatch);
            }
        }

        context.mark_key_checked(key);
    }

    Ok(())
}
