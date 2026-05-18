mod complete_generic_args;
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

use super::type_substitutor::{
    GenericInstantiateContext, GenericInstantiateFrame, SubstitutorValue, TypeSubstitutor,
    UninferredTplPolicy,
};
pub use complete_generic_args::{
    GenericArgumentCompletion, complete_type_generic_args, complete_type_generic_args_in_type,
};
pub use infer_call_func_generic::{build_self_type, infer_call_func_generic, infer_self_type};
pub(in crate::semantic::generic) use inference_widening::{
    is_primitive_or_literal_type, regularize_tpl_candidate_type, widen_tpl_candidate_type,
};
use instantiate_mapped_type::instantiate_mapped_type as instantiate_mapped_type_inner;
pub use instantiate_special_generic::get_keyof_members;

pub fn instantiate_type_generic(
    db: &DbIndex,
    ty: &LuaType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let context = GenericInstantiateContext::new(db, substitutor);
    let frame = context.root_frame();
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
    let Some(frame) = frame.enter() else {
        return ty.clone();
    };

    match ty {
        LuaType::Array(array_type) => instantiate_array(context, frame, array_type.get_base()),
        LuaType::Tuple(tuple) => instantiate_tuple(context, frame, tuple),
        LuaType::DocFunction(doc_func) => instantiate_doc_function(
            context,
            frame.with_policy(UninferredTplPolicy::PreserveTplRef),
            doc_func,
        ),
        LuaType::Object(object) => instantiate_object(context, frame, object),
        LuaType::Union(union) => instantiate_union(context, frame, union),
        LuaType::Intersection(intersection) => {
            instantiate_intersection(context, frame, intersection)
        }
        LuaType::Generic(generic) => instantiate_generic(context, frame, generic),
        LuaType::TableGeneric(table_params) => {
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
            if let Some(typ) = context.substitutor.get_self_type() {
                typ.clone()
            } else {
                LuaType::SelfInfer
            }
        }
        LuaType::TypeGuard(guard) => {
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
                        if let Some(value) = context.substitutor.get(tpl.get_tpl_id()) {
                            match value {
                                SubstitutorValue::None => new_types
                                    .push(instantiate_uninferred_tpl_fallback(tpl, context, frame)),
                                SubstitutorValue::Params(params) => {
                                    for (_, ty) in params {
                                        new_types.push(ty.clone().unwrap_or(LuaType::Unknown));
                                    }
                                }
                                SubstitutorValue::MultiTypes { values, .. } => {
                                    new_types.extend(
                                        values.iter().map(|value| value.resolved().clone()),
                                    );
                                }
                                SubstitutorValue::Type { value, .. } => {
                                    new_types.push(value.resolved().clone())
                                }
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
                        if let Some(value) = context.substitutor.get(tpl.get_tpl_id()) {
                            match value {
                                SubstitutorValue::None => {
                                    let ty =
                                        instantiate_uninferred_tpl_fallback(tpl, context, frame);
                                    new_params.push((origin_param.0.clone(), Some(ty)));
                                }
                                SubstitutorValue::Type { value, .. } => {
                                    let resolved_type = value.resolved().clone();
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
                                SubstitutorValue::Params(params) => {
                                    for param in params.iter() {
                                        new_params.push(param.clone());
                                    }
                                }
                                SubstitutorValue::MultiTypes { values, .. } => {
                                    for (i, value) in values.iter().enumerate() {
                                        let param_name = format!("var{}", i);
                                        new_params
                                            .push((param_name, Some(value.resolved().clone())));
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
    LuaType::from_vec(instantiate_types(context, frame, union.into_vec().iter()))
}

fn instantiate_intersection(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    intersection: &LuaIntersectionType,
) -> LuaType {
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

    if !context.substitutor.check_recursion(&type_decl_id)
        && let Some(type_decl) = context.db.get_type_index().get_type_decl(&type_decl_id)
        && type_decl.is_alias()
    {
        let Some(alias_context) = context.enter_alias(&type_decl_id) else {
            return LuaType::Generic(LuaGenericType::new(type_decl_id, new_params).into());
        };
        let new_substitutor =
            TypeSubstitutor::from_alias(context.db, new_params.clone(), type_decl_id.clone());
        let alias_context = alias_context.with_substitutor(&new_substitutor);
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
    if let Some(value) = context.substitutor.get(tpl.get_tpl_id()) {
        match value {
            SubstitutorValue::None => {
                return instantiate_uninferred_tpl_fallback(tpl, context, frame);
            }
            SubstitutorValue::Type { value, .. } => {
                return value.resolved().clone();
            }
            SubstitutorValue::MultiTypes { values, .. } => {
                return LuaType::Variadic(
                    VariadicType::Multi(
                        values
                            .iter()
                            .map(|value| value.resolved().clone())
                            .collect(),
                    )
                    .into(),
                );
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

fn instantiate_const_tpl_ref(
    tpl: &GenericTpl,
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
) -> LuaType {
    if let Some(value) = context.substitutor.get(tpl.get_tpl_id()) {
        match value {
            SubstitutorValue::None => {
                return instantiate_uninferred_tpl_fallback(tpl, context, frame);
            }
            SubstitutorValue::Type { value, .. } => {
                return value.resolved().clone();
            }
            SubstitutorValue::MultiTypes { values, .. } => {
                return LuaType::Variadic(
                    VariadicType::Multi(
                        values
                            .iter()
                            .map(|value| value.resolved().clone())
                            .collect(),
                    )
                    .into(),
                );
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
    match variadic {
        VariadicType::Base(base) => match base {
            LuaType::TplRef(tpl) => {
                if let Some(value) = context.substitutor.get(tpl.get_tpl_id()) {
                    match value {
                        SubstitutorValue::None => {
                            let fallback = instantiate_uninferred_tpl_fallback(tpl, context, frame);
                            return match fallback {
                                LuaType::Variadic(_) | LuaType::Never => fallback,
                                LuaType::Nil | LuaType::Any | LuaType::Unknown => fallback,
                                _ => LuaType::Variadic(VariadicType::Base(fallback).into()),
                            };
                        }
                        SubstitutorValue::Type { value, .. } => {
                            let resolved_type = value.resolved().clone();
                            if matches!(
                                resolved_type,
                                LuaType::Nil | LuaType::Any | LuaType::Unknown | LuaType::Never
                            ) {
                                return resolved_type;
                            }
                            return LuaType::Variadic(VariadicType::Base(resolved_type).into());
                        }
                        SubstitutorValue::MultiTypes { values, .. } => {
                            return LuaType::Variadic(
                                VariadicType::Multi(
                                    values
                                        .iter()
                                        .map(|value| value.resolved().clone())
                                        .collect(),
                                )
                                .into(),
                            );
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
