use std::{collections::HashMap, ops::Deref};

use crate::{
    DbIndex, GenericTpl, LuaArrayType, LuaSignatureId, LuaTupleStatus,
    db_index::{
        LuaFunctionType, LuaGenericType, LuaIntersectionType, LuaObjectType, LuaTupleType, LuaType,
        LuaUnionType, VariadicType,
    },
};

use super::{
    instantiate_special_generic::instantiate_alias_call,
    type_substitutor::{SubstitutorValue, TypeSubstitutor},
};

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
    let mut new_types = Vec::new();
    for t in tuple_types {
        if let LuaType::Variadic(inner) = t {
            match inner.deref() {
                VariadicType::Base(base) => {
                    if let LuaType::TplRef(tpl) = base {
                        if tpl.is_variadic() {
                            if let Some(generics) = substitutor.get_variadic(tpl.get_tpl_id()) {
                                new_types.extend_from_slice(&generics);
                            }
                            break;
                        }

                        if let Some(value) = substitutor.get(tpl.get_tpl_id()) {
                            match value {
                                SubstitutorValue::None => {}
                                SubstitutorValue::MultiTypes(types) => {
                                    for typ in types {
                                        new_types.push(typ.clone());
                                    }
                                }
                                SubstitutorValue::Params(params) => {
                                    for (_, ty) in params {
                                        new_types.push(ty.clone().unwrap_or(LuaType::Unknown));
                                    }
                                }
                                SubstitutorValue::Type(ty) => new_types.push(ty.clone()),
                                SubstitutorValue::MultiBase(base) => new_types.push(base.clone()),
                            }
                        }
                    }
                }
                VariadicType::Multi(_) => (),
            }

            break;
        }

        let t = instantiate_type_generic(db, t, substitutor);
        new_types.push(t);
    }
    LuaType::Tuple(LuaTupleType::new(new_types, tuple.status).into())
}

pub fn instantiate_doc_function(
    db: &DbIndex,
    doc_func: &LuaFunctionType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let tpl_func_params = doc_func.get_params();
    let tpl_ret = doc_func.get_ret();
    let async_state = doc_func.get_async_state();
    let colon_define = doc_func.is_colon_define();

    let mut new_params = Vec::new();
    for (i, origin_param) in tpl_func_params.iter().enumerate() {
        let origin_param_type = if let Some(ty) = &origin_param.1 {
            ty
        } else {
            new_params.push((origin_param.0.clone(), None));
            continue;
        };
        match origin_param_type {
            LuaType::Variadic(variadic) => match variadic.deref() {
                VariadicType::Base(base) => {
                    if let LuaType::TplRef(tpl) = base {
                        if tpl.is_variadic() {
                            if let Some(generics) = substitutor.get_variadic(tpl.get_tpl_id()) {
                                for (j, typ) in generics.iter().enumerate() {
                                    let param_name = format!("param{}", i + j);
                                    new_params.push((param_name, Some(typ.clone())));
                                }
                            }
                            continue;
                        }
                        if let Some(value) = substitutor.get(tpl.get_tpl_id()) {
                            match value {
                                SubstitutorValue::Params(params) => {
                                    for param in params {
                                        new_params.push(param.clone());
                                    }
                                }
                                SubstitutorValue::MultiTypes(types) => {
                                    for (i, typ) in types.iter().enumerate() {
                                        let param_name = format!("param{}", i);
                                        new_params.push((param_name, Some(typ.clone())));
                                    }
                                }
                                _ => {
                                    new_params.push((
                                        "...".to_string(),
                                        Some(LuaType::Variadic(
                                            VariadicType::Base(LuaType::Any).into(),
                                        )),
                                    ));
                                }
                            }
                        }
                    }
                }
                VariadicType::Multi(_) => (),
            },
            _ => {
                let new_type = instantiate_type_generic(db, origin_param_type, substitutor);
                // 如果是 rest 参数且实例化后的类型是 tuple, 那么我们将展开 tuple
                if origin_param.0 == "..." && tpl_func_params.len() == i + 1 {
                    if let LuaType::Tuple(tuple_type) = &new_type {
                        let base_index = new_params.len();
                        for (offset, tuple_element) in tuple_type.get_types().iter().enumerate() {
                            let param_name = format!("args_{}", base_index + offset);
                            new_params.push((param_name, Some(tuple_element.clone())));
                        }
                        continue;
                    }
                }
                new_params.push((origin_param.0.clone(), Some(new_type)));
            }
        }
    }

    // 将 substitutor 中存储的类型的 def 转为 ref
    let mut modified_substitutor = substitutor.clone();
    modified_substitutor.convert_def_to_ref();
    let inst_ret_type = instantiate_type_generic(db, tpl_ret, &modified_substitutor);
    LuaType::DocFunction(
        LuaFunctionType::new(async_state, colon_define, new_params, inst_ret_type).into(),
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
        let key = instantiate_type_generic(db, key, substitutor);
        let value = instantiate_type_generic(db, value, substitutor);
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
        if let LuaType::Variadic(variadic) = &new_param {
            match variadic.deref() {
                VariadicType::Base(_) => {}
                VariadicType::Multi(types) => {
                    for typ in types {
                        new_params.push(typ.clone());
                    }
                    continue;
                }
            }
        }
        new_params.push(new_param);
    }

    let base = generic.get_base_type();
    let type_decl_id = if let LuaType::Ref(id) = base {
        id
    } else {
        return LuaType::Unknown;
    };

    if !substitutor.check_recursion(&type_decl_id)
        && let Some(type_decl) = db.get_type_index().get_type_decl(&type_decl_id)
        && type_decl.is_alias()
    {
        let new_substitutor = TypeSubstitutor::from_alias(new_params.clone(), type_decl_id.clone());
        if let Some(origin) = type_decl.get_alias_origin(db, Some(&new_substitutor)) {
            return origin;
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
    if tpl.is_variadic() {
        if let Some(generics) = substitutor.get_variadic(tpl.get_tpl_id()) {
            if generics.len() == 1 {
                return generics[0].clone();
            } else {
                return LuaType::Tuple(
                    LuaTupleType::new(generics.clone(), LuaTupleStatus::DocResolve).into(),
                );
            }
        } else {
            return LuaType::Never;
        }
    }

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
    if let Some(signature) = db.get_signature_index().get(signature_id) {
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

    LuaType::Signature(*signature_id)
}

fn instantiate_variadic_type(
    db: &DbIndex,
    variadic: &VariadicType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    match variadic {
        VariadicType::Base(base) => {
            if let LuaType::TplRef(tpl) = base {
                if tpl.is_variadic() {
                    if let Some(generics) = substitutor.get_variadic(tpl.get_tpl_id()) {
                        if generics.len() == 1 {
                            return generics[0].clone();
                        } else {
                            return LuaType::Variadic(VariadicType::Multi(generics.clone()).into());
                        }
                    } else {
                        return LuaType::Never;
                    }
                }

                if let Some(value) = substitutor.get(tpl.get_tpl_id()) {
                    match value {
                        SubstitutorValue::None => {
                            return LuaType::Never;
                        }
                        SubstitutorValue::Type(ty) => return ty.clone(),
                        SubstitutorValue::MultiTypes(types) => {
                            return LuaType::Variadic(VariadicType::Multi(types.clone()).into());
                        }
                        SubstitutorValue::Params(params) => {
                            let types = params
                                .iter()
                                .filter_map(|(_, ty)| ty.clone())
                                .collect::<Vec<_>>();
                            return LuaType::Variadic(VariadicType::Multi(types).into());
                        }
                        SubstitutorValue::MultiBase(base) => {
                            return LuaType::Variadic(VariadicType::Base(base.clone()).into());
                        }
                    }
                } else {
                    return LuaType::Never;
                }
            }
        }
        VariadicType::Multi(types) => {
            if types.iter().any(|it| it.contain_tpl()) {
                let mut new_types = Vec::new();
                for t in types {
                    let t = instantiate_type_generic(db, t, substitutor);
                    match t {
                        LuaType::Never => {}
                        LuaType::Variadic(variadic) => match variadic.deref() {
                            VariadicType::Base(base) => new_types.push(base.clone()),
                            VariadicType::Multi(multi) => {
                                for mt in multi {
                                    new_types.push(mt.clone());
                                }
                            }
                        },
                        _ => new_types.push(t),
                    }
                }
                return LuaType::Variadic(VariadicType::Multi(new_types).into());
            }
        }
    }

    LuaType::Variadic(variadic.clone().into())
}
