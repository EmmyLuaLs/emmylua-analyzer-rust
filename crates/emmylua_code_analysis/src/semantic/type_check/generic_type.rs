use std::{collections::HashMap, sync::Arc};

use crate::{
    LuaGenericType, LuaMemberOwner, LuaType, LuaTypeCache, LuaTypeDeclId, RenderLevel,
    TypeSubstitutor, humanize_type, instantiate_type_generic,
    semantic::{member::find_members, type_check::type_check_context::TypeCheckContext},
};

use super::{
    TypeCheckResult, check_general_type_compact, type_check_fail_reason::TypeCheckFailReason,
    type_check_guard::TypeCheckGuard,
};

pub fn check_generic_type_compact(
    context: &mut TypeCheckContext,
    source_generic: &LuaGenericType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let base_id = source_generic.get_base_type_id();
    if let Some(decl) = context
        .db
        .get_type_index()
        .get_type_decl(&source_generic.get_base_type_id())
        && decl.is_alias()
    {
        let substitutor =
            TypeSubstitutor::from_alias(source_generic.get_params().clone(), base_id.clone());
        if let Some(alias_origin) = decl.get_alias_origin(context.db, Some(&substitutor)) {
            return check_general_type_compact(
                context,
                &alias_origin,
                compact_type,
                check_guard.next_level()?,
            );
        }
    }

    // 不检查尚未实例化的泛型类
    let is_tpl = source_generic.contain_tpl();

    match compact_type {
        LuaType::Generic(compact_generic) => {
            if is_tpl {
                return Ok(());
            }
            let first_result = check_generic_type_compact_generic(
                context,
                source_generic,
                compact_generic,
                check_guard.next_level()?,
            );
            if first_result.is_ok() {
                return Ok(());
            }

            if let Some(supers) = context
                .db
                .get_type_index()
                .get_super_types(&compact_generic.get_base_type_id())
            {
                for mut super_type in supers {
                    if super_type.contain_tpl() {
                        let substitutor =
                            TypeSubstitutor::from_type_array(compact_generic.get_params().clone());
                        super_type =
                            instantiate_type_generic(context.db, &super_type, &substitutor);
                    }

                    let result = check_generic_type_compact(
                        context,
                        source_generic,
                        &super_type,
                        check_guard.next_level()?,
                    );
                    if result.is_ok() {
                        return Ok(());
                    }
                }
            }

            first_result
        }
        LuaType::TableConst(range) => check_generic_type_compact_table(
            context,
            source_generic,
            LuaMemberOwner::Element(range.clone()),
            check_guard.next_level()?,
        ),
        LuaType::Ref(ref_id) | LuaType::Def(ref_id) => {
            if is_tpl {
                return Ok(());
            }
            check_generic_type_compact_ref_type(
                context,
                source_generic,
                ref_id,
                check_guard.next_level()?,
            )
        }
        _ => Err(TypeCheckFailReason::TypeNotMatch),
    }
}

fn check_generic_type_compact_generic(
    context: &mut TypeCheckContext,
    source_generic: &LuaGenericType,
    compact_generic: &LuaGenericType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let source_base_id = source_generic.get_base_type_id();
    let compact_base_id = compact_generic.get_base_type_id();
    if compact_base_id != source_base_id {
        return Err(TypeCheckFailReason::TypeNotMatch);
    }

    let source_params = source_generic.get_params();
    let compact_params = compact_generic.get_params();
    if source_params.len() != compact_params.len() {
        return Err(TypeCheckFailReason::TypeNotMatch);
    }

    let next_guard = check_guard.next_level()?;
    for (source_param, compact_param) in source_params.iter().zip(compact_params.iter()) {
        check_general_type_compact(context, source_param, compact_param, next_guard)?;
    }

    Ok(())
}

fn check_generic_type_compact_table(
    context: &mut TypeCheckContext,
    source_generic: &LuaGenericType,
    table_owner: LuaMemberOwner,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let member_index = context.db.get_member_index();

    // 构建表成员映射
    let table_member_map: HashMap<_, _> = member_index
        .get_members(&table_owner)
        .map(|members| {
            members
                .iter()
                .map(|m| (m.get_key().clone(), m.get_id()))
                .collect()
        })
        .unwrap_or_default();

    // 获取泛型类型的成员, 使用 find_members 来获取包括继承的所有成员
    let source_type = LuaType::Generic(Arc::new(source_generic.clone()));
    let Some(source_type_members) = find_members(context.db, &source_type) else {
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
                let table_member_type = context
                    .db
                    .get_type_index()
                    .get_type_cache(&table_member.get_id().into())
                    .unwrap_or(&LuaTypeCache::InferType(LuaType::Any))
                    .as_type();

                if let Err(err) = check_general_type_compact(
                    context,
                    &source_member_type,
                    table_member_type,
                    next_guard,
                ) && err.is_type_not_match()
                {
                    if !context.detail {
                        return Err(TypeCheckFailReason::TypeNotMatch);
                    }
                    return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                        t!(
                            "member %{name} type not match, expect %{expect}, got %{got}",
                            name = key.to_path(),
                            expect =
                                humanize_type(context.db, &source_member_type, RenderLevel::Simple),
                            got = humanize_type(context.db, table_member_type, RenderLevel::Simple)
                        )
                        .to_string(),
                    ));
                }
            }
            None if !source_member_type.is_optional() => {
                if !context.detail {
                    return Err(TypeCheckFailReason::TypeNotMatch);
                }

                return Err(TypeCheckFailReason::TypeNotMatchWithReason(
                    t!("missing member %{name}, in table", name = key.to_path()).to_string(),
                ));
            }
            _ => {} // 可选成员未找到，继续检查
        }
    }

    // 检查超类型
    let source_base_id = source_generic.get_base_type_id();
    if let Some(supers) = context.db.get_type_index().get_super_types(&source_base_id) {
        let element_range = table_owner
            .get_element_range()
            .ok_or(TypeCheckFailReason::TypeNotMatch)?;
        let table_type = LuaType::TableConst(element_range.clone());

        for super_type in supers {
            check_general_type_compact(context, &super_type, &table_type, next_guard)?;
        }
    }

    Ok(())
}

fn check_generic_type_compact_ref_type(
    context: &mut TypeCheckContext,
    source_generic: &LuaGenericType,
    ref_id: &LuaTypeDeclId,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let type_decl = context
        .db
        .get_type_index()
        .get_type_decl(ref_id)
        .ok_or(TypeCheckFailReason::TypeNotMatch)?;

    if type_decl.is_alias() {
        if let Some(origin_type) = type_decl.get_alias_origin(context.db, None) {
            return check_general_type_compact(
                context,
                &LuaType::Generic(source_generic.clone().into()),
                &origin_type,
                check_guard.next_level()?,
            );
        }
    }

    for super_type in context
        .db
        .get_type_index()
        .get_super_types(ref_id)
        .unwrap_or_default()
    {
        if check_generic_type_compact(
            context,
            source_generic,
            &super_type,
            check_guard.next_level()?,
        )
        .is_ok()
        {
            return Ok(());
        }
    }

    // 如果泛型参数是`any`, 那么我们只需要匹配基础类型
    if source_generic.get_params().iter().any(|p| p.is_any()) {
        return check_general_type_compact(
            context,
            &source_generic.get_base_type(),
            &LuaType::Ref(ref_id.clone()),
            check_guard.next_level()?,
        );
    }

    Err(TypeCheckFailReason::TypeNotMatch)
}
