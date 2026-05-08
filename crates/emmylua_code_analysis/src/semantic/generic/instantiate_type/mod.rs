mod complete_generic_args;
mod instantiate_conditional_generic;
mod instantiate_func_generic;
mod instantiate_special_generic;

use hashbrown::{HashMap, HashSet};
use std::{ops::Deref, sync::Arc};

use crate::{
    DbIndex, GenericTpl, GenericTplId, LuaArrayType, LuaMappedType, LuaMemberKey,
    LuaOperatorMetaMethod, LuaSignatureId, LuaTupleStatus, LuaTupleType, LuaTypeDeclId,
    LuaTypeNode, TypeOps,
    db_index::{
        LuaFunctionType, LuaGenericType, LuaIntersectionType, LuaObjectType, LuaType, LuaUnionType,
        VariadicType,
    },
    semantic::infer::InferFailReason,
};

use super::type_substitutor::{
    GenericInstantiateContext, SubstitutorValue, TypeSubstitutor, UninferredTplPolicy,
};
pub use complete_generic_args::{
    GenericArgumentCompletion, complete_type_generic_args, complete_type_generic_args_in_type,
};
pub use instantiate_func_generic::{build_self_type, infer_self_type, instantiate_func_generic};
pub use instantiate_special_generic::get_keyof_members;

pub(crate) fn collect_callable_overload_groups(
    db: &DbIndex,
    callable_type: &LuaType,
    groups: &mut Vec<Vec<Arc<LuaFunctionType>>>,
) -> Result<(), InferFailReason> {
    let mut visiting_aliases = HashSet::new();
    collect_callable_overload_groups_inner(db, callable_type, groups, &mut visiting_aliases)
}

fn collect_callable_overload_groups_inner(
    db: &DbIndex,
    callable_type: &LuaType,
    groups: &mut Vec<Vec<Arc<LuaFunctionType>>>,
    visiting_aliases: &mut HashSet<LuaTypeDeclId>,
) -> Result<(), InferFailReason> {
    match callable_type {
        LuaType::Ref(type_id) | LuaType::Def(type_id) => {
            let Some(type_decl) = db.get_type_index().get_type_decl(type_id) else {
                return Ok(());
            };
            if !visiting_aliases.insert(type_id.clone()) {
                return Ok(());
            }

            let result = if let Some(origin_type) = type_decl.get_alias_origin(db, None) {
                collect_callable_overload_groups_inner(db, &origin_type, groups, visiting_aliases)
            } else {
                Ok(())
            };
            visiting_aliases.remove(type_id);
            result?;
        }
        LuaType::Generic(generic) => {
            let type_id = generic.get_base_type_id();
            if !visiting_aliases.insert(type_id.clone()) {
                return Ok(());
            }
            let substitutor = TypeSubstitutor::from_type_array(generic.get_params().to_vec());
            let Some(type_decl) = db.get_type_index().get_type_decl(&type_id) else {
                visiting_aliases.remove(&type_id);
                return Ok(());
            };

            let result = if let Some(origin_type) =
                type_decl.get_alias_origin(db, Some(&substitutor))
            {
                collect_callable_overload_groups_inner(db, &origin_type, groups, visiting_aliases)
            } else {
                Ok(())
            };
            visiting_aliases.remove(&type_id);
            result?;
        }
        LuaType::Union(union) => {
            for member in union.into_vec() {
                collect_callable_overload_groups_inner(db, &member, groups, visiting_aliases)?;
            }
        }
        LuaType::Intersection(intersection) => {
            for member in intersection.get_types() {
                collect_callable_overload_groups_inner(db, member, groups, visiting_aliases)?;
            }
        }
        LuaType::DocFunction(doc_func) => groups.push(vec![doc_func.clone()]),
        LuaType::Signature(sig_id) => {
            let Some(signature) = db.get_signature_index().get(sig_id) else {
                return Ok(());
            };
            let mut overloads = signature.overloads.to_vec();
            overloads.push(signature.to_doc_func_type());
            groups.push(overloads);
        }
        _ => {}
    }

    Ok(())
}

pub fn instantiate_type_generic(
    db: &DbIndex,
    ty: &LuaType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let context = GenericInstantiateContext::new(db, substitutor);
    instantiate_type_generic_with_context(&context, ty)
}

pub(super) fn instantiate_type_generic_with_context(
    context: &GenericInstantiateContext,
    ty: &LuaType,
) -> LuaType {
    match ty {
        LuaType::Array(array_type) => instantiate_array(context, array_type.get_base()),
        LuaType::Tuple(tuple) => instantiate_tuple(context, tuple),
        LuaType::DocFunction(doc_func) => instantiate_doc_function_with_context(
            &context.with_policy(UninferredTplPolicy::PreserveTplRef),
            doc_func,
        ),
        LuaType::Object(object) => instantiate_object(context, object),
        LuaType::Union(union) => instantiate_union(context, union),
        LuaType::Intersection(intersection) => instantiate_intersection(context, intersection),
        LuaType::Generic(generic) => instantiate_generic_with_context(context, generic),
        LuaType::TableGeneric(table_params) => instantiate_table_generic(context, table_params),
        LuaType::TplRef(tpl) => instantiate_tpl_ref(tpl, context),
        LuaType::ConstTplRef(tpl) => instantiate_const_tpl_ref(tpl, context),
        LuaType::Signature(sig_id) => instantiate_signature(context, sig_id),
        LuaType::Call(alias_call) => {
            instantiate_special_generic::instantiate_alias_call(context, alias_call)
        }
        LuaType::Variadic(variadic) => instantiate_variadic_type(context, variadic),
        LuaType::SelfInfer => {
            if let Some(typ) = context.substitutor.get_self_type() {
                typ.clone()
            } else {
                LuaType::SelfInfer
            }
        }
        LuaType::TypeGuard(guard) => {
            let inner = instantiate_type_generic_with_context(context, guard.deref());
            LuaType::TypeGuard(inner.into())
        }
        LuaType::Conditional(conditional) => {
            instantiate_conditional_generic::instantiate_conditional(context, conditional)
        }
        LuaType::Mapped(mapped) => instantiate_mapped_type(context, mapped.deref()),
        _ => ty.clone(),
    }
}

fn instantiate_types<'a, I>(context: &GenericInstantiateContext, types: I) -> Vec<LuaType>
where
    I: IntoIterator<Item = &'a LuaType>,
{
    types
        .into_iter()
        .map(|ty| instantiate_type_generic_with_context(context, ty))
        .collect()
}

fn instantiate_type_pairs<'a, I>(
    context: &GenericInstantiateContext,
    pairs: I,
) -> Vec<(LuaType, LuaType)>
where
    I: IntoIterator<Item = &'a (LuaType, LuaType)>,
{
    pairs
        .into_iter()
        .map(|(key, value)| {
            (
                instantiate_type_generic_with_context(context, key),
                instantiate_type_generic_with_context(context, value),
            )
        })
        .collect()
}

fn instantiate_array(context: &GenericInstantiateContext, base: &LuaType) -> LuaType {
    let base = instantiate_type_generic_with_context(context, base);
    LuaType::Array(LuaArrayType::from_base_type(base).into())
}

fn instantiate_tuple(context: &GenericInstantiateContext, tuple: &LuaTupleType) -> LuaType {
    let mut new_types = Vec::new();
    for t in tuple.get_types() {
        if let LuaType::Variadic(inner) = t {
            match inner.deref() {
                VariadicType::Base(base) => {
                    if let LuaType::TplRef(tpl) = base {
                        if let Some(value) = context.substitutor.get(tpl.get_tpl_id()) {
                            match value {
                                SubstitutorValue::None => new_types
                                    .push(instantiate_uninferred_tpl_fallback(tpl, context)),
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
                                SubstitutorValue::Type(ty) => new_types.push(ty.default().clone()),
                                SubstitutorValue::MultiBase(base) => new_types.push(base.clone()),
                            }
                        } else {
                            new_types.push(LuaType::Variadic(inner.clone()));
                        }
                    }
                }
                VariadicType::Multi(_) => (),
            }

            break;
        }

        let t = instantiate_type_generic_with_context(context, t);
        new_types.push(t);
    }
    LuaType::Tuple(LuaTupleType::new(new_types, tuple.status).into())
}

pub fn instantiate_doc_function(
    db: &DbIndex,
    doc_func: &LuaFunctionType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let context = GenericInstantiateContext::new(db, substitutor);
    instantiate_doc_function_with_context(&context, doc_func)
}

fn instantiate_doc_function_with_context(
    context: &GenericInstantiateContext,
    doc_func: &LuaFunctionType,
) -> LuaType {
    let tpl_func_params = doc_func.get_params();
    let tpl_ret = doc_func.get_ret();
    let async_state = doc_func.get_async_state();
    let colon_define = doc_func.is_colon_define();

    let mut new_params = Vec::new();
    for origin_param in tpl_func_params.iter() {
        let origin_param_type = if let Some(ty) = &origin_param.1 {
            ty
        } else {
            new_params.push((origin_param.0.clone(), None));
            continue;
        };
        match origin_param_type {
            LuaType::Variadic(variadic) => match variadic.deref() {
                VariadicType::Base(base) => match base {
                    LuaType::TplRef(tpl) => {
                        if let Some(value) = context.substitutor.get(tpl.get_tpl_id()) {
                            match value {
                                SubstitutorValue::None => {
                                    let ty = instantiate_uninferred_tpl_fallback(tpl, context);
                                    new_params.push((origin_param.0.clone(), Some(ty)));
                                }
                                SubstitutorValue::Type(ty) => {
                                    let resolved_type = ty.default();
                                    // 如果参数是 `...: T...`
                                    if origin_param.0 == "..." {
                                        // 类型是 tuple, 那么我们将展开 tuple
                                        if let LuaType::Tuple(tuple) = resolved_type {
                                            let base_index = new_params.len();
                                            for (i, typ) in tuple.get_types().iter().enumerate() {
                                                let param_name = format!("var{}", base_index + i);
                                                new_params.push((param_name, Some(typ.clone())));
                                            }
                                        } else {
                                            new_params.push((
                                                origin_param.0.clone(),
                                                Some(resolved_type.clone()),
                                            ));
                                        }
                                        continue;
                                    }
                                    // 一个错误的情况, 我们不应该允许 `非...参数名: T...`, 因此构造的 Variadic 是一个错误的结果, 应在更上层报错
                                    new_params.push((
                                        origin_param.0.clone(),
                                        Some(LuaType::Variadic(
                                            VariadicType::Base(resolved_type.clone()).into(),
                                        )),
                                    ));
                                }
                                SubstitutorValue::Params(params) => {
                                    for param in params.iter() {
                                        new_params.push(param.clone());
                                    }
                                }
                                SubstitutorValue::MultiTypes(types) => {
                                    for (i, typ) in types.iter().enumerate() {
                                        let param_name = format!("var{}", i);
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
                        } else {
                            new_params
                                .push((origin_param.0.clone(), Some(origin_param_type.clone())));
                        }
                    }
                    LuaType::Generic(generic) => {
                        let new_type = instantiate_generic_with_context(context, generic);
                        // 如果是 rest 参数且实例化后的类型是 tuple, 那么我们将展开 tuple
                        if let LuaType::Tuple(tuple_type) = &new_type {
                            let base_index = new_params.len();
                            for (offset, tuple_element) in tuple_type.get_types().iter().enumerate()
                            {
                                let param_name = format!("var{}", base_index + offset);
                                new_params.push((param_name, Some(tuple_element.clone())));
                            }
                            continue;
                        }
                        new_params.push((origin_param.0.clone(), Some(new_type)));
                    }
                    _ => {}
                },
                VariadicType::Multi(_) => (),
            },
            _ => {
                let new_type = instantiate_type_generic_with_context(context, origin_param_type);
                new_params.push((origin_param.0.clone(), Some(new_type)));
            }
        }
    }

    let mut inst_ret_type = instantiate_type_generic_with_context(context, tpl_ret);
    // 对于可变返回值, 如果实例化是 tuple, 那么我们将展开 tuple
    if let LuaType::Variadic(_) = &&tpl_ret
        && let LuaType::Tuple(tuple) = &inst_ret_type
    {
        match tuple.len() {
            0 => {}
            1 => inst_ret_type = tuple.get_types()[0].clone(),
            _ => {
                inst_ret_type =
                    LuaType::Variadic(VariadicType::Multi(tuple.get_types().to_vec()).into())
            }
        }
    }
    // 重新判断是否是可变参数
    let is_variadic = new_params
        .last()
        .is_some_and(|(name, ty)| match name.as_str() {
            "..." => !ty.as_ref().is_some_and(
                |ty| matches!(ty, LuaType::Variadic(variadic) if variadic.get_max_len().is_some()),
            ),
            _ => ty.as_ref().is_some_and(
                |ty| matches!(ty, LuaType::Variadic(variadic) if variadic.get_max_len().is_none()),
            ),
        });

    LuaType::DocFunction(
        LuaFunctionType::new(
            async_state,
            colon_define,
            is_variadic,
            new_params,
            inst_ret_type,
        )
        .into(),
    )
}

fn instantiate_object(context: &GenericInstantiateContext, object: &LuaObjectType) -> LuaType {
    let new_fields = object
        .get_fields()
        .iter()
        .map(|(key, field)| {
            (
                key.clone(),
                instantiate_type_generic_with_context(context, field),
            )
        })
        .collect::<HashMap<_, _>>();

    let new_index_access = instantiate_type_pairs(context, object.get_index_access().iter());

    LuaType::Object(LuaObjectType::new_with_fields(new_fields, new_index_access).into())
}

fn instantiate_union(context: &GenericInstantiateContext, union: &LuaUnionType) -> LuaType {
    LuaType::from_vec(instantiate_types(context, union.into_vec().iter()))
}

fn instantiate_intersection(
    context: &GenericInstantiateContext,
    intersection: &LuaIntersectionType,
) -> LuaType {
    LuaType::Intersection(
        LuaIntersectionType::new(instantiate_types(context, intersection.get_types().iter()))
            .into(),
    )
}

pub fn instantiate_generic(
    db: &DbIndex,
    generic: &LuaGenericType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let context = GenericInstantiateContext::new(db, substitutor);
    instantiate_generic_with_context(&context, generic)
}

fn instantiate_generic_with_context(
    context: &GenericInstantiateContext,
    generic: &LuaGenericType,
) -> LuaType {
    let generic_params = generic.get_params();
    let new_params = instantiate_types(context, generic_params.iter());

    let base = generic.get_base_type();
    let type_decl_id = if let LuaType::Ref(id) = base {
        id
    } else {
        return LuaType::Unknown;
    };

    if !context.substitutor.check_recursion(&type_decl_id)
        && let Some(type_decl) = context.db.get_type_index().get_type_decl(&type_decl_id)
        && type_decl.is_alias()
    {
        let new_substitutor = TypeSubstitutor::from_alias(new_params.clone(), type_decl_id.clone());
        if let Some(origin) = type_decl.get_alias_origin(context.db, Some(&new_substitutor)) {
            return origin;
        }
    }

    LuaType::Generic(LuaGenericType::new(type_decl_id, new_params).into())
}

fn instantiate_table_generic(
    context: &GenericInstantiateContext,
    table_params: &[LuaType],
) -> LuaType {
    LuaType::TableGeneric(instantiate_types(context, table_params.iter()).into())
}

fn instantiate_uninferred_tpl_fallback(
    tpl: &GenericTpl,
    context: &GenericInstantiateContext,
) -> LuaType {
    // 一些情况下需要保留 TplRef, 例如高阶函数调用
    if context.should_preserve_tpl_ref() && tpl.get_default_type().is_none() {
        return LuaType::TplRef(tpl.clone().into());
    }

    // 显式默认值优先, 然后是 extends 约束, 最后才是 unknown.
    if let Some(default_type) = tpl.get_default_type() {
        return instantiate_type_generic_with_context(context, default_type);
    }

    if let Some(constraint) = tpl.get_constraint() {
        return instantiate_type_generic_with_context(context, constraint);
    }

    LuaType::Unknown
}

fn instantiate_tpl_ref(tpl: &GenericTpl, context: &GenericInstantiateContext) -> LuaType {
    if let Some(value) = context.substitutor.get(tpl.get_tpl_id()) {
        match value {
            SubstitutorValue::None => {
                return instantiate_uninferred_tpl_fallback(tpl, context);
            }
            SubstitutorValue::Type(ty) => return ty.default().clone(),
            SubstitutorValue::MultiTypes(types) => {
                return LuaType::Variadic(VariadicType::Multi(types.clone()).into());
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

fn instantiate_const_tpl_ref(tpl: &GenericTpl, context: &GenericInstantiateContext) -> LuaType {
    if let Some(value) = context.substitutor.get(tpl.get_tpl_id()) {
        match value {
            SubstitutorValue::None => {
                return instantiate_uninferred_tpl_fallback(tpl, context);
            }
            SubstitutorValue::Type(ty) => return ty.raw().clone(),
            SubstitutorValue::MultiTypes(types) => {
                return LuaType::Variadic(VariadicType::Multi(types.clone()).into());
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

    LuaType::ConstTplRef(tpl.clone().into())
}

fn instantiate_signature(
    context: &GenericInstantiateContext,
    signature_id: &LuaSignatureId,
) -> LuaType {
    if let Some(signature) = context.db.get_signature_index().get(signature_id) {
        let origin_type = {
            let fake_doc_function = signature.to_doc_func_type();
            instantiate_doc_function_with_context(context, &fake_doc_function)
        };
        if signature.overloads.is_empty() {
            return origin_type;
        } else {
            let mut result = Vec::new();
            for overload in signature.overloads.iter() {
                result.push(instantiate_doc_function_with_context(
                    context,
                    &(*overload).clone(),
                ));
            }
            result.push(origin_type); // 我们需要将原始类型放到最后
            return LuaType::from_vec(result);
        }
    }

    LuaType::Signature(*signature_id)
}

fn instantiate_variadic_type(
    context: &GenericInstantiateContext,
    variadic: &VariadicType,
) -> LuaType {
    match variadic {
        VariadicType::Base(base) => match base {
            LuaType::TplRef(tpl) => {
                if let Some(value) = context.substitutor.get(tpl.get_tpl_id()) {
                    match value {
                        SubstitutorValue::None => {
                            let fallback = instantiate_uninferred_tpl_fallback(tpl, context);
                            return match fallback {
                                LuaType::Variadic(_) | LuaType::Never => fallback,
                                LuaType::Nil | LuaType::Any | LuaType::Unknown => fallback,
                                _ => LuaType::Variadic(VariadicType::Base(fallback).into()),
                            };
                        }
                        SubstitutorValue::Type(ty) => {
                            let resolved_type = ty.default();
                            if matches!(
                                resolved_type,
                                LuaType::Nil | LuaType::Any | LuaType::Unknown | LuaType::Never
                            ) {
                                return resolved_type.clone();
                            }
                            return LuaType::Variadic(
                                VariadicType::Base(resolved_type.clone()).into(),
                            );
                        }
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
            LuaType::Generic(generic) => {
                return instantiate_generic_with_context(context, generic);
            }
            _ => {}
        },
        VariadicType::Multi(types) => {
            if types.iter().any(LuaTypeNode::contains_tpl_node) {
                let mut new_types = Vec::new();
                for t in types {
                    let t = instantiate_type_generic_with_context(context, t);
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

fn instantiate_mapped_type(context: &GenericInstantiateContext, mapped: &LuaMappedType) -> LuaType {
    let constraint = mapped
        .param
        .1
        .type_constraint
        .as_ref()
        .map(|ty| instantiate_type_generic_with_context(context, ty));

    if let Some(constraint) = constraint {
        let mut key_types = Vec::new();
        collect_mapped_key_atoms(&constraint, &mut key_types);

        let mut visited = HashSet::new();
        let mut fields: Vec<(LuaMemberKey, LuaType)> = Vec::new();
        let mut index_access: Vec<(LuaType, LuaType)> = Vec::new();

        for key_ty in key_types {
            if !visited.insert(key_ty.clone()) {
                continue;
            }

            let value_ty = instantiate_mapped_value(context, mapped, mapped.param.0, &key_ty);

            if let Some(member_key) = key_type_to_member_key(&key_ty) {
                if let Some((_, existing)) = fields.iter_mut().find(|(key, _)| key == &member_key) {
                    let merged = LuaType::from_vec(vec![existing.clone(), value_ty]);
                    *existing = merged;
                } else {
                    fields.push((member_key, value_ty));
                }
            } else {
                index_access.push((key_ty, value_ty));
            }
        }

        if !fields.is_empty() || !index_access.is_empty() {
            // key 从 0 开始递增才被视为元组
            if constraint.is_tuple() {
                let mut index = 0;
                let mut is_tuple = true;
                for (key, _) in &fields {
                    if let LuaMemberKey::Integer(i) = key {
                        if *i != index {
                            is_tuple = false;
                            break;
                        }
                        index += 1;
                    } else {
                        is_tuple = false;
                        break;
                    }
                }
                if is_tuple {
                    let types = fields.into_iter().map(|(_, ty)| ty).collect();
                    return LuaType::Tuple(
                        LuaTupleType::new(types, LuaTupleStatus::InferResolve).into(),
                    );
                }
            }
            let field_map: HashMap<LuaMemberKey, LuaType> = fields.into_iter().collect();
            return LuaType::Object(LuaObjectType::new_with_fields(field_map, index_access).into());
        }
    }

    instantiate_type_generic_with_context(context, &mapped.value)
}

fn instantiate_mapped_value(
    context: &GenericInstantiateContext,
    mapped: &LuaMappedType,
    tpl_id: GenericTplId,
    replacement: &LuaType,
) -> LuaType {
    let mut local_substitutor = context.substitutor.clone();
    local_substitutor.insert_type(tpl_id, replacement.clone(), true);
    let local_context = context.with_substitutor(&local_substitutor);
    let mut result = instantiate_type_generic_with_context(&local_context, &mapped.value);
    // 根据 readonly 和 optional 属性进行处理
    if mapped.is_optional {
        result = TypeOps::Union.apply(context.db, &result, &LuaType::Nil);
    }
    // TODO: 处理 readonly, 但目前 readonly 的实现存在问题, 这里我们先跳过

    result
}

pub(super) fn key_type_to_member_key(key_ty: &LuaType) -> Option<LuaMemberKey> {
    match key_ty {
        LuaType::DocStringConst(s) => Some(LuaMemberKey::Name(s.deref().clone())),
        LuaType::StringConst(s) => Some(LuaMemberKey::Name(s.deref().clone())),
        LuaType::DocIntegerConst(i) => Some(LuaMemberKey::Integer(*i)),
        LuaType::IntegerConst(i) => Some(LuaMemberKey::Integer(*i)),
        _ => None,
    }
}

fn collect_mapped_key_atoms(key_ty: &LuaType, acc: &mut Vec<LuaType>) {
    match key_ty {
        LuaType::Union(union) => {
            for member in union.into_vec() {
                collect_mapped_key_atoms(&member, acc);
            }
        }
        LuaType::MultiLineUnion(multi) => {
            for (member, _) in multi.get_unions() {
                collect_mapped_key_atoms(member, acc);
            }
        }
        LuaType::Variadic(variadic) => match variadic.deref() {
            VariadicType::Base(base) => collect_mapped_key_atoms(base, acc),
            VariadicType::Multi(types) => {
                for member in types {
                    collect_mapped_key_atoms(member, acc);
                }
            }
        },
        LuaType::Tuple(tuple) => {
            for member in tuple.get_types() {
                collect_mapped_key_atoms(member, acc);
            }
        }
        LuaType::Unknown | LuaType::Never => {}
        _ => acc.push(key_ty.clone()),
    }
}

pub(super) fn get_default_constructor(db: &DbIndex, decl_id: &LuaTypeDeclId) -> Option<LuaType> {
    let ids = db
        .get_operator_index()
        .get_operators(&decl_id.clone().into(), LuaOperatorMetaMethod::Call)?;

    let id = ids.first()?;
    let operator = db.get_operator_index().get_operator(id)?;
    Some(operator.get_operator_func(db))
}
