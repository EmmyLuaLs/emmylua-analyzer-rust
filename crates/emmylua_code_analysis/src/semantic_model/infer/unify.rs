//! Generic unification (unify) and substitution (lightweight version for call inference).
//!
//! Actual args -> formal params: `fun<T>(list: T[], cb: fun(T, number): U): U[]`
//! infers `T = number` from an actual `list: number[]`, then substitutes it into the
//! returns and callback params.

use std::collections::HashMap;

use crate::{GenericTplId, LuaType};

pub type TplBindings = HashMap<GenericTplId, LuaType>;

/// Structural unification: bind `TplRef`s in `param` to concrete types in `arg`
/// (recursively). Returns success (false on structural mismatch).
pub fn unify_bindings(param: &LuaType, arg: &LuaType, bindings: &mut TplBindings) -> bool {
    use LuaType::*;

    match param {
        // Generic parameter -> bind (if the actual arg is a concrete type).
        TplRef(tpl) => {
            if matches!(arg, Any) {
                return true;
            }
            let tpl_id = tpl.get_tpl_id();
            match bindings.get(&tpl_id) {
                Some(existing) => existing == arg,
                None => {
                    bindings.insert(tpl_id, arg.clone());
                    true
                }
            }
        }
        // Structural isomorphism: recurse into Array/TableGeneric/Tuple.
        Array(param_array) => {
            if let Array(arg_array) = arg {
                unify_bindings(param_array.get_base(), arg_array.get_base(), bindings)
            } else {
                false
            }
        }
        TableGeneric(param_generic) => {
            if let TableGeneric(arg_generic) = arg
                && param_generic.len() == arg_generic.len()
            {
                for (p, a) in param_generic.iter().zip(arg_generic.iter()) {
                    if !unify_bindings(p, a, bindings) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        }
        Tuple(param_tuple) => {
            if let Tuple(arg_tuple) = arg
                && param_tuple.get_types().len() == arg_tuple.get_types().len()
            {
                for (p, a) in param_tuple
                    .get_types()
                    .iter()
                    .zip(arg_tuple.get_types().iter())
                {
                    if !unify_bindings(p, a, bindings) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        }
        Object(param_object) => {
            if let Object(arg_object) = arg {
                for (key, expected) in param_object.get_fields() {
                    let Some(actual) = arg_object.get_fields().get(key) else {
                        continue;
                    };
                    if !unify_bindings(expected, actual, bindings) {
                        return false;
                    }
                }
                if param_object.get_index_access().len() != arg_object.get_index_access().len() {
                    return false;
                }
                for ((param_key, param_value), (arg_key, arg_value)) in param_object
                    .get_index_access()
                    .iter()
                    .zip(arg_object.get_index_access().iter())
                {
                    if !unify_bindings(param_key, arg_key, bindings)
                        || !unify_bindings(param_value, arg_value, bindings)
                    {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        }
        Generic(param_generic) => {
            if let Generic(arg_generic) = arg
                && param_generic.get_base_type_id() == arg_generic.get_base_type_id()
                && param_generic.get_params().len() == arg_generic.get_params().len()
            {
                for (p, a) in param_generic
                    .get_params()
                    .iter()
                    .zip(arg_generic.get_params())
                {
                    if !unify_bindings(p, a, bindings) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        }
        Intersection(param_intersection) => {
            if let Intersection(arg_intersection) = arg
                && param_intersection.get_types().len() == arg_intersection.get_types().len()
            {
                for (p, a) in param_intersection
                    .get_types()
                    .iter()
                    .zip(arg_intersection.get_types())
                {
                    if !unify_bindings(p, a, bindings) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        }
        Call(param_call) => {
            if let Call(arg_call) = arg
                && param_call.get_call_kind() == arg_call.get_call_kind()
                && param_call.get_operands().len() == arg_call.get_operands().len()
            {
                for (p, a) in param_call
                    .get_operands()
                    .iter()
                    .zip(arg_call.get_operands())
                {
                    if !unify_bindings(p, a, bindings) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        }
        Conditional(param_conditional) => {
            if let Conditional(arg_conditional) = arg {
                for (p, a) in [
                    param_conditional.get_checked_type(),
                    param_conditional.get_extends_type(),
                    param_conditional.get_true_type(),
                    param_conditional.get_false_type(),
                ]
                .iter()
                .zip([
                    arg_conditional.get_checked_type(),
                    arg_conditional.get_extends_type(),
                    arg_conditional.get_true_type(),
                    arg_conditional.get_false_type(),
                ]) {
                    if !unify_bindings(p, a, bindings) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        }
        Mapped(param_mapped) => {
            if let Mapped(arg_mapped) = arg {
                unify_bindings(&param_mapped.value, &arg_mapped.value, bindings)
            } else {
                false
            }
        }
        TypeGuard(param_guard) => {
            if let TypeGuard(arg_guard) = arg {
                unify_bindings(param_guard, arg_guard, bindings)
            } else {
                false
            }
        }
        MultiLineUnion(param_union) => {
            if let MultiLineUnion(arg_union) = arg
                && param_union.get_unions().len() == arg_union.get_unions().len()
            {
                for ((p, _), (a, _)) in param_union.get_unions().iter().zip(arg_union.get_unions())
                {
                    if !unify_bindings(p, a, bindings) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        }
        // Variadic types: `T...` can unify with a single value or a structurally
        // matching Variadic.
        Variadic(param_variadic) => match (param_variadic.as_ref(), arg) {
            (crate::VariadicType::Base(p), Variadic(arg_variadic)) => match arg_variadic.as_ref() {
                crate::VariadicType::Base(a) => unify_bindings(p, a, bindings),
                crate::VariadicType::Multi(_) => false,
            },
            (crate::VariadicType::Base(p), _) if !matches!(arg, Unknown | Any) => {
                unify_bindings(p, arg, bindings)
            }
            (crate::VariadicType::Base(_), _) => true,
            (crate::VariadicType::Multi(p_types), Variadic(arg_variadic)) => {
                if let crate::VariadicType::Multi(a_types) = arg_variadic.as_ref()
                    && p_types.len() == a_types.len()
                {
                    for (p, a) in p_types.iter().zip(a_types.iter()) {
                        if !unify_bindings(p, a, bindings) {
                            return false;
                        }
                    }
                    true
                } else {
                    false
                }
            }
            (crate::VariadicType::Multi(_), Unknown | Any) => true,
            (crate::VariadicType::Multi(_), _) => false,
        },
        // Fallback: matching param/arg types are ok; unknown/any actual args are ok.
        _ => {
            if matches!(arg, Unknown | Any) {
                true
            } else {
                param == arg
            }
        }
    }
}

/// Substitution: replace `TplRef`s in `ty` with bound values (recursively).
pub fn substitute(ty: &LuaType, bindings: &TplBindings) -> LuaType {
    use LuaType::*;
    match ty {
        TplRef(tpl) => {
            if let Some(value) = bindings.get(&tpl.get_tpl_id()) {
                value.clone()
            } else {
                ty.clone()
            }
        }
        Array(array) => {
            let base = substitute(array.get_base(), bindings);
            Array(crate::Arc::new(crate::LuaArrayType::from_base_type(base)))
        }
        TableGeneric(generic) => {
            let params = generic
                .iter()
                .map(|t| substitute(t, bindings))
                .collect::<Vec<_>>();
            TableGeneric(crate::Arc::new(params))
        }
        Tuple(tuple) => {
            let types = tuple
                .get_types()
                .iter()
                .map(|t| substitute(t, bindings))
                .collect();
            Tuple(crate::Arc::new(crate::LuaTupleType::new(
                types,
                crate::LuaTupleStatus::DocResolve,
            )))
        }
        DocFunction(fun) => {
            let params = fun
                .get_params()
                .iter()
                .map(|(name, ty)| (name.clone(), ty.as_ref().map(|t| substitute(t, bindings))))
                .collect();
            let ret = substitute(fun.get_ret(), bindings);
            DocFunction(crate::Arc::new(crate::LuaFunctionType::new(
                fun.get_async_state(),
                fun.is_colon_define(),
                fun.is_variadic(),
                params,
                ret,
                Some(fun.get_generic_params().to_vec()),
            )))
        }
        Union(union) => {
            let types = union
                .into_vec()
                .iter()
                .map(|t| substitute(t, bindings))
                .collect();
            Union(crate::Arc::new(crate::LuaUnionType::from_vec(types)))
        }
        Generic(generic) => Generic(crate::Arc::new(crate::LuaGenericType::new(
            generic.get_base_type_id(),
            generic
                .get_params()
                .iter()
                .map(|t| substitute(t, bindings))
                .collect(),
        ))),
        Object(object) => {
            let fields = object
                .get_fields()
                .iter()
                .map(|(key, ty)| (key.clone(), substitute(ty, bindings)))
                .collect();
            let index_access = object
                .get_index_access()
                .iter()
                .map(|(key, ty)| (substitute(key, bindings), substitute(ty, bindings)))
                .collect();
            Object(crate::Arc::new(crate::LuaObjectType::new_with_fields(
                fields,
                index_access,
            )))
        }
        Intersection(intersection) => {
            Intersection(crate::Arc::new(crate::LuaIntersectionType::new(
                intersection
                    .get_types()
                    .iter()
                    .map(|t| substitute(t, bindings))
                    .collect(),
            )))
        }
        Call(call) => Call(crate::Arc::new(crate::LuaAliasCallType::new(
            call.get_call_kind(),
            call.get_operands()
                .iter()
                .map(|t| substitute(t, bindings))
                .collect(),
        ))),
        Conditional(conditional) => Conditional(crate::Arc::new(crate::LuaConditionalType::new(
            substitute(conditional.get_checked_type(), bindings),
            substitute(conditional.get_extends_type(), bindings),
            substitute(conditional.get_true_type(), bindings),
            substitute(conditional.get_false_type(), bindings),
            conditional.get_infer_params().to_vec(),
            conditional.has_new,
        ))),
        Mapped(mapped) => Mapped(crate::Arc::new(crate::LuaMappedType::new(
            (
                mapped.param.0,
                crate::GenericParam::new(
                    mapped.param.1.name.clone(),
                    mapped
                        .param
                        .1
                        .constraint
                        .as_ref()
                        .map(|t| substitute(t, bindings)),
                    mapped
                        .param
                        .1
                        .default
                        .as_ref()
                        .map(|t| substitute(t, bindings)),
                    mapped.param.1.is_const,
                    mapped.param.1.attributes.clone(),
                ),
            ),
            substitute(&mapped.value, bindings),
            mapped.is_readonly,
            mapped.is_optional,
        ))),
        StrTplRef(str_tpl) => StrTplRef(crate::Arc::new(crate::LuaStringTplType::new(
            str_tpl.get_prefix(),
            str_tpl.get_name(),
            str_tpl.get_tpl_id(),
            str_tpl.get_suffix(),
            str_tpl.get_constraint().map(|t| substitute(t, bindings)),
        ))),
        MultiLineUnion(union) => MultiLineUnion(crate::Arc::new(crate::LuaMultiLineUnion::new(
            union
                .get_unions()
                .iter()
                .map(|(ty, desc)| (substitute(ty, bindings), desc.clone()))
                .collect(),
        ))),
        TypeGuard(guard) => TypeGuard(crate::Arc::new(substitute(guard, bindings))),
        Variadic(variadic) => {
            let substituted = match variadic.as_ref() {
                crate::VariadicType::Base(base) => {
                    let sub = substitute(base, bindings);
                    if let Variadic(inner) = sub {
                        // `R...` where R is bound to an entire multi-return -> expand to
                        // that Variadic directly.
                        return Variadic(inner);
                    }
                    crate::VariadicType::Base(sub)
                }
                crate::VariadicType::Multi(types) => crate::VariadicType::Multi(
                    types.iter().map(|t| substitute(t, bindings)).collect(),
                ),
            };
            Variadic(crate::Arc::new(substituted))
        }
        Instance(inst) => Instance(crate::Arc::new(crate::LuaInstanceType::new(
            substitute(inst.get_base(), bindings),
            inst.get_range().clone(),
        ))),
        _ => ty.clone(),
    }
}
