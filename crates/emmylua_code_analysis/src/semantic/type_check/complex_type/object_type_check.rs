use std::collections::{HashMap, hash_map::Entry};

use crate::{
    LuaMemberKey, LuaMemberOwner, LuaObjectType, LuaTupleType, LuaType, RenderLevel,
    TypeCheckFailReason, TypeCheckResult, humanize_type,
    semantic::{
        member::find_members,
        type_check::{
            check_general_type_compact, type_check_context::TypeCheckContext,
            type_check_guard::TypeCheckGuard,
        },
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
            return check_object_type_compact_table_const(
                context,
                source_object,
                inst,
                check_guard.next_level()?,
            );
        }
        LuaType::Ref(type_id) => {
            return check_object_type_compact_type_ref(
                context,
                source_object,
                type_id,
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

struct TypeMembers {
    map: HashMap<LuaMemberKey, LuaType>,
    index_keys: Vec<LuaMemberKey>,
}

fn collect_type_members(
    context: &TypeCheckContext,
    type_id: &crate::LuaTypeDeclId,
    collect_index_keys: bool,
) -> Option<TypeMembers> {
    let type_members = find_members(context.db, &LuaType::Ref(type_id.clone()))?;

    // Build a merged view of class members (including supertypes). When the same key appears
    // multiple times (override), keep the first one (subclass wins).
    let mut map: HashMap<LuaMemberKey, LuaType> = HashMap::new();
    let mut index_keys: Vec<LuaMemberKey> = Vec::new();
    if collect_index_keys {
        index_keys.reserve(type_members.len());
    }

    for member in type_members {
        let key = member.key;
        let typ = member.typ;

        if let Entry::Vacant(entry) = map.entry(key) {
            if collect_index_keys {
                index_keys.push(entry.key().clone());
            }
            entry.insert(typ);
        }
    }

    Some(TypeMembers { map, index_keys })
}

fn check_member_value(
    context: &mut TypeCheckContext,
    key: &LuaMemberKey,
    key_type_for_display: Option<&LuaType>,
    source_type: &LuaType,
    member_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    match check_general_type_compact(context, source_type, member_type, check_guard.next_level()?) {
        Ok(_) => Ok(()),
        Err(TypeCheckFailReason::TypeNotMatch) => {
            let mut key_display = key.to_path();
            if key_display.is_empty() {
                if let Some(key_type_for_display) = key_type_for_display {
                    key_display =
                        humanize_type(context.db, key_type_for_display, RenderLevel::Simple);
                }
            }

            Err(TypeCheckFailReason::TypeNotMatchWithReason(
                t!(
                    "member %{key} not match, expect %{typ}, but got %{got}",
                    key = key_display,
                    typ = humanize_type(context.db, source_type, RenderLevel::Simple),
                    got = humanize_type(context.db, member_type, RenderLevel::Simple)
                )
                .to_string(),
            ))
        }
        Err(e) => Err(e),
    }
}

fn member_key_type_for_index(member_key: &LuaMemberKey) -> Option<LuaType> {
    match member_key {
        LuaMemberKey::Integer(i) => Some(LuaType::IntegerConst(*i)),
        LuaMemberKey::Name(name) => Some(LuaType::StringConst(name.clone().into())),
        LuaMemberKey::ExprType(typ) => Some(typ.clone()),
        LuaMemberKey::None => None,
    }
}

fn check_object_type_compact_table_const(
    context: &mut TypeCheckContext,
    source_object: &LuaObjectType,
    table_range: &crate::InFiled<rowan::TextRange>,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let db = context.db;
    let member_owner = LuaMemberOwner::Element(table_range.clone());
    let member_index = db.get_member_index();
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
        let member_type = match member_item.resolve_type(db) {
            Ok(t) => t,
            _ => {
                continue;
            }
        };

        check_member_value(context, key, None, source_type, &member_type, check_guard)?;
    }

    if source_object.get_index_access().is_empty() {
        return Ok(());
    }

    // 检查索引访问字段
    let members = member_index.get_members(&member_owner).unwrap_or_default();
    for (key_type, source_type) in source_object.get_index_access() {
        for member in &members {
            let member = *member;
            if source_fields.contains_key(member.get_key()) {
                continue;
            }
            let Some(member_key_type) = member_key_type_for_index(member.get_key()) else {
                continue;
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
                Some(cache) => cache.as_type(),
                None => continue,
            };

            check_member_value(
                context,
                member.get_key(),
                Some(&member_key_type),
                source_type,
                member_type,
                check_guard,
            )?;
            break;
        }
    }

    Ok(())
}

fn check_object_type_compact_type_ref(
    context: &mut TypeCheckContext,
    source_object: &LuaObjectType,
    type_id: &crate::LuaTypeDeclId,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let source_fields = source_object.get_fields();
    let has_index_access = !source_object.get_index_access().is_empty();

    let Some(type_members) = collect_type_members(context, type_id, has_index_access) else {
        return Ok(());
    };

    for (key, source_type) in source_fields {
        let Some(member_type) = type_members.map.get(key) else {
            if source_type.is_nullable() || source_type.is_any() {
                continue;
            }

            return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                t!("missing member %{key}", key = key.to_path().to_string()).to_string(),
            ));
        };

        check_member_value(context, key, None, source_type, member_type, check_guard)?;
    }

    if !has_index_access {
        return Ok(());
    }

    for (key_type, source_type) in source_object.get_index_access() {
        for member_key in &type_members.index_keys {
            if source_fields.contains_key(member_key) {
                continue;
            }

            let Some(member_key_type) = member_key_type_for_index(member_key) else {
                continue;
            };

            let key_match = match check_general_type_compact(
                context,
                key_type,
                &member_key_type,
                check_guard.next_level()?,
            ) {
                Ok(_) => true,
                Err(err) if err.is_type_not_match() => false,
                Err(err) => return Err(err),
            };

            if !key_match {
                continue;
            }

            let Some(member_type) = type_members.map.get(member_key) else {
                continue;
            };

            check_member_value(
                context,
                member_key,
                Some(&member_key_type),
                source_type,
                member_type,
                check_guard,
            )?;
            break;
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
