use std::{collections::HashMap, sync::Arc};

use crate::{
    humanize_type, semantic::member::find_members, DbIndex, LuaGenericType, LuaMemberOwner,
    LuaType, LuaTypeCache, RenderLevel, TypeSubstitutor,
};

use super::{
    check_general_type_compact, check_ref_type_compact,
    type_check_fail_reason::TypeCheckFailReason, type_check_guard::TypeCheckGuard, TypeCheckResult,
};

pub fn check_generic_type_compact(
    db: &DbIndex,
    source_generic: &LuaGenericType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    // 不检查尚未实例化的泛型类
    let is_tpl = source_generic.contain_tpl();

    let source_base_id = source_generic.get_base_type_id();
    let type_decl = db
        .get_type_index()
        .get_type_decl(&source_base_id)
        .ok_or(TypeCheckFailReason::TypeNotMatch)?;

    if type_decl.is_alias() {
        let type_params = source_generic.get_params();
        let substitutor = TypeSubstitutor::from_alias(type_params.clone(), source_base_id);
        if let Some(origin_type) = type_decl.get_alias_origin(db, Some(&substitutor)) {
            return check_general_type_compact(
                db,
                &origin_type,
                compact_type,
                check_guard.next_level()?,
            );
        }
    }

    match compact_type {
        LuaType::Generic(compact_generic) => {
            if is_tpl {
                return Ok(());
            }
            check_generic_type_compact_generic(
                db,
                source_generic,
                compact_generic,
                check_guard.next_level()?,
            )
        }
        LuaType::TableConst(range) => check_generic_type_compact_table(
            db,
            source_generic,
            LuaMemberOwner::Element(range.clone()),
            check_guard.next_level()?,
        ),
        LuaType::Ref(_) | LuaType::Def(_) => {
            if is_tpl {
                return Ok(());
            }
            check_ref_type_compact(
                db,
                &source_generic.get_base_type_id(),
                compact_type,
                check_guard.next_level()?,
            )
        }
        _ => Err(TypeCheckFailReason::TypeNotMatch),
    }
}

fn check_generic_type_compact_generic(
    db: &DbIndex,
    source_generic: &LuaGenericType,
    compact_generic: &LuaGenericType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let source_base_id = source_generic.get_base_type_id();
    let compact_base_id = compact_generic.get_base_type_id();
    if source_base_id != compact_base_id {
        return Err(TypeCheckFailReason::TypeNotMatch);
    }

    let source_params = source_generic.get_params();
    let compact_params = compact_generic.get_params();
    if source_params.len() != compact_params.len() {
        return Err(TypeCheckFailReason::TypeNotMatch);
    }

    let next_guard = check_guard.next_level()?;
    for (source_param, compact_param) in source_params.iter().zip(compact_params.iter()) {
        check_general_type_compact(db, source_param, compact_param, next_guard)?;
    }

    Ok(())
}

fn check_generic_type_compact_table(
    db: &DbIndex,
    source_generic: &LuaGenericType,
    table_owner: LuaMemberOwner,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let member_index = db.get_member_index();

    // 构建表成员映射
    let table_member_map: HashMap<_, _> = member_index
        .get_members(&table_owner)
        .map(|members| {
            members
                .iter()
                .map(|m| (m.get_key().clone(), m.get_id().clone()))
                .collect()
        })
        .unwrap_or_default();

    // 获取泛型类型的成员，使用 find_members 来获取包括继承的所有成员
    let source_type = LuaType::Generic(Arc::new(source_generic.clone()));
    let Some(source_type_members) = find_members(db, &source_type) else {
        return Ok(()); // 空成员无需检查
    };

    // 提前计算下一级检查守卫
    let next_guard = check_guard.next_level()?;

    for source_member in source_type_members {
        let source_member_type = source_member.typ;
        let key = source_member.key;

        match table_member_map.get(&key) {
            Some(table_member_id) => {
                let table_member = member_index
                    .get_member(table_member_id)
                    .ok_or(TypeCheckFailReason::TypeNotMatch)?;
                let table_member_type = db
                    .get_type_index()
                    .get_type_cache(&table_member.get_id().into())
                    .unwrap_or(&LuaTypeCache::InferType(LuaType::Any))
                    .as_type();

                if let Err(TypeCheckFailReason::TypeNotMatch) = check_general_type_compact(
                    db,
                    &source_member_type,
                    &table_member_type,
                    next_guard,
                ) {
                    return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                        t!(
                            "member %{name} type not match, expect %{expect}, got %{got}",
                            name = key.to_path(),
                            expect = humanize_type(db, &source_member_type, RenderLevel::Simple),
                            got = humanize_type(db, &table_member_type, RenderLevel::Simple)
                        )
                        .to_string(),
                    ));
                }
            }
            None if !source_member_type.is_optional() => {
                return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                    t!("missing member %{name}, in table", name = key.to_path()).to_string(),
                ));
            }
            _ => {} // 可选成员未找到，继续检查
        }
    }

    // 检查超类型
    let source_base_id = source_generic.get_base_type_id();
    if let Some(supers) = db.get_type_index().get_super_types(&source_base_id) {
        let element_range = table_owner
            .get_element_range()
            .ok_or(TypeCheckFailReason::TypeNotMatch)?;
        let table_type = LuaType::TableConst(element_range.clone());

        for super_type in supers {
            check_general_type_compact(db, &super_type, &table_type, next_guard)?;
        }
    }

    Ok(())
}
