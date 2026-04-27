use std::ops::Deref;

use crate::{
    LuaMemberKey, LuaMemberOwner, LuaObjectType, LuaTupleType, LuaType, RenderLevel,
    TypeCheckFailReason, TypeCheckResult, VariadicType, humanize_type,
    semantic::type_check::{
        check_general_type_compact, type_check_context::TypeCheckContext,
        type_check_guard::TypeCheckGuard,
    },
};

pub fn check_tuple_type_compact(
    context: &mut TypeCheckContext,
    tuple: &LuaTupleType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    match compact_type {
        LuaType::Tuple(compact_tuple) => {
            return check_tuple_type_compact_tuple(
                context,
                tuple,
                compact_tuple,
                check_guard.next_level()?,
            );
        }
        LuaType::Array(array_type) => {
            for source_type in tuple.get_types() {
                check_general_type_compact(
                    context,
                    array_type.get_base(),
                    source_type,
                    check_guard.next_level()?,
                )?;
            }

            return Ok(());
        }
        LuaType::TableConst(inst) => {
            let table_member_owner = LuaMemberOwner::Element(inst.clone());
            return check_tuple_type_compact_table(
                context,
                tuple,
                table_member_owner,
                check_guard.next_level()?,
            );
        }
        LuaType::Object(object) => {
            return check_tuple_type_compact_object_type(
                context,
                tuple,
                object,
                check_guard.next_level()?,
            );
        }
        // for any untyped table
        LuaType::Table => return Ok(()),
        _ => {}
    }

    Err(TypeCheckFailReason::DonotCheck)
}

fn check_tuple_type_compact_tuple(
    context: &mut TypeCheckContext,
    source_tuple: &LuaTupleType,
    compact_tuple: &LuaTupleType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let source_tuple_members = source_tuple.get_types();
    let compact_tuple_members = compact_tuple.get_types();

    check_tuple_types_compact_tuple_types(
        context,
        0,
        source_tuple_members,
        compact_tuple_members,
        check_guard,
    )
}

fn check_tuple_types_compact_tuple_types(
    context: &mut TypeCheckContext,
    source_start: usize,
    sources: &[LuaType],
    compacts: &[LuaType],
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let source_size = sources.len();
    let compact_size = compacts.len();

    for i in 0..source_size {
        let source_tuple_member_type = &sources[i];
        if i >= compact_size {
            if source_tuple_member_type.is_optional() {
                continue;
            } else {
                return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                    t!("missing tuple member %{idx}", idx = i + source_start + 1).to_string(),
                ));
            }
        }
        let compact_tuple_member_type = &compacts[i];
        match (source_tuple_member_type, compact_tuple_member_type) {
            (LuaType::Variadic(variadic), _) => {
                if let VariadicType::Base(inner) = variadic.deref() {
                    let compact_rest_len = compact_size - i;
                    if compact_rest_len == 0 {
                        return Ok(());
                    }
                    let mut new_source_types = vec![];
                    for _ in 0..compact_rest_len {
                        new_source_types.push(inner.clone());
                    }
                    return check_tuple_types_compact_tuple_types(
                        context,
                        i,
                        &new_source_types,
                        &compacts[i..],
                        check_guard.next_level()?,
                    );
                }
            }
            (_, LuaType::Variadic(variadic)) => {
                if let VariadicType::Base(compact_inner) = variadic.deref() {
                    let source_rest_len = source_size - i;
                    if source_rest_len == 0 {
                        return Ok(());
                    }
                    let mut new_compact_types = vec![];
                    for _ in 0..source_rest_len {
                        new_compact_types.push(compact_inner.clone());
                    }
                    return check_tuple_types_compact_tuple_types(
                        context,
                        i,
                        &sources[i..],
                        &new_compact_types,
                        check_guard.next_level()?,
                    );
                }
            }
            _ => {
                match check_general_type_compact(
                    context,
                    source_tuple_member_type,
                    compact_tuple_member_type,
                    check_guard.next_level()?,
                ) {
                    Ok(_) => {}
                    Err(TypeCheckFailReason::TypeNotMatch) => {
                        return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                            t!(
                                "tuple member %{idx} not match, expect %{typ}, but got %{got}",
                                idx = i + source_start + 1,
                                typ = humanize_type(
                                    context.db(),
                                    source_tuple_member_type,
                                    RenderLevel::Simple
                                ),
                                got = humanize_type(
                                    context.db(),
                                    compact_tuple_member_type,
                                    RenderLevel::Simple
                                )
                            )
                            .to_string(),
                        ));
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }
    }

    Ok(())
}

fn check_tuple_type_compact_table(
    context: &mut TypeCheckContext,
    source_tuple: &LuaTupleType,
    table_owner: LuaMemberOwner,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let member_index = context.db().get_member_index();
    let tuple_members = source_tuple.get_types();
    for (i, source_tuple_member_type) in tuple_members.iter().enumerate() {
        let key = LuaMemberKey::Integer((i + 1) as i64);
        if let Some(member_item) = member_index.get_member_item(&table_owner, &key) {
            let member_type = member_item
                .resolve_type(context.db())
                .map_err(|_| TypeCheckFailReason::TypeNotMatch)?;
            match check_general_type_compact(
                context,
                source_tuple_member_type,
                &member_type,
                check_guard.next_level()?,
            ) {
                Ok(_) => {}
                Err(TypeCheckFailReason::TypeNotMatch) => {
                    return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                        t!(
                            "tuple member %{idx} not match, expect %{typ}, but got %{got}",
                            idx = i + 1,
                            typ = humanize_type(
                                context.db(),
                                source_tuple_member_type,
                                RenderLevel::Simple
                            ),
                            got = humanize_type(context.db(), &member_type, RenderLevel::Simple)
                        )
                        .to_string(),
                    ));
                }
                Err(e) => {
                    return Err(e);
                }
            }
        } else if source_tuple_member_type.is_optional() {
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
    context: &mut TypeCheckContext,
    source_tuple: &LuaTupleType,
    object_type: &LuaObjectType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let object_members = object_type.get_fields();

    let tuple_members = source_tuple.get_types();
    // for i in 0..size {
    for (i, source_tuple_member_type) in tuple_members.iter().enumerate() {
        let key = LuaMemberKey::Integer((i + 1) as i64);
        if let Some(object_member_type) = object_members.get(&key) {
            match check_general_type_compact(
                context,
                source_tuple_member_type,
                object_member_type,
                check_guard.next_level()?,
            ) {
                Ok(_) => {}
                Err(TypeCheckFailReason::TypeNotMatch) => {
                    return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                        t!(
                            "tuple member %{idx} not match, expect %{typ}, but got %{got}",
                            idx = i + 1,
                            typ = humanize_type(
                                context.db(),
                                source_tuple_member_type,
                                RenderLevel::Simple
                            ),
                            got =
                                humanize_type(context.db(), object_member_type, RenderLevel::Simple)
                        )
                        .to_string(),
                    ));
                }
                Err(e) => {
                    return Err(e);
                }
            }
        } else if source_tuple_member_type.is_nullable() || source_tuple_member_type.is_any() {
            continue;
        } else {
            return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                t!("missing tuple member %{idx}", idx = i + 1).to_string(),
            ));
        }
    }

    Ok(())
}
