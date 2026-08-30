//! # Semantic layer type evaluation
//!
//! Type-level evaluation lives here: `T[K]`, `keyof T`, mapped type expansion, alias indexing, etc.
//! All consumers (renderer / type_check / member access / pcall) should reuse this module as much as possible,
//! instead of duplicating the same evaluation logic in the display or call layers.

use std::collections::HashMap;
use std::sync::Arc;

use smol_str::SmolStr;

use crate::{
    GenericParam, GenericTpl, GenericTplId, LuaAliasCallKind, LuaAliasCallType, LuaArrayType,
    LuaConditionalType, LuaFunctionType, LuaGenericType, LuaIntersectionType, LuaMappedType,
    LuaMemberKey, LuaObjectType, LuaTupleStatus, LuaTupleType, LuaType, LuaTypeDeclId,
    LuaUnionType, TypeDef, TypeDefKind, VariadicType,
};

use super::SemanticModel;
use super::infer::unify;

/// Extract keys usable as object fields from literal types (strings/integers, including unions).
pub fn literal_keys(ty: &LuaType) -> Vec<(LuaMemberKey, LuaType)> {
    use LuaType::*;
    match ty {
        StringConst(s) => vec![(LuaMemberKey::Name(SmolStr::new(s.as_str())), ty.clone())],
        DocStringConst(s) => vec![(LuaMemberKey::Name(SmolStr::new(s.as_str())), ty.clone())],
        IntegerConst(i) => vec![(LuaMemberKey::Integer(*i), ty.clone())],
        DocIntegerConst(i) => vec![(LuaMemberKey::Integer(*i), ty.clone())],
        Union(union) => union.into_vec().iter().flat_map(literal_keys).collect(),
        _ => Vec::new(),
    }
}

/// Expand the built-in `Merge<T, U>` alias: right-side fields override left-side fields.
pub fn eval_merge_call(model: &SemanticModel, call: &LuaAliasCallType) -> Option<LuaType> {
    if call.get_call_kind() != LuaAliasCallKind::Merge {
        return None;
    }
    let operands = call.get_operands();
    if operands.len() < 2 {
        return None;
    }
    let mut fields = hashbrown::HashMap::new();
    let mut has_info = false;
    for operand in operands {
        let infos = model.member_infos(operand);
        if infos.is_empty() {
            if !matches!(
                operand,
                LuaType::Object(_)
                    | LuaType::Ref(_)
                    | LuaType::Def(_)
                    | LuaType::Generic(_)
                    | LuaType::TableConst(_)
            ) {
                return None;
            }
        } else {
            has_info = true;
        }
        for info in infos {
            if let LuaMemberKey::Name(_) = &info.key {
                fields.insert(info.key.clone(), info.typ);
            }
        }
    }
    if !has_info {
        return None;
    }
    Some(LuaType::Object(Arc::new(LuaObjectType::new_with_fields(
        fields,
        Vec::new(),
    ))))
}

/// Extract member keys from the source type of `keyof T` (tuples use numeric slots; named types use member tables).
fn keyof_keys(model: &SemanticModel, ty: &LuaType) -> Vec<(LuaMemberKey, LuaType)> {
    use LuaType::*;
    match ty {
        Union(union) => union
            .into_vec()
            .iter()
            .flat_map(|component| keyof_keys(model, component))
            .collect(),
        Tuple(tuple) => tuple
            .get_types()
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let key = LuaMemberKey::Integer(index as i64);
                (key.clone(), member_key_literal(&key))
            })
            .collect(),
        Object(object) => object
            .get_fields()
            .iter()
            .map(|(key, _)| (key.clone(), member_key_literal(key)))
            .chain(object.get_index_access().iter().filter_map(|(key, _)| {
                member_key_from_literal(key).map(|key| (key.clone(), member_key_literal(&key)))
            }))
            .collect(),
        Ref(_) | Def(_) | Generic(_) | TableConst(_) | Array(_) => model
            .member_infos(ty)
            .into_iter()
            .map(|info| {
                let key_ty = member_key_literal(&info.key);
                (info.key, key_ty)
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn member_key_literal(key: &LuaMemberKey) -> LuaType {
    match key {
        LuaMemberKey::Name(name) => LuaType::StringConst(SmolStr::new(name.as_str()).into()),
        LuaMemberKey::Integer(i) => LuaType::IntegerConst(*i),
        _ => LuaType::Unknown,
    }
}

/// Extract a single member key from a literal type.
pub fn member_key_from_literal(ty: &LuaType) -> Option<LuaMemberKey> {
    match ty {
        LuaType::StringConst(s) | LuaType::DocStringConst(s) => {
            Some(LuaMemberKey::Name(SmolStr::new(s.as_str())))
        }
        LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => Some(LuaMemberKey::Integer(*i)),
        _ => None,
    }
}

/// Try to concretely evaluate `T[K]` (K as literal / union / keyof / alias).
pub fn eval_index_access(model: &SemanticModel, base: &LuaType, key: &LuaType) -> Option<LuaType> {
    match key {
        LuaType::Call(call) if call.get_call_kind() == LuaAliasCallKind::KeyOf => {
            let key_base = call.get_operands().first()?;
            let infos = model.member_infos(key_base);
            if infos.is_empty() {
                return None;
            }
            Some(LuaType::from_vec(
                infos.into_iter().map(|info| info.typ).collect(),
            ))
        }
        LuaType::Ref(id) | LuaType::Def(id) => {
            let def = model.type_def_of(id)?;
            let target = model.alias_target(&def)?;
            eval_index_access(model, base, &target)
        }
        LuaType::Union(union) => {
            let mut types = Vec::new();
            for component in union.into_vec() {
                if let Some(ty) = eval_index_access(model, base, &component) {
                    if !types.contains(&ty) {
                        types.push(ty);
                    }
                }
            }
            if types.is_empty() {
                None
            } else {
                Some(LuaType::from_vec(types))
            }
        }
        _ => {
            let member_key = member_key_from_literal(key)?;
            model.member_type(base, &member_key)
        }
    }
}

/// Expand `Call(Index)` into field values/unions (semantic layer).
pub fn expand_index_call(model: &SemanticModel, call: &LuaAliasCallType) -> Option<LuaType> {
    if call.get_call_kind() != LuaAliasCallKind::Index {
        return None;
    }
    let operands = call.get_operands();
    if operands.len() != 2 {
        return None;
    }
    let base = &operands[0];
    let key = &operands[1];
    if let LuaType::Call(key_call) = key
        && key_call.get_call_kind() == LuaAliasCallKind::KeyOf
        && let Some(key_base) = key_call.get_operands().first()
    {
        let infos = model.member_infos(key_base);
        if infos.is_empty() {
            return None;
        }
        return Some(LuaType::from_vec(
            infos.into_iter().map(|info| info.typ).collect(),
        ));
    }
    let keys = literal_keys(key);
    if keys.is_empty() {
        return None;
    }
    let mut types = Vec::new();
    for (_, key_ty) in keys {
        if let Some(member_key) = member_key_from_literal(&key_ty) {
            let ty = match base {
                LuaType::Tuple(tuple) => match &member_key {
                    LuaMemberKey::Integer(i) => tuple.get_types().get(*i as usize).cloned(),
                    _ => None,
                },
                _ => model.member_type(base, &member_key),
            };
            if let Some(ty) = ty
                && !types.contains(&ty)
            {
                types.push(ty);
            }
        }
    }
    if types.is_empty() {
        None
    } else {
        Some(LuaType::from_vec(types))
    }
}

/// Normalize parameter-name references in mapped values (usually `Ref("P")`) into `TplRef(P)`.
fn bind_mapped_param_ref(ty: &LuaType, param_id: GenericTplId, param_name: &str) -> LuaType {
    use LuaType::*;
    match ty {
        Ref(id) | Def(id) if id.get_name() == param_name => TplRef(Arc::new(GenericTpl::new(
            param_id,
            SmolStr::new(param_name),
            None,
            None,
            false,
            None,
        ))),
        Array(array) => Array(Arc::new(LuaArrayType::from_base_type(
            bind_mapped_param_ref(array.get_base(), param_id, param_name),
        ))),
        Tuple(tuple) => Tuple(Arc::new(LuaTupleType::new(
            tuple
                .get_types()
                .iter()
                .map(|t| bind_mapped_param_ref(t, param_id, param_name))
                .collect(),
            tuple.status,
        ))),
        Union(union) => Union(Arc::new(LuaUnionType::from_vec(
            union
                .into_vec()
                .iter()
                .map(|t| bind_mapped_param_ref(t, param_id, param_name))
                .collect(),
        ))),
        Object(object) => Object(Arc::new(LuaObjectType::new_with_fields(
            object
                .get_fields()
                .iter()
                .map(|(k, v)| (k.clone(), bind_mapped_param_ref(v, param_id, param_name)))
                .collect(),
            object
                .get_index_access()
                .iter()
                .map(|(k, v)| {
                    (
                        bind_mapped_param_ref(k, param_id, param_name),
                        bind_mapped_param_ref(v, param_id, param_name),
                    )
                })
                .collect(),
        ))),
        Variadic(variadic) => Variadic(Arc::new(match variadic.as_ref() {
            VariadicType::Base(base) => {
                VariadicType::Base(bind_mapped_param_ref(base, param_id, param_name))
            }
            VariadicType::Multi(types) => VariadicType::Multi(
                types
                    .iter()
                    .map(|t| bind_mapped_param_ref(t, param_id, param_name))
                    .collect(),
            ),
        })),
        Call(call) => Call(Arc::new(LuaAliasCallType::new(
            call.get_call_kind(),
            call.get_operands()
                .iter()
                .map(|t| bind_mapped_param_ref(t, param_id, param_name))
                .collect(),
        ))),
        Conditional(conditional) => Conditional(Arc::new(LuaConditionalType::new(
            bind_mapped_param_ref(conditional.get_checked_type(), param_id, param_name),
            bind_mapped_param_ref(conditional.get_extends_type(), param_id, param_name),
            bind_mapped_param_ref(conditional.get_true_type(), param_id, param_name),
            bind_mapped_param_ref(conditional.get_false_type(), param_id, param_name),
            conditional.get_infer_params().to_vec(),
            conditional.has_new,
        ))),
        _ => ty.clone(),
    }
}

fn eval_mapped_value(
    model: &SemanticModel,
    mapped_value: &LuaType,
    param_id: GenericTplId,
    param_name: &str,
    key: &LuaType,
) -> LuaType {
    let value = bind_mapped_param_ref(mapped_value, param_id, param_name);
    let mut bindings = HashMap::new();
    bindings.insert(param_id, key.clone());
    let value = unify::substitute(&value, &bindings);
    if let LuaType::Call(call) = &value {
        if let Some(expanded) = expand_index_call(model, call) {
            return expanded;
        }
    }
    value
}

/// Unbound generic parameters in call results must not leak as bare `TplRef`; uniformly degrade them to `Unknown`.
/// This is the semantic rule after type instantiation: generics that cannot be determined at the call site should no longer be exposed.
pub fn sanitize_unresolved_generics(
    ty: &LuaType,
    allowed: &std::collections::HashSet<GenericTplId>,
) -> LuaType {
    use LuaType::*;
    match ty {
        TplRef(tpl) if !allowed.contains(&tpl.get_tpl_id()) => Unknown,
        Array(array) => Array(Arc::new(LuaArrayType::from_base_type(
            sanitize_unresolved_generics(array.get_base(), allowed),
        ))),
        Tuple(tuple) => Tuple(Arc::new(LuaTupleType::new(
            tuple
                .get_types()
                .iter()
                .map(|t| sanitize_unresolved_generics(t, allowed))
                .collect(),
            tuple.status,
        ))),
        Union(union) => Union(Arc::new(LuaUnionType::from_vec(
            union
                .into_vec()
                .iter()
                .map(|t| sanitize_unresolved_generics(t, allowed))
                .collect(),
        ))),
        Object(object) => Object(Arc::new(LuaObjectType::new_with_fields(
            object
                .get_fields()
                .iter()
                .map(|(k, v)| (k.clone(), sanitize_unresolved_generics(v, allowed)))
                .collect(),
            object
                .get_index_access()
                .iter()
                .map(|(k, v)| {
                    (
                        sanitize_unresolved_generics(k, allowed),
                        sanitize_unresolved_generics(v, allowed),
                    )
                })
                .collect(),
        ))),
        TableGeneric(generic) => TableGeneric(Arc::new(
            generic
                .iter()
                .map(|t| sanitize_unresolved_generics(t, allowed))
                .collect(),
        )),
        Variadic(variadic) => Variadic(Arc::new(match variadic.as_ref() {
            VariadicType::Base(base) => {
                VariadicType::Base(sanitize_unresolved_generics(base, allowed))
            }
            VariadicType::Multi(types) => VariadicType::Multi(
                types
                    .iter()
                    .map(|t| sanitize_unresolved_generics(t, allowed))
                    .collect(),
            ),
        })),
        Call(call) => Call(Arc::new(LuaAliasCallType::new(
            call.get_call_kind(),
            call.get_operands()
                .iter()
                .map(|t| sanitize_unresolved_generics(t, allowed))
                .collect(),
        ))),
        // Do not backfill nested function bodies: inner functions may have their own generic context.
        DocFunction(_) => ty.clone(),
        _ => ty.clone(),
    }
}

/// Clean up call results: first perform model-free generic sanitization, then degrade unbound generic parameters that appear as `Ref`/`Def` in "generic parameter positions" (e.g. `T` in `CountObservable<T>` has no matching TypeDef) to `Unknown`. Bare `Ref("A")` in return types, string templates, etc. remain unchanged.
pub fn sanitize_unresolved_generics_with_model(
    model: &SemanticModel,
    ty: &LuaType,
    allowed: &std::collections::HashSet<GenericTplId>,
) -> LuaType {
    use LuaType::*;
    let ty = sanitize_unresolved_generics(ty, allowed);
    match ty {
        Generic(generic) => {
            let params: Vec<LuaType> = generic
                .get_params()
                .iter()
                .map(|param| sanitize_generic_param(model, param))
                .collect();
            Generic(Arc::new(LuaGenericType::new(
                generic.get_base_type_id().clone(),
                params,
            )))
        }
        Array(array) => Array(Arc::new(LuaArrayType::from_base_type(
            sanitize_unresolved_generics_with_model(model, array.get_base(), allowed),
        ))),
        Tuple(tuple) => Tuple(Arc::new(LuaTupleType::new(
            tuple
                .get_types()
                .iter()
                .map(|ty| sanitize_unresolved_generics_with_model(model, ty, allowed))
                .collect(),
            tuple.status,
        ))),
        Union(union) => Union(Arc::new(LuaUnionType::from_vec(
            union
                .into_vec()
                .iter()
                .map(|ty| sanitize_unresolved_generics_with_model(model, ty, allowed))
                .collect(),
        ))),
        Object(object) => Object(Arc::new(LuaObjectType::new_with_fields(
            object
                .get_fields()
                .iter()
                .map(|(key, ty)| {
                    (
                        key.clone(),
                        sanitize_unresolved_generics_with_model(model, ty, allowed),
                    )
                })
                .collect(),
            object
                .get_index_access()
                .iter()
                .map(|(key, ty)| {
                    (
                        sanitize_unresolved_generics_with_model(model, key, allowed),
                        sanitize_unresolved_generics_with_model(model, ty, allowed),
                    )
                })
                .collect(),
        ))),
        TableGeneric(generic) => TableGeneric(Arc::new(
            generic
                .iter()
                .map(|ty| sanitize_unresolved_generics_with_model(model, ty, allowed))
                .collect(),
        )),
        Variadic(variadic) => Variadic(Arc::new(match variadic.as_ref() {
            VariadicType::Base(base) => VariadicType::Base(
                sanitize_unresolved_generics_with_model(model, base, allowed),
            ),
            VariadicType::Multi(types) => VariadicType::Multi(
                types
                    .iter()
                    .map(|ty| sanitize_unresolved_generics_with_model(model, ty, allowed))
                    .collect(),
            ),
        })),
        Call(call) => Call(Arc::new(LuaAliasCallType::new(
            call.get_call_kind(),
            call.get_operands()
                .iter()
                .map(|ty| sanitize_unresolved_generics_with_model(model, ty, allowed))
                .collect(),
        ))),
        other => other,
    }
}

/// Recursively sanitize generic parameter positions: `T` (no matching TypeDef) degrades to `Unknown`.
fn sanitize_generic_param(model: &SemanticModel, ty: &LuaType) -> LuaType {
    use LuaType::*;
    match ty {
        Ref(id) | Def(id) if model.type_def_of(&id).is_none() => Unknown,
        Generic(generic) => Generic(Arc::new(LuaGenericType::new(
            generic.get_base_type_id().clone(),
            generic
                .get_params()
                .iter()
                .map(|param| sanitize_generic_param(model, param))
                .collect(),
        ))),
        Array(array) => Array(Arc::new(LuaArrayType::from_base_type(
            sanitize_generic_param(model, array.get_base()),
        ))),
        Tuple(tuple) => Tuple(Arc::new(LuaTupleType::new(
            tuple
                .get_types()
                .iter()
                .map(|ty| sanitize_generic_param(model, ty))
                .collect(),
            tuple.status,
        ))),
        Union(union) => Union(Arc::new(LuaUnionType::from_vec(
            union
                .into_vec()
                .iter()
                .map(|ty| sanitize_generic_param(model, ty))
                .collect(),
        ))),
        Object(object) => Object(Arc::new(LuaObjectType::new_with_fields(
            object
                .get_fields()
                .iter()
                .map(|(key, ty)| (key.clone(), sanitize_generic_param(model, ty)))
                .collect(),
            object
                .get_index_access()
                .iter()
                .map(|(key, ty)| {
                    (
                        sanitize_generic_param(model, key),
                        sanitize_generic_param(model, ty),
                    )
                })
                .collect(),
        ))),
        TableGeneric(generic) => TableGeneric(Arc::new(
            generic
                .iter()
                .map(|ty| sanitize_generic_param(model, ty))
                .collect(),
        )),
        Variadic(variadic) => Variadic(Arc::new(match variadic.as_ref() {
            VariadicType::Base(base) => VariadicType::Base(sanitize_generic_param(model, base)),
            VariadicType::Multi(types) => VariadicType::Multi(
                types
                    .iter()
                    .map(|ty| sanitize_generic_param(model, ty))
                    .collect(),
            ),
        })),
        Call(call) => Call(Arc::new(LuaAliasCallType::new(
            call.get_call_kind(),
            call.get_operands()
                .iter()
                .map(|ty| sanitize_generic_param(model, ty))
                .collect(),
        ))),
        other => other.clone(),
    }
}

/// Expand a mapped type into `Object` (when keys are literals).
pub fn expand_mapped(model: &SemanticModel, mapped: &LuaMappedType) -> Option<LuaType> {
    let constr = mapped.param.1.constraint.as_ref()?;
    let keys = match constr {
        LuaType::Call(call) if call.get_call_kind() == LuaAliasCallKind::KeyOf => {
            let ty = call.get_operands().first()?;
            keyof_keys(model, ty)
        }
        _ => literal_keys(constr),
    };
    if keys.is_empty() {
        return None;
    }
    let mut fields = hashbrown::HashMap::new();
    for (key, key_ty) in keys {
        let value = eval_mapped_value(
            model,
            &mapped.value,
            mapped.param.0,
            mapped.param.1.name.as_str(),
            &key_ty,
        );
        fields.insert(key, value);
    }
    Some(LuaType::Object(Arc::new(LuaObjectType::new_with_fields(
        fields,
        Vec::new(),
    ))))
}

/// Build a mapping from class generic names to arguments (`T -> string`) from a receiver generic instance (`Box<string>`).
pub fn class_generic_map(model: &SemanticModel, ty: &LuaType) -> Option<HashMap<String, LuaType>> {
    let LuaType::Generic(generic) = ty else {
        return None;
    };
    let def = model.type_def_of(&generic.get_base_type_id())?;
    let params = generic.get_params();
    Some(
        def.generic_params
            .iter()
            .enumerate()
            .filter_map(|(index, param)| {
                params
                    .get(index)
                    .map(|value| (param.name.to_string(), value.clone()))
            })
            .collect(),
    )
}

/// Recursively substitute class generics referenced by name in a type (`Ref("T")` -> concrete argument).
pub fn substitute_named_refs(ty: &LuaType, map: &HashMap<String, LuaType>) -> LuaType {
    use LuaType::*;
    match ty {
        Ref(id) | Def(id) => map
            .get(id.get_name())
            .cloned()
            .unwrap_or_else(|| ty.clone()),
        Array(array) => Array(Arc::new(LuaArrayType::from_base_type(
            substitute_named_refs(array.get_base(), map),
        ))),
        Tuple(tuple) => Tuple(Arc::new(LuaTupleType::new(
            tuple
                .get_types()
                .iter()
                .map(|t| substitute_named_refs(t, map))
                .collect(),
            tuple.status,
        ))),
        DocFunction(fun) => DocFunction(Arc::new(LuaFunctionType::new(
            fun.get_async_state(),
            fun.is_colon_define(),
            fun.is_variadic(),
            fun.get_params()
                .iter()
                .map(|(name, ty)| {
                    (
                        name.clone(),
                        ty.as_ref().map(|t| substitute_named_refs(t, map)),
                    )
                })
                .collect(),
            substitute_named_refs(fun.get_ret(), map),
            Some(fun.get_generic_params().to_vec()),
        ))),
        Object(object) => Object(Arc::new(LuaObjectType::new_with_fields(
            object
                .get_fields()
                .iter()
                .map(|(key, ty)| (key.clone(), substitute_named_refs(ty, map)))
                .collect(),
            object
                .get_index_access()
                .iter()
                .map(|(key, ty)| {
                    (
                        substitute_named_refs(key, map),
                        substitute_named_refs(ty, map),
                    )
                })
                .collect(),
        ))),
        Union(union) => Union(Arc::new(LuaUnionType::from_vec(
            union
                .into_vec()
                .iter()
                .map(|t| substitute_named_refs(t, map))
                .collect(),
        ))),
        Intersection(intersection) => Intersection(Arc::new(LuaIntersectionType::new(
            intersection
                .get_types()
                .iter()
                .map(|t| substitute_named_refs(t, map))
                .collect(),
        ))),
        Generic(generic) => Generic(Arc::new(LuaGenericType::new(
            generic.get_base_type_id(),
            generic
                .get_params()
                .iter()
                .map(|t| substitute_named_refs(t, map))
                .collect(),
        ))),
        TableGeneric(generic) => TableGeneric(Arc::new(
            generic
                .iter()
                .map(|t| substitute_named_refs(t, map))
                .collect(),
        )),
        Variadic(variadic) => Variadic(Arc::new(match variadic.as_ref() {
            VariadicType::Base(base) => VariadicType::Base(substitute_named_refs(base, map)),
            VariadicType::Multi(types) => VariadicType::Multi(
                types
                    .iter()
                    .map(|t| substitute_named_refs(t, map))
                    .collect(),
            ),
        })),
        Call(call) => Call(Arc::new(LuaAliasCallType::new(
            call.get_call_kind(),
            call.get_operands()
                .iter()
                .map(|t| substitute_named_refs(t, map))
                .collect(),
        ))),
        Conditional(conditional) => Conditional(Arc::new(LuaConditionalType::new(
            substitute_named_refs(conditional.get_checked_type(), map),
            substitute_named_refs(conditional.get_extends_type(), map),
            substitute_named_refs(conditional.get_true_type(), map),
            substitute_named_refs(conditional.get_false_type(), map),
            conditional.get_infer_params().to_vec(),
            conditional.has_new,
        ))),
        Mapped(mapped) => Mapped(Arc::new(LuaMappedType::new(
            (
                mapped.param.0,
                GenericParam::new(
                    mapped.param.1.name.clone(),
                    mapped
                        .param
                        .1
                        .constraint
                        .as_ref()
                        .map(|t| substitute_named_refs(t, map)),
                    mapped
                        .param
                        .1
                        .default
                        .as_ref()
                        .map(|t| substitute_named_refs(t, map)),
                    mapped.param.1.is_const,
                    mapped.param.1.attributes.clone(),
                ),
            ),
            substitute_named_refs(&mapped.value, map),
            mapped.is_readonly,
            mapped.is_optional,
        ))),
        _ => ty.clone(),
    }
}

/// Expand alias calls represented as `LuaType::Generic` (`ExtractFoo<T>` -> alias target) and recursively evaluate conditional types.
/// After call-site generic bindings are substituted, calling this avoids leaking return types as `Alias<...>`.
pub fn expand_alias_generic(model: &SemanticModel, ty: &LuaType) -> LuaType {
    let mut visited = Vec::new();
    expand_alias_generic_inner(model, ty, &mut visited)
}

fn expand_alias_generic_inner(
    model: &SemanticModel,
    ty: &LuaType,
    visited: &mut Vec<LuaTypeDeclId>,
) -> LuaType {
    use LuaType::*;
    match ty {
        Generic(generic) => {
            let base_id = generic.get_base_type_id();
            // `std.RawGet<T, K>`: expand via type-level raw get into member types.
            if matches!(base_id.get_name(), "std.RawGet" | "RawGet") {
                let params = generic.get_params().to_vec();
                if params.len() == 2 {
                    let owner = expand_alias_generic_inner(model, &params[0], visited);
                    let key = expand_alias_generic_inner(model, &params[1], visited);
                    if let Some(ty) = eval_index_access(model, &owner, &key) {
                        return ty;
                    }
                }
            }
            // Built-in `Merge<T, U>` exists as `LuaGenericType` (no user alias definition).
            if base_id.get_name() == "Merge" {
                let operands: Vec<LuaType> = generic
                    .get_params()
                    .iter()
                    .map(|param| expand_alias_generic_inner(model, param, visited))
                    .collect();
                if operands.len() >= 2
                    && let Some(merged) = eval_merge_call(
                        model,
                        &LuaAliasCallType::new(LuaAliasCallKind::Merge, operands),
                    )
                {
                    return merged;
                }
            }
            let def = model.type_def_of(&base_id);
            if let Some(def) = &def
                && def.kind == TypeDefKind::Alias
                && !visited.contains(&base_id)
            {
                visited.push(base_id.clone());
                if let Some(target) = model.alias_target(def) {
                    let bindings: HashMap<GenericTplId, LuaType> = generic
                        .get_params()
                        .iter()
                        .enumerate()
                        .map(|(index, param)| (GenericTplId::Type(index as u32), param.clone()))
                        .collect();
                    // When rich projection does not carry the alias generic context, it projects `T` into a named reference;
                    // substitute by name once more here to ensure `checked_type` is also substituted.
                    let name_bindings: HashMap<::std::string::String, LuaType> = def
                        .generic_params
                        .iter()
                        .enumerate()
                        .filter_map(|(index, param)| {
                            generic
                                .get_params()
                                .get(index)
                                .map(|value| (param.name.to_string(), value.clone()))
                        })
                        .collect();
                    if let Conditional(cond) = &target {
                        // When generic arguments are not yet substituted (e.g. `T` in `MockParameters<T>` is still a
                        // class generic), do not expand the conditional alias: evaluating early would treat unbound parameters as
                        // non-matching and incorrectly fold to `false`/`never`. Expand after concrete arguments are substituted.
                        if generic
                            .get_params()
                            .iter()
                            .any(contains_unbound_function_tpl)
                        {
                            visited.pop();
                            let params = generic
                                .get_params()
                                .iter()
                                .map(|param| expand_alias_generic_inner(model, param, visited))
                                .collect();
                            return Generic(Arc::new(LuaGenericType::new(base_id, params)));
                        }
                        let evaluated =
                            eval_conditional_alias(model, cond, &bindings, &name_bindings);
                        let expanded = expand_alias_generic_inner(model, &evaluated, visited);
                        visited.pop();
                        return expanded;
                    }
                    let named = substitute_named_refs(&target, &name_bindings);
                    let substituted = unify::substitute(&named, &bindings);
                    let expanded = expand_alias_generic_inner(model, &substituted, visited);
                    visited.pop();
                    return expanded;
                }
                visited.pop();
            }
            let params = generic
                .get_params()
                .iter()
                .map(|param| expand_alias_generic_inner(model, param, visited))
                .collect();
            Generic(Arc::new(LuaGenericType::new(base_id, params)))
        }
        Array(array) => Array(Arc::new(LuaArrayType::from_base_type(
            expand_alias_generic_inner(model, array.get_base(), visited),
        ))),
        Tuple(tuple) => Tuple(Arc::new(LuaTupleType::new(
            tuple
                .get_types()
                .iter()
                .map(|t| expand_alias_generic_inner(model, t, visited))
                .collect(),
            tuple.status,
        ))),
        Union(union) => Union(Arc::new(LuaUnionType::from_vec(
            union
                .into_vec()
                .iter()
                .map(|t| expand_alias_generic_inner(model, t, visited))
                .collect(),
        ))),
        Intersection(intersection) => Intersection(Arc::new(LuaIntersectionType::new(
            intersection
                .get_types()
                .iter()
                .map(|t| expand_alias_generic_inner(model, t, visited))
                .collect(),
        ))),
        Object(object) => Object(Arc::new(LuaObjectType::new_with_fields(
            object
                .get_fields()
                .iter()
                .map(|(k, v)| (k.clone(), expand_alias_generic_inner(model, v, visited)))
                .collect(),
            object
                .get_index_access()
                .iter()
                .map(|(k, v)| {
                    (
                        expand_alias_generic_inner(model, k, visited),
                        expand_alias_generic_inner(model, v, visited),
                    )
                })
                .collect(),
        ))),
        Variadic(variadic) => Variadic(Arc::new(match variadic.as_ref() {
            VariadicType::Base(base) => {
                VariadicType::Base(expand_alias_generic_inner(model, base, visited))
            }
            VariadicType::Multi(types) => VariadicType::Multi(
                types
                    .iter()
                    .map(|t| expand_alias_generic_inner(model, t, visited))
                    .collect(),
            ),
        })),
        Call(call) => {
            let call = LuaAliasCallType::new(
                call.get_call_kind(),
                call.get_operands()
                    .iter()
                    .map(|t| expand_alias_generic_inner(model, t, visited))
                    .collect(),
            );
            if call.get_call_kind() == LuaAliasCallKind::Merge
                && let Some(merged) = eval_merge_call(model, &call)
            {
                return merged;
            }
            Call(Arc::new(call))
        }
        Mapped(mapped) => {
            if let Some(expanded) = expand_mapped(model, mapped) {
                expand_alias_generic_inner(model, &expanded, visited)
            } else {
                Mapped(Arc::new(LuaMappedType::new(
                    (
                        mapped.param.0,
                        GenericParam::new(
                            mapped.param.1.name.clone(),
                            mapped
                                .param
                                .1
                                .constraint
                                .as_ref()
                                .map(|t| expand_alias_generic_inner(model, t, visited)),
                            mapped
                                .param
                                .1
                                .default
                                .as_ref()
                                .map(|t| expand_alias_generic_inner(model, t, visited)),
                            mapped.param.1.is_const,
                            mapped.param.1.attributes.clone(),
                        ),
                    ),
                    expand_alias_generic_inner(model, &mapped.value, visited),
                    mapped.is_readonly,
                    mapped.is_optional,
                )))
            }
        }
        Conditional(conditional) => eval_conditionals_inner(
            model,
            &Conditional(Arc::new(LuaConditionalType::new(
                expand_alias_generic_inner(model, conditional.get_checked_type(), visited),
                expand_alias_generic_inner(model, conditional.get_extends_type(), visited),
                expand_alias_generic_inner(model, conditional.get_true_type(), visited),
                expand_alias_generic_inner(model, conditional.get_false_type(), visited),
                conditional.get_infer_params().to_vec(),
                conditional.has_new,
            ))),
            &mut Vec::new(),
        ),
        _ => ty.clone(),
    }
}

/// Evaluate conditional types during generic alias instantiation, preserving distributive semantics for naked type parameters.
fn eval_conditional_alias(
    model: &SemanticModel,
    conditional: &LuaConditionalType,
    bindings: &HashMap<GenericTplId, LuaType>,
    name_bindings: &HashMap<String, LuaType>,
) -> LuaType {
    let checked = conditional.get_checked_type();
    let naked_tpl: Option<(GenericTplId, String)> = match checked {
        LuaType::TplRef(tpl) => Some((tpl.get_tpl_id(), tpl.get_name().to_string())),
        LuaType::Ref(id) | LuaType::Def(id) => name_bindings
            .get(id.get_name())
            .map(|_| (GenericTplId::Type(0), id.get_name().to_string())),
        _ => None,
    };
    if let Some((tpl_id, tpl_name)) = naked_tpl {
        let bound = bindings
            .get(&tpl_id)
            .or_else(|| name_bindings.get(&tpl_name))
            .cloned();
        if let Some(bound) = bound {
            // The generic argument itself may be a named alias (`T = Procedure`); expand the alias first,
            // otherwise `T extends (fun(...: infer P))` will use `Ref(Procedure)` as the function source
            // and incorrectly go to the false branch.
            let expanded = match &bound {
                LuaType::Ref(id) | LuaType::Def(id) => model
                    .type_def_of(id)
                    .filter(|def| def.kind == TypeDefKind::Alias)
                    .and_then(|def| model.alias_target(&def))
                    .unwrap_or_else(|| bound.clone()),
                other => other.clone(),
            };
            let bound = expand_alias_generic(model, &expanded);
            if bound.is_never() {
                return LuaType::Never;
            }
            if let LuaType::Union(union) = &bound {
                let mut results = Vec::new();
                for member in union.into_vec() {
                    let mut member_bindings = bindings.clone();
                    member_bindings.insert(tpl_id, member.clone());
                    let mut member_names = name_bindings.clone();
                    member_names.insert(tpl_name.clone(), member.clone());
                    let raw = LuaType::Conditional(Arc::new(conditional.clone()));
                    let substituted = unify::substitute(&raw, &member_bindings);
                    let substituted = substitute_named_refs(&substituted, &member_names);
                    let mut cond_visiting = Vec::new();
                    let result = match substituted {
                        LuaType::Conditional(c) => {
                            eval_conditional_once(model, c.as_ref(), &mut cond_visiting)
                        }
                        other => other,
                    };
                    if !result.is_never() && !results.contains(&result) {
                        results.push(result);
                    }
                }
                return LuaType::from_vec(results);
            }
            // Non-union: write the expanded argument back into bindings, then use the general conditional evaluation.
            let mut resolved_bindings = bindings.clone();
            resolved_bindings.insert(tpl_id, bound.clone());
            let mut resolved_names = name_bindings.clone();
            resolved_names.insert(tpl_name.clone(), bound.clone());
            let raw = LuaType::Conditional(Arc::new(conditional.clone()));
            let substituted = unify::substitute(&raw, &resolved_bindings);
            let substituted = substitute_named_refs(&substituted, &resolved_names);
            let mut cond_visiting = Vec::new();
            return match substituted {
                LuaType::Conditional(c) => {
                    eval_conditional_once(model, c.as_ref(), &mut cond_visiting)
                }
                other => other,
            };
        }
    }

    let raw = LuaType::Conditional(Arc::new(conditional.clone()));
    let substituted = unify::substitute(&raw, bindings);
    let substituted = substitute_named_refs(&substituted, name_bindings);
    let mut cond_visiting = Vec::new();
    match substituted {
        LuaType::Conditional(c) => eval_conditional_once(model, c.as_ref(), &mut cond_visiting),
        other => other,
    }
}

/// Evaluate conditional types (internal `infer` pattern matching). Recursively handle conditional types in structures.
pub fn eval_conditionals(model: &SemanticModel, ty: &LuaType) -> LuaType {
    eval_conditionals_inner(model, ty, &mut Vec::new())
}

fn eval_conditionals_inner(
    model: &SemanticModel,
    ty: &LuaType,
    visiting: &mut Vec<GenericTplId>,
) -> LuaType {
    use LuaType::*;
    match ty {
        Conditional(conditional) => eval_conditional_once(model, conditional, visiting),
        Array(array) => Array(Arc::new(LuaArrayType::from_base_type(
            eval_conditionals_inner(model, array.get_base(), visiting),
        ))),
        Tuple(tuple) => Tuple(Arc::new(LuaTupleType::new(
            tuple
                .get_types()
                .iter()
                .map(|t| eval_conditionals_inner(model, t, visiting))
                .collect(),
            tuple.status,
        ))),
        Union(union) => Union(Arc::new(LuaUnionType::from_vec(
            union
                .into_vec()
                .iter()
                .map(|t| eval_conditionals_inner(model, t, visiting))
                .collect(),
        ))),
        Intersection(intersection) => Intersection(Arc::new(LuaIntersectionType::new(
            intersection
                .get_types()
                .iter()
                .map(|t| eval_conditionals_inner(model, t, visiting))
                .collect(),
        ))),
        Object(object) => Object(Arc::new(LuaObjectType::new_with_fields(
            object
                .get_fields()
                .iter()
                .map(|(k, v)| (k.clone(), eval_conditionals_inner(model, v, visiting)))
                .collect(),
            object
                .get_index_access()
                .iter()
                .map(|(k, v)| {
                    (
                        eval_conditionals_inner(model, k, visiting),
                        eval_conditionals_inner(model, v, visiting),
                    )
                })
                .collect(),
        ))),
        Generic(generic) => Generic(Arc::new(LuaGenericType::new(
            generic.get_base_type_id(),
            generic
                .get_params()
                .iter()
                .map(|t| eval_conditionals_inner(model, t, visiting))
                .collect(),
        ))),
        Variadic(variadic) => Variadic(Arc::new(match variadic.as_ref() {
            VariadicType::Base(base) => {
                VariadicType::Base(eval_conditionals_inner(model, base, visiting))
            }
            VariadicType::Multi(types) => VariadicType::Multi(
                types
                    .iter()
                    .map(|t| eval_conditionals_inner(model, t, visiting))
                    .collect(),
            ),
        })),
        Call(call) => Call(Arc::new(LuaAliasCallType::new(
            call.get_call_kind(),
            call.get_operands()
                .iter()
                .map(|t| eval_conditionals_inner(model, t, visiting))
                .collect(),
        ))),
        _ => ty.clone(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConditionalCheck {
    True,
    False,
    Both,
}

fn eval_conditional_once(
    model: &SemanticModel,
    conditional: &LuaConditionalType,
    visiting: &mut Vec<GenericTplId>,
) -> LuaType {
    let checked = conditional.get_checked_type();
    let extends = conditional.get_extends_type();
    // Do not decide before function-level generics are substituted: `T extends (fun(...: infer P))` should not
    // take the false branch while T is still an unbound TplRef. Evaluate after call-site/diagnostic bindings are substituted.
    if contains_unbound_function_tpl(checked) || contains_unbound_function_tpl(extends) {
        return LuaType::Conditional(Arc::new(conditional.clone()));
    }

    // Distributive conditional types: when a naked type parameter is instantiated as a union, evaluate each union member separately.
    if let LuaType::Union(union) = checked {
        let mut results = Vec::new();
        for member in union.into_vec() {
            let member_conditional = LuaConditionalType::new(
                member.clone(),
                extends.clone(),
                conditional.get_true_type().clone(),
                conditional.get_false_type().clone(),
                conditional.get_infer_params().to_vec(),
                conditional.has_new,
            );
            let member_result = eval_conditional_once(model, &member_conditional, visiting);
            if !member_result.is_never() && !results.contains(&member_result) {
                results.push(member_result);
            }
        }
        return LuaType::from_vec(results);
    }

    if checked.is_never() {
        return LuaType::Never;
    }

    if contains_conditional_infer(extends) {
        // `new (fun(...: infer P): any)`: when the source is a constructor type/class table, extract its constructor signature first.
        let infer_source = if conditional.has_new {
            match try_constructor_function(model, checked) {
                Some(fun) => LuaType::DocFunction(Arc::new(fun)),
                None => checked.clone(),
            }
        } else {
            checked.clone()
        };
        // `T[K] extends Wrapper<infer U>`: first evaluate the index access into tuple/object members, then collect infer.
        let infer_source = if let LuaType::Call(call) = &infer_source
            && let Some(expanded) = expand_index_call(model, call)
        {
            expanded
        } else {
            infer_source
        };
        let mut infer_assignments: HashMap<GenericTplId, InferCandidates> = HashMap::new();
        if collect_infer_assignments(
            model,
            &infer_source,
            extends,
            &mut infer_assignments,
            InferVariance::Covariant,
        ) {
            let bindings: HashMap<GenericTplId, LuaType> = infer_assignments
                .into_iter()
                .filter_map(|(id, candidates)| candidates.finalize(model).map(|ty| (id, ty)))
                .collect();
            let true_ty = unify::substitute(conditional.get_true_type(), &bindings);
            eval_conditionals_inner(model, &true_ty, visiting)
        } else {
            eval_conditionals_inner(model, conditional.get_false_type(), visiting)
        }
    } else {
        match check_conditional_extends(model, checked, extends) {
            ConditionalCheck::True => {
                eval_conditionals_inner(model, conditional.get_true_type(), visiting)
            }
            ConditionalCheck::False => {
                eval_conditionals_inner(model, conditional.get_false_type(), visiting)
            }
            ConditionalCheck::Both => {
                let true_ty = eval_conditionals_inner(model, conditional.get_true_type(), visiting);
                let false_ty =
                    eval_conditionals_inner(model, conditional.get_false_type(), visiting);
                LuaType::from_vec(vec![true_ty, false_ty])
            }
        }
    }
}

fn check_conditional_extends(
    model: &SemanticModel,
    source: &LuaType,
    target: &LuaType,
) -> ConditionalCheck {
    // Conditional type comparison must preserve literal identity: `"a" extends "b"` is false.
    match (source, target) {
        (LuaType::StringConst(a), LuaType::StringConst(b))
        | (LuaType::DocStringConst(a), LuaType::DocStringConst(b))
        | (LuaType::StringConst(a), LuaType::DocStringConst(b))
        | (LuaType::DocStringConst(a), LuaType::StringConst(b)) => {
            return if a == b {
                ConditionalCheck::True
            } else {
                ConditionalCheck::False
            };
        }
        (LuaType::IntegerConst(a), LuaType::IntegerConst(b))
        | (LuaType::DocIntegerConst(a), LuaType::DocIntegerConst(b))
        | (LuaType::IntegerConst(a), LuaType::DocIntegerConst(b))
        | (LuaType::DocIntegerConst(a), LuaType::IntegerConst(b)) => {
            return if a == b {
                ConditionalCheck::True
            } else {
                ConditionalCheck::False
            };
        }
        (LuaType::BooleanConst(a), LuaType::BooleanConst(b))
        | (LuaType::DocBooleanConst(a), LuaType::DocBooleanConst(b))
        | (LuaType::BooleanConst(a), LuaType::DocBooleanConst(b))
        | (LuaType::DocBooleanConst(a), LuaType::BooleanConst(b)) => {
            return if a == b {
                ConditionalCheck::True
            } else {
                ConditionalCheck::False
            };
        }
        _ => {}
    }
    if source.is_any() {
        return ConditionalCheck::Both;
    }
    if target.is_any() || matches!(target, LuaType::Unknown) {
        return ConditionalCheck::True;
    }
    if source.is_unknown() {
        return ConditionalCheck::False;
    }
    if source.is_never() {
        return ConditionalCheck::True;
    }
    if let LuaType::Union(union) = source {
        let mut result = ConditionalCheck::False;
        for member in union.into_vec() {
            result =
                merge_conditional_check(result, check_conditional_extends(model, &member, target));
            if result == ConditionalCheck::Both {
                break;
            }
        }
        return result;
    }
    if let LuaType::Union(union) = target {
        for member in union.into_vec() {
            if matches!(
                check_conditional_extends(model, source, &member),
                ConditionalCheck::True | ConditionalCheck::Both
            ) {
                return ConditionalCheck::True;
            }
        }
        return ConditionalCheck::False;
    }
    if model.type_check(source, target) {
        ConditionalCheck::True
    } else {
        ConditionalCheck::False
    }
}

fn merge_conditional_check(left: ConditionalCheck, right: ConditionalCheck) -> ConditionalCheck {
    match (left, right) {
        (ConditionalCheck::True, ConditionalCheck::True) => ConditionalCheck::True,
        (ConditionalCheck::False, ConditionalCheck::False) => ConditionalCheck::False,
        _ => ConditionalCheck::Both,
    }
}

#[derive(Debug, Clone, Copy)]
enum InferVariance {
    Covariant,
    Contravariant,
}

impl InferVariance {
    fn flip(self) -> Self {
        match self {
            InferVariance::Covariant => InferVariance::Contravariant,
            InferVariance::Contravariant => InferVariance::Covariant,
        }
    }
}

#[derive(Debug, Default)]
struct InferCandidates {
    covariant: Option<LuaType>,
    contravariant: Option<LuaType>,
}

impl InferCandidates {
    fn insert(&mut self, variance: InferVariance, ty: &LuaType) {
        match variance {
            InferVariance::Covariant => {
                self.covariant = Some(match &self.covariant {
                    Some(existing) => LuaType::from_vec(vec![existing.clone(), ty.clone()]),
                    None => ty.clone(),
                });
            }
            InferVariance::Contravariant => {
                self.contravariant = Some(match &self.contravariant {
                    Some(existing) => {
                        LuaType::Intersection(Arc::new(LuaIntersectionType::new(vec![
                            existing.clone(),
                            ty.clone(),
                        ])))
                    }
                    None => ty.clone(),
                });
            }
        }
    }

    fn finalize(self, model: &SemanticModel) -> Option<LuaType> {
        if let Some(covariant) = self.covariant {
            return Some(covariant);
        }
        self.contravariant
            .as_ref()
            .map(|ty| simplify_infer_intersection(model, ty))
    }
}

/// Contravariant candidate intersections need simplification: mutually exclusive base types (`string & number`) have no value in the type system,
/// so they should reduce to `never` instead of keeping an always-unsatisfiable intersection.
fn simplify_infer_intersection(model: &SemanticModel, ty: &LuaType) -> LuaType {
    let LuaType::Intersection(intersection) = ty else {
        return ty.clone();
    };
    let mut components = Vec::new();
    collect_intersection_components(intersection, &mut components);
    let mut seen = Vec::new();
    for component in components {
        if component.is_never() {
            return LuaType::Never;
        }
        if !seen.contains(&component) {
            seen.push(component);
        }
    }
    for (i, left) in seen.iter().enumerate() {
        for right in seen.iter().skip(i + 1) {
            if primitive_intersection_is_empty(model, left, right) {
                return LuaType::Never;
            }
        }
    }
    if seen.len() == 1 {
        seen.pop().expect("len checked")
    } else {
        LuaType::Intersection(Arc::new(LuaIntersectionType::new(seen)))
    }
}

fn collect_intersection_components(intersection: &LuaIntersectionType, out: &mut Vec<LuaType>) {
    for ty in intersection.get_types() {
        if let LuaType::Intersection(inner) = ty {
            collect_intersection_components(inner, out);
        } else {
            out.push(ty.clone());
        }
    }
}

fn primitive_intersection_is_empty(
    _model: &SemanticModel,
    left: &LuaType,
    right: &LuaType,
) -> bool {
    use LuaType::*;
    fn group(ty: &LuaType) -> Option<&'static str> {
        match ty {
            String | StringConst(_) | DocStringConst(_) => Some("string"),
            Number | Integer | IntegerConst(_) | DocIntegerConst(_) | FloatConst(_) => {
                Some("number")
            }
            Boolean | BooleanConst(_) | DocBooleanConst(_) => Some("boolean"),
            Nil => Some("nil"),
            _ => None,
        }
    }
    match (group(left), group(right)) {
        (Some(a), Some(b)) => a != b,
        _ => false,
    }
}

pub fn contains_conditional_infer(ty: &LuaType) -> bool {
    use LuaType::*;
    match ty {
        TplRef(tpl) => tpl.get_tpl_id().is_conditional_infer(),
        Array(array) => contains_conditional_infer(array.get_base()),
        Tuple(tuple) => tuple.get_types().iter().any(contains_conditional_infer),
        Union(union) => union.into_vec().iter().any(contains_conditional_infer),
        Intersection(intersection) => intersection
            .get_types()
            .iter()
            .any(contains_conditional_infer),
        Object(object) => {
            object.get_fields().values().any(contains_conditional_infer)
                || object
                    .get_index_access()
                    .iter()
                    .any(|(k, v)| contains_conditional_infer(k) || contains_conditional_infer(v))
        }
        DocFunction(fun) => {
            fun.get_params()
                .iter()
                .any(|(_, ty)| ty.as_ref().is_some_and(contains_conditional_infer))
                || contains_conditional_infer(fun.get_ret())
        }
        Generic(generic) => generic.get_params().iter().any(contains_conditional_infer),
        TableGeneric(generic) => generic.iter().any(contains_conditional_infer),
        Variadic(variadic) => match variadic.as_ref() {
            VariadicType::Base(base) => contains_conditional_infer(base),
            VariadicType::Multi(types) => types.iter().any(contains_conditional_infer),
        },
        Call(call) => call.get_operands().iter().any(contains_conditional_infer),
        Conditional(conditional) => {
            contains_conditional_infer(conditional.get_checked_type())
                || contains_conditional_infer(conditional.get_extends_type())
                || contains_conditional_infer(conditional.get_true_type())
                || contains_conditional_infer(conditional.get_false_type())
        }
        _ => false,
    }
}

fn contains_unbound_function_tpl(ty: &LuaType) -> bool {
    use LuaType::*;
    match ty {
        TplRef(tpl) => !tpl.get_tpl_id().is_conditional_infer(),
        Array(array) => contains_unbound_function_tpl(array.get_base()),
        Tuple(tuple) => tuple.get_types().iter().any(contains_unbound_function_tpl),
        Union(union) => union.into_vec().iter().any(contains_unbound_function_tpl),
        Intersection(intersection) => intersection
            .get_types()
            .iter()
            .any(contains_unbound_function_tpl),
        Object(object) => {
            object
                .get_fields()
                .values()
                .any(contains_unbound_function_tpl)
                || object.get_index_access().iter().any(|(k, v)| {
                    contains_unbound_function_tpl(k) || contains_unbound_function_tpl(v)
                })
        }
        DocFunction(fun) => {
            fun.get_params()
                .iter()
                .any(|(_, ty)| ty.as_ref().is_some_and(contains_unbound_function_tpl))
                || contains_unbound_function_tpl(fun.get_ret())
        }
        Generic(generic) => generic
            .get_params()
            .iter()
            .any(contains_unbound_function_tpl),
        TableGeneric(generic) => generic.iter().any(contains_unbound_function_tpl),
        Variadic(variadic) => match variadic.as_ref() {
            VariadicType::Base(base) => contains_unbound_function_tpl(base),
            VariadicType::Multi(types) => types.iter().any(contains_unbound_function_tpl),
        },
        Call(call) => call
            .get_operands()
            .iter()
            .any(contains_unbound_function_tpl),
        Conditional(conditional) => {
            contains_unbound_function_tpl(conditional.get_checked_type())
                || contains_unbound_function_tpl(conditional.get_extends_type())
                || contains_unbound_function_tpl(conditional.get_true_type())
                || contains_unbound_function_tpl(conditional.get_false_type())
        }
        Mapped(mapped) => {
            contains_unbound_function_tpl(&mapped.value)
                || mapped
                    .param
                    .1
                    .constraint
                    .as_ref()
                    .is_some_and(contains_unbound_function_tpl)
                || mapped
                    .param
                    .1
                    .default
                    .as_ref()
                    .is_some_and(contains_unbound_function_tpl)
        }
        _ => false,
    }
}

/// `has_new` condition: resolve class tables/named types into constructor signatures (`---@overload fun(...)`).
fn try_constructor_function(model: &SemanticModel, ty: &LuaType) -> Option<LuaFunctionType> {
    match ty {
        LuaType::Ref(id) | LuaType::Def(id) => {
            let def = model.type_def_of(id)?;
            constructor_function_from_def(model, &def)
        }
        LuaType::TableConst(table) => {
            let facts = model.file_facts_of(table.file_id)?;
            let decl = facts.decls.iter().find(|d| {
                d.value_expr_syntax
                    .map(|s| s.get_range())
                    .is_some_and(|range| range == table.value)
            })?;
            let def = facts
                .type_defs
                .iter()
                .find(|def| def.owner_syntax.is_some() && def.owner_syntax == decl.owner_syntax)?;
            constructor_function_from_def(model, def)
        }
        LuaType::StringConst(s) | LuaType::DocStringConst(s) => {
            let def = model.resolve_type_def(s.as_ref())?;
            constructor_function_from_def(model, &def)
        }
        _ => None,
    }
}

fn constructor_function_from_def(model: &SemanticModel, def: &TypeDef) -> Option<LuaFunctionType> {
    for syntax in &def.call_overloads {
        let ty = model.doc_type_lua_in(def.file_id, *syntax, &def.generic_params);
        if let LuaType::DocFunction(fun) = ty {
            return Some(fun.as_ref().clone());
        }
    }
    None
}

fn collect_infer_assignments(
    model: &SemanticModel,
    source: &LuaType,
    pattern: &LuaType,
    assignments: &mut HashMap<GenericTplId, InferCandidates>,
    variance: InferVariance,
) -> bool {
    use LuaType::*;
    if let TplRef(tpl) = pattern
        && tpl.get_tpl_id().is_conditional_infer()
    {
        assignments
            .entry(tpl.get_tpl_id())
            .or_default()
            .insert(variance, source);
        return true;
    }
    match pattern {
        Array(pattern_array) => match source {
            Array(source_array) => collect_infer_assignments(
                model,
                source_array.get_base(),
                pattern_array.get_base(),
                assignments,
                variance,
            ),
            _ => false,
        },
        Tuple(pattern_tuple) => match source {
            Tuple(source_tuple)
                if pattern_tuple.get_types().len() == source_tuple.get_types().len() =>
            {
                for (p, s) in pattern_tuple
                    .get_types()
                    .iter()
                    .zip(source_tuple.get_types().iter())
                {
                    if !collect_infer_assignments(model, s, p, assignments, variance) {
                        return false;
                    }
                }
                true
            }
            _ => false,
        },
        Variadic(pattern_variadic) => match source {
            Variadic(source_variadic) => {
                match (pattern_variadic.as_ref(), source_variadic.as_ref()) {
                    (VariadicType::Base(pattern_base), VariadicType::Base(source_base)) => {
                        collect_infer_assignments(
                            model,
                            source_base,
                            pattern_base,
                            assignments,
                            variance,
                        )
                    }
                    (VariadicType::Base(pattern_base), VariadicType::Multi(source_types)) => {
                        let source = if source_types.len() == 1 {
                            source_types[0].clone()
                        } else {
                            Tuple(Arc::new(LuaTupleType::new(
                                source_types.clone(),
                                LuaTupleStatus::InferResolve,
                            )))
                        };
                        collect_infer_assignments(
                            model,
                            &source,
                            pattern_base,
                            assignments,
                            variance,
                        )
                    }
                    _ => false,
                }
            }
            _ => false,
        },
        Generic(pattern_generic) => match source {
            Generic(source_generic) => {
                if pattern_generic.get_base_type_id() != source_generic.get_base_type_id()
                    || pattern_generic.get_params().len() != source_generic.get_params().len()
                {
                    return false;
                }
                for (p, s) in pattern_generic
                    .get_params()
                    .iter()
                    .zip(source_generic.get_params().iter())
                {
                    if !collect_infer_assignments(model, s, p, assignments, variance) {
                        return false;
                    }
                }
                true
            }
            _ => false,
        },
        Object(pattern_object) => match source {
            Object(source_object) => infer_from_object_to_object(
                model,
                source_object,
                pattern_object,
                assignments,
                variance,
            ),
            Ref(_) | Def(_) | TableConst(_) => {
                infer_from_class_to_object(model, source, pattern_object, assignments, variance)
            }
            _ => false,
        },
        DocFunction(pattern_fun) => match source {
            DocFunction(source_fun) => {
                collect_infer_from_function(model, source_fun, pattern_fun, assignments, variance)
            }
            _ => false,
        },
        _ => {
            if contains_conditional_infer(pattern) {
                false
            } else {
                source == pattern || model.type_check(source, pattern)
            }
        }
    }
}

fn infer_from_object_to_object(
    model: &SemanticModel,
    source_object: &LuaObjectType,
    pattern_object: &LuaObjectType,
    assignments: &mut HashMap<GenericTplId, InferCandidates>,
    variance: InferVariance,
) -> bool {
    for (key, pattern_field_ty) in pattern_object.get_fields() {
        match source_object.get_fields().get(key) {
            Some(source_field_ty) => {
                if !collect_infer_assignments(
                    model,
                    source_field_ty,
                    pattern_field_ty,
                    assignments,
                    variance,
                ) {
                    return false;
                }
            }
            None if contains_conditional_infer(pattern_field_ty) => return false,
            _ => {}
        }
    }
    true
}

fn infer_from_class_to_object(
    model: &SemanticModel,
    source: &LuaType,
    pattern_object: &LuaObjectType,
    assignments: &mut HashMap<GenericTplId, InferCandidates>,
    variance: InferVariance,
) -> bool {
    for (key, pattern_field_ty) in pattern_object.get_fields() {
        if let Some(source_field_ty) = model.member_type(source, key) {
            if !collect_infer_assignments(
                model,
                &source_field_ty,
                pattern_field_ty,
                assignments,
                variance,
            ) {
                return false;
            }
        } else if contains_conditional_infer(pattern_field_ty) {
            return false;
        }
    }
    true
}

fn collect_infer_from_function(
    model: &SemanticModel,
    source_fun: &LuaFunctionType,
    pattern_fun: &LuaFunctionType,
    assignments: &mut HashMap<GenericTplId, InferCandidates>,
    variance: InferVariance,
) -> bool {
    let pattern_params = pattern_fun.get_params();
    let source_params = source_fun.get_params();
    let has_variadic = pattern_params
        .last()
        .is_some_and(|(name, ty)| name == "..." || ty.as_ref().is_some_and(|ty| ty.is_variadic()));
    let normal_param_len = if has_variadic {
        pattern_params.len().saturating_sub(1)
    } else {
        pattern_params.len()
    };
    if !has_variadic && source_params.len() > normal_param_len {
        return false;
    }
    for (i, (_, pattern_param)) in pattern_params.iter().take(normal_param_len).enumerate() {
        let source_param = source_params.get(i).and_then(|(_, ty)| ty.as_ref());
        let pattern_ty = pattern_param.as_ref();
        match (source_param, pattern_ty) {
            (Some(source_ty), Some(pattern_ty)) => {
                if !collect_infer_assignments(
                    model,
                    source_ty,
                    pattern_ty,
                    assignments,
                    variance.flip(),
                ) {
                    return false;
                }
            }
            (Some(_), None) => {}
            (None, Some(pattern_ty)) if contains_conditional_infer(pattern_ty) => return false,
            _ => {}
        }
    }
    if has_variadic
        && let Some((_, variadic_param)) = pattern_params.last()
        && let Some(pattern_ty) = variadic_param
        && contains_conditional_infer(pattern_ty)
    {
        let rest = if normal_param_len < source_params.len() {
            &source_params[normal_param_len..]
        } else {
            &[]
        };
        let rest_types: Vec<LuaType> = rest
            .iter()
            .map(|(_, ty)| ty.clone().unwrap_or(LuaType::Any))
            .collect();
        let ty = match rest_types.len() {
            0 => LuaType::Never,
            1 => {
                if source_fun.is_variadic() {
                    rest_types[0].clone()
                } else {
                    LuaType::Tuple(Arc::new(LuaTupleType::new(
                        rest_types,
                        LuaTupleStatus::InferResolve,
                    )))
                }
            }
            _ => LuaType::Tuple(Arc::new(LuaTupleType::new(
                rest_types,
                LuaTupleStatus::InferResolve,
            ))),
        };
        return collect_infer_assignments(model, &ty, pattern_ty, assignments, variance.flip());
    }
    let pattern_ret = pattern_fun.get_ret();
    if contains_conditional_infer(pattern_ret) {
        collect_infer_assignments(
            model,
            source_fun.get_ret(),
            pattern_ret,
            assignments,
            variance,
        )
    } else {
        true
    }
}
