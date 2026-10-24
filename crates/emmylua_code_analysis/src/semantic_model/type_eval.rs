//! # Semantic layer type evaluation
//!
//! Type-level evaluation lives here: `T[K]`, `keyof T`, mapped type expansion, alias indexing, etc.
//! All consumers (renderer / type_check / member access / pcall) should reuse this module as much as possible,
//! instead of duplicating the same evaluation logic in the display or call layers.

use std::rc::Rc;

use hashbrown::HashMap;
use smol_str::SmolStr;

use crate::{
    GenericParam, GenericTpl, GenericTplId, LuaAliasCallKind, LuaAliasCallType, LuaArrayType,
    LuaConditionalType, LuaFunctionType, LuaGenericType, LuaIntersectionType, LuaMappedType,
    LuaMemberKey, LuaObjectType, LuaTupleStatus, LuaTupleType, LuaType, LuaTypeNode, LuaUnionType,
    TypeDef, TypeDefKind, VariadicType,
};

use super::SemanticModel;
use super::infer::unify;
use super::type_check::{RequiredChecks, TypeCheckFailReason, TypeCheckResult, check_all};

mod alias_expansion;
use alias_expansion::AliasPath;
pub(crate) use alias_expansion::{expand_alias_branches, expand_alias_head, with_expanded_alias};

#[cfg(test)]
mod tests;

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
    let mut fields = HashMap::new();
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
    Some(LuaType::Object(Rc::new(LuaObjectType::new_with_fields(
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
    enum Task {
        Enter(LuaType),
        Leave,
    }
    let mut path = AliasPath::new(model)?;
    let mut pending = vec![Task::Enter(key.clone())];
    let mut keys = Vec::new();
    while let Some(task) = pending.pop() {
        let key = match task {
            Task::Leave => {
                path.leave();
                continue;
            }
            Task::Enter(key) => key,
        };
        match &key {
            LuaType::Generic(_) => {
                let expanded = expand_alias_generic(model, &key);
                if expanded != key && path.enter(&key).is_some() {
                    pending.push(Task::Leave);
                    pending.push(Task::Enter(expanded));
                }
                continue;
            }
            LuaType::Ref(id) | LuaType::Def(id) => {
                let Some(def) = model.type_def_of(id) else {
                    continue;
                };
                if path.enter(&key).is_none() {
                    continue;
                }
                if let Some(target) = model.alias_target(&def) {
                    pending.push(Task::Leave);
                    pending.push(Task::Enter(target));
                } else {
                    path.leave();
                }
                continue;
            }
            LuaType::Union(union) => {
                for component in union.into_vec().into_iter().rev() {
                    pending.push(Task::Enter(component));
                }
                continue;
            }
            _ => {}
        }
        keys.push(key);
    }
    // Key expansion is complete. Base lookup follows an independent alias path.
    let keys = keys.iter().flat_map(|key| match key {
        LuaType::Call(call) if call.get_call_kind() == LuaAliasCallKind::KeyOf => call
            .get_operands()
            .first()
            .map(|base| keyof_keys(model, base))
            .unwrap_or_default(),
        _ => literal_keys(key),
    });
    let mut types = Vec::new();
    for (key, _) in keys {
        let ty = match (base, &key) {
            (LuaType::Tuple(tuple), LuaMemberKey::Integer(i)) => {
                tuple.get_types().get(*i as usize).cloned()
            }
            _ => model.member_type(base, &key),
        };
        if let Some(ty) = ty
            && !types.contains(&ty)
        {
            types.push(ty);
        }
    }
    (!types.is_empty()).then(|| LuaType::from_vec(types))
}

/// Expand `Call(Index)` into field values/unions (semantic layer).
pub fn expand_index_call(model: &SemanticModel, call: &LuaAliasCallType) -> Option<LuaType> {
    if call.get_call_kind() != LuaAliasCallKind::Index {
        return None;
    }
    let [base, key] = call.get_operands().as_slice() else {
        return None;
    };
    eval_index_access(model, base, key)
}

/// Normalize parameter-name references in mapped values (usually `Ref("P")`) into `TplRef(P)`.
fn bind_mapped_param_ref(ty: &LuaType, param_id: GenericTplId, param_name: &str) -> LuaType {
    use LuaType::*;
    match ty {
        Ref(id) | Def(id) if id.get_name() == param_name => TplRef(Rc::new(GenericTpl::new(
            param_id,
            SmolStr::new(param_name),
            None,
            None,
            false,
            None,
        ))),
        Array(_) | Tuple(_) | Union(_) | Object(_) | Variadic(_) | Call(_) | Conditional(_) => {
            map_alias_children(ty, &mut |child| {
                bind_mapped_param_ref(child, param_id, param_name)
            })
        }
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

/// Replace unbound template references in call results with `Unknown`.
/// Nested functions keep their own generic context.
pub fn sanitize_unresolved_generics(
    ty: &LuaType,
    allowed: &std::collections::HashSet<GenericTplId>,
) -> LuaType {
    use LuaType::*;
    match ty {
        TplRef(tpl) if !allowed.contains(&tpl.get_tpl_id()) => Unknown,
        TableGeneric(params) => TableGeneric(Rc::new(
            params
                .iter()
                .map(|ty| sanitize_unresolved_generics(ty, allowed))
                .collect(),
        )),
        Array(_) | Tuple(_) | Union(_) | Object(_) | Variadic(_) | Call(_) => {
            map_alias_children(ty, &mut |child| {
                sanitize_unresolved_generics(child, allowed)
            })
        }
        _ => ty.clone(),
    }
}

/// Clean up call results, also replacing unresolved names inside generic arguments.
/// Bare named return types and nested function scopes remain unchanged.
pub fn sanitize_unresolved_generics_with_model(
    model: &SemanticModel,
    ty: &LuaType,
    allowed: &std::collections::HashSet<GenericTplId>,
) -> LuaType {
    use LuaType::*;
    match ty {
        TplRef(tpl) if !allowed.contains(&tpl.get_tpl_id()) => Unknown,
        Generic(_) => map_alias_children(ty, &mut |child| sanitize_generic_param(model, child)),
        TableGeneric(params) => TableGeneric(Rc::new(
            params
                .iter()
                .map(|ty| sanitize_unresolved_generics_with_model(model, ty, allowed))
                .collect(),
        )),
        Array(_) | Tuple(_) | Union(_) | Object(_) | Variadic(_) | Call(_) => {
            map_alias_children(ty, &mut |child| {
                sanitize_unresolved_generics_with_model(model, child, allowed)
            })
        }
        _ => ty.clone(),
    }
}

/// Replace unresolved named types inside generic arguments with `Unknown`.
fn sanitize_generic_param(model: &SemanticModel, ty: &LuaType) -> LuaType {
    use LuaType::*;
    match ty {
        Ref(id) | Def(id) if model.type_def_of(id).is_none() => Unknown,
        TableGeneric(params) => TableGeneric(Rc::new(
            params
                .iter()
                .map(|ty| sanitize_generic_param(model, ty))
                .collect(),
        )),
        Generic(_) | Array(_) | Tuple(_) | Union(_) | Object(_) | Variadic(_) | Call(_) => {
            map_alias_children(ty, &mut |child| sanitize_generic_param(model, child))
        }
        _ => ty.clone(),
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
    let mut fields = HashMap::new();
    for (key, key_ty) in keys {
        let value = eval_mapped_value(
            model,
            &mapped.value,
            mapped.param.0,
            mapped.param.1.name.as_str(),
            &key_ty,
        );
        // `{ [P in keyof T]?: T[P] }` (`Partial<T>`): every produced field is optional.
        let value = if mapped.is_optional && !value.is_nullable() {
            LuaType::from_vec(vec![value, LuaType::Nil])
        } else {
            value
        };
        fields.insert(key, value);
    }
    Some(LuaType::Object(Rc::new(LuaObjectType::new_with_fields(
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

/// Substitute class generics referenced by name without traversing replacements.
pub fn substitute_named_refs(ty: &LuaType, map: &HashMap<String, LuaType>) -> LuaType {
    if !ty.any_type(|ty| match ty {
        LuaType::Ref(id) | LuaType::Def(id) => map.contains_key(id.get_name()),
        _ => false,
    }) {
        return ty.clone();
    }
    map_type(ty, &mut |ty| match ty {
        LuaType::Ref(id) | LuaType::Def(id) => map.get(id.get_name()).cloned(),
        _ => None,
    })
}

/// Rewrite the existing type tree iteratively, without visiting replacements.
pub(super) fn map_type(
    ty: &LuaType,
    replace: &mut impl FnMut(&LuaType) -> Option<LuaType>,
) -> LuaType {
    enum Task<'a> {
        Enter(&'a LuaType),
        Rebuild(&'a LuaType, usize),
    }
    let mut pending = vec![Task::Enter(ty)];
    let mut values = Vec::new();
    while let Some(task) = pending.pop() {
        match task {
            Task::Enter(ty) => {
                if let Some(result) = replace(ty) {
                    values.push(result);
                    continue;
                }
                let mut children = Vec::new();
                map_alias_children(ty, &mut |child| {
                    children.push(child);
                    child.clone()
                });
                pending.push(Task::Rebuild(ty, children.len()));
                pending.extend(children.into_iter().rev().map(Task::Enter));
            }
            Task::Rebuild(ty, count) => {
                let mut children = values.split_off(values.len() - count).into_iter();
                values.push(map_alias_children(ty, &mut |_| {
                    children.next().expect("each child has a result")
                }));
            }
        }
    }
    values.pop().expect("type traversal must produce a result")
}

/// Normalize existing structures and resolve computations to their outer shape.
/// Fields produced by an alias stay lazy until a consumer reads them.
pub fn expand_alias_generic(model: &SemanticModel, ty: &LuaType) -> LuaType {
    let Some(evaluation) = crate::semantic_model::cache::TypeEvaluationScope::new(&model.cache)
    else {
        return ty.clone();
    };
    let result = map_type(ty, &mut |current| {
        if !evaluation.visit(current) {
            return Some(current.clone());
        }
        match current {
            LuaType::TableGeneric(_) => Some(current.clone()),
            LuaType::Generic(_)
            | LuaType::Call(_)
            | LuaType::Conditional(_)
            | LuaType::Mapped(_)
                if alias_expansion::needs_reduction(model, current) =>
            {
                Some(expand_alias_branches(model, current).unwrap_or_else(|| current.clone()))
            }
            _ => None,
        }
    });
    if model.cache.borrow().evaluation_exhausted {
        ty.clone()
    } else {
        result
    }
}

/// Rebuild the structures used by alias and conditional evaluation.
/// The caller supplies children; this helper does not recurse or evaluate queries.
fn map_alias_children<'a>(
    ty: &'a LuaType,
    map: &mut impl FnMut(&'a LuaType) -> LuaType,
) -> LuaType {
    use LuaType::*;
    match ty {
        TableGeneric(types) => TableGeneric(Rc::new(types.iter().map(&mut *map).collect())),
        Generic(generic) => Generic(Rc::new(LuaGenericType::new(
            generic.get_base_type_id(),
            generic.get_params().iter().map(&mut *map).collect(),
        ))),
        Array(array) => Array(Rc::new(LuaArrayType::from_base_type(map(array.get_base())))),
        Tuple(tuple) => Tuple(Rc::new(LuaTupleType::new(
            tuple.get_types().iter().map(&mut *map).collect(),
            tuple.status,
        ))),
        Union(union) => Union(Rc::new(LuaUnionType::from_vec(
            union.iter().map(&mut *map).collect(),
        ))),
        Intersection(intersection) => Intersection(Rc::new(LuaIntersectionType::new(
            intersection.get_types().iter().map(&mut *map).collect(),
        ))),
        Object(object) => Object(Rc::new(LuaObjectType::new_with_fields(
            object
                .get_fields()
                .iter()
                .map(|(k, v)| (k.clone(), map(v)))
                .collect(),
            object
                .get_index_access()
                .iter()
                .map(|(k, v)| (map(k), map(v)))
                .collect(),
        ))),
        DocFunction(fun) => DocFunction(Rc::new(LuaFunctionType::new(
            fun.get_async_state(),
            fun.is_colon_define(),
            fun.is_variadic(),
            fun.get_params()
                .iter()
                .map(|(name, ty)| (name.clone(), ty.as_ref().map(&mut *map)))
                .collect(),
            map(fun.get_ret()),
            Some(fun.get_generic_params().to_vec()),
        ))),
        Variadic(variadic) => Variadic(Rc::new(match variadic.as_ref() {
            VariadicType::Base(base) => VariadicType::Base(map(base)),
            VariadicType::Repeat(base) => VariadicType::Repeat(map(base)),
            VariadicType::Multi(types) => {
                VariadicType::Multi(types.iter().map(&mut *map).collect())
            }
        })),
        Call(call) => Call(Rc::new(LuaAliasCallType::new(
            call.get_call_kind(),
            call.get_operands().iter().map(&mut *map).collect(),
        ))),
        Mapped(mapped) => Mapped(Rc::new(LuaMappedType::new(
            (
                mapped.param.0,
                GenericParam::new(
                    mapped.param.1.name.clone(),
                    mapped.param.1.constraint.as_ref().map(&mut *map),
                    mapped.param.1.default.as_ref().map(&mut *map),
                    mapped.param.1.is_const,
                    mapped.param.1.attributes.clone(),
                ),
            ),
            map(&mapped.value),
            mapped.is_readonly,
            mapped.is_optional,
        ))),
        Conditional(conditional) => Conditional(Rc::new(LuaConditionalType::new(
            map(conditional.get_checked_type()),
            map(conditional.get_extends_type()),
            map(conditional.get_true_type()),
            map(conditional.get_false_type()),
            conditional.get_infer_params().to_vec(),
            conditional.has_new,
        ))),
        _ => ty.clone(),
    }
}

/// Evaluate conditional types during generic alias instantiation, preserving distributive semantics for naked type parameters.
fn eval_conditional_alias(
    model: &SemanticModel,
    conditional: &LuaConditionalType,
    bindings: &HashMap<GenericTplId, LuaType>,
    name_bindings: &HashMap<String, LuaType>,
    resolved_bound: LuaType,
) -> LuaType {
    let naked_tpl = match conditional.get_checked_type() {
        LuaType::TplRef(tpl) => Some((tpl.get_tpl_id(), tpl.get_name().to_string())),
        LuaType::Ref(id) | LuaType::Def(id) if name_bindings.contains_key(id.get_name()) => {
            Some((GenericTplId::Type(0), id.get_name().to_string()))
        }
        _ => None,
    };
    let evaluate = |bound: LuaType| {
        let mut bindings = bindings.clone();
        let mut names = name_bindings.clone();
        if let Some((id, name)) = &naked_tpl {
            bindings.insert(*id, bound.clone());
            names.insert(name.clone(), bound.clone());
        }
        let raw = LuaType::Conditional(Rc::new(conditional.clone()));
        let substituted = substitute_named_refs(&unify::substitute(&raw, &bindings), &names);
        match substituted {
            LuaType::Conditional(conditional) => {
                let conditional = LuaConditionalType::new(
                    bound,
                    conditional.get_extends_type().clone(),
                    conditional.get_true_type().clone(),
                    conditional.get_false_type().clone(),
                    conditional.get_infer_params().to_vec(),
                    conditional.has_new,
                );
                eval_conditional_once(model, &conditional)
            }
            other => other,
        }
    };
    // Only a naked parameter distributes bindings into each union branch.
    if naked_tpl.is_some()
        && let LuaType::Union(union) = &resolved_bound
    {
        let mut results = Vec::new();
        for member in union.into_vec() {
            let result = evaluate(member);
            if !result.is_never() && !results.contains(&result) {
                results.push(result);
            }
        }
        LuaType::from_vec(results)
    } else {
        evaluate(resolved_bound)
    }
}

/// Evaluate conditional types (internal `infer` pattern matching). Recursively handle conditional types in structures.
pub fn eval_conditionals(model: &SemanticModel, ty: &LuaType) -> LuaType {
    let Some(evaluation) = crate::semantic_model::cache::TypeEvaluationScope::new(&model.cache)
    else {
        return ty.clone();
    };
    if !evaluation.visit(ty) {
        return ty.clone();
    }
    eval_conditionals_inner(model, ty)
}

fn eval_conditionals_inner(model: &SemanticModel, ty: &LuaType) -> LuaType {
    if !ty.any_type(|ty| matches!(ty, LuaType::Conditional(_))) {
        return ty.clone();
    }
    map_type(ty, &mut |ty| match ty {
        LuaType::Conditional(conditional) => Some(eval_conditional_once(model, conditional)),
        LuaType::Mapped(_) | LuaType::TableGeneric(_) => Some(ty.clone()),
        _ => None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConditionalCheck {
    True,
    False,
    Both,
    Unresolved,
}

fn eval_conditional_once(model: &SemanticModel, conditional: &LuaConditionalType) -> LuaType {
    let checked = conditional.get_checked_type();
    let extends = conditional.get_extends_type();
    // Do not decide before function-level generics are substituted: `T extends (fun(...: infer P))` should not
    // take the false branch while T is still an unbound TplRef. Evaluate after call-site/diagnostic bindings are substituted.
    if contains_unbound_function_tpl(checked) || contains_unbound_function_tpl(extends) {
        return LuaType::Conditional(Rc::new(conditional.clone()));
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
            let member_result = eval_conditional_once(model, &member_conditional);
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
        let revision = model.cache.borrow().revision();
        // `new (fun(...: infer P): any)`: when the source is a constructor type/class table, extract its constructor signature first.
        let infer_source = if conditional.has_new {
            match try_constructor_function(model, checked) {
                Some(fun) => LuaType::DocFunction(Rc::new(fun)),
                None => checked.clone(),
            }
        } else {
            checked.clone()
        };
        if !model.cache.borrow().is_complete(revision) {
            return LuaType::Conditional(Rc::new(conditional.clone()));
        }
        let mut infer_assignments: HashMap<GenericTplId, InferCandidates> = HashMap::new();
        let matched = collect_infer_assignments(
            model,
            &infer_source,
            extends,
            &mut infer_assignments,
            InferVariance::Covariant,
        );
        if matched.is_ok() {
            let bindings: HashMap<GenericTplId, LuaType> = infer_assignments
                .into_iter()
                .filter_map(|(id, candidates)| candidates.finalize(model).map(|ty| (id, ty)))
                .collect();
            let true_ty = unify::substitute(conditional.get_true_type(), &bindings);
            eval_conditionals_inner(model, &true_ty)
        } else if matched.is_err_and(|err| err.is_type_not_match()) {
            eval_conditionals_inner(model, conditional.get_false_type())
        } else {
            LuaType::Conditional(Rc::new(conditional.clone()))
        }
    } else {
        match check_conditional_extends(model, checked, extends) {
            ConditionalCheck::True => eval_conditionals_inner(model, conditional.get_true_type()),
            ConditionalCheck::False => eval_conditionals_inner(model, conditional.get_false_type()),
            ConditionalCheck::Both => {
                let true_ty = eval_conditionals_inner(model, conditional.get_true_type());
                let false_ty = eval_conditionals_inner(model, conditional.get_false_type());
                LuaType::from_vec(vec![true_ty, false_ty])
            }
            ConditionalCheck::Unresolved => LuaType::Conditional(Rc::new(conditional.clone())),
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
            if result == ConditionalCheck::Unresolved {
                break;
            }
        }
        return result;
    }
    if let LuaType::Union(union) = target {
        let mut unresolved = false;
        for member in union.into_vec() {
            match check_conditional_extends(model, source, &member) {
                ConditionalCheck::True | ConditionalCheck::Both => return ConditionalCheck::True,
                ConditionalCheck::Unresolved => unresolved = true,
                ConditionalCheck::False => {}
            }
        }
        return if unresolved {
            ConditionalCheck::Unresolved
        } else {
            ConditionalCheck::False
        };
    }
    match model.type_check_cached(source, target, true) {
        Some(true) => ConditionalCheck::True,
        Some(false) => ConditionalCheck::False,
        None => ConditionalCheck::Unresolved,
    }
}

fn merge_conditional_check(left: ConditionalCheck, right: ConditionalCheck) -> ConditionalCheck {
    match (left, right) {
        (ConditionalCheck::Unresolved, _) | (_, ConditionalCheck::Unresolved) => {
            ConditionalCheck::Unresolved
        }
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
                        LuaType::Intersection(Rc::new(LuaIntersectionType::new(vec![
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
        LuaType::Intersection(Rc::new(LuaIntersectionType::new(seen)))
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
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        match ty {
            LuaType::TplRef(tpl) if tpl.get_tpl_id().is_conditional_infer() => return true,
            // Mapped patterns are reduced by type comparison. Their nested
            // conditionals do not bind the enclosing condition.
            LuaType::Mapped(_) | LuaType::TypeGuard(_) | LuaType::MultiLineUnion(_) => {}
            _ => ty.push_direct_children(&mut pending),
        }
    }
    false
}

fn contains_unbound_function_tpl(ty: &LuaType) -> bool {
    ty.any_type(|ty| matches!(ty, LuaType::TplRef(tpl) if !tpl.get_tpl_id().is_conditional_infer()))
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
            let decl = facts.decl_by_value_range(table.value)?;
            let def = facts.type_def_by_owner_syntax(decl.owner_syntax?)?;
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
) -> TypeCheckResult {
    use LuaType::*;
    if let TplRef(tpl) = pattern
        && tpl.get_tpl_id().is_conditional_infer()
    {
        assignments
            .entry(tpl.get_tpl_id())
            .or_default()
            .insert(variance, source);
        return Ok(());
    }
    // Preserve direct generic argument inference. Other patterns need the
    // source's outer constructor, including aliases in lazily produced fields.
    let source = if matches!((source, pattern), (Generic(s), Generic(p))
        if s.get_base_type_id() == p.get_base_type_id())
    {
        source.clone()
    } else {
        let revision = model.cache.borrow().revision();
        let source = expand_alias_head(model, source).ok_or(TypeCheckFailReason::Unresolved)?;
        if !model.cache.borrow().is_complete(revision) {
            return Err(TypeCheckFailReason::Unresolved);
        }
        source
    };
    let source = &source;
    match pattern {
        Array(pattern_array) => match source {
            Array(source_array) => collect_infer_assignments(
                model,
                source_array.get_base(),
                pattern_array.get_base(),
                assignments,
                variance,
            ),
            _ => Err(TypeCheckFailReason::TypeNotMatch),
        },
        Tuple(pattern_tuple) => match source {
            Tuple(source_tuple)
                if pattern_tuple.get_types().len() == source_tuple.get_types().len() =>
            {
                check_all(
                    pattern_tuple
                        .get_types()
                        .iter()
                        .zip(source_tuple.get_types())
                        .map(|(p, s)| {
                            collect_infer_assignments(model, s, p, assignments, variance)
                        }),
                )
            }
            _ => Err(TypeCheckFailReason::TypeNotMatch),
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
                            Tuple(Rc::new(LuaTupleType::new(
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
                    _ => Err(TypeCheckFailReason::TypeNotMatch),
                }
            }
            _ => Err(TypeCheckFailReason::TypeNotMatch),
        },
        Generic(pattern_generic) => match source {
            Generic(source_generic) => {
                if pattern_generic.get_base_type_id() != source_generic.get_base_type_id()
                    || pattern_generic.get_params().len() != source_generic.get_params().len()
                {
                    return Err(TypeCheckFailReason::TypeNotMatch);
                }
                check_all(
                    pattern_generic
                        .get_params()
                        .iter()
                        .zip(source_generic.get_params())
                        .map(|(p, s)| {
                            collect_infer_assignments(model, s, p, assignments, variance)
                        }),
                )
            }
            _ => Err(TypeCheckFailReason::TypeNotMatch),
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
            _ => Err(TypeCheckFailReason::TypeNotMatch),
        },
        DocFunction(pattern_fun) => match source {
            DocFunction(source_fun) => {
                collect_infer_from_function(model, source_fun, pattern_fun, assignments, variance)
            }
            _ => Err(TypeCheckFailReason::TypeNotMatch),
        },
        _ => {
            if contains_conditional_infer(pattern) {
                Err(TypeCheckFailReason::TypeNotMatch)
            } else {
                match model.type_check_cached(source, pattern, false) {
                    Some(true) => Ok(()),
                    Some(false) => Err(TypeCheckFailReason::TypeNotMatch),
                    None => Err(TypeCheckFailReason::Unresolved),
                }
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
) -> TypeCheckResult {
    let mut checks = RequiredChecks::default();
    for (key, pattern_field_ty) in pattern_object.get_fields() {
        match source_object.get_fields().get(key) {
            Some(source_field_ty) => checks.require(collect_infer_assignments(
                model,
                source_field_ty,
                pattern_field_ty,
                assignments,
                variance,
            ))?,
            None if contains_conditional_infer(pattern_field_ty) => {
                return Err(TypeCheckFailReason::TypeNotMatch);
            }
            _ => {}
        }
    }
    checks.finish()
}

fn infer_from_class_to_object(
    model: &SemanticModel,
    source: &LuaType,
    pattern_object: &LuaObjectType,
    assignments: &mut HashMap<GenericTplId, InferCandidates>,
    variance: InferVariance,
) -> TypeCheckResult {
    let mut checks = RequiredChecks::default();
    for (key, pattern_field_ty) in pattern_object.get_fields() {
        let revision = model.cache.borrow().revision();
        let source_field_ty = model.member_type(source, key);
        if !model.cache.borrow().is_complete(revision) {
            checks.require(Err(TypeCheckFailReason::Unresolved))?;
            continue;
        }
        if let Some(source_field_ty) = source_field_ty {
            checks.require(collect_infer_assignments(
                model,
                &source_field_ty,
                pattern_field_ty,
                assignments,
                variance,
            ))?;
        } else if contains_conditional_infer(pattern_field_ty) {
            return Err(TypeCheckFailReason::TypeNotMatch);
        }
    }
    checks.finish()
}

fn infer_fixed_param_count(function: &LuaFunctionType) -> usize {
    let params = function.get_params();
    let has_variadic = params
        .last()
        .is_some_and(|(name, ty)| name == "..." || ty.as_ref().is_some_and(|ty| ty.is_variadic()));
    params.len() - usize::from(has_variadic)
}

fn collect_infer_from_function(
    model: &SemanticModel,
    source_fun: &LuaFunctionType,
    pattern_fun: &LuaFunctionType,
    assignments: &mut HashMap<GenericTplId, InferCandidates>,
    variance: InferVariance,
) -> TypeCheckResult {
    let pattern_params = pattern_fun.get_params();
    let source_params = source_fun.get_params();
    let normal_param_len = infer_fixed_param_count(pattern_fun);
    let has_variadic = normal_param_len < pattern_params.len();
    if !has_variadic && source_params.len() > normal_param_len {
        return Err(TypeCheckFailReason::TypeNotMatch);
    }
    let mut checks = RequiredChecks::default();
    for (i, (_, pattern_param)) in pattern_params.iter().take(normal_param_len).enumerate() {
        let source_param = source_params.get(i).and_then(|(_, ty)| ty.as_ref());
        let pattern_ty = pattern_param.as_ref();
        match (source_param, pattern_ty) {
            (Some(source_ty), Some(pattern_ty)) => checks.require(collect_infer_assignments(
                model,
                source_ty,
                pattern_ty,
                assignments,
                variance.flip(),
            ))?,
            (None, Some(pattern_ty)) if contains_conditional_infer(pattern_ty) => {
                return Err(TypeCheckFailReason::TypeNotMatch);
            }
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
                    LuaType::Tuple(Rc::new(LuaTupleType::new(
                        rest_types,
                        LuaTupleStatus::InferResolve,
                    )))
                }
            }
            _ => LuaType::Tuple(Rc::new(LuaTupleType::new(
                rest_types,
                LuaTupleStatus::InferResolve,
            ))),
        };
        checks.require(collect_infer_assignments(
            model,
            &ty,
            pattern_ty,
            assignments,
            variance.flip(),
        ))?;
        return checks.finish();
    }
    let pattern_ret = pattern_fun.get_ret();
    if contains_conditional_infer(pattern_ret) {
        checks.require(collect_infer_assignments(
            model,
            source_fun.get_ret(),
            pattern_ret,
            assignments,
            variance,
        ))?;
    }
    checks.finish()
}
