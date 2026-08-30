//! Assignment-specific compatibility checks: assignment semantics not yet covered by the general type_check path.
//!
//! These checks use salsa member/type-definition facilities; the goal is to gradually fold them into the general checks.

use crate::{LuaMemberKey, LuaType, SemanticId, TypeDef, TypeScope};

use super::context::TypeCheckContext;
use super::guard::TypeCheckGuard;
use super::{TypeCheckResult, check_general_type_compact};

pub(crate) fn check_special(
    context: &mut TypeCheckContext,
    source: &LuaType,
    target: &LuaType,
    guard: TypeCheckGuard,
) -> Option<TypeCheckResult> {
    if let Some(result) = check_object_index_against_class(context, source, target, guard) {
        return Some(result);
    }
    if let Some(result) = check_named_intersection(context, source, target, guard) {
        return Some(result);
    }
    if let Some(result) = check_function_assign(context, source, target, guard) {
        return Some(result);
    }
    None
}

/// Check integer index signatures item-by-item for `Object -> Ref/Def`.
fn check_object_index_against_class(
    context: &mut TypeCheckContext,
    source: &LuaType,
    target: &LuaType,
    guard: TypeCheckGuard,
) -> Option<TypeCheckResult> {
    let (LuaType::Object(source_object), LuaType::Ref(_) | LuaType::Def(_)) = (source, target)
    else {
        return None;
    };
    let def = named_def(context.model, target)?;
    let mut index_signatures: Vec<(String, LuaType)> = Vec::new();
    collect_members_with_index_signatures(
        context.model,
        &def,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut index_signatures,
    );
    if index_signatures.is_empty() {
        return None;
    }
    let mut matched = false;
    for (source_key, source_value) in source_object.get_index_access() {
        let Some((_, expected)) = index_signatures.iter().find(|(signature, _)| {
            matches!(
                signature.as_str(),
                "integer" | "number" | "int"
                    if matches!(
                        source_key,
                        LuaType::Integer | LuaType::Number | LuaType::IntegerConst(_)
                    )
            )
        }) else {
            continue;
        };
        matched = true;
        if !matches!(expected, LuaType::Any | LuaType::Unknown) && {
            let next_guard = match guard.next_level() {
                Ok(g) => g,
                Err(err) => return Some(Err(err)),
            };
            check_general_type_compact(context, source_value, expected, next_guard).is_err()
        } {
            return Some(Err(context.mismatch(source, target)));
        }
    }
    if matched { Some(Ok(())) } else { None }
}

/// Check member completeness for `Ref/Def -> Intersection`.
fn check_named_intersection(
    context: &mut TypeCheckContext,
    source: &LuaType,
    target: &LuaType,
    guard: TypeCheckGuard,
) -> Option<TypeCheckResult> {
    let LuaType::Intersection(intersection) = target else {
        return None;
    };
    let source_def = named_def(context.model, source)?;
    let mut source_members: Vec<(LuaMemberKey, LuaType)> = Vec::new();
    collect_named_members(
        context.model,
        &source_def,
        &mut Vec::new(),
        &mut source_members,
    );
    let mut members: Vec<(LuaMemberKey, LuaType)> = Vec::new();
    for component in intersection.get_types() {
        let Some(def) = named_def(context.model, component) else {
            return Some(Err(context.mismatch(source, target)));
        };
        members.clear();
        collect_named_members(context.model, &def, &mut Vec::new(), &mut members);
        for (key, expected) in &members {
            let Some((_, actual)) = source_members.iter().find(|(existing, _)| existing == key)
            else {
                if expected.is_nullable() {
                    continue;
                }
                return Some(Err(context.mismatch(source, target)));
            };
            let res = {
                let next_guard = match guard.next_level() {
                    Ok(g) => g,
                    Err(err) => return Some(Err(err)),
                };
                check_general_type_compact(context, actual, expected, next_guard).is_err()
            };
            if res {
                return Some(Err(context.mismatch(source, target)));
            }
        }
    }
    Some(Ok(()))
}

/// Function assignment: parameter contravariance and return covariance.
fn check_function_assign(
    context: &mut TypeCheckContext,
    source: &LuaType,
    target: &LuaType,
    guard: TypeCheckGuard,
) -> Option<TypeCheckResult> {
    let (LuaType::DocFunction(source_fun), LuaType::DocFunction(target_fun)) = (source, target)
    else {
        return None;
    };
    let source_params = source_fun.get_params();
    let target_params = target_fun.get_params();
    if source_params.len() != target_params.len() {
        return Some(Err(context.mismatch(source, target)));
    }
    for (source_param, target_param) in source_params.iter().zip(target_params.iter()) {
        if let (Some(source_ty), Some(target_ty)) = (&source_param.1, &target_param.1) {
            // Parameter position is contravariant: the target parameter type must be acceptable by the source parameter type (including union component rules).
            let next_guard = match guard.next_level() {
                Ok(g) => g,
                Err(err) => return Some(Err(err)),
            };
            if param_accepts_assign(context, target_ty, source_ty, next_guard).is_err() {
                return Some(Err(context.mismatch(source, target)));
            }
        }
    }
    let next_guard = match guard.next_level() {
        Ok(g) => g,
        Err(err) => return Some(Err(err)),
    };
    let ret_result = check_general_type_compact(
        context,
        source_fun.get_ret(),
        target_fun.get_ret(),
        next_guard,
    );
    Some(ret_result)
}

fn param_accepts_assign(
    context: &mut TypeCheckContext,
    target: &LuaType,
    source: &LuaType,
    guard: TypeCheckGuard,
) -> TypeCheckResult {
    if let LuaType::Union(target_union) = target {
        for component in target_union.into_vec() {
            let next_guard = guard.next_level()?;
            param_accepts_assign(context, &component, source, next_guard)?;
        }
        return Ok(());
    }
    if let LuaType::Union(source_union) = source {
        for component in source_union.into_vec() {
            let next_guard = guard.next_level()?;
            if param_accepts_assign(context, target, &component, next_guard).is_ok() {
                return Ok(());
            }
        }
        return Err(context.mismatch(source, target));
    }
    check_general_type_compact(context, target, source, guard)
}

fn named_def(model: &crate::semantic_model::SemanticModel<'_>, ty: &LuaType) -> Option<TypeDef> {
    let id = match ty {
        LuaType::Ref(id) | LuaType::Def(id) => id,
        LuaType::Generic(generic) => {
            return named_def(model, &LuaType::Ref(generic.get_base_type_id()));
        }
        _ => return None,
    };
    crate::semantic_model::member::type_def_of(model, id)
}

fn collect_named_members(
    model: &crate::semantic_model::SemanticModel<'_>,
    def: &TypeDef,
    visited: &mut Vec<SemanticId>,
    out: &mut Vec<(LuaMemberKey, LuaType)>,
) {
    if visited.contains(&def.id) {
        return;
    }
    visited.push(def.id.clone());
    for member_ref in model.members_of_owner(&def.id) {
        let Some(facts) = model.file_facts_of(member_ref.file_id) else {
            continue;
        };
        let Some(member) = facts.member_by_id(&member_ref.id) else {
            continue;
        };
        let key = member.key.clone();
        if !out.iter().any(|(existing, _)| existing == &key) {
            let ty = model
                .type_of_member(&member_ref.id)
                .unwrap_or(LuaType::Unknown);
            out.push((key, ty));
        }
    }
    for super_name in &def.super_names {
        let super_def = model
            .resolve_type_def_in(def.file_id, super_name.as_str())
            .or_else(|| {
                model
                    .type_defs_in_scope(TypeScope::Global, super_name.as_str())
                    .into_iter()
                    .next()
            });
        if let Some(super_def) = super_def {
            collect_named_members(model, &super_def, visited, out);
        }
    }
}

fn collect_members_with_index_signatures(
    model: &crate::semantic_model::SemanticModel<'_>,
    def: &TypeDef,
    visited: &mut Vec<SemanticId>,
    out: &mut Vec<(String, LuaType)>,
    required_out: &mut Vec<(String, LuaType)>,
    index_signatures: &mut Vec<(String, LuaType)>,
) {
    if visited.contains(&def.id) {
        return;
    }
    visited.push(def.id.clone());
    for member_ref in model.members_of_owner(&def.id) {
        let Some(facts) = model.file_facts_of(member_ref.file_id) else {
            continue;
        };
        let Some(member) = facts.member_by_id(&member_ref.id) else {
            continue;
        };
        let name = member_ref.name.to_string();
        let ty = model
            .type_of_member(&member_ref.id)
            .unwrap_or(LuaType::Unknown);
        if member.is_index_signature {
            if !index_signatures
                .iter()
                .any(|(existing, _)| existing == &name)
            {
                index_signatures.push((name, ty));
            }
        } else {
            if !out.iter().any(|(existing, _)| existing == &name) {
                out.push((name.clone(), ty.clone()));
            }
            if !member.is_nullable && !required_out.iter().any(|(existing, _)| existing == &name) {
                required_out.push((name, ty));
            }
        }
    }
    for super_name in &def.super_names {
        if let Some(super_def) = model
            .type_defs_in_scope(TypeScope::Global, super_name.as_str())
            .into_iter()
            .next()
        {
            collect_members_with_index_signatures(
                model,
                &super_def,
                visited,
                out,
                required_out,
                index_signatures,
            );
        }
    }
}
