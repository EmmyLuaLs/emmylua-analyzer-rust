//! Additional value compatibility checks for named structural types.

use crate::semantic_model::type_check::complex_type::applicable_index_infos;
use crate::{LuaMemberKey, LuaType, SemanticId, TypeDef};

use crate::semantic_model::SemanticModel;

use super::context::TypeCheckContext;
use super::{RequiredChecks, TypeCheckFailReason, check_general_type_compact};

pub(crate) fn check_special(
    context: &mut TypeCheckContext,
    source: &LuaType,
    target: &LuaType,
) -> Result<Option<()>, TypeCheckFailReason> {
    if check_object_index_against_class(context, source, target)?.is_some() {
        return Ok(Some(()));
    }
    if check_named_intersection(context, source, target)?.is_some() {
        return Ok(Some(()));
    }
    Ok(None)
}

/// Check integer index signatures item-by-item for `Object -> Ref/Def`.
fn check_object_index_against_class(
    context: &mut TypeCheckContext,
    source: &LuaType,
    target: &LuaType,
) -> Result<Option<()>, TypeCheckFailReason> {
    let (LuaType::Object(source_object), LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_)) =
        (source, target)
    else {
        return Ok(None);
    };
    let mut matched = false;
    let mut checks = RequiredChecks::default();
    for (source_key, source_value) in source_object.get_index_access() {
        let infos = applicable_index_infos(context, target, source_key)?;
        let Some(info) = infos.first() else {
            continue;
        };
        let expected = &info.value_type;
        matched = true;
        if !matches!(expected, LuaType::Any | LuaType::Unknown) {
            checks
                .require(check_general_type_compact(context, source_value, expected))
                .map_err(|err| {
                    if err.is_type_not_match() {
                        context.mismatch(source, target)
                    } else {
                        err
                    }
                })?;
        }
    }
    checks.finish()?;
    Ok(matched.then_some(()))
}

/// Check member completeness for `Ref/Def -> Intersection`.
fn check_named_intersection(
    context: &mut TypeCheckContext,
    source: &LuaType,
    target: &LuaType,
) -> Result<Option<()>, TypeCheckFailReason> {
    let LuaType::Intersection(intersection) = target else {
        return Ok(None);
    };
    let Some(source_def) = named_def(context.model, source) else {
        return Ok(None);
    };
    let mut source_members: Vec<(LuaMemberKey, LuaType)> = Vec::new();
    context.require_complete(|| {
        collect_named_members(
            context.model,
            &source_def,
            &mut Vec::new(),
            &mut source_members,
        );
    })?;
    let mut members: Vec<(LuaMemberKey, LuaType)> = Vec::new();
    let mut checks = RequiredChecks::default();
    for component in intersection.get_types() {
        let Some(def) = named_def(context.model, component) else {
            return Err(context.mismatch(source, target));
        };
        members.clear();
        if let Err(err) = context.require_complete(|| {
            collect_named_members(context.model, &def, &mut Vec::new(), &mut members);
        }) {
            checks.require(Err(err))?;
            continue;
        }
        for (key, expected) in &members {
            let actual = source_members
                .iter()
                .find(|(existing, _)| existing == key)
                .map_or(&LuaType::Nil, |(_, actual)| actual);
            checks
                .require(check_general_type_compact(context, actual, expected))
                .map_err(|err| {
                    if err.is_type_not_match() {
                        context.mismatch(source, target)
                    } else {
                        err
                    }
                })?;
        }
    }
    checks.finish()?;
    Ok(Some(()))
}

fn named_def(model: &SemanticModel<'_>, ty: &LuaType) -> Option<TypeDef> {
    let id = match ty {
        LuaType::Ref(id) | LuaType::Def(id) => id,
        LuaType::Generic(generic) => {
            return named_def(model, &LuaType::Ref(generic.get_base_type_id()));
        }
        _ => return None,
    };
    model.type_def_of(id)
}

fn collect_named_members(
    model: &SemanticModel<'_>,
    def: &TypeDef,
    visited: &mut Vec<SemanticId>,
    out: &mut Vec<(LuaMemberKey, LuaType)>,
) {
    if visited.contains(&def.id) {
        return;
    }
    visited.push(def.id.clone());
    for member_ref in model.members_of_owner(&def.id).iter() {
        let Some(facts) = model.file_facts_of(member_ref.file_id) else {
            continue;
        };
        let Some(member) = facts.member_by_id(&member_ref.id) else {
            continue;
        };
        if member.index_key_syntax.is_some() {
            continue;
        }
        let key = member.key.clone();
        if !out.iter().any(|(existing, _)| existing == &key) {
            let ty = model
                .type_of_member(&member_ref.id)
                .unwrap_or(LuaType::Unknown);
            out.push((key, ty));
        }
    }
    for super_def in model.base_type_defs(&def.type_decl_id()) {
        collect_named_members(model, &super_def, visited, out);
    }
}
