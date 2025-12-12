use std::sync::Arc;

use crate::{
    LuaAliasCallKind, LuaAliasCallType, LuaMemberKey, LuaType, LuaUnionType, TypeCheckFailReason,
    TypeCheckResult, get_keyof_members,
    semantic::type_check::{
        check_general_type_compact, type_check_context::TypeCheckContext,
        type_check_guard::TypeCheckGuard,
    },
};

pub fn check_call_type_compact(
    context: &mut TypeCheckContext,
    source_call: &LuaAliasCallType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    if let LuaAliasCallKind::KeyOf = source_call.get_call_kind() {
        let source_operands = source_call.get_operands().iter().collect::<Vec<_>>();
        if source_operands.len() != 1 {
            return Err(TypeCheckFailReason::TypeNotMatch);
        }
        match compact_type {
            LuaType::Call(compact_call) => {
                if compact_call.get_call_kind() == LuaAliasCallKind::KeyOf {
                    if compact_call.as_ref() == source_call {
                        return Ok(());
                    }
                    let compact_operands = compact_call.get_operands().iter().collect::<Vec<_>>();
                    if compact_operands.len() != 1 {
                        return Err(TypeCheckFailReason::TypeNotMatch);
                    }

                    let source_key_types = LuaType::Union(Arc::new(LuaUnionType::from_vec(
                        get_keyof_keys(context, &source_operands[0]),
                    )));
                    let compact_key_types = LuaType::Union(Arc::new(LuaUnionType::from_vec(
                        get_keyof_keys(context, &compact_operands[0]),
                    )));
                    return check_general_type_compact(
                        context,
                        &source_key_types,
                        &compact_key_types,
                        check_guard.next_level()?,
                    );
                }
            }
            _ => {
                let key_types = get_keyof_keys(context, &source_operands[0]);
                for key_type in &key_types {
                    match check_general_type_compact(
                        context,
                        &key_type,
                        compact_type,
                        check_guard.next_level()?,
                    ) {
                        Ok(_) => return Ok(()),
                        Err(e) if e.is_type_not_match() => {}
                        Err(e) => return Err(e),
                    }
                }
                return Err(TypeCheckFailReason::TypeNotMatch);
            }
        }
    }

    // TODO: 实现其他 call 类型的检查
    Ok(())
}

fn get_keyof_keys(context: &TypeCheckContext, prefix_type: &LuaType) -> Vec<LuaType> {
    let members = get_keyof_members(context.db, prefix_type).unwrap_or_default();
    let key_types = members
        .iter()
        .filter_map(|m| match &m.key {
            LuaMemberKey::Integer(i) => Some(LuaType::DocIntegerConst(*i)),
            LuaMemberKey::Name(s) => Some(LuaType::DocStringConst(s.clone().into())),
            _ => None,
        })
        .collect::<Vec<_>>();
    key_types
}
