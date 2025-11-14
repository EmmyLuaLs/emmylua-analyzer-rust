use crate::{
    LuaMemberKey, LuaMemberOwner, LuaType, TypeCheckFailReason, TypeCheckResult, TypeOps,
    find_index_operations,
    semantic::type_check::{
        check_general_type_compact, type_check_context::TypeCheckContext,
        type_check_guard::TypeCheckGuard,
    },
};

pub fn check_array_type_compact(
    context: &mut TypeCheckContext,
    source_base: &LuaType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let source_base = if context.db.get_emmyrc().strict.array_index {
        TypeOps::Union.apply(context.db, source_base, &LuaType::Nil)
    } else {
        source_base.clone()
    };

    match compact_type {
        LuaType::Array(compact_array_type) => {
            return check_general_type_compact(
                context,
                &source_base,
                compact_array_type.get_base(),
                check_guard.next_level()?,
            );
        }
        LuaType::Tuple(tuple_type) => {
            for element_type in tuple_type.get_types() {
                check_general_type_compact(
                    context,
                    &source_base,
                    element_type,
                    check_guard.next_level()?,
                )?;
            }

            return Ok(());
        }
        LuaType::TableConst(inst) => {
            let table_member_owner = LuaMemberOwner::Element(inst.clone());
            return check_array_type_compact_table(
                context,
                &source_base,
                table_member_owner,
                check_guard.next_level()?,
            );
        }
        LuaType::Object(compact_object) => {
            let compact_base = compact_object
                .cast_down_array_base(context.db)
                .ok_or(TypeCheckFailReason::TypeNotMatch)?;
            return check_general_type_compact(
                context,
                &source_base,
                &compact_base,
                check_guard.next_level()?,
            );
        }
        LuaType::Table => return Ok(()),
        LuaType::TableGeneric(compact_types) => {
            if compact_types.len() == 2 {
                for typ in compact_types.iter() {
                    check_general_type_compact(
                        context,
                        &source_base,
                        typ,
                        check_guard.next_level()?,
                    )?;
                }

                return Ok(());
            }
        }
        LuaType::Any => return Ok(()),
        LuaType::Ref(_) | LuaType::Def(_) => {
            return check_array_type_compact_ref_def(
                context,
                &source_base,
                compact_type,
                check_guard.next_level()?,
            );
        }
        _ => {}
    }

    Err(TypeCheckFailReason::DonotCheck)
}

fn check_array_type_compact_ref_def(
    context: &mut TypeCheckContext,
    source_base: &LuaType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let Some(members) = find_index_operations(context.db, compact_type) else {
        return Err(TypeCheckFailReason::TypeNotMatch);
    };

    for member in &members {
        if let LuaMemberKey::ExprType(key_type) = &member.key
            && key_type.is_integer()
            && let Ok(()) =
                check_general_type_compact(context, source_base, &member.typ, check_guard)
        {
            return Ok(());
        }
    }

    Err(TypeCheckFailReason::TypeNotMatch)
}

fn check_array_type_compact_table(
    context: &mut TypeCheckContext,
    source_base: &LuaType,
    table_owner: LuaMemberOwner,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let member_index = context.db.get_member_index();

    let member_len = member_index.get_member_len(&table_owner);
    for i in 0..member_len {
        let key = LuaMemberKey::Integer((i + 1) as i64);
        if let Some(member_item) = member_index.get_member_item(&table_owner, &key) {
            let member_type = member_item
                .resolve_type(context.db)
                .map_err(|_| TypeCheckFailReason::TypeNotMatch)?;
            check_general_type_compact(
                context,
                source_base,
                &member_type,
                check_guard.next_level()?,
            )?;
        } else {
            return Err(TypeCheckFailReason::TypeNotMatch);
        }
    }

    Ok(())
}
