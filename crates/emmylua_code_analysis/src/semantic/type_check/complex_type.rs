use std::collections::HashMap;

use crate::{
    humanize_type, DbIndex, LuaMemberKey, LuaMemberOwner, LuaObjectType, LuaTupleType, LuaType,
    LuaUnionType, RenderLevel,
};

use super::{
    check_general_type_compact, check_type_compact, type_check_fail_reason::TypeCheckFailReason,
    type_check_guard::TypeCheckGuard, TypeCheckResult,
};

// all is duck typing
pub fn check_complex_type_compact(
    db: &DbIndex,
    source: &LuaType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    match source {
        LuaType::Array(source_base) => match compact_type {
            LuaType::Array(compact_base) => {
                return check_general_type_compact(
                    db,
                    &source_base,
                    compact_base,
                    check_guard.next_level()?,
                );
            }
            LuaType::Tuple(tuple_type) => {
                for element_type in tuple_type.get_types() {
                    if check_general_type_compact(
                        db,
                        source_base,
                        element_type,
                        check_guard.next_level()?,
                    )
                    .is_err()
                    {
                        return Err(TypeCheckFailReason::TypeNotMatch);
                    }
                }

                return Ok(());
            }
            LuaType::TableConst(inst) => {
                let table_member_owner = LuaMemberOwner::Element(inst.clone());
                return check_array_type_compact_table(
                    db,
                    &source_base,
                    table_member_owner,
                    check_guard.next_level()?,
                );
            }
            LuaType::Object(compact_object) => {
                let compact_base = compact_object
                    .cast_down_array_base()
                    .ok_or(TypeCheckFailReason::TypeNotMatch)?;
                return check_general_type_compact(
                    db,
                    source_base,
                    &compact_base,
                    check_guard.next_level()?,
                );
            }
            LuaType::Table => return Ok(()),
            _ => {}
        },
        LuaType::Nullable(source_base) => match compact_type {
            LuaType::Nil => return Ok(()),
            LuaType::Nullable(compact_base) => {
                return check_general_type_compact(
                    db,
                    &source_base,
                    compact_base,
                    check_guard.next_level()?,
                );
            }
            _ => {
                if check_general_type_compact(
                    db,
                    &source_base,
                    compact_type,
                    check_guard.next_level()?,
                )
                .is_ok()
                {
                    return Ok(());
                }
            }
        },
        LuaType::Tuple(tuple) => {
            match compact_type {
                LuaType::Tuple(compact_tuple) => {
                    return check_tuple_type_compact_tuple(
                        db,
                        tuple,
                        compact_tuple,
                        check_guard.next_level()?,
                    );
                }
                LuaType::Array(array_base) => {
                    for source_type in tuple.get_types() {
                        if check_general_type_compact(
                            db,
                            array_base,
                            source_type,
                            check_guard.next_level()?,
                        )
                        .is_err()
                        {
                            return Err(TypeCheckFailReason::TypeNotMatch);
                        }
                    }

                    return Ok(());
                }
                LuaType::TableConst(inst) => {
                    let table_member_owner = LuaMemberOwner::Element(inst.clone());
                    return check_tuple_type_compact_table(
                        db,
                        tuple,
                        table_member_owner,
                        check_guard.next_level()?,
                    );
                }
                LuaType::Object(object) => {
                    return check_tuple_type_compact_object_type(
                        db,
                        tuple,
                        object,
                        check_guard.next_level()?,
                    );
                }
                // for any untyped table
                LuaType::Table => return Ok(()),
                _ => {}
            }
        }
        LuaType::Object(source_object) => match compact_type {
            LuaType::Object(compact_object) => {
                return check_object_type_compact_object_type(
                    db,
                    source_object,
                    compact_object,
                    check_guard.next_level()?,
                );
            }
            LuaType::TableConst(inst) => {
                let table_member_owner = LuaMemberOwner::Element(inst.clone());
                return check_object_type_compact_member_owner(
                    db,
                    source_object,
                    table_member_owner,
                    check_guard.next_level()?,
                );
            }
            LuaType::Ref(type_id) => {
                let member_owner = LuaMemberOwner::Type(type_id.clone());
                return check_object_type_compact_member_owner(
                    db,
                    source_object,
                    member_owner,
                    check_guard.next_level()?,
                );
            }
            LuaType::Tuple(compact_tuple) => {
                return check_object_type_compact_tuple(
                    db,
                    source_object,
                    compact_tuple,
                    check_guard.next_level()?,
                );
            }
            LuaType::Table => return Ok(()),
            _ => {}
        },
        LuaType::TableGeneric(source_generic_param) => {
            match compact_type {
                LuaType::Table | LuaType::Global => return Ok(()),
                LuaType::TableGeneric(compact_generic_param) => {
                    if source_generic_param.len() == 2 && compact_generic_param.len() == 2 {
                        let source_key = &source_generic_param[0];
                        let source_value = &source_generic_param[1];
                        let compact_key = &compact_generic_param[0];
                        let compact_value = &compact_generic_param[1];

                        if check_general_type_compact(
                            db,
                            source_key,
                            compact_key,
                            check_guard.next_level()?,
                        )
                        .is_err()
                            || check_general_type_compact(
                                db,
                                source_value,
                                compact_value,
                                check_guard.next_level()?,
                            )
                            .is_err()
                        {
                            return Err(TypeCheckFailReason::TypeNotMatch);
                        }
                        return Ok(());
                    }
                }
                LuaType::TableConst(inst) => {
                    let table_member_owner = LuaMemberOwner::Element(inst.clone());
                    return check_table_generic_compact_member_owner(
                        db,
                        source_generic_param,
                        table_member_owner,
                        check_guard.next_level()?,
                    );
                }
                LuaType::Array(base) => {
                    if source_generic_param.len() == 2 {
                        let key = &source_generic_param[0];
                        let value = &source_generic_param[1];
                        if key.is_any() && check_type_compact(db, value, base).is_ok() {
                            return Ok(());
                        }
                    }
                }
                LuaType::Tuple(tuple) => {
                    if source_generic_param.len() == 2 {
                        let key = &source_generic_param[0];
                        let value = &source_generic_param[1];
                        if key.is_any() {
                            for tuple_type in tuple.get_types() {
                                if check_general_type_compact(
                                    db,
                                    value,
                                    tuple_type,
                                    check_guard.next_level()?,
                                )
                                .is_err()
                                {
                                    return Err(TypeCheckFailReason::TypeNotMatch);
                                }
                            }

                            return Ok(());
                        }

                        return Ok(());
                    }
                }
                // maybe support object
                // need check later
                LuaType::Ref(_) | LuaType::Def(_) | LuaType::Userdata => return Ok(()),
                _ => {}
            }
        }
        LuaType::Union(union_type) => {
            match compact_type {
                LuaType::Union(compact_union) => {
                    return check_union_type_compact_union(
                        db,
                        union_type,
                        compact_union,
                        check_guard.next_level()?,
                    );
                }
                _ => {}
            }
            for sub_type in union_type.get_types() {
                if check_general_type_compact(db, sub_type, compact_type, check_guard.next_level()?)
                    .is_ok()
                {
                    return Ok(());
                }
            }

            return Err(TypeCheckFailReason::TypeNotMatch);
        }
        // need check later
        LuaType::Generic(_) => {
            return Ok(());
        }
        LuaType::MultiLineUnion(multi_union) => {
            let union = multi_union.to_union();
            return check_complex_type_compact(db, &union, &compact_type, check_guard.next_level()?);
        }

        // donot check for now
        // LuaType::Intersection(_) |
        _ => {}
    }
    // Do I need to check union types?
    if let LuaType::Union(union) = compact_type {
        for sub_compact in union.get_types() {
            if check_complex_type_compact(db, source, sub_compact, check_guard.next_level()?)
                .is_err()
            {
                return Err(TypeCheckFailReason::TypeNotMatch);
            }
        }

        return Ok(());
    }

    Err(TypeCheckFailReason::TypeNotMatch)
}

fn check_array_type_compact_table(
    db: &DbIndex,
    source_base: &LuaType,
    table_owner: LuaMemberOwner,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let member_index = db.get_member_index();
    let default_map = HashMap::new();
    let member_map = member_index
        .get_member_map(table_owner.clone())
        .unwrap_or(&default_map);

    let size = member_map.len();
    for i in 0..size {
        let key = LuaMemberKey::Integer((i + 1) as i64);
        if let Some(member_id) = member_map.get(&key) {
            let member = member_index
                .get_member(member_id)
                .ok_or(TypeCheckFailReason::TypeNotMatch)?;
            if check_general_type_compact(
                db,
                source_base,
                &member.get_decl_type(),
                check_guard.next_level()?,
            )
            .is_err()
            {
                return Err(TypeCheckFailReason::TypeNotMatch);
            }
        }
    }

    Ok(())
}

fn check_tuple_type_compact_tuple(
    db: &DbIndex,
    source_tuple: &LuaTupleType,
    compact_tuple: &LuaTupleType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let source_tuple_members = source_tuple.get_types();
    let compact_tuple_members = compact_tuple.get_types();
    let source_size = source_tuple_members.len();
    let compact_size = compact_tuple_members.len();

    for i in 0..source_size {
        let source_tuple_member_type = &source_tuple_members[i];
        if i >= compact_size {
            if source_tuple_member_type.is_optional() || source_tuple_member_type.is_any() {
                continue;
            } else {
                return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                    t!("missing tuple member %{idx}", idx = i + 1).to_string(),
                ));
            }
        }
        let compact_tuple_member_type = &compact_tuple_members[i];

        if check_general_type_compact(
            db,
            source_tuple_member_type,
            compact_tuple_member_type,
            check_guard.next_level()?,
        )
        .is_err()
        {
            return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                t!(
                    "tuple member %{idx} not match, expect %{typ}, but got %{got}",
                    idx = i + 1,
                    typ = humanize_type(db, source_tuple_member_type, RenderLevel::Simple),
                    got = humanize_type(db, compact_tuple_member_type, RenderLevel::Simple)
                )
                .to_string(),
            ));
        }
    }

    Ok(())
}

fn check_tuple_type_compact_table(
    db: &DbIndex,
    source_tuple: &LuaTupleType,
    table_owner: LuaMemberOwner,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let member_index = db.get_member_index();
    let default_map = HashMap::new();
    let member_map = member_index
        .get_member_map(table_owner.clone())
        .unwrap_or(&default_map);

    let tuple_members = source_tuple.get_types();
    let size = tuple_members.len();
    for i in 0..size {
        let source_tuple_member_type = &tuple_members[i];
        let key = LuaMemberKey::Integer((i + 1) as i64);
        if let Some(member_id) = member_map.get(&key) {
            let member = member_index
                .get_member(member_id)
                .ok_or(TypeCheckFailReason::TypeNotMatch)?;
            if check_general_type_compact(
                db,
                source_tuple_member_type,
                &member.get_decl_type(),
                check_guard.next_level()?,
            )
            .is_err()
            {
                return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                    t!(
                        "tuple member %{idx} not match, expect %{typ}, but got %{got}",
                        idx = i + 1,
                        typ = humanize_type(db, source_tuple_member_type, RenderLevel::Simple),
                        got = humanize_type(db, member.get_decl_type(), RenderLevel::Simple)
                    )
                    .to_string(),
                ));
            }
        } else if source_tuple_member_type.is_optional() || source_tuple_member_type.is_any() {
            continue;
        } else {
            return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                t!("missing tuple member %{idx}", idx = i + 1).to_string(),
            ));
        }
    }

    Ok(())
}

fn check_tuple_type_compact_object_type(
    db: &DbIndex,
    source_tuple: &LuaTupleType,
    object_type: &LuaObjectType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let object_members = object_type.get_fields();

    let tuple_members = source_tuple.get_types();
    let size = tuple_members.len();
    for i in 0..size {
        let source_tuple_member_type = &tuple_members[i];
        let key = LuaMemberKey::Integer((i + 1) as i64);
        if let Some(object_member_type) = object_members.get(&key) {
            if check_general_type_compact(
                db,
                source_tuple_member_type,
                object_member_type,
                check_guard.next_level()?,
            )
            .is_err()
            {
                return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                    t!(
                        "tuple member %{idx} not match, expect %{typ}, but got %{got}",
                        idx = i + 1,
                        typ = humanize_type(db, source_tuple_member_type, RenderLevel::Simple),
                        got = humanize_type(db, object_member_type, RenderLevel::Simple)
                    )
                    .to_string(),
                ));
            }
        } else if source_tuple_member_type.is_optional() || source_tuple_member_type.is_any() {
            continue;
        } else {
            return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                t!("missing tuple member %{idx}", idx = i + 1).to_string(),
            ));
        }
    }

    Ok(())
}

fn check_object_type_compact_object_type(
    db: &DbIndex,
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
                if source_type.is_optional() || source_type.is_any() {
                    continue;
                } else {
                    return Err(TypeCheckFailReason::TypeNotMatch);
                }
            }
        };
        if check_general_type_compact(db, source_type, compact_type, check_guard.next_level()?)
            .is_err()
        {
            return Err(TypeCheckFailReason::TypeNotMatch);
        }
    }

    Ok(())
}

fn check_object_type_compact_member_owner(
    db: &DbIndex,
    source_object: &LuaObjectType,
    member_owner: LuaMemberOwner,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let member_index = db.get_member_index();
    let default_map = HashMap::new();
    let members = member_index
        .get_member_map(member_owner.clone())
        .unwrap_or(&default_map);

    for (key, source_type) in source_object.get_fields() {
        let member_id = match members.get(key) {
            Some(id) => id,
            None => {
                if source_type.is_optional() || source_type.is_any() {
                    continue;
                } else {
                    return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                        t!("missing member %{key}", key = key.to_path().to_string()).to_string(),
                    ));
                }
            }
        };
        let member = member_index
            .get_member(member_id)
            .ok_or(TypeCheckFailReason::TypeNotMatch)?;
        let member_type = member.get_decl_type();
        if check_general_type_compact(db, source_type, member_type, check_guard.next_level()?)
            .is_err()
        {
            return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                t!(
                    "member %{key} not match, expect %{typ}, but got %{got}",
                    key = key.to_path().to_string(),
                    typ = humanize_type(db, source_type, RenderLevel::Simple),
                    got = humanize_type(db, member_type, RenderLevel::Simple)
                )
                .to_string(),
            ));
        }
    }

    Ok(())
}

fn check_object_type_compact_tuple(
    db: &DbIndex,
    source_object: &LuaObjectType,
    tuple_type: &LuaTupleType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let source_members = source_object.get_fields();
    for (source_key, source_type) in source_members {
        let idx = match source_key {
            LuaMemberKey::Integer(i) => i - 1,
            _ => {
                if source_type.is_optional() || source_type.is_any() {
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
                if source_type.is_optional() || source_type.is_any() {
                    continue;
                } else {
                    return Err(TypeCheckFailReason::TypeNotMatch);
                }
            }
        };

        if check_general_type_compact(
            db,
            source_type,
            tuple_member_type,
            check_guard.next_level()?,
        )
        .is_err()
        {
            return Err(TypeCheckFailReason::TypeNotMatch);
        }
    }

    Ok(())
}

fn check_table_generic_compact_member_owner(
    db: &DbIndex,
    source_generic_params: &Vec<LuaType>,
    member_owner: LuaMemberOwner,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    if source_generic_params.len() != 2 {
        return Err(TypeCheckFailReason::TypeNotMatch);
    }

    let member_index = db.get_member_index();
    let default_map = HashMap::new();
    let members = member_index
        .get_member_map(member_owner.clone())
        .unwrap_or(&default_map);

    let source_key = &source_generic_params[0];
    let source_value = &source_generic_params[1];

    for (key, value) in members {
        let key_type = match key {
            LuaMemberKey::Integer(i) => LuaType::IntegerConst(*i),
            LuaMemberKey::Name(s) => LuaType::StringConst(s.clone().into()),
            _ => LuaType::Any,
        };

        let member = member_index
            .get_member(value)
            .ok_or(TypeCheckFailReason::TypeNotMatch)?;
        let member_type = member.get_decl_type();
        if check_general_type_compact(db, source_key, &key_type, check_guard.next_level()?).is_err()
            || check_general_type_compact(db, source_value, &member_type, check_guard.next_level()?)
                .is_err()
        {
            return Err(TypeCheckFailReason::TypeNotMatch);
        }
    }

    Ok(())
}

// too complex
fn check_union_type_compact_union(
    db: &DbIndex,
    source_union: &LuaUnionType,
    compact_union: &LuaUnionType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let source_types = source_union.get_types();
    let compact_types = compact_union.get_types();
    for compact_sub_type in compact_types {
        let mut is_match = false;
        for source_sub_type in source_types {
            if check_general_type_compact(
                db,
                source_sub_type,
                compact_sub_type,
                check_guard.next_level()?,
            )
            .is_ok()
            {
                is_match = true;
                break;
            }
        }

        if !is_match {
            return Err(TypeCheckFailReason::TypeNotMatch);
        }
    }

    Ok(())
}
