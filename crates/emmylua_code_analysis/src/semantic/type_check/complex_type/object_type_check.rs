use crate::{
    LuaMemberKey, LuaMemberOwner, LuaObjectType, LuaTupleType, LuaType, RenderLevel,
    TypeCheckFailReason, TypeCheckResult, humanize_type,
    semantic::type_check::{
        check_general_type_compact, type_check_context::TypeCheckContext,
        type_check_guard::TypeCheckGuard,
    },
};

pub fn check_object_type_compact(
    context: &mut TypeCheckContext,
    source_object: &LuaObjectType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    match compact_type {
        LuaType::Object(compact_object) => {
            return check_object_type_compact_object_type(
                context,
                source_object,
                compact_object,
                check_guard.next_level()?,
            );
        }
        LuaType::TableConst(inst) => {
            let table_member_owner = LuaMemberOwner::Element(inst.clone());
            return check_object_type_compact_member_owner(
                context,
                source_object,
                table_member_owner,
                check_guard.next_level()?,
            );
        }
        LuaType::Ref(type_id) => {
            let member_owner = LuaMemberOwner::Type(type_id.clone());
            return check_object_type_compact_member_owner(
                context,
                source_object,
                member_owner,
                check_guard.next_level()?,
            );
        }
        LuaType::Tuple(compact_tuple) => {
            return check_object_type_compact_tuple(
                context,
                source_object,
                compact_tuple,
                check_guard.next_level()?,
            );
        }
        LuaType::Array(array_type) => {
            return check_object_type_compact_array(
                context,
                source_object,
                array_type.get_base(),
                check_guard.next_level()?,
            );
        }
        LuaType::Table => return Ok(()),
        _ => {}
    }

    Err(TypeCheckFailReason::DonotCheck)
}

fn check_object_type_compact_object_type(
    context: &mut TypeCheckContext,
    source_object: &LuaObjectType,
    compact_object: &LuaObjectType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let source_members = source_object.get_fields();
    let compact_members = compact_object.get_fields();

    for (key, source_type) in source_members {
        let compact_type = match compact_members.get(key) {
            Some(t) => t,
            None => {
                if source_type.is_nullable() || source_type.is_any() {
                    continue;
                } else {
                    return Err(TypeCheckFailReason::TypeNotMatch);
                }
            }
        };
        check_general_type_compact(
            context,
            source_type,
            compact_type,
            check_guard.next_level()?,
        )?;
    }

    Ok(())
}

fn check_object_type_compact_member_owner(
    context: &mut TypeCheckContext,
    source_object: &LuaObjectType,
    member_owner: LuaMemberOwner,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let member_index = context.db.get_member_index();
    let source_fields = source_object.get_fields();

    // 检查名称字段
    for (key, source_type) in source_fields {
        let member_item = match member_index.get_member_item(&member_owner, key) {
            Some(member_item) => member_item,
            None => {
                if source_type.is_nullable() || source_type.is_any() {
                    continue;
                } else {
                    return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                        t!("missing member %{key}", key = key.to_path().to_string()).to_string(),
                    ));
                }
            }
        };
        let member_type = match member_item.resolve_type(context.db) {
            Ok(t) => t,
            _ => {
                continue;
            }
        };

        match check_general_type_compact(
            context,
            source_type,
            &member_type,
            check_guard.next_level()?,
        ) {
            Ok(_) => {}
            Err(TypeCheckFailReason::TypeNotMatch) => {
                return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                    t!(
                        "member %{key} not match, expect %{typ}, but got %{got}",
                        key = key.to_path().to_string(),
                        typ = humanize_type(context.db, source_type, RenderLevel::Simple),
                        got = humanize_type(context.db, &member_type, RenderLevel::Simple)
                    )
                    .to_string(),
                ));
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    // 检查索引访问字段
    let members = member_index.get_members(&member_owner).unwrap_or_default();
    for (key_type, source_type) in source_object.get_index_access() {
        for member in &members {
            let member = *member;
            if source_fields.contains_key(member.get_key()) {
                continue;
            }
            let member_key_type = match member.get_key() {
                LuaMemberKey::Integer(i) => LuaType::IntegerConst(*i),
                LuaMemberKey::Name(name) => LuaType::StringConst(name.clone().into()),
                LuaMemberKey::ExprType(typ) => typ.clone(),
                LuaMemberKey::None => continue,
            };

            let key_match = match check_general_type_compact(
                context,
                key_type,
                &member_key_type,
                check_guard.next_level()?,
            ) {
                Ok(_) => true,
                Err(err) => {
                    if err.is_type_not_match() {
                        false
                    } else {
                        return Err(err);
                    }
                }
            };

            if !key_match {
                continue;
            }

            let member_type = match context
                .db
                .get_type_index()
                .get_type_cache(&member.get_id().into())
            {
                Some(cache) => cache.as_type().clone(),
                None => continue,
            };

            match check_general_type_compact(
                context,
                source_type,
                &member_type,
                check_guard.next_level()?,
            ) {
                Ok(_) => {
                    break;
                }
                Err(TypeCheckFailReason::TypeNotMatch) => {
                    let mut key_display = member.get_key().to_path();
                    if key_display.is_empty() {
                        key_display =
                            humanize_type(context.db, &member_key_type, RenderLevel::Simple);
                    }
                    return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                        t!(
                            "member %{key} not match, expect %{typ}, but got %{got}",
                            key = key_display,
                            typ = humanize_type(context.db, source_type, RenderLevel::Simple),
                            got = humanize_type(context.db, &member_type, RenderLevel::Simple)
                        )
                        .to_string(),
                    ));
                }
                Err(err) => {
                    return Err(err);
                }
            }
        }
    }

    Ok(())
}

fn check_object_type_compact_tuple(
    context: &mut TypeCheckContext,
    source_object: &LuaObjectType,
    tuple_type: &LuaTupleType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let source_members = source_object.get_fields();
    for (source_key, source_type) in source_members {
        let idx = match source_key {
            LuaMemberKey::Integer(i) => i - 1,
            _ => {
                if source_type.is_nullable() || source_type.is_any() {
                    continue;
                } else {
                    return Err(TypeCheckFailReason::TypeNotMatch);
                }
            }
        };

        if idx < 0 {
            continue;
        }

        let idx = idx as usize;
        let tuple_member_type = match tuple_type.get_type(idx) {
            Some(t) => t,
            None => {
                if source_type.is_nullable() || source_type.is_any() {
                    continue;
                } else {
                    return Err(TypeCheckFailReason::TypeNotMatch);
                }
            }
        };

        check_general_type_compact(
            context,
            source_type,
            tuple_member_type,
            check_guard.next_level()?,
        )?;
    }

    Ok(())
}

fn check_object_type_compact_array(
    context: &mut TypeCheckContext,
    source_object: &LuaObjectType,
    array: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let index_access = source_object.get_index_access();
    if index_access.is_empty() {
        return Err(TypeCheckFailReason::TypeNotMatch);
    }
    for (key, source_type) in index_access {
        if !key.is_integer() {
            continue;
        }
        match check_general_type_compact(context, source_type, array, check_guard.next_level()?) {
            Ok(_) => {
                return Ok(());
            }
            Err(e) if e.is_type_not_match() => {}
            Err(e) => {
                return Err(e);
            }
        }
    }
    Err(TypeCheckFailReason::TypeNotMatch)
}
