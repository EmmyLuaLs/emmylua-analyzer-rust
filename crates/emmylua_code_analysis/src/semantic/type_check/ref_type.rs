use hashbrown::HashMap;

use crate::{
    LuaMemberKey, LuaMemberOwner, LuaObjectType, LuaTupleType, LuaType, LuaTypeDecl, LuaTypeDeclId,
    RenderLevel, humanize_type,
    semantic::{
        member::find_members,
        type_check::{
            intersection_utils::intersection_to_object, type_check_context::TypeCheckContext,
        },
    },
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
            if result.is_err() && should_retry_alias_nominal_check(compact_type) {
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

fn should_retry_alias_nominal_check(compact_type: &LuaType) -> bool {
    match compact_type {
        LuaType::Ref(_) | LuaType::Def(_) => true,
        LuaType::Generic(generic) => {
            matches!(generic.get_base_type(), LuaType::Ref(_) | LuaType::Def(_))
        }
        _ => false,
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

    // 整数 enum 参与位运算时结果会被推断为宽泛 Integer, 但直接写入整数常量仍需匹配 enum 字段.
    if let LuaType::Union(union_types) = &enum_fields
        && union_types
            .into_vec()
            .iter()
            .all(|t| matches!(t, LuaType::DocIntegerConst(_) | LuaType::IntegerConst(_)))
        && matches!(compact_type, LuaType::Integer)
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
        LuaType::Intersection(intersection) => {
            if let Some(object_type) = intersection_to_object(context.db, intersection) {
                check_ref_type_compact_object(
                    context,
                    &object_type,
                    source_id,
                    check_guard.next_level()?,
                )
            } else {
                Err(TypeCheckFailReason::TypeNotMatch)
            }
        }
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
    let table_members = member_index.get_members(&table_owner).unwrap_or_default();
    let table_member_map: HashMap<_, _> = table_members
        .iter()
        .map(|member| {
            let member_type = context
                .db
                .get_type_index()
                .get_type_cache(&member.get_id().into())
                .map(|cache| cache.as_type().clone())
                .unwrap_or(LuaType::Any);
            (member.get_key().clone(), member_type)
        })
        .collect();

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
            .map(|cache| cache.as_type().clone())
            .unwrap_or(LuaType::Any);
        let key = source_member.get_key();

        if context.is_key_checked(key) {
            continue;
        }

        if let LuaMemberKey::TypeKey(source_key_type) = key {
            // 索引签名约束已有索引字段, 不要求表字面量必须包含索引字段.
            for table_member in &table_members {
                let Some(table_key_type) = table_member.get_key().to_index_type() else {
                    continue;
                };

                let key_match = match check_general_type_compact(
                    context,
                    source_key_type,
                    &table_key_type,
                    check_guard.next_level()?,
                ) {
                    Ok(_) => true,
                    Err(err) if err.is_type_not_match() => false,
                    Err(err) => return Err(err),
                };

                if !key_match {
                    continue;
                }

                let table_member_type = table_member_map
                    .get(table_member.get_key())
                    .unwrap_or(&LuaType::Any);
                check_ref_member_type(
                    context,
                    table_member.get_key(),
                    &source_member_type,
                    table_member_type,
                    check_guard,
                )?;
            }

            context.mark_key_checked(key.clone());
            continue;
        }

        match table_member_map.get(key) {
            Some(table_member_type) => {
                check_ref_member_type(
                    context,
                    key,
                    &source_member_type,
                    table_member_type,
                    check_guard,
                )?;
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

fn check_ref_member_type(
    context: &mut TypeCheckContext,
    key: &LuaMemberKey,
    expect: &LuaType,
    got: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    if let Err(err) = check_general_type_compact(context, expect, got, check_guard.next_level()?)
        && err.is_type_not_match()
    {
        if !context.detail {
            return Err(TypeCheckFailReason::TypeNotMatch);
        }

        return Err(TypeCheckFailReason::TypeNotMatchWithReason(
            t!(
                "member %{name} type not match, expect %{expect}, got %{got}",
                name = key.to_path(),
                expect = humanize_type(context.db, expect, RenderLevel::Simple),
                got = humanize_type(context.db, got, RenderLevel::Simple)
            )
            .to_string(),
        ));
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
                check_ref_member_type(context, &key, &source_member_type, field_type, check_guard)?;
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
        if let LuaMemberKey::TypeKey(t) = key {
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
                    if member.typ.is_optional() {
                        continue;
                    }
                    return Err(TypeCheckFailReason::TypeNotMatch);
                };

                check_general_type_compact(
                    context,
                    &member.typ,
                    tuple_type,
                    check_guard.next_level()?,
                )?;
            }
            LuaMemberKey::TypeKey(LuaType::Integer) => {
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
                if member.typ.is_optional() {
                    continue;
                }
                return Err(TypeCheckFailReason::TypeNotMatch);
            }
        }

        context.mark_key_checked(key);
    }

    Ok(())
}
