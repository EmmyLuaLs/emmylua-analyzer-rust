use super::{
    instantiate_special_generic::instantiate_alias_call,
    type_substitutor::{SubstitutorValue, TypeSubstitutor},
};
use crate::{
    DbIndex, GenericTpl, GenericTplId, LuaArrayType, LuaSignatureId,
    db_index::{
        LuaFunctionType, LuaGenericType, LuaIntersectionType, LuaObjectType, LuaTupleType, LuaType,
        LuaUnionType, VariadicType,
    },
};
use itertools::Itertools;
use std::sync::Arc;
use std::{collections::HashMap, ops::Deref};

pub fn instantiate_type_generic(
    db: &DbIndex,
    ty: &LuaType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    match ty {
        LuaType::Array(array_type) => instantiate_array(db, array_type.get_base(), substitutor),
        LuaType::Tuple(tuple) => instantiate_tuple(db, tuple, substitutor),
        LuaType::DocFunction(doc_func) => instantiate_doc_function(db, doc_func, substitutor),
        LuaType::Object(object) => instantiate_object(db, object, substitutor),
        LuaType::Union(union) => instantiate_union(db, union, substitutor),
        LuaType::Intersection(intersection) => {
            instantiate_intersection(db, intersection, substitutor)
        }
        LuaType::Generic(generic) => instantiate_generic(db, generic, substitutor),
        LuaType::TableGeneric(table_params) => {
            instantiate_table_generic(db, table_params, substitutor)
        }
        LuaType::TplRef(tpl) => instantiate_tpl_ref(db, tpl, substitutor),
        LuaType::Signature(sig_id) => instantiate_signature(db, sig_id, substitutor),
        LuaType::Call(alias_call) => instantiate_alias_call(db, alias_call, substitutor),
        LuaType::Variadic(variadic) => instantiate_variadic_type(db, variadic, substitutor),
        LuaType::SelfInfer => {
            if let Some(typ) = substitutor.get_self_type() {
                typ.clone()
            } else {
                LuaType::SelfInfer
            }
        }
        LuaType::TypeGuard(guard) => {
            let inner = instantiate_type_generic(db, guard.deref(), substitutor);
            LuaType::TypeGuard(inner.into())
        }
        _ => ty.clone(),
    }
}

fn instantiate_array(db: &DbIndex, base: &LuaType, substitutor: &TypeSubstitutor) -> LuaType {
    let base = instantiate_type_generic(db, base, substitutor);
    LuaType::Array(LuaArrayType::from_base_type(base).into())
}

fn instantiate_tuple(db: &DbIndex, tuple: &LuaTupleType, substitutor: &TypeSubstitutor) -> LuaType {
    let tuple_types = tuple.get_types();
    let new_types = collapse_variadics_in_vec(
        tuple_types
            .iter()
            .map(|typ| instantiate_type_generic(db, typ, substitutor))
            .collect(),
    );
    LuaType::Tuple(LuaTupleType::new(new_types, tuple.status).into())
}

pub fn instantiate_doc_function(
    db: &DbIndex,
    doc_func: &LuaFunctionType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let tpl_func_params = doc_func.get_params();
    let tpl_ret = doc_func.get_ret();
    let is_async = doc_func.is_async();
    let colon_define = doc_func.is_colon_define();

    let mut new_params = Vec::new();
    for i in 0..tpl_func_params.len() {
        let origin_param = &tpl_func_params[i];

        let origin_param_type = if let Some(ty) = &origin_param.1 {
            ty
        } else {
            new_params.push((origin_param.0.clone(), None));
            continue;
        };

        // Special case when function parameter is a variadic with known parameter names.
        // We want to preserve these parameter names.
        let mut origin_param_multi_variadic_names = None;
        if let LuaType::Variadic(variadic) = origin_param_type {
            if let VariadicType::Base(base) = variadic.deref() {
                if let LuaType::TplRef(tpl) = base {
                    if let Some(value) = substitutor.get(tpl.get_tpl_id()) {
                        if let SubstitutorValue::Params(params) = value {
                            origin_param_multi_variadic_names =
                                Some(params.iter().map(|param| &param.0).collect::<Vec<_>>())
                        }
                    }
                }
            }
        }

        let new_type = instantiate_type_generic(db, &origin_param_type, &substitutor);
        match new_type {
            LuaType::Variadic(variadic) => match variadic.deref() {
                VariadicType::Base(base) => {
                    new_params.push(("...".to_string(), Some(base.clone())));
                }
                VariadicType::Multi(types) => {
                    for typ_with_name in types
                        .iter()
                        .zip_longest(origin_param_multi_variadic_names.unwrap_or_default())
                    {
                        let (Some(typ), name) = typ_with_name.left_and_right() else {
                            break;
                        };
                        let name = name
                            .cloned()
                            .unwrap_or_else(|| format!("p{}", new_params.len()));
                        new_params.push((name, Some(typ.clone())));
                    }
                }
            },
            _ => {
                new_params.push((origin_param.0.clone(), Some(new_type)));
            }
        }
    }

    // 将 substitutor 中存储的类型的 def 转为 ref
    let mut modified_substitutor = substitutor.clone();
    modified_substitutor.convert_def_to_ref();
    let inst_ret_type = instantiate_type_generic(db, &tpl_ret, &modified_substitutor);
    let inst_ret_type = collapse_variadic_in_function_return_type(inst_ret_type);
    LuaType::DocFunction(
        LuaFunctionType::new(is_async, colon_define, new_params, inst_ret_type).into(),
    )
}

fn instantiate_object(
    db: &DbIndex,
    object: &LuaObjectType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let fields = object.get_fields();
    let index_access = object.get_index_access();

    let mut new_fields = HashMap::new();
    for (key, field) in fields {
        let new_field = instantiate_type_generic(db, field, substitutor);
        new_fields.insert(key.clone(), new_field);
    }

    let mut new_index_access = Vec::new();
    for (key, value) in index_access {
        let key = instantiate_type_generic(db, &key, substitutor);
        let value = instantiate_type_generic(db, &value, substitutor);
        new_index_access.push((key, value));
    }

    LuaType::Object(LuaObjectType::new_with_fields(new_fields, new_index_access).into())
}

fn instantiate_union(db: &DbIndex, union: &LuaUnionType, substitutor: &TypeSubstitutor) -> LuaType {
    let types = union.into_vec();
    let mut result_types = Vec::new();
    for t in types {
        let t = instantiate_type_generic(db, &t, substitutor);
        result_types.push(t);
    }

    LuaType::from_vec(result_types)
}

fn instantiate_intersection(
    db: &DbIndex,
    intersection: &LuaIntersectionType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let types = intersection.get_types();
    let mut new_types = Vec::new();
    for t in types {
        let t = instantiate_type_generic(db, t, substitutor);
        new_types.push(t);
    }

    LuaType::Intersection(LuaIntersectionType::new(new_types).into())
}

fn instantiate_generic(
    db: &DbIndex,
    generic: &LuaGenericType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let generic_params = generic.get_params();
    let mut new_params = Vec::new();
    for param in generic_params {
        let new_param = instantiate_type_generic(db, param, substitutor);
        new_params.push(new_param);
    }

    let base = generic.get_base_type();
    let type_decl_id = if let LuaType::Ref(id) = base {
        id
    } else {
        return LuaType::Unknown;
    };

    if !substitutor.check_recursion(&type_decl_id) {
        if let Some(type_decl) = db.get_type_index().get_type_decl(&type_decl_id) {
            if type_decl.is_alias() {
                let new_substitutor =
                    TypeSubstitutor::from_alias(new_params.clone(), type_decl_id.clone());
                if let Some(origin) = type_decl.get_alias_origin(db, Some(&new_substitutor)) {
                    return origin;
                }
            }
        }
    }

    LuaType::Generic(LuaGenericType::new(type_decl_id, new_params).into())
}

fn instantiate_table_generic(
    db: &DbIndex,
    table_params: &Vec<LuaType>,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let mut new_params = Vec::new();
    for param in table_params {
        let new_param = instantiate_type_generic(db, param, substitutor);
        new_params.push(new_param);
    }

    LuaType::TableGeneric(new_params.into())
}

fn instantiate_tpl_ref(_: &DbIndex, tpl: &GenericTpl, substitutor: &TypeSubstitutor) -> LuaType {
    if let Some(value) = substitutor.get(tpl.get_tpl_id()) {
        match value {
            SubstitutorValue::None => {}
            SubstitutorValue::Type(ty) => return ty.clone(),
            SubstitutorValue::MultiTypes(types) => {
                return types.first().unwrap_or(&LuaType::Unknown).clone();
            }
            SubstitutorValue::Params(params) => {
                return params
                    .first()
                    .unwrap_or(&(String::new(), None))
                    .1
                    .clone()
                    .unwrap_or(LuaType::Unknown);
            }
            SubstitutorValue::MultiBase(base) => return base.clone(),
        }
    }

    LuaType::TplRef(tpl.clone().into())
}

fn instantiate_signature(
    db: &DbIndex,
    signature_id: &LuaSignatureId,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    if let Some(signature) = db.get_signature_index().get(&signature_id) {
        let origin_type = {
            let fake_doc_function = signature.to_doc_func_type();
            instantiate_doc_function(db, &fake_doc_function, substitutor)
        };
        if signature.overloads.is_empty() {
            return origin_type;
        } else {
            let mut result = Vec::new();
            for overload in signature.overloads.iter() {
                result.push(instantiate_doc_function(
                    db,
                    &(*overload).clone(),
                    substitutor,
                ));
            }
            result.push(origin_type); // 我们需要将原始类型放到最后
            return LuaType::from_vec(result);
        }
    }

    return LuaType::Signature(signature_id.clone());
}

fn instantiate_variadic_type(
    db: &DbIndex,
    variadic: &VariadicType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    match variadic {
        VariadicType::Base(base) => {
            if base.contain_tpl() {
                let tpl_refs = base.find_all_tpl();

                let Ok((tpl_ref_ids, len, need_unwrapping)) =
                    check_tpl_params_for_variadic_expansion(substitutor, base)
                else {
                    return LuaType::Unknown;
                };

                if tpl_ref_ids.is_empty() {
                    // There's nothing to expand, no further work needed.
                    return LuaType::Variadic(variadic.clone().into());
                }

                // Iterate over all found multi variadics and expand our type
                // for each of them.
                //
                // We should take care to deal with base variadics and return
                // an expansion of correct length.
                //
                // If there are no multi variadics and no base variadics, then
                // expansion will have length of 1.
                //
                // If there are multi variadics, and they don't have a base variadic
                // at the end, then expansion will have length of these multi variadics.
                //
                // If there are multi variadics, and all of them have a base variadic
                // at the end, then expansion will have length of these multi variadics,
                // and the last expanded element will become a base variadic.
                //
                // Finally, if there are no multi variadics but there are base variadics,
                // the expansion will be a single base variadic.
                //
                // To achieve these results, we must unwrap base variadics before substitution,
                // and then re-wrap the substitution result. That is, if we're expanding
                // `Future<T>`, and `T` is `Multi([A, B, Base(C)])`, we want to end up with
                // `Future<A>, Future<B>, Future<C>...`, and not
                // `Future<A>, Future<B>, Future<C...>`.
                //
                // Examples:
                //
                // - if `T` is `Base(A)`, then `Future<T>...` expands into `Future<A>...`;
                // - if `T` is `Multi([A, B])`, then `Future<T>...` expands into `Future<A>, Future<B>`;
                // - if `T` is `Multi([A, Base(B)])`, then `Future<T>...` expands into `Future<A>, Future<B>...`.
                let mut new_types = Vec::new();
                for i in 0..len {
                    let is_last = i == len - 1;

                    expand_variadic_element(
                        db,
                        substitutor,
                        base,
                        i,
                        &tpl_refs,
                        is_last,
                        &mut new_types,
                        need_unwrapping,
                    );
                }

                // Re-wrap last type into a base variadic.
                if need_unwrapping {
                    if let Some(last) = new_types.pop() {
                        new_types.push(LuaType::Variadic(VariadicType::Base(last).into()));
                    }
                }

                LuaType::Variadic(VariadicType::Multi(new_types).into())
            } else {
                LuaType::Variadic(variadic.clone().into())
            }
        }
        VariadicType::Multi(types) => {
            if types.iter().any(|it| it.contain_tpl()) {
                let mut new_types = Vec::new();
                for t in types {
                    new_types.push(instantiate_type_generic(db, t, substitutor));
                }
                LuaType::Variadic(VariadicType::Multi(new_types).into())
            } else {
                LuaType::Variadic(variadic.clone().into())
            }
        }
    }
}

fn check_tpl_params_for_variadic_expansion(
    substitutor: &TypeSubstitutor,
    base: &LuaType,
) -> Result<(Vec<GenericTplId>, usize, bool), ()> {
    // Check all tpl refs in the type we're expanding.
    //
    // If there are multi variadics, we expect that all of them
    // have the same shape. First, all of them should have the same
    // length. Second, if one of them has a base variadic at the end,
    // then all of them should have a base variadic at the end.

    let tpl_refs = base.find_all_tpl();

    // Common length of all found multi variadics. `None` if there are
    // no multi variadics found.
    let mut len = None;
    // `False` if any multi variadic doesn't have a base at the end.
    let mut all_multi_variadics_contain_base = true;
    // `True` if any multi variadic has a base at the end.
    let mut some_multi_variadics_contain_base = false;
    // `True` if there is a base variadic, or if there are multi variadics
    // with a base at the end.
    let mut has_base_variadic = false;

    let mut tpl_ref_ids = Vec::new();

    for tpl_ref in &tpl_refs {
        let Some(tpl_id) = tpl_ref.get_tpl_id() else {
            // This is a `SelfInfer`, we don't care about it.
            continue;
        };

        let (multi_len, is_variadic_base) = match substitutor.get(tpl_id) {
            Some(SubstitutorValue::MultiTypes(types)) => (
                types.len(),
                types.last().is_some_and(|last| last.is_variadic_base()),
            ),
            Some(SubstitutorValue::Params(params)) => (
                params.len(),
                params
                    .last()
                    .and_then(|(_, last)| last.as_ref())
                    .is_some_and(|last| last.is_variadic_base()),
            ),
            Some(SubstitutorValue::MultiBase(_)) => {
                // A variadic with unlimited length.
                tpl_ref_ids.push(tpl_id);
                has_base_variadic = true;
                continue;
            }
            Some(SubstitutorValue::Type(_)) => {
                // This is not a variadic parameter, it doesn't affect
                // expansion length.
                tpl_ref_ids.push(tpl_id);
                continue;
            }
            None | Some(SubstitutorValue::None) => {
                continue;
            }
        };

        tpl_ref_ids.push(tpl_id);
        if let Some(prev_len) = len {
            if prev_len != multi_len {
                // Variadic expansion contains packs of different length.
                return Err(());
            }
        } else {
            len = Some(multi_len);
        }
        if is_variadic_base {
            has_base_variadic = true;
            some_multi_variadics_contain_base = true;
        } else {
            all_multi_variadics_contain_base = false;
        }

        if some_multi_variadics_contain_base && !all_multi_variadics_contain_base {
            // Shapes of multi variadics are not consistent.
            return Err(());
        }
    }

    let need_unwrapping = has_base_variadic && all_multi_variadics_contain_base;

    Ok((tpl_ref_ids, len.unwrap_or(1), need_unwrapping))
}

fn expand_variadic_element(
    db: &DbIndex,
    substitutor: &TypeSubstitutor,
    base: &LuaType,
    i: usize,
    tpl_refs: &[LuaType],
    is_last: bool,
    new_types: &mut Vec<LuaType>,
    need_unwrapping: bool,
) {
    // Prepare all substitutions.
    let mut new_substitutor = substitutor.clone();
    for tpl_ref in tpl_refs {
        let Some(tpl_id) = tpl_ref.get_tpl_id() else {
            continue;
        };

        // Get type we'll be substituting for this `tpl_id`.
        let replacement_typ = match substitutor.get(tpl_id) {
            Some(SubstitutorValue::Type(typ)) => typ.clone(),
            Some(SubstitutorValue::Params(params)) => {
                let replacement_typ = params
                    .get(i)
                    .and_then(|param| param.1.clone())
                    .unwrap_or(LuaType::Unknown);
                if need_unwrapping && is_last {
                    unwrap_variadic_base(replacement_typ)
                } else {
                    replacement_typ
                }
            }
            Some(SubstitutorValue::MultiTypes(types)) => {
                let replacement_typ = types.get(i).cloned().unwrap_or(LuaType::Unknown);
                if need_unwrapping && is_last {
                    unwrap_variadic_base(replacement_typ)
                } else {
                    replacement_typ
                }
            }
            Some(SubstitutorValue::MultiBase(typ)) => {
                // Non-multi base variadics are always unwrapped and the re-wrapped.
                typ.clone()
            }
            _ => LuaType::Unknown,
        };

        // Insert substitution type into the new substitutor.
        // We take care to choose the right `SubstitutorValue`
        // to facilitate any nested expansions.
        new_substitutor.reset_type(tpl_id);
        match replacement_typ {
            LuaType::Variadic(variadic) => match Arc::unwrap_or_clone(variadic) {
                VariadicType::Multi(multi) => new_substitutor.insert_multi_types(tpl_id, multi),
                VariadicType::Base(base) => new_substitutor.insert_multi_base(tpl_id, base),
            },
            replacement_typ => new_substitutor.insert_type(tpl_id, replacement_typ),
        }
    }

    // Run substitution and save the result.
    new_types.push(instantiate_type_generic(db, base, &mut new_substitutor));
}

fn unwrap_variadic_base(replacement_typ: LuaType) -> LuaType {
    // Unwrap last base in a multi variadic. See above for details.
    match replacement_typ {
        LuaType::Variadic(variadic) => match Arc::unwrap_or_clone(variadic) {
            VariadicType::Multi(multi) => LuaType::Variadic(VariadicType::Multi(multi).into()),
            VariadicType::Base(base) => base,
        },
        replacement_typ => replacement_typ,
    }
}

/// Collapse variadic of pattern `multi<A, B, multi<C, D, ...>>` into a single
/// flat `multi<A, B, C, D, ...>`.
fn collapse_variadic_in_function_return_type(typ: LuaType) -> LuaType {
    match typ {
        LuaType::Variadic(variadic) => match Arc::unwrap_or_clone(variadic) {
            VariadicType::Multi(returns) => {
                let returns = collapse_variadics_in_vec(returns);
                match returns.len() {
                    0 => LuaType::Nil,
                    1 => returns[0].clone(),
                    _ => LuaType::Variadic(VariadicType::Multi(returns).into()),
                }
            }
            VariadicType::Base(base) => LuaType::Variadic(VariadicType::Base(base).into()),
        },
        typ => typ,
    }
}

/// Flatten variadics at the end of a vector.
pub fn collapse_variadics_in_vec(mut typs: Vec<LuaType>) -> Vec<LuaType> {
    while let Some(last) = typs.pop() {
        match last {
            LuaType::Variadic(variadic) => match variadic.deref() {
                VariadicType::Multi(multi) => {
                    typs.extend(multi.iter().cloned());
                }
                _ => {
                    typs.push(LuaType::Variadic(variadic));
                    break;
                }
            },
            last => {
                typs.push(last);
                break;
            }
        }
    }

    typs
}
