use std::collections::HashMap;

use crate::{
    humanize_type, DbIndex, LuaMemberKey, LuaMemberOwner, LuaType, LuaTypeDeclId, RenderLevel,
};

use super::{
    check_general_type_compact, is_sub_type_of, type_check_fail_reason::TypeCheckFailReason,
    type_check_guard::TypeCheckGuard, TypeCheckResult,
};

pub fn check_ref_type_compact(
    db: &DbIndex,
    source_id: &LuaTypeDeclId,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let type_decl = db
        .get_type_index()
        .get_type_decl(source_id)
        // unreachable!
        .ok_or(TypeCheckFailReason::TypeNotMatch)?;

    if type_decl.is_alias() {
        if let Some(origin_type) = type_decl.get_alias_origin(db, None) {
            return check_general_type_compact(
                db,
                &origin_type,
                compact_type,
                check_guard.next_level()?,
            );
        }
        if let Some(members) = type_decl.get_alias_union_members() {
            for member_id in members {
                let member = db
                    .get_member_index()
                    .get_member(member_id)
                    .ok_or(TypeCheckFailReason::TypeNotMatch)?;
                let alias_member_type = member.get_decl_type();
                if check_general_type_compact(
                    db,
                    alias_member_type,
                    compact_type,
                    check_guard.next_level()?,
                )
                .is_ok()
                {
                    return Ok(());
                }
            }
        }
    } else if type_decl.is_enum() {
        let enum_member_owner = LuaMemberOwner::Type(source_id.clone());

        let member_map = db
            .get_member_index()
            .get_member_map(enum_member_owner)
            .ok_or(TypeCheckFailReason::TypeNotMatch)?;

        if type_decl.is_enum_key() {
            for member_key in member_map.keys() {
                let fake_type = match member_key {
                    LuaMemberKey::Name(name) => LuaType::DocStringConst(name.clone().into()),
                    LuaMemberKey::Integer(i) => LuaType::IntegerConst(i.clone()),
                    LuaMemberKey::None => continue,
                };

                if check_general_type_compact(
                    db,
                    &fake_type,
                    compact_type,
                    check_guard.next_level()?,
                )
                .is_ok()
                {
                    return Ok(());
                }
            }
        } else {
            for member_one in member_map.values() {
                let member_id = member_one.get_member_id();
                let member = db
                    .get_member_index()
                    .get_member(member_id)
                    .ok_or(TypeCheckFailReason::TypeNotMatch)?;
                let member_type = member.get_decl_type();
                let member_fake_type = match member_type {
                    LuaType::StringConst(s) => &LuaType::DocStringConst(s.clone().into()),
                    LuaType::IntegerConst(i) => &LuaType::DocIntegerConst(i.clone()),
                    _ => member_type,
                };

                if check_general_type_compact(
                    db,
                    member_fake_type,
                    compact_type,
                    check_guard.next_level()?,
                )
                .is_ok()
                {
                    return Ok(());
                }
            }
        }
    } else {
        // check same id
        let compact_id = match compact_type {
            LuaType::Def(compact_id) => compact_id,
            LuaType::Ref(compact_id) => compact_id,
            LuaType::TableConst(range) => {
                let table_member_owner = LuaMemberOwner::Element(range.clone());
                return check_ref_type_compact_table(
                    db,
                    source_id,
                    table_member_owner,
                    check_guard.next_level()?,
                );
            }
            _ => return Err(TypeCheckFailReason::TypeNotMatch),
        };

        if source_id == compact_id {
            return Ok(());
        }

        if is_sub_type_of(db, compact_id, source_id) {
            return Ok(());
        }
    }

    Err(TypeCheckFailReason::TypeNotMatch)
}

fn check_ref_type_compact_table(
    db: &DbIndex,
    source_type_id: &LuaTypeDeclId,
    table_owner: LuaMemberOwner,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let member_index = db.get_member_index();
    let table_member_map = match member_index.get_member_map(table_owner.clone()) {
        Some(map) => map,
        None => &HashMap::new(),
    };

    let source_type_owner = LuaMemberOwner::Type(source_type_id.clone());
    let source_type_members = match member_index.get_member_map(source_type_owner) {
        Some(map) => map,
        // empty member donot need check
        None => return Ok(()),
    };

    for (key, type_member_one) in source_type_members {
        let type_member_id = type_member_one.get_member_id();
        let source_type_member = member_index
            .get_member(type_member_id)
            .ok_or(TypeCheckFailReason::TypeNotMatch)?;

        let source_member_type = source_type_member.get_decl_type();

        if let Some(table_member_one) = table_member_map.get(key) {
            let table_member_id = table_member_one.get_member_id();
            let table_member = member_index
                .get_member(table_member_id)
                .ok_or(TypeCheckFailReason::TypeNotMatch)?;
            let table_member_type = table_member.get_decl_type();
            if !check_general_type_compact(
                db,
                source_member_type,
                table_member_type,
                check_guard.next_level()?,
            )
            .is_ok()
            {
                return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                    t!(
                        "member %{name} type not match, expect %{expect}, got %{got}",
                        name = key.to_path(),
                        expect = humanize_type(db, source_member_type, RenderLevel::Simple),
                        got = humanize_type(db, table_member_type, RenderLevel::Simple)
                    )
                    .to_string(),
                ));
            }
        } else if source_member_type.is_optional() {
            continue;
        } else {
            return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                t!("missing member %{name}, in table", name = key.to_path()).to_string(),
            ));
        }
    }

    let supers = db.get_type_index().get_super_types(source_type_id);
    if let Some(supers) = supers {
        let table_type = LuaType::TableConst(
            table_owner
                .get_element_range()
                .ok_or(TypeCheckFailReason::TypeNotMatch)?
                .clone(),
        );
        for super_type in supers {
            let result =
                check_general_type_compact(db, &super_type, &table_type, check_guard.next_level()?);
            if !result.is_ok() {
                return result;
            }
        }
    }

    Ok(())
}
