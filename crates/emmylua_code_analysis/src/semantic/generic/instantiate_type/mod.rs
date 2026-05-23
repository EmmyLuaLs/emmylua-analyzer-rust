mod complete_generic_args;
mod context;
mod infer_call_func_generic;
mod inference_widening;
mod instantiate_conditional_generic;
mod instantiate_mapped_type;
mod instantiate_special_generic;

use hashbrown::HashMap;
use std::ops::Deref;

use crate::{
    DbIndex, GenericTpl, LuaArrayType, LuaMemberKey, LuaOperatorMetaMethod, LuaSignatureId,
    LuaTupleType, LuaTypeDeclId, LuaTypeNode,
    db_index::{
        LuaFunctionType, LuaGenericType, LuaIntersectionType, LuaObjectType, LuaType, LuaUnionType,
        VariadicType,
    },
};

use super::{TypeMapper, TypeMapperValue, get_mapped_value};
pub use complete_generic_args::{
    GenericArgumentCompletion, complete_type_generic_args, complete_type_generic_args_in_type,
};
pub use context::TplResolvePolicy;
use context::{GenericInstantiateContext, GenericInstantiateFrame};
pub use infer_call_func_generic::{build_self_type, infer_call_func_generic, infer_self_type};
pub(in crate::semantic::generic) use inference_widening::{
    is_primitive_or_literal_type, regularize_tpl_candidate_type, widen_tpl_candidate_type,
};
use instantiate_mapped_type::instantiate_mapped_type as instantiate_mapped_type_inner;
pub use instantiate_special_generic::get_keyof_members;

pub fn instantiate_type_generic(db: &DbIndex, ty: &LuaType, mapper: &TypeMapper) -> LuaType {
    instantiate_type_generic_full(db, ty, mapper, None, TplResolvePolicy::Fallback)
}

pub fn instantiate_type_generic_full(
    db: &DbIndex,
    ty: &LuaType,
    mapper: &TypeMapper,
    self_type: Option<&LuaType>,
    root_policy: TplResolvePolicy,
) -> LuaType {
    let context = GenericInstantiateContext::new(db, mapper, self_type);
    let frame = context.root_frame().with_policy(root_policy);
    match ty {
        LuaType::DocFunction(doc_func) => instantiate_doc_function(&context, frame, doc_func),
        _ => instantiate_type_generic_inner(&context, frame, ty),
    }
}

pub(super) fn instantiate_type_generic_inner(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    ty: &LuaType,
) -> LuaType {
    if is_simple_instantiate_leaf(ty) {
        return ty.clone();
    }

    let Some(frame) = frame.enter() else {
        return ty.clone();
    };

    match ty {
        LuaType::Array(array_type) => {
            if !requires_instantiation_walk(ty) {
                return ty.clone();
            }
            instantiate_array(context, frame, array_type.get_base())
        }
        LuaType::Tuple(tuple) => {
            if !requires_instantiation_walk(ty) {
                return ty.clone();
            }
            instantiate_tuple(context, frame, tuple)
        }
        LuaType::DocFunction(doc_func) => {
            if !requires_instantiation_walk(ty) {
                return ty.clone();
            }
            instantiate_doc_function(
                context,
                frame.with_policy(TplResolvePolicy::PreserveTplRef),
                doc_func,
            )
        }
        LuaType::Object(object) => {
            if !requires_instantiation_walk(ty) {
                return ty.clone();
            }
            instantiate_object(context, frame, object)
        }
        LuaType::Union(union) => {
            if !requires_instantiation_walk(ty) {
                return ty.clone();
            }
            instantiate_union(context, frame, union)
        }
        LuaType::Intersection(intersection) => {
            if !requires_instantiation_walk(ty) {
                return ty.clone();
            }
            instantiate_intersection(context, frame, intersection)
        }
        LuaType::Generic(generic) => instantiate_generic(context, frame, generic),
        LuaType::TableGeneric(table_params) => {
            if !requires_instantiation_walk(ty) {
                return ty.clone();
            }
            instantiate_table_generic(context, frame, table_params)
        }
        LuaType::TplRef(tpl) => instantiate_tpl_ref(tpl, context, frame),
        LuaType::ConstTplRef(tpl) => instantiate_const_tpl_ref(tpl, context, frame),
        LuaType::Signature(sig_id) => instantiate_signature(context, frame, sig_id),
        LuaType::Call(alias_call) => {
            instantiate_special_generic::instantiate_alias_call(context, frame, alias_call)
        }
        LuaType::Variadic(variadic) => instantiate_variadic_type(context, frame, variadic),
        LuaType::SelfInfer => {
            if let Some(typ) = context.self_type() {
                typ.clone()
            } else {
                LuaType::SelfInfer
            }
        }
        LuaType::TypeGuard(guard) => {
            if !requires_instantiation_walk(ty) {
                return ty.clone();
            }
            let inner = instantiate_type_generic_inner(context, frame, guard.deref());
            LuaType::TypeGuard(inner.into())
        }
        LuaType::Conditional(conditional) => {
            instantiate_conditional_generic::instantiate_conditional(context, frame, conditional)
        }
        LuaType::Mapped(mapped) => instantiate_mapped_type_inner(context, frame, mapped.deref()),
        _ => ty.clone(),
    }
}

fn requires_instantiation_walk(ty: &LuaType) -> bool {
    match ty {
        LuaType::TplRef(_)
        | LuaType::StrTplRef(_)
        | LuaType::ConstTplRef(_)
        | LuaType::SelfInfer
        | LuaType::Generic(_)
        | LuaType::Signature(_)
        | LuaType::Call(_)
        | LuaType::Conditional(_)
        | LuaType::Mapped(_) => true,
        LuaType::Array(array_type) => requires_instantiation_walk(array_type.get_base()),
        LuaType::Tuple(tuple) => tuple.any_type(requires_instantiation_walk),
        LuaType::DocFunction(doc_func) => doc_func.any_type(requires_instantiation_walk),
        LuaType::Object(object) => object.any_type(requires_instantiation_walk),
        LuaType::Union(union) => union.any_type(requires_instantiation_walk),
        LuaType::Intersection(intersection) => intersection.any_type(requires_instantiation_walk),
        LuaType::TableGeneric(table_params) => table_params.iter().any(requires_instantiation_walk),
        LuaType::Variadic(variadic) => variadic.any_type(requires_instantiation_walk),
        LuaType::TypeGuard(guard) => requires_instantiation_walk(guard.deref()),
        LuaType::MultiLineUnion(inner) => inner.any_type(requires_instantiation_walk),
        LuaType::DocAttribute(attr) => attr.any_type(requires_instantiation_walk),
        _ => false,
    }
}

fn instantiate_types<'a, I>(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    types: I,
) -> Vec<LuaType>
where
    I: IntoIterator<Item = &'a LuaType>,
{
    types
        .into_iter()
        .map(|ty| instantiate_type_generic_inner(context, frame, ty))
        .collect()
}

fn instantiate_type_pairs<'a, I>(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    pairs: I,
) -> Vec<(LuaType, LuaType)>
where
    I: IntoIterator<Item = &'a (LuaType, LuaType)>,
{
    pairs
        .into_iter()
        .map(|(key, value)| {
            (
                instantiate_type_generic_inner(context, frame, key),
                instantiate_type_generic_inner(context, frame, value),
            )
        })
        .collect()
}

fn instantiate_array(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    base: &LuaType,
) -> LuaType {
    let base = instantiate_type_generic_inner(context, frame, base);
    LuaType::Array(LuaArrayType::from_base_type(base).into())
}

fn instantiate_tuple(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    tuple: &LuaTupleType,
) -> LuaType {
    let mut new_types = Vec::new();
    for t in tuple.get_types() {
        if let LuaType::Variadic(inner) = t {
            match inner.deref() {
                VariadicType::Base(base) => {
                    if let LuaType::TplRef(tpl) = base {
                        if let Some(value) = get_mapped_value(tpl.get_tpl_id(), &context.mapper) {
                            match value {
                                TypeMapperValue::None => new_types
                                    .push(instantiate_uninferred_tpl_fallback(tpl, context, frame)),
                                TypeMapperValue::Params(params) => {
                                    for (_, ty) in params {
                                        new_types.push(ty.unwrap_or(LuaType::Unknown));
                                    }
                                }
                                TypeMapperValue::MultiTypes(values) => {
                                    new_types.extend(values);
                                }
                                TypeMapperValue::Type(value) => new_types.push(value),
                                TypeMapperValue::MultiBase(base) => new_types.push(base),
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

        let t = instantiate_type_generic_inner(context, frame, t);
        new_types.push(t);
    }
    LuaType::Tuple(LuaTupleType::new(new_types, tuple.status).into())
}

fn instantiate_doc_function(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    doc_func: &LuaFunctionType,
) -> LuaType {
    if !doc_func.any_type(requires_instantiation_walk) {
        return LuaType::DocFunction(doc_func.clone().into());
    }

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
                    LuaType::TplRef(tpl) | LuaType::ConstTplRef(tpl) => {
                        if let Some(value) = get_mapped_value(tpl.get_tpl_id(), &context.mapper) {
                            match value {
                                TypeMapperValue::None => {
                                    let ty =
                                        instantiate_uninferred_tpl_fallback(tpl, context, frame);
                                    new_params.push((origin_param.0.clone(), Some(ty)));
                                }
                                TypeMapperValue::Type(resolved_type) => {
                                    // 如果参数是 `...: T...`
                                    if origin_param.0 == "..." {
                                        // 类型是 tuple, 那么我们将展开 tuple
                                        if let LuaType::Tuple(tuple) = &resolved_type {
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
                                            VariadicType::Base(resolved_type).into(),
                                        )),
                                    ));
                                }
                                TypeMapperValue::Params(params) => {
                                    for param in params {
                                        new_params.push(param);
                                    }
                                }
                                TypeMapperValue::MultiTypes(values) => {
                                    for (i, value) in values.into_iter().enumerate() {
                                        let param_name = format!("var{}", i);
                                        new_params.push((param_name, Some(value)));
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
                        let new_type = instantiate_generic(context, frame, generic);
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
                let new_type = instantiate_type_generic_inner(context, frame, origin_param_type);
                new_params.push((origin_param.0.clone(), Some(new_type)));
            }
        }
    }

    let mut inst_ret_type = instantiate_type_generic_inner(context, frame, tpl_ret);
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

fn instantiate_object(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    object: &LuaObjectType,
) -> LuaType {
    if !object.any_type(requires_instantiation_walk) {
        return LuaType::Object(object.clone().into());
    }

    let new_fields = object
        .get_fields()
        .iter()
        .map(|(key, field)| {
            (
                key.clone(),
                instantiate_type_generic_inner(context, frame, field),
            )
        })
        .collect::<HashMap<_, _>>();

    let new_index_access = instantiate_type_pairs(context, frame, object.get_index_access().iter());

    LuaType::Object(LuaObjectType::new_with_fields(new_fields, new_index_access).into())
}

fn instantiate_union(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    union: &LuaUnionType,
) -> LuaType {
    if !union.any_type(requires_instantiation_walk) {
        return LuaType::Union(union.clone().into());
    }

    LuaType::from_vec(instantiate_types(context, frame, union.into_vec().iter()))
}

fn instantiate_intersection(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    intersection: &LuaIntersectionType,
) -> LuaType {
    if !intersection.any_type(requires_instantiation_walk) {
        return LuaType::Intersection(intersection.clone().into());
    }

    LuaType::Intersection(
        LuaIntersectionType::new(instantiate_types(
            context,
            frame,
            intersection.get_types().iter(),
        ))
        .into(),
    )
}

fn instantiate_generic(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    generic: &LuaGenericType,
) -> LuaType {
    let generic_params = generic.get_params();
    let new_params = instantiate_types(context, frame, generic_params.iter());

    let base = generic.get_base_type();
    let type_decl_id = if let LuaType::Ref(id) = base {
        id
    } else {
        return LuaType::Unknown;
    };

    if let Some(type_decl) = context.db.get_type_index().get_type_decl(&type_decl_id)
        && type_decl.is_alias()
    {
        let Some(alias_stack) = context.enter_alias_stack(&type_decl_id) else {
            return LuaType::Generic(LuaGenericType::new(type_decl_id, new_params).into());
        };
        let alias_mapper = TypeMapper::from_alias(context.db, new_params.clone(), &type_decl_id);
        let alias_mapper = TypeMapper::merge(Some(alias_mapper), context.mapper.clone());
        let alias_context = context.with_alias_stack(alias_stack, alias_mapper);
        if let Some(origin) = type_decl.get_alias_ref() {
            return instantiate_type_generic_inner(&alias_context, frame, origin);
        }
    }

    LuaType::Generic(LuaGenericType::new(type_decl_id, new_params).into())
}

fn instantiate_table_generic(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    table_params: &[LuaType],
) -> LuaType {
    if !table_params.iter().any(requires_instantiation_walk) {
        return LuaType::TableGeneric(table_params.to_vec().into());
    }

    LuaType::TableGeneric(instantiate_types(context, frame, table_params.iter()).into())
}

fn instantiate_uninferred_tpl_fallback(
    tpl: &GenericTpl,
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
) -> LuaType {
    // 一些情况下需要保留 TplRef, 例如高阶函数调用
    if frame.should_preserve_tpl_ref() && tpl.get_default_type().is_none() {
        return LuaType::TplRef(tpl.clone().into());
    }

    // 显式默认值优先, 然后是 extends 约束, 最后才是 unknown.
    if let Some(default_type) = tpl.get_default_type() {
        return instantiate_type_generic_inner(context, frame, default_type);
    }

    if let Some(constraint) = tpl.get_constraint() {
        return instantiate_type_generic_inner(context, frame, constraint);
    }

    LuaType::Unknown
}

fn instantiate_tpl_ref(
    tpl: &GenericTpl,
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
) -> LuaType {
    if let Some(value) = get_mapped_value(tpl.get_tpl_id(), &context.mapper) {
        match value {
            TypeMapperValue::None => {
                return instantiate_uninferred_tpl_fallback(tpl, context, frame);
            }
            TypeMapperValue::Type(value) => {
                return value;
            }
            TypeMapperValue::MultiTypes(values) => {
                return LuaType::Variadic(VariadicType::Multi(values).into());
            }
            TypeMapperValue::Params(params) => {
                return params
                    .first()
                    .and_then(|(_, ty)| ty.clone())
                    .unwrap_or(LuaType::Unknown);
            }
            TypeMapperValue::MultiBase(base) => return base,
        }
    }

    LuaType::TplRef(tpl.clone().into())
}

fn instantiate_const_tpl_ref(
    tpl: &GenericTpl,
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
) -> LuaType {
    if let Some(value) = get_mapped_value(tpl.get_tpl_id(), &context.mapper) {
        match value {
            TypeMapperValue::None => {
                if frame.should_preserve_tpl_ref() && tpl.get_default_type().is_none() {
                    return LuaType::ConstTplRef(tpl.clone().into());
                }
                return instantiate_uninferred_tpl_fallback(tpl, context, frame);
            }
            TypeMapperValue::Type(value) => {
                return value;
            }
            TypeMapperValue::MultiTypes(values) => {
                return LuaType::Variadic(VariadicType::Multi(values).into());
            }
            TypeMapperValue::Params(params) => {
                return params
                    .first()
                    .and_then(|(_, ty)| ty.clone())
                    .unwrap_or(LuaType::Unknown);
            }
            TypeMapperValue::MultiBase(base) => return base,
        }
    }

    LuaType::ConstTplRef(tpl.clone().into())
}

fn instantiate_signature(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    signature_id: &LuaSignatureId,
) -> LuaType {
    if let Some(signature) = context.db.get_signature_index().get(signature_id) {
        let origin_type = {
            let fake_doc_function = signature.to_doc_func_type();
            instantiate_doc_function(context, frame, &fake_doc_function)
        };
        if signature.overloads.is_empty() {
            return origin_type;
        } else {
            let mut result = Vec::new();
            for overload in signature.overloads.iter() {
                result.push(instantiate_doc_function(
                    context,
                    frame,
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
    frame: GenericInstantiateFrame,
    variadic: &VariadicType,
) -> LuaType {
    if !variadic.any_type(requires_instantiation_walk) {
        return LuaType::Variadic(variadic.clone().into());
    }

    match variadic {
        VariadicType::Base(base) => match base {
            LuaType::TplRef(tpl) => {
                if let Some(value) = get_mapped_value(tpl.get_tpl_id(), &context.mapper) {
                    match value {
                        TypeMapperValue::None => {
                            let fallback = instantiate_uninferred_tpl_fallback(tpl, context, frame);
                            return match fallback {
                                LuaType::Variadic(_) | LuaType::Never => fallback,
                                LuaType::Nil | LuaType::Any | LuaType::Unknown => fallback,
                                _ => LuaType::Variadic(VariadicType::Base(fallback).into()),
                            };
                        }
                        TypeMapperValue::Type(resolved_type) => {
                            if matches!(
                                resolved_type,
                                LuaType::Nil | LuaType::Any | LuaType::Unknown | LuaType::Never
                            ) {
                                return resolved_type;
                            }
                            return LuaType::Variadic(VariadicType::Base(resolved_type).into());
                        }
                        TypeMapperValue::MultiTypes(values) => {
                            return LuaType::Variadic(VariadicType::Multi(values).into());
                        }
                        TypeMapperValue::Params(params) => {
                            let types = params
                                .into_iter()
                                .filter_map(|(_, ty)| ty)
                                .collect::<Vec<_>>();
                            return LuaType::Variadic(VariadicType::Multi(types).into());
                        }
                        TypeMapperValue::MultiBase(base) => {
                            return LuaType::Variadic(VariadicType::Base(base).into());
                        }
                    }
                } else {
                    return LuaType::Never;
                }
            }
            LuaType::Generic(generic) => {
                return instantiate_generic(context, frame, generic);
            }
            _ => {}
        },
        VariadicType::Multi(types) => {
            if types.iter().any(LuaTypeNode::contains_tpl_node) {
                let mut new_types = Vec::new();
                for t in types {
                    let t = instantiate_type_generic_inner(context, frame, t);
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

pub(super) fn key_type_to_member_key(key_ty: &LuaType) -> Option<LuaMemberKey> {
    match key_ty {
        LuaType::DocStringConst(s) => Some(LuaMemberKey::Name(s.deref().clone())),
        LuaType::StringConst(s) => Some(LuaMemberKey::Name(s.deref().clone())),
        LuaType::DocIntegerConst(i) => Some(LuaMemberKey::Integer(*i)),
        LuaType::IntegerConst(i) => Some(LuaMemberKey::Integer(*i)),
        _ => None,
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

fn is_simple_instantiate_leaf(ty: &LuaType) -> bool {
    matches!(
        ty,
        LuaType::Unknown
            | LuaType::Any
            | LuaType::Nil
            | LuaType::Table
            | LuaType::Userdata
            | LuaType::Function
            | LuaType::Thread
            | LuaType::Boolean
            | LuaType::String
            | LuaType::Integer
            | LuaType::Number
            | LuaType::Io
            | LuaType::Global
            | LuaType::Never
            | LuaType::BooleanConst(_)
            | LuaType::StringConst(_)
            | LuaType::IntegerConst(_)
            | LuaType::FloatConst(_)
            | LuaType::TableConst(_)
            | LuaType::Ref(_)
            | LuaType::Def(_)
            | LuaType::DocStringConst(_)
            | LuaType::DocIntegerConst(_)
            | LuaType::DocBooleanConst(_)
            | LuaType::Namespace(_)
            | LuaType::Language(_)
            | LuaType::ModuleRef(_)
    )
}
