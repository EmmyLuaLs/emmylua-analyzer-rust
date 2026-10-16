//! Generic type checks: generic parameter matching + base type nominal checks.

use std::rc::Rc;

use crate::{LuaGenericType, LuaType, LuaTypeDeclId};

use super::context::TypeCheckContext;
use super::ref_type;
use super::sub_type::is_sub_type_of;
use super::{
    TypeCheckFailReason, TypeCheckResult, check_all, check_alternatives, check_general_type_compact,
};

pub fn check_generic_type_compact(
    context: &mut TypeCheckContext,
    source_generic: &LuaGenericType,
    compact_type: &LuaType,
) -> TypeCheckResult {
    let is_tpl = source_generic.contain_tpl();

    match compact_type {
        LuaType::Generic(compact_generic) => {
            if is_tpl {
                return Ok(());
            }
            let first_result =
                check_generic_type_compact_generic(context, source_generic, compact_generic);
            let Err(failure) = first_result else {
                return Ok(());
            };
            // Generic inheritance: the source generic class inherits from the target generic class (keeping parent generic arguments).
            let inherited = context
                .require_complete(|| {
                    is_generic_sub_of_generic(context, source_generic, compact_generic)
                })
                .and_then(|matches| {
                    if matches {
                        Ok(())
                    } else {
                        Err(TypeCheckFailReason::TypeNotMatch)
                    }
                });
            // Inheritance chain.
            check_alternatives(
                failure,
                std::iter::once(inherited).chain(
                    context
                        .super_types_of(&compact_generic.get_base_type_id())
                        .iter()
                        .map(|super_type| {
                            check_general_type_compact(
                                context,
                                &LuaType::Generic(Rc::new(source_generic.clone())),
                                super_type,
                            )
                        }),
                ),
            )
        }
        LuaType::Ref(ref_id) | LuaType::Def(ref_id) => {
            if is_tpl {
                return Ok(());
            }
            check_generic_type_compact_ref_type(context, source_generic, ref_id)
        }
        LuaType::Table | LuaType::TableConst(_) | LuaType::Object(_) | LuaType::Tuple(_) => {
            // Table structures: M0 checks by base type name (`table`).
            Ok(())
        }
        LuaType::Union(union) => check_all(union.iter().map(|sub| {
            check_general_type_compact(
                context,
                &LuaType::Generic(Rc::new(source_generic.clone())),
                sub,
            )
        })),
        _ => Err(context.mismatch(
            &LuaType::Generic(Rc::new(source_generic.clone())),
            compact_type,
        )),
    }
}

fn is_generic_sub_of_generic(
    context: &TypeCheckContext,
    source_generic: &LuaGenericType,
    target: &LuaGenericType,
) -> bool {
    use std::collections::HashSet;
    let target_base = target.get_base_type_id();
    let mut stack = vec![source_generic.get_base_type_id().clone()];
    let mut visited = HashSet::new();
    visited.insert(source_generic.get_base_type_id().clone());
    while let Some(current) = stack.pop() {
        let Some(def) = context.type_def_of(&current) else {
            continue;
        };
        for super_type in ref_type::full_super_types(context, &def) {
            match super_type {
                LuaType::Generic(super_generic)
                    if super_generic.get_base_type_id() == target_base =>
                {
                    if super_generic.get_params() == target.get_params() {
                        return true;
                    }
                }
                LuaType::Ref(super_id) => {
                    if super_id == target_base && target.get_params().is_empty() {
                        return true;
                    }
                    if visited.insert(super_id.clone()) {
                        stack.push(super_id);
                    }
                }
                _ => {}
            }
        }
    }
    false
}

fn check_generic_type_compact_generic(
    context: &mut TypeCheckContext,
    source_generic: &LuaGenericType,
    compact_generic: &LuaGenericType,
) -> TypeCheckResult {
    let source_base_id = source_generic.get_base_type_id();
    let compact_base_id = compact_generic.get_base_type_id();
    if compact_base_id != source_base_id {
        return Err(context.mismatch(
            &LuaType::Generic(Rc::new(source_generic.clone())),
            &LuaType::Generic(Rc::new(compact_generic.clone())),
        ));
    }
    let source_params = source_generic.get_params();
    let compact_params = compact_generic.get_params();
    if source_params.len() != compact_params.len() {
        return Err(context.mismatch(
            &LuaType::Generic(Rc::new(source_generic.clone())),
            &LuaType::Generic(Rc::new(compact_generic.clone())),
        ));
    }
    check_all(
        source_params
            .iter()
            .zip(compact_params)
            .map(|(source_param, compact_param)| {
                check_general_type_compact(context, source_param, compact_param)
            }),
    )
}

fn check_generic_type_compact_ref_type(
    context: &mut TypeCheckContext,
    source_generic: &LuaGenericType,
    ref_id: &LuaTypeDeclId,
) -> TypeCheckResult {
    let base_id = source_generic.get_base_type_id();
    if &base_id == ref_id
        || is_sub_type_of(context, ref_id, &base_id)
        || is_sub_type_of(context, &base_id, ref_id)
    {
        return Ok(());
    }
    // If a generic parameter is `any`, only match the base type.
    if source_generic.get_params().iter().any(|p| p.is_any()) {
        return check_general_type_compact(
            context,
            &source_generic.get_base_type(),
            &LuaType::Ref(ref_id.clone()),
        );
    }
    Err(context.mismatch(
        &LuaType::Generic(Rc::new(source_generic.clone())),
        &LuaType::Ref(ref_id.clone()),
    ))
}
