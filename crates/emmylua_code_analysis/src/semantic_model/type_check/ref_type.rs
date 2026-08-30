//! Ref/Def type checks: nominal + inheritance (`is_sub_type_of` via salsa `super_names`).
//!
//! Parts still missing from salsa fall back: alias origin, enum field unions, call operator.

use crate::{LuaType, LuaTypeDeclId};
use emmylua_parser::{LuaAstNode, LuaDocTagClass};

use super::context::TypeCheckContext;
use super::guard::TypeCheckGuard;
use super::sub_type::{get_base_type_id, is_sub_type_of};
use super::{TypeCheckResult, check_general_type_compact};

/// Generic inheritance matching: whether sub's inheritance chain contains a generic parent matching the target generic instance arguments.
fn is_sub_type_of_generic(
    context: &TypeCheckContext,
    sub: &LuaTypeDeclId,
    target: &crate::LuaGenericType,
) -> bool {
    use std::collections::HashSet;
    let target_base = target.get_base_type_id();
    if sub == &target_base && target.get_params().is_empty() {
        return true;
    }
    let mut stack = vec![sub.clone()];
    let mut visited = HashSet::new();
    visited.insert(sub.clone());
    while let Some(current) = stack.pop() {
        let super_types = if let Some(def) = context.type_def_of(&current) {
            full_super_types(context, &def)
        } else {
            context.super_types_of(&current)
        };
        for super_type in super_types {
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

/// Extract full parent types from `---@class` annotations (preserving generic arguments such as `Holder<string>`).
pub(crate) fn full_super_types(context: &TypeCheckContext, def: &crate::TypeDef) -> Vec<LuaType> {
    let Some(tree) = context.model.syntax_tree_of(def.file_id) else {
        return Vec::new();
    };
    let root = tree.get_red_root();
    let Some(token) = root.token_at_offset(def.name_range.start()).right_biased() else {
        return Vec::new();
    };
    let Some(tag) = token.parent_ancestors().find_map(LuaDocTagClass::cast) else {
        return Vec::new();
    };
    let Some(supers) = tag.get_supers() else {
        return Vec::new();
    };
    supers
        .get_types()
        .filter_map(|super_ty| {
            let ty = context.model.doc_type_lua_in(
                def.file_id,
                super_ty.get_syntax_id(),
                &def.generic_params,
            );
            (!matches!(ty, LuaType::Unknown)).then_some(ty)
        })
        .collect()
}

pub fn check_ref_type_compact(
    context: &mut TypeCheckContext,
    source_id: &LuaTypeDeclId,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    // The type definition must exist.
    let Some(type_def) = context.type_def_of(source_id) else {
        return Err(context.mismatch(&LuaType::Ref(source_id.clone()), compact_type));
    };
    let _ = type_def;

    if context.is_alias(source_id) {
        // Source is an alias: expand to its target type and re-check as the new source (recursive aliases are cycle-protected by the guard).
        if let Some(target) = context.alias_target_of(source_id) {
            return check_general_type_compact(
                context,
                &target,
                compact_type,
                check_guard.next_level()?,
            );
        }
        // The alias's expected type must accept every branch of the actual union.
        if let LuaType::Union(compact_union) = compact_type {
            for compact_sub_type in compact_union.into_vec() {
                check_ref_type_compact(
                    context,
                    source_id,
                    &compact_sub_type,
                    check_guard.next_level()?,
                )?;
            }
            return Ok(());
        }
        // Without an alias origin type, fall back to nominal: same id passes.
        if matches!(compact_type, LuaType::Def(id) | LuaType::Ref(id) if id == source_id) {
            return Ok(());
        }
        return Err(context.mismatch(&LuaType::Ref(source_id.clone()), compact_type));
    }

    if context.is_enum(source_id) {
        return check_ref_enum(context, source_id, compact_type, check_guard);
    }

    check_ref_class(context, source_id, compact_type, check_guard)
}

fn check_ref_enum(
    context: &mut TypeCheckContext,
    source_id: &LuaTypeDeclId,
    compact_type: &LuaType,
    _check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    // Directly match the same type.
    if matches!(compact_type, LuaType::Def(id) | LuaType::Ref(id) if id == source_id) {
        return Ok(());
    }
    // Integer enum: broad Integer participates (salsa has no enum field union, so this is relaxed).
    if matches!(
        compact_type,
        LuaType::Integer | LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_)
    ) {
        return Ok(());
    }
    // An enum's runtime table is still a table, so `pairs(DamageType)` etc. should be accepted.
    if matches!(compact_type, LuaType::Table) {
        return Ok(());
    }
    // Same base type name.
    if let Some(base_id) = get_base_type_id(compact_type)
        && is_sub_type_of(context, source_id, &base_id)
    {
        return Ok(());
    }
    Err(context.mismatch(&LuaType::Ref(source_id.clone()), compact_type))
}

fn check_ref_class(
    context: &mut TypeCheckContext,
    source_id: &LuaTypeDeclId,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    match compact_type {
        LuaType::Def(id) | LuaType::Ref(id) => {
            if source_id == id {
                return Ok(());
            }
            // Check subtype relationship (bidirectional: nominal assignment allows super ↔ sub).
            if is_sub_type_of(context, id, source_id) || is_sub_type_of(context, source_id, id) {
                return Ok(());
            }
            // Extra handling for enum targets (salsa has no field union, so fall back to base type name).
            if context.is_enum(id) {
                if let Some(base_id) = get_base_type_id(&LuaType::Ref(source_id.clone()))
                    && is_sub_type_of(context, id, &base_id)
                {
                    return Ok(());
                }
            }
            Err(context.mismatch(&LuaType::Ref(source_id.clone()), compact_type))
        }
        LuaType::Table => Ok(()),
        LuaType::Union(union_type) => {
            for typ in union_type.into_vec() {
                check_general_type_compact(
                    context,
                    &LuaType::Ref(source_id.clone()),
                    &typ,
                    check_guard.next_level()?,
                )?;
            }
            Ok(())
        }
        LuaType::Generic(generic) => {
            let base_type_id = generic.get_base_type_id();
            if (context.strict_generic && is_sub_type_of_generic(context, source_id, generic))
                || (!context.strict_generic
                    && (source_id == &base_type_id
                        || is_sub_type_of(context, &base_type_id, source_id)
                        || is_sub_type_of(context, source_id, &base_type_id)))
            {
                Ok(())
            } else {
                Err(context.mismatch(&LuaType::Ref(source_id.clone()), compact_type))
            }
        }
        // Table/object/tuple/intersection structures: M0 uses base type name (`table`) plus nominal fallback (salsa member-structure checks are left for later).
        LuaType::TableConst(_)
        | LuaType::Object(_)
        | LuaType::Tuple(_)
        | LuaType::Intersection(_) => {
            if let Some(base_id) = get_base_type_id(compact_type)
                && is_sub_type_of(context, source_id, &base_id)
            {
                return Ok(());
            }
            Err(context.mismatch(&LuaType::Ref(source_id.clone()), compact_type))
        }
        _ => {
            if let Some(base_type_id) = get_base_type_id(compact_type) {
                if source_id == &base_type_id
                    || is_sub_type_of(context, &base_type_id, source_id)
                    || is_sub_type_of(context, source_id, &base_type_id)
                {
                    Ok(())
                } else {
                    Err(context.mismatch(&LuaType::Ref(source_id.clone()), compact_type))
                }
            } else {
                Err(context.mismatch(&LuaType::Ref(source_id.clone()), compact_type))
            }
        }
    }
}
