use crate::{
    DbIndex, LuaMemberKey, LuaMemberOwner, LuaType, TypeCheckFailReason, TypeCheckResult, TypeOps,
    find_index_operations,
    semantic::type_check::{check_general_type_compact, type_check_guard::TypeCheckGuard},
};

pub fn check_array_type_compact(
    db: &DbIndex,
    source_base: &LuaType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let source_base = TypeOps::Union.apply(db, source_base, &LuaType::Nil);

    match compact_type {
        LuaType::Array(compact_array_type) => {
            return check_general_type_compact(
                db,
                &source_base,
                compact_array_type.get_base(),
                check_guard.next_level()?,
            );
        }
        LuaType::Tuple(tuple_type) => {
            for element_type in tuple_type.get_types() {
                check_general_type_compact(
                    db,
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
                db,
                &source_base,
                table_member_owner,
                check_guard.next_level()?,
            );
        }
        LuaType::Object(compact_object) => {
            let compact_base = compact_object
                .cast_down_array_base(db)
                .ok_or(TypeCheckFailReason::TypeNotMatch)?;
            return check_general_type_compact(
                db,
                &source_base,
                &compact_base,
                check_guard.next_level()?,
            );
        }
        LuaType::Table => return Ok(()),
        LuaType::TableGeneric(compact_types) => {
            if compact_types.len() == 2 {
                for typ in compact_types.iter() {
                    check_general_type_compact(db, &source_base, typ, check_guard.next_level()?)?;
                }

                return Ok(());
            }
        }
        LuaType::Any => return Ok(()),
        LuaType::Ref(_) | LuaType::Def(_) => {
            return check_array_type_compact_ref_def(
                db,
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
    db: &DbIndex,
    source_base: &LuaType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let Some(members) = find_index_operations(db, compact_type) else {
        return Err(TypeCheckFailReason::TypeNotMatch);
    };

    for member in &members {
        match &member.key {
            LuaMemberKey::ExprType(key_type) => {
                if key_type.is_integer() {
                    match check_general_type_compact(db, source_base, &member.typ, check_guard) {
                        Ok(()) => return Ok(()),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
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

    let member_len = member_index.get_member_len(&table_owner);
    for i in 0..member_len {
        let key = LuaMemberKey::Integer((i + 1) as i64);
        if let Some(member_item) = member_index.get_member_item(&table_owner, &key) {
            let member_type = member_item
                .resolve_type(db)
                .map_err(|_| TypeCheckFailReason::TypeNotMatch)?;
            check_general_type_compact(db, source_base, &member_type, check_guard.next_level()?)?;
        } else {
            return Err(TypeCheckFailReason::TypeNotMatch);
        }
    }

    Ok(())
}
