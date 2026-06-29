mod complete_generic_args;
mod instantiate_conditional_generic;
mod instantiate_special_generic;

use hashbrown::{HashMap, HashSet};
use std::ops::Deref;

use smol_str::SmolStr;

use crate::{
    DbIndex, GenericTpl, GenericTplId, LuaArrayType, LuaMappedType, LuaMemberKey,
    LuaOperatorMetaMethod, LuaSignatureId, LuaTupleStatus, LuaTupleType, LuaTypeDeclId,
    LuaTypeNode, TypeOps,
    db_index::{
        LuaFunctionType, LuaGenericType, LuaIntersectionType, LuaObjectType, LuaType, LuaUnionType,
        VariadicType,
    },
};

use super::type_substitutor::{
    GenericCandidate, GenericInstantiateContext, GenericResolveMode, LiteralPolicy,
    SubstitutorValue, TypeSubstitutor,
};
pub use complete_generic_args::{
    GenericArgumentCompletion, complete_type_generic_args, complete_type_generic_args_in_type,
};
pub use instantiate_special_generic::get_keyof_members;

pub fn instantiate_type_generic(
    db: &DbIndex,
    ty: &LuaType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    instantiate_type_generic_full(db, ty, substitutor, GenericResolveMode::Value)
}

pub fn instantiate_type_generic_full(
    db: &DbIndex,
    ty: &LuaType,
    substitutor: &TypeSubstitutor,
    resolve_mode: GenericResolveMode,
) -> LuaType {
    let context = GenericInstantiateContext::new(db, substitutor);
    let context = context.with_resolve_mode(resolve_mode);
    match ty {
        LuaType::DocFunction(doc_func) => {
            let signature_context = context.with_resolve_mode(GenericResolveMode::Value);
            instantiate_doc_function_with_context(&signature_context, doc_func)
        }
        _ => instantiate_type_generic_inner(&context, ty),
    }
}

pub(super) fn instantiate_type_generic_inner(
    context: &GenericInstantiateContext,
    ty: &LuaType,
) -> LuaType {
    match ty {
        LuaType::Array(array_type) => instantiate_array(context, array_type.get_base()),
        LuaType::Tuple(tuple) => instantiate_tuple(context, tuple),
        LuaType::DocFunction(doc_func) => {
            let signature_context = context.with_resolve_mode(GenericResolveMode::Value);
            instantiate_nested_doc_function(&signature_context, doc_func)
        }
        LuaType::Object(object) => instantiate_object(context, object),
        LuaType::Union(union) => instantiate_union(context, union),
        LuaType::Intersection(intersection) => instantiate_intersection(context, intersection),
        LuaType::Generic(generic) => instantiate_generic_type(context, generic),
        LuaType::TableGeneric(table_params) => instantiate_table_generic(context, table_params),
        LuaType::TplRef(tpl) => instantiate_tpl_ref(tpl, context),
        LuaType::Signature(sig_id) => {
            let signature_context = context.with_resolve_mode(GenericResolveMode::Value);
            instantiate_signature(&signature_context, sig_id)
        }
        LuaType::Call(alias_call) => {
            instantiate_special_generic::instantiate_alias_call(context, alias_call)
        }
        LuaType::Variadic(variadic) => {
            let variadic_context = context.with_resolve_mode(GenericResolveMode::Value);
            instantiate_variadic_type(&variadic_context, variadic)
        }
        LuaType::SelfInfer => {
            if let Some(typ) = context.substitutor.get_self_type() {
                typ.clone()
            } else {
                LuaType::SelfInfer
            }
        }
        LuaType::TypeGuard(guard) => {
            let inner = instantiate_type_generic_inner(context, guard.deref());
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
        .map(|ty| instantiate_type_generic_inner(context, ty))
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
                instantiate_type_generic_inner(context, key),
                instantiate_type_generic_inner(context, value),
            )
        })
        .collect()
}

fn instantiate_array(context: &GenericInstantiateContext, base: &LuaType) -> LuaType {
    let base = instantiate_type_generic_inner(context, base);
    LuaType::Array(LuaArrayType::from_base_type(base).into())
}

fn instantiate_tuple(context: &GenericInstantiateContext, tuple: &LuaTupleType) -> LuaType {
    let mut new_types = Vec::new();
    for t in tuple.get_types() {
        if let LuaType::Variadic(inner) = t {
            let variadic_context = context.with_resolve_mode(GenericResolveMode::Value);
            match inner.deref() {
                VariadicType::Base(base) => {
                    if let LuaType::TplRef(tpl) = base {
                        if let Some(value) = context.substitutor.get(tpl.get_tpl_id()) {
                            match value {
                                SubstitutorValue::None => new_types.push(
                                    instantiate_uninferred_tpl_fallback(tpl, &variadic_context),
                                ),
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
                                SubstitutorValue::Type(ty) => new_types.push(
                                    substitutor_type_for_tpl(&variadic_context, tpl, ty).clone(),
                                ),
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

        let t = instantiate_type_generic_inner(context, t);
        new_types.push(t);
    }
    LuaType::Tuple(LuaTupleType::new(new_types, tuple.status).into())
}

fn instantiate_doc_function_with_context(
    context: &GenericInstantiateContext,
    doc_func: &LuaFunctionType,
) -> LuaType {
    let tpl_func_params = doc_func.get_params();
    let tpl_ret = doc_func.get_ret();
    let async_state = doc_func.get_async_state();
    let colon_define = doc_func.is_colon_define();
    let generic_params = instantiate_function_generic_params(context, doc_func);

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
                        let variadic_context = context.with_resolve_mode(GenericResolveMode::Value);
                        if let Some(value) = context.substitutor.get(tpl.get_tpl_id()) {
                            match value {
                                SubstitutorValue::None => {
                                    let ty =
                                        instantiate_uninferred_tpl_fallback(tpl, &variadic_context);
                                    new_params.push((origin_param.0.clone(), Some(ty)));
                                }
                                SubstitutorValue::Type(ty) => {
                                    let resolved_type =
                                        substitutor_type_for_tpl(&variadic_context, tpl, ty);
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
                        let new_type = instantiate_generic_type(context, generic);
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
                let new_type = instantiate_type_generic_inner(context, origin_param_type);
                new_params.push((origin_param.0.clone(), Some(new_type)));
            }
        }
    }

    let mut inst_ret_type = instantiate_type_generic_inner(context, tpl_ret);
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
            Some(generic_params),
        )
        .into(),
    )
}

fn instantiate_nested_doc_function(
    context: &GenericInstantiateContext,
    doc_func: &LuaFunctionType,
) -> LuaType {
    let mut transferred_params = Vec::new();
    let mut transferred_tpls = HashSet::new();
    collect_pending_function_generic_params(
        context,
        doc_func,
        &mut transferred_params,
        &mut transferred_tpls,
    );

    if transferred_tpls.is_empty() {
        return instantiate_doc_function_with_context(context, doc_func);
    }

    let mut generic_params = doc_func.get_generic_params().to_vec();
    for generic_param in transferred_params {
        if generic_params
            .iter()
            .any(|tpl| tpl.get_tpl_id() == generic_param.get_tpl_id())
        {
            continue;
        }

        generic_params.push(generic_param);
    }

    let nested_substitutor = context
        .substitutor
        .without_pending_tpls(|tpl_id| transferred_tpls.contains(&tpl_id));
    let nested_context = context.with_substitutor(&nested_substitutor);
    let doc_func = LuaFunctionType::new(
        doc_func.get_async_state(),
        doc_func.is_colon_define(),
        doc_func.is_variadic(),
        doc_func.get_params().to_vec(),
        doc_func.get_ret().clone(),
        Some(generic_params),
    );
    instantiate_doc_function_with_context(&nested_context, &doc_func)
}

fn collect_pending_function_generic_params(
    context: &GenericInstantiateContext,
    doc_func: &LuaFunctionType,
    generic_params: &mut Vec<GenericTpl>,
    generic_tpls: &mut HashSet<GenericTplId>,
) {
    for generic_tpl in doc_func.get_generic_params() {
        let tpl_id = generic_tpl.get_tpl_id();
        if is_pending_tpl(context, tpl_id) && generic_tpls.insert(tpl_id) {
            generic_params.push(generic_tpl.clone());
        }
    }

    doc_func.visit_nested_types(&mut |ty| match ty {
        LuaType::TplRef(tpl) => {
            let tpl_id = tpl.get_tpl_id();
            if is_pending_tpl(context, tpl_id) && generic_tpls.insert(tpl_id) {
                generic_params.push(tpl.as_ref().clone());
            }
        }
        LuaType::StrTplRef(str_tpl) => {
            let tpl_id = str_tpl.get_tpl_id();
            if is_pending_tpl(context, tpl_id) && generic_tpls.insert(tpl_id) {
                generic_params.push(GenericTpl::new(
                    tpl_id,
                    SmolStr::new(str_tpl.get_name()),
                    str_tpl.get_constraint().cloned(),
                    None,
                    false,
                    None,
                ));
            }
        }
        _ => {}
    });
}

fn is_pending_tpl(context: &GenericInstantiateContext, tpl_id: GenericTplId) -> bool {
    matches!(
        context.substitutor.get(tpl_id),
        Some(SubstitutorValue::None)
    )
}

fn instantiate_function_generic_params(
    context: &GenericInstantiateContext,
    doc_func: &LuaFunctionType,
) -> Vec<GenericTpl> {
    doc_func
        .get_generic_params()
        .iter()
        .filter_map(|generic_tpl| {
            let tpl_id = generic_tpl.get_tpl_id();
            let param = generic_tpl.get_param();
            // substitutor 中存在该泛型时, 说明它有实际类型, 无需保留.
            if context.substitutor.get(tpl_id).is_some() {
                return None;
            }

            // 对约束与默认值做一次实例化尝试以传递给后续.
            let constraint = param
                .constraint
                .as_ref()
                .map(|ty| instantiate_type_generic_inner(context, ty));
            let default_type = param
                .default
                .as_ref()
                .map(|ty| instantiate_type_generic_inner(context, ty));
            Some(GenericTpl::new(
                tpl_id,
                param.name.clone(),
                constraint,
                default_type,
                param.is_const,
                param.attributes.clone(),
            ))
        })
        .collect()
}

fn instantiate_object(context: &GenericInstantiateContext, object: &LuaObjectType) -> LuaType {
    let new_fields = object
        .get_fields()
        .iter()
        .map(|(key, field)| (key.clone(), instantiate_type_generic_inner(context, field)))
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

fn instantiate_generic_type(
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
    // 显式默认值优先, 然后是 extends 约束, 最后才是 unknown.
    if let Some(default_type) = tpl.get_default_type() {
        return instantiate_type_generic_inner(context, default_type);
    }

    if let Some(constraint) = tpl.get_constraint() {
        return instantiate_type_generic_inner(context, constraint);
    }

    LuaType::Unknown
}

fn instantiate_tpl_ref(tpl: &GenericTpl, context: &GenericInstantiateContext) -> LuaType {
    if let Some(value) = context.substitutor.get(tpl.get_tpl_id()) {
        match value {
            SubstitutorValue::None => {
                return instantiate_uninferred_tpl_fallback(tpl, context);
            }
            SubstitutorValue::Type(ty) => {
                return substitutor_type_for_tpl(context, tpl, ty).clone();
            }
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

fn substitutor_type_for_tpl<'a>(
    context: &GenericInstantiateContext,
    tpl: &GenericTpl,
    value: &'a GenericCandidate,
) -> &'a LuaType {
    value.resolve(context.resolve_mode, tpl.is_const())
}

fn instantiate_signature(
    context: &GenericInstantiateContext,
    signature_id: &LuaSignatureId,
) -> LuaType {
    // Substitution can make a signature mention itself again through its return
    // type, e.g. `pairs(self)` on a table that also contains the method.
    // Leave the nested occurrence opaque instead of expanding forever.
    let Some(_signature_guard) = context.enter_signature(*signature_id) else {
        return LuaType::Signature(*signature_id);
    };

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
            LuaType::TplRef(tpl) => match context.substitutor.get(tpl.get_tpl_id()) {
                Some(SubstitutorValue::Type(ty)) => {
                    let resolved_type = substitutor_type_for_tpl(context, tpl, ty);
                    if matches!(
                        resolved_type,
                        LuaType::Nil | LuaType::Any | LuaType::Unknown | LuaType::Never
                    ) {
                        return resolved_type.clone();
                    }
                    return LuaType::Variadic(VariadicType::Base(resolved_type.clone()).into());
                }
                Some(SubstitutorValue::MultiTypes(types)) => {
                    return LuaType::Variadic(VariadicType::Multi(types.clone()).into());
                }
                Some(SubstitutorValue::Params(params)) => {
                    let types = params
                        .iter()
                        .filter_map(|(_, ty)| ty.clone())
                        .collect::<Vec<_>>();
                    return LuaType::Variadic(VariadicType::Multi(types).into());
                }
                Some(SubstitutorValue::MultiBase(base)) => {
                    return LuaType::Variadic(VariadicType::Base(base.clone()).into());
                }
                Some(SubstitutorValue::None) | None => {
                    let fallback = instantiate_uninferred_tpl_fallback(tpl, context);
                    return match fallback {
                        LuaType::Variadic(_) | LuaType::Never => fallback,
                        LuaType::Nil | LuaType::Any | LuaType::Unknown => fallback,
                        _ => LuaType::Variadic(VariadicType::Base(fallback).into()),
                    };
                }
            },
            LuaType::Generic(generic) => {
                return instantiate_generic_type(context, generic);
            }
            _ => {}
        },
        VariadicType::Multi(types) => {
            if types.iter().any(LuaTypeNode::contains_tpl_node) {
                let mut new_types = Vec::new();
                for t in types {
                    let t = instantiate_type_generic_inner(context, t);
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
    let key_context = context.with_resolve_mode(GenericResolveMode::Literal);
    let constraint = mapped
        .param
        .1
        .constraint
        .as_ref()
        .map(|ty| instantiate_type_generic_inner(&key_context, ty));

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

    instantiate_type_generic_inner(context, &mapped.value)
}

fn instantiate_mapped_value(
    context: &GenericInstantiateContext,
    mapped: &LuaMappedType,
    tpl_id: GenericTplId,
    replacement: &LuaType,
) -> LuaType {
    let mut local_substitutor = context.substitutor.clone();
    local_substitutor.insert_value(
        tpl_id,
        SubstitutorValue::Type(GenericCandidate::new(
            replacement.clone(),
            LiteralPolicy::Preserve,
        )),
    );
    let local_context = context.with_substitutor(&local_substitutor);
    let local_context = local_context.with_resolve_mode(GenericResolveMode::Literal);
    let mut result = instantiate_type_generic_inner(&local_context, &mapped.value);
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
