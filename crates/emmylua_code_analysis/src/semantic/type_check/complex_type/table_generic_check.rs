use crate::{
    check_type_compact,
    semantic::type_check::{check_general_type_compact, type_check_guard::TypeCheckGuard},
    DbIndex, LuaMemberKey, LuaMemberOwner, LuaType, LuaTypeCache, TypeCheckFailReason,
    TypeCheckResult,
};

pub fn check_table_generic_type_compact(
    db: &DbIndex,
    source_generic_param: &Vec<LuaType>,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    match compact_type {
        LuaType::Table | LuaType::Global => return Ok(()),
        LuaType::TableGeneric(compact_generic_param) => {
            if source_generic_param.len() == 2 && compact_generic_param.len() == 2 {
                let source_key = &source_generic_param[0];
                let source_value = &source_generic_param[1];
                let compact_key = &compact_generic_param[0];
                let compact_value = &compact_generic_param[1];

                check_general_type_compact(db, source_key, compact_key, check_guard.next_level()?)?;
                check_general_type_compact(
                    db,
                    source_value,
                    compact_value,
                    check_guard.next_level()?,
                )?;
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
        LuaType::Array(array_type) => {
            if source_generic_param.len() == 2 {
                let key = &source_generic_param[0];
                let value = &source_generic_param[1];
                if key.is_any() || key.is_integer() {
                    return check_type_compact(db, value, array_type.get_base());
                }
            }
        }
        LuaType::Tuple(tuple) => {
            if source_generic_param.len() == 2 {
                let key = &source_generic_param[0];
                let value = &source_generic_param[1];
                if key.is_any() {
                    for tuple_type in tuple.get_types() {
                        check_general_type_compact(
                            db,
                            value,
                            tuple_type,
                            check_guard.next_level()?,
                        )?;
                    }

                    return Ok(());
                }

                return Ok(());
            }
        }
        LuaType::Userdata => return Ok(()),
        // maybe support object
        // need check later
        LuaType::Ref(id) | LuaType::Def(id) => {
            let owner = LuaMemberOwner::Type(id.clone());
            return check_table_generic_compact_member_owner(
                db,
                source_generic_param,
                owner,
                check_guard.next_level()?,
            );
        }
        LuaType::Generic(_) => {
            return Ok(());
        }
        LuaType::Union(union) => {
            for union_type in union.into_vec() {
                check_table_generic_type_compact(
                    db,
                    source_generic_param,
                    &union_type,
                    check_guard,
                )?;
            }

            return Ok(());
        }
        _ => {}
    }

    Err(TypeCheckFailReason::TypeNotMatch)
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
    let members = match member_index.get_members(&member_owner) {
        Some(members) => members,
        None => return Ok(()),
    };

    let source_key = &source_generic_params[0];
    let source_value = &source_generic_params[1];

    for member in members {
        let key = member.get_key();
        let key_type = match key {
            LuaMemberKey::Integer(i) => LuaType::IntegerConst(*i),
            LuaMemberKey::Name(s) => LuaType::StringConst(s.clone().into()),
            _ => LuaType::Any,
        };

        let member_type = db
            .get_type_index()
            .get_type_cache(&member.get_id().into())
            .unwrap_or(&LuaTypeCache::InferType(LuaType::Unknown))
            .as_type();
        check_general_type_compact(db, source_key, &key_type, check_guard.next_level()?)?;
        check_general_type_compact(db, source_value, &member_type, check_guard.next_level()?)?;
    }

    Ok(())
}
