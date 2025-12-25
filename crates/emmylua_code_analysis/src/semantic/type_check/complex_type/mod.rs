mod array_type_check;
mod call_type_check;
mod intersection_type_check;
mod object_type_check;
mod table_generic_check;
mod tuple_type_check;

use array_type_check::check_array_type_compact;
use call_type_check::check_call_type_compact;
use intersection_type_check::check_intersection_type_compact;
use object_type_check::check_object_type_compact;
use table_generic_check::check_table_generic_type_compact;
use tuple_type_check::check_tuple_type_compact;

use crate::{
    LuaType, LuaUnionType, TypeSubstitutor,
    semantic::type_check::type_check_context::TypeCheckContext,
};

use super::{
    TypeCheckResult, check_general_type_compact, type_check_fail_reason::TypeCheckFailReason,
    type_check_guard::TypeCheckGuard,
};

// all is duck typing
pub fn check_complex_type_compact(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    // TODO: 缓存以提高性能
    // 如果是泛型+不包含模板参数+alias, 那么尝试实例化再检查
    if let LuaType::Generic(generic) = compact_type {
        if !generic.contain_tpl() {
            let base_id = generic.get_base_type_id();
            if let Some(decl) = context.db.get_type_index().get_type_decl(&base_id)
                && decl.is_alias()
            {
                let substitutor =
                    TypeSubstitutor::from_alias(generic.get_params().clone(), base_id.clone());
                if let Some(alias_origin) = decl.get_alias_origin(context.db, Some(&substitutor)) {
                    return check_general_type_compact(
                        context,
                        source,
                        &alias_origin,
                        check_guard.next_level()?,
                    );
                }
            }
        }
    }

    match source {
        LuaType::Array(source_array_type) => {
            match check_array_type_compact(
                context,
                source_array_type.get_base(),
                compact_type,
                check_guard,
            ) {
                Err(TypeCheckFailReason::DonotCheck) => {}
                result => return result,
            }
        }
        LuaType::Tuple(tuple) => {
            match check_tuple_type_compact(context, tuple, compact_type, check_guard) {
                Err(TypeCheckFailReason::DonotCheck) => {}
                result => return result,
            }
        }
        LuaType::Object(source_object) => {
            match check_object_type_compact(context, source_object, compact_type, check_guard) {
                Err(TypeCheckFailReason::DonotCheck) => {}
                result => return result,
            }
        }
        LuaType::TableGeneric(source_generic_param) => {
            match check_table_generic_type_compact(
                context,
                source_generic_param,
                compact_type,
                check_guard,
            ) {
                Err(TypeCheckFailReason::DonotCheck) => {}
                result => return result,
            }
        }
        LuaType::Intersection(source_intersection) => {
            match check_intersection_type_compact(
                context,
                source_intersection,
                compact_type,
                check_guard,
            ) {
                Err(TypeCheckFailReason::DonotCheck) => {}
                result => return result,
            }
        }
        LuaType::Union(union_type) => {
            if let LuaType::Union(compact_union) = compact_type {
                return check_union_type_compact_union(
                    context,
                    source,
                    compact_union,
                    check_guard.next_level()?,
                );
            }
            for sub_type in union_type.into_vec() {
                match check_general_type_compact(
                    context,
                    &sub_type,
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
        LuaType::Generic(_) => {
            return Ok(());
        }
        LuaType::Call(alias_call) => {
            return check_call_type_compact(context, alias_call, compact_type, check_guard);
        }
        LuaType::MultiLineUnion(multi_union) => {
            let union = multi_union.to_union();
            return check_complex_type_compact(
                context,
                &union,
                compact_type,
                check_guard.next_level()?,
            );
        }
        _ => {}
    }
    // Do I need to check union types?
    if let LuaType::Union(union) = compact_type {
        for sub_compact in union.into_vec() {
            match check_complex_type_compact(
                context,
                source,
                &sub_compact,
                check_guard.next_level()?,
            ) {
                Ok(_) => {}
                Err(e) => return Err(e),
            }
        }

        return Ok(());
    }

    Err(TypeCheckFailReason::TypeNotMatch)
}

// too complex
fn check_union_type_compact_union(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_union: &LuaUnionType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let compact_types = compact_union.into_vec();
    for compact_sub_type in compact_types {
        check_general_type_compact(
            context,
            source,
            &compact_sub_type,
            check_guard.next_level()?,
        )?;
    }

    Ok(())
}
