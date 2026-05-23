use std::{collections::HashMap as StdHashMap, ops::Deref, sync::Arc};

use emmylua_parser::{LuaAstNode, LuaExpr};
use itertools::Itertools;
use rowan::NodeOrToken;
use smol_str::SmolStr;

use crate::{
    GenericTplId, InferFailReason, InferGuard, InferGuardRef, LuaFunctionType, LuaGenericType,
    LuaMemberInfo, LuaMemberKey, LuaMemberOwner, LuaSemanticDeclId, LuaTupleType, LuaType,
    LuaTypeNode, LuaUnionType, SemanticDeclLevel, VariadicType, check_type_compact,
    infer_node_semantic_decl, instantiate_type_generic,
    semantic::{
        generic::TypeMapper,
        member::{find_index_operations, get_member_map},
    },
};

use super::{
    InferenceCandidate, InferenceCandidateView, InferenceContext, InferencePriority,
    InferenceVariance, escape_alias, get_str_tpl_infer_type,
};

pub(in crate::semantic::generic) fn infer_types(
    context: &mut InferenceContext,
    source: &LuaType,
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
) -> Result<(), InferFailReason> {
    infer_types_inner(
        context,
        source,
        target,
        original_target,
        variance,
        priority,
        None,
        &InferGuard::new(),
    )
}

pub(in crate::semantic::generic) fn infer_types_from_expr(
    context: &mut InferenceContext,
    source: &LuaType,
    target: &LuaType,
    original_target: &LuaType,
    arg_expr: &LuaExpr,
) -> Result<(), InferFailReason> {
    infer_types_inner(
        context,
        source,
        target,
        original_target,
        InferenceVariance::Covariant,
        InferencePriority::Normal,
        Some(arg_expr),
        &InferGuard::new(),
    )
}

fn infer_types_inner(
    context: &mut InferenceContext,
    source: &LuaType,
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    let target = escape_alias(context.db, target);
    if !source.contains_tpl_node() {
        return Ok(());
    }

    let top_level = target == *original_target;
    match source {
        LuaType::TplRef(tpl) => {
            if tpl.get_tpl_id().is_func() {
                let candidate = match variance {
                    InferenceVariance::Covariant => {
                        InferenceCandidate::from_expr_arg(arg_expr, target.clone())
                    }
                    InferenceVariance::Contravariant => InferenceCandidate::ordinary(target),
                };
                context.insert_type(tpl.get_tpl_id(), candidate, variance, top_level, priority);
            }
        }
        LuaType::ConstTplRef(tpl) => {
            if tpl.get_tpl_id().is_func() {
                context.insert_type(
                    tpl.get_tpl_id(),
                    InferenceCandidate::const_preserving(target),
                    variance,
                    top_level,
                    priority,
                );
            }
        }
        LuaType::StrTplRef(str_tpl) => {
            if let LuaType::StringConst(s) | LuaType::DocStringConst(s) = target {
                let type_name = SmolStr::new(format!(
                    "{}{}{}",
                    str_tpl.get_prefix(),
                    s,
                    str_tpl.get_suffix()
                ));
                context.insert_type(
                    str_tpl.get_tpl_id(),
                    InferenceCandidate::regular_type(get_str_tpl_infer_type(&type_name)),
                    variance,
                    top_level,
                    priority,
                );
            }
        }
        LuaType::Array(array_type) => {
            array_infer_types(
                context,
                array_type.get_base(),
                &target,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::TableGeneric(params) => {
            table_generic_infer_types(
                context,
                params,
                &target,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Generic(generic) => {
            generic_infer_types(
                context,
                generic,
                &target,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Union(union) => {
            let members = union.into_vec();
            let mut error_count = 0;
            let mut last_error = InferFailReason::None;
            for member in &members {
                match infer_types_inner(
                    context,
                    member,
                    &target,
                    original_target,
                    variance,
                    priority,
                    arg_expr,
                    &infer_guard.fork(),
                ) {
                    Ok(_) => {}
                    Err(err) => {
                        error_count += 1;
                        last_error = err;
                    }
                }
            }
            if error_count == members.len() {
                return Err(last_error);
            }
        }
        LuaType::DocFunction(func) => {
            function_infer_types(
                context,
                func,
                &target,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Tuple(tuple) => {
            tuple_infer_types(
                context,
                tuple,
                &target,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Object(object) => {
            object_infer_types(
                context,
                object,
                &target,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        _ => {}
    }

    Ok(())
}

fn array_infer_types(
    context: &mut InferenceContext,
    base: &LuaType,
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    match target {
        LuaType::Array(target_array) => infer_types_inner(
            context,
            base,
            target_array.get_base(),
            original_target,
            variance,
            priority,
            arg_expr,
            infer_guard,
        )?,
        LuaType::Tuple(target_tuple) => {
            let target_base = target_tuple.cast_down_array_base(context.db);
            infer_types_inner(
                context,
                base,
                &target_base,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Object(target_object) => {
            let target_base = target_object
                .cast_down_array_base(context.db)
                .ok_or(InferFailReason::None)?;
            infer_types_inner(
                context,
                base,
                &target_base,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        _ => {}
    }

    Ok(())
}

fn table_generic_infer_types(
    context: &mut InferenceContext,
    table_params: &[LuaType],
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    if table_params.len() != 2 {
        return Err(InferFailReason::None);
    }

    match target {
        LuaType::TableGeneric(target_params) => {
            let min_len = table_params.len().min(target_params.len());
            for i in 0..min_len {
                infer_types_inner(
                    context,
                    &table_params[i],
                    &target_params[i],
                    original_target,
                    variance,
                    priority,
                    arg_expr,
                    infer_guard,
                )?;
            }
        }
        LuaType::Array(target_array) => {
            infer_types_inner(
                context,
                &table_params[0],
                &LuaType::Integer,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
            infer_types_inner(
                context,
                &table_params[1],
                target_array.get_base(),
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Tuple(target_tuple) => {
            let keys = (0..target_tuple.get_types().len())
                .map(|i| LuaType::IntegerConst((i as i64) + 1))
                .collect::<Vec<_>>();
            let key_type = LuaType::Union(LuaUnionType::from_vec(keys).into());
            let target_base = target_tuple.cast_down_array_base(context.db);
            infer_types_inner(
                context,
                &table_params[0],
                &key_type,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
            infer_types_inner(
                context,
                &table_params[1],
                &target_base,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::TableConst(inst) => {
            table_generic_member_owner_infer_types(
                context,
                table_params,
                LuaMemberOwner::Element(inst.clone()),
                &[],
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Ref(type_id) | LuaType::Def(type_id) => {
            table_generic_member_owner_infer_types(
                context,
                table_params,
                LuaMemberOwner::Type(type_id.clone()),
                &[],
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Generic(generic) => {
            table_generic_member_owner_infer_types(
                context,
                table_params,
                LuaMemberOwner::Type(generic.get_base_type_id()),
                generic.get_params(),
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Object(obj) => {
            let mut keys =
                Vec::with_capacity(obj.get_fields().len() + obj.get_index_access().len());
            let mut values =
                Vec::with_capacity(obj.get_fields().len() + obj.get_index_access().len());
            for (key, value) in obj.get_fields() {
                match key {
                    LuaMemberKey::Integer(i) => keys.push(LuaType::IntegerConst(*i)),
                    LuaMemberKey::Name(name) => {
                        keys.push(LuaType::StringConst(name.clone().into()))
                    }
                    _ => {}
                }
                values.push(value.clone());
            }
            for (key, value) in obj.get_index_access() {
                keys.push(key.clone());
                values.push(value.clone());
            }
            let key_type = LuaType::Union(LuaUnionType::from_vec(keys).into());
            let value_type = LuaType::Union(LuaUnionType::from_vec(values).into());
            infer_types_inner(
                context,
                &table_params[0],
                &key_type,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
            infer_types_inner(
                context,
                &table_params[1],
                &value_type,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Global | LuaType::Any | LuaType::Table | LuaType::Userdata => {
            infer_types_inner(
                context,
                &table_params[0],
                &LuaType::Any,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
            infer_types_inner(
                context,
                &table_params[1],
                &LuaType::Any,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        _ => {}
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn table_generic_member_owner_infer_types(
    context: &mut InferenceContext,
    table_params: &[LuaType],
    owner: LuaMemberOwner,
    target_params: &[LuaType],
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    if table_params.len() != 2 {
        return Err(InferFailReason::None);
    }

    let owner_type = match &owner {
        LuaMemberOwner::Element(inst) => LuaType::TableConst(inst.clone()),
        LuaMemberOwner::Type(type_id) => match target_params.len() {
            0 => LuaType::Ref(type_id.clone()),
            _ => LuaType::Generic(Arc::new(LuaGenericType::new(
                type_id.clone(),
                target_params.to_vec(),
            ))),
        },
        _ => return Err(InferFailReason::None),
    };

    let members = get_member_map(context.db, &owner_type).ok_or(InferFailReason::None)?;
    if is_pairs_call(context).unwrap_or(false)
        && try_handle_pairs_metamethod(
            context,
            table_params,
            &members,
            original_target,
            variance,
            priority,
            arg_expr,
            infer_guard,
        )
        .is_ok()
    {
        return Ok(());
    }

    let target_key_type = table_params[0].clone();
    let mut keys = Vec::with_capacity(members.len());
    let mut values = Vec::with_capacity(members.len());
    for (key, members) in members {
        let key_type = match key {
            LuaMemberKey::Integer(i) => LuaType::IntegerConst(i),
            LuaMemberKey::Name(name) => LuaType::StringConst(name.clone().into()),
            LuaMemberKey::ExprType(ty) => ty,
            _ => continue,
        };

        if !target_key_type.is_generic()
            && check_type_compact(context.db, &target_key_type, &key_type).is_err()
        {
            continue;
        }

        keys.push(key_type);
        values.push(member_infos_type(members));
    }

    if keys.is_empty() {
        find_index_operations(context.db, &owner_type)
            .ok_or(InferFailReason::None)?
            .iter()
            .for_each(|member| {
                if target_key_type.is_generic() {
                    return;
                }
                let LuaMemberKey::ExprType(key_type) = &member.key else {
                    return;
                };
                if check_type_compact(context.db, &target_key_type, key_type).is_ok() {
                    keys.push(key_type.clone());
                    values.push(member.typ.clone());
                }
            });
    }

    let key_type = match &keys[..] {
        [] => return Err(InferFailReason::None),
        [first] => first.clone(),
        _ => LuaType::Union(LuaUnionType::from_vec(keys).into()),
    };
    let value_type = match &values[..] {
        [first] => first.clone(),
        _ => LuaType::Union(LuaUnionType::from_vec(values).into()),
    };

    infer_types_inner(
        context,
        &table_params[0],
        &key_type,
        original_target,
        variance,
        priority,
        arg_expr,
        infer_guard,
    )?;
    infer_types_inner(
        context,
        &table_params[1],
        &value_type,
        original_target,
        variance,
        priority,
        arg_expr,
        infer_guard,
    )
}

fn generic_infer_types(
    context: &mut InferenceContext,
    source_generic: &LuaGenericType,
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    match target {
        LuaType::Generic(target_generic) => {
            let source_base = source_generic.get_base_type_id_ref();
            let target_base = target_generic.get_base_type_id_ref();
            if source_base == target_base {
                for (start, (source_param, target_param)) in source_generic
                    .get_params()
                    .iter()
                    .zip(target_generic.get_params())
                    .enumerate()
                {
                    match source_param {
                        LuaType::Variadic(variadic) => {
                            variadic_infer_types(
                                context,
                                variadic,
                                &target_generic.get_params()[start..],
                                original_target,
                                variance,
                                priority,
                            )?;
                            break;
                        }
                        _ => infer_types_inner(
                            context,
                            source_param,
                            target_param,
                            original_target,
                            variance,
                            priority,
                            arg_expr,
                            infer_guard,
                        )?,
                    }
                }
                return Ok(());
            }

            let target_decl = context
                .db
                .get_type_index()
                .get_type_decl(target_base)
                .ok_or(InferFailReason::None)?;
            if target_decl.is_alias() {
                let mapper = TypeMapper::from_alias(
                    context.db,
                    target_generic.get_params().clone(),
                    target_base,
                );
                if let Some(origin_type) = target_decl.get_alias_origin(context.db, Some(&mapper)) {
                    return generic_infer_types(
                        context,
                        source_generic,
                        &origin_type,
                        original_target,
                        variance,
                        priority,
                        arg_expr,
                        infer_guard,
                    );
                }
            } else if let Some(super_types) =
                context.db.get_type_index().get_super_types(target_base)
            {
                for mut super_type in super_types {
                    if super_type.contains_tpl_node() {
                        let mapper =
                            TypeMapper::from_type_array(target_generic.get_params().clone());
                        super_type = instantiate_type_generic(context.db, &super_type, &mapper);
                    }
                    generic_infer_types(
                        context,
                        source_generic,
                        &super_type,
                        original_target,
                        variance,
                        priority,
                        arg_expr,
                        &infer_guard.fork(),
                    )?;
                }
            }
        }
        LuaType::Ref(type_id) | LuaType::Def(type_id) => {
            infer_guard.check(type_id)?;
            let type_decl = context
                .db
                .get_type_index()
                .get_type_decl(type_id)
                .ok_or(InferFailReason::None)?;
            if let Some(origin_type) = type_decl.get_alias_origin(context.db, None) {
                return generic_infer_types(
                    context,
                    source_generic,
                    &origin_type,
                    original_target,
                    variance,
                    priority,
                    arg_expr,
                    infer_guard,
                );
            }

            for super_type in context
                .db
                .get_type_index()
                .get_super_types(type_id)
                .unwrap_or_default()
            {
                generic_infer_types(
                    context,
                    source_generic,
                    &super_type,
                    original_target,
                    variance,
                    priority,
                    arg_expr,
                    &infer_guard.fork(),
                )?;
            }
        }
        LuaType::Union(union) => {
            for member in union.into_vec() {
                generic_infer_types(
                    context,
                    source_generic,
                    &member,
                    original_target,
                    variance,
                    priority,
                    arg_expr,
                    &infer_guard.fork(),
                )?;
            }
        }
        _ => {
            let mapper = TypeMapper::empty();
            let generic_ty = LuaType::Generic(source_generic.clone().into());
            let ty = instantiate_type_generic(context.db, &generic_ty, &mapper);
            if LuaType::from(source_generic.clone()) != ty {
                infer_types_inner(
                    context,
                    &ty,
                    target,
                    original_target,
                    variance,
                    priority,
                    arg_expr,
                    infer_guard,
                )?;
            }
        }
    }

    Ok(())
}

fn function_infer_types(
    context: &mut InferenceContext,
    source_func: &LuaFunctionType,
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    match target {
        LuaType::DocFunction(target_func) => {
            function_doc_infer_types(
                context,
                source_func,
                target_func,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Signature(signature_id) => {
            let signature = context
                .db
                .get_signature_index()
                .get(signature_id)
                .ok_or(InferFailReason::None)?;
            if !signature.is_resolve_return() {
                return check_lambda_inference(context, *signature_id);
            }

            let fake_func = signature.to_doc_func_type();
            function_doc_infer_types(
                context,
                source_func,
                &fake_func,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        _ => {}
    }

    Ok(())
}

fn function_doc_infer_types(
    context: &mut InferenceContext,
    source_func: &LuaFunctionType,
    target_func: &LuaFunctionType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    let mut source_params = source_func.get_params().to_vec();
    if source_func.is_colon_define() {
        source_params.insert(0, ("self".to_string(), Some(LuaType::Any)));
    }

    let mut target_params = target_func.get_params().to_vec();
    if target_func.is_colon_define() {
        target_params.insert(0, ("self".to_string(), Some(LuaType::Any)));
    }

    param_list_infer_types(
        context,
        &source_params,
        &target_params,
        original_target,
        variance.flip(),
        priority,
        arg_expr,
        infer_guard,
    )?;
    return_type_infer_types(
        context,
        source_func.get_ret(),
        target_func.get_ret(),
        original_target,
        variance,
        priority,
        arg_expr,
        infer_guard,
    )
}

#[allow(clippy::too_many_arguments)]
fn param_list_infer_types(
    context: &mut InferenceContext,
    sources: &[(String, Option<LuaType>)],
    targets: &[(String, Option<LuaType>)],
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    let mut target_offset = 0;
    for i in 0..sources.len() {
        let source = match sources.get(i) {
            Some((_, ty)) => ty.clone().unwrap_or(LuaType::Any),
            None => break,
        };

        match &source {
            LuaType::Variadic(inner) => {
                let i = i + target_offset;
                if i >= targets.len() {
                    if let VariadicType::Base(base) = inner.deref() {
                        insert_tpl_ref_candidate(
                            context,
                            base,
                            LuaType::Nil,
                            variance,
                            false,
                            priority,
                        );
                    }
                    break;
                }

                if let Some((tpl_id, _)) = variadic_base_tpl_ref(inner.deref())
                    && let Some(len) = context.inferred_variadic_len(tpl_id)
                {
                    target_offset += len - 1;
                    continue;
                }

                let mut target_rest_params = &targets[i..];
                if i + 1 < sources.len() {
                    let source_rest_len = sources.len() - i - 1;
                    if source_rest_len >= target_rest_params.len() {
                        continue;
                    }
                    let target_rest_len = target_rest_params.len() - source_rest_len;
                    target_rest_params = &target_rest_params[..target_rest_len];
                    if target_rest_len > 1 {
                        target_offset += target_rest_len - 1;
                    }
                }

                function_varargs_infer_types(context, inner, target_rest_params)?;
            }
            _ => {
                let target = match targets.get(i + target_offset) {
                    Some((_, ty)) => ty.clone().unwrap_or(LuaType::Any),
                    None => break,
                };
                infer_types_inner(
                    context,
                    &source,
                    &target,
                    original_target,
                    variance,
                    priority,
                    arg_expr,
                    infer_guard,
                )?;
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(in crate::semantic::generic) fn return_type_infer_types(
    context: &mut InferenceContext,
    source: &LuaType,
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    match (source, target) {
        (LuaType::Variadic(source_variadic), LuaType::Variadic(target_variadic)) => {
            match target_variadic.deref() {
                VariadicType::Base(target_base) => match source_variadic.deref() {
                    VariadicType::Base(source_base) => {
                        insert_tpl_ref_candidate(
                            context,
                            source_base,
                            target_base.clone(),
                            variance,
                            false,
                            priority,
                        );
                    }
                    VariadicType::Multi(source_multi) => {
                        for ret_type in source_multi {
                            match ret_type {
                                LuaType::Variadic(inner) => {
                                    if let VariadicType::Base(base) = inner.deref() {
                                        insert_tpl_ref_candidate(
                                            context,
                                            base,
                                            target_base.clone(),
                                            variance,
                                            false,
                                            priority,
                                        );
                                    }
                                    break;
                                }
                                _ => {
                                    insert_tpl_ref_candidate(
                                        context,
                                        ret_type,
                                        target_base.clone(),
                                        variance,
                                        false,
                                        priority,
                                    );
                                }
                            }
                        }
                    }
                },
                VariadicType::Multi(target_types) => {
                    variadic_infer_types(
                        context,
                        source_variadic,
                        target_types,
                        original_target,
                        variance,
                        priority,
                    )?;
                }
            }
        }
        (LuaType::Variadic(variadic), _) => {
            variadic_infer_types(
                context,
                variadic,
                std::slice::from_ref(target),
                original_target,
                variance,
                priority,
            )?;
        }
        (_, LuaType::Variadic(variadic)) => {
            multi_param_infer_multi_return(
                context,
                std::slice::from_ref(source),
                variadic,
                original_target,
                variance,
                priority,
            )?;
        }
        _ => infer_types_inner(
            context,
            source,
            target,
            original_target,
            variance,
            priority,
            arg_expr,
            infer_guard,
        )?,
    }

    Ok(())
}

fn tpl_ref_info(ty: &LuaType) -> Option<(GenericTplId, bool)> {
    match ty {
        LuaType::TplRef(tpl_ref) => Some((tpl_ref.get_tpl_id(), false)),
        LuaType::ConstTplRef(tpl_ref) => Some((tpl_ref.get_tpl_id(), true)),
        _ => None,
    }
}

fn variadic_base_tpl_ref(variadic: &VariadicType) -> Option<(GenericTplId, bool)> {
    let VariadicType::Base(base) = variadic else {
        return None;
    };
    tpl_ref_info(base)
}

fn tpl_ref_candidate(is_const_tpl: bool, ty: LuaType) -> InferenceCandidate {
    if is_const_tpl {
        InferenceCandidate::const_preserving(ty)
    } else {
        InferenceCandidate::ordinary(ty)
    }
}

fn insert_tpl_ref_candidate(
    context: &mut InferenceContext,
    source: &LuaType,
    target: LuaType,
    variance: InferenceVariance,
    top_level: bool,
    priority: InferencePriority,
) {
    if let Some((tpl_id, is_const_tpl)) = tpl_ref_info(source) {
        context.insert_type(
            tpl_id,
            tpl_ref_candidate(is_const_tpl, target),
            variance,
            top_level,
            priority,
        );
    }
}

fn function_varargs_infer_types(
    context: &mut InferenceContext,
    variadic: &VariadicType,
    target_rest_params: &[(String, Option<LuaType>)],
) -> Result<(), InferFailReason> {
    if let Some((tpl_id, _)) = variadic_base_tpl_ref(variadic) {
        context.add_variadic_params(
            tpl_id,
            target_rest_params
                .iter()
                .map(|(name, ty)| (name.clone(), ty.clone()))
                .collect(),
        );
    }

    Ok(())
}

pub(in crate::semantic::generic) fn variadic_infer_types(
    context: &mut InferenceContext,
    source: &VariadicType,
    target_rest_types: &[LuaType],
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
) -> Result<(), InferFailReason> {
    match source {
        VariadicType::Base(base) => match base {
            LuaType::TplRef(tpl_ref) => {
                let tpl_id = tpl_ref.get_tpl_id();
                match target_rest_types.len() {
                    0 => context.insert_type(
                        tpl_id,
                        InferenceCandidate::ordinary(LuaType::Nil),
                        variance,
                        false,
                        priority,
                    ),
                    1 => match &target_rest_types[0] {
                        LuaType::Variadic(variadic) => match variadic.deref() {
                            VariadicType::Multi(types) => match types.len() {
                                0 => context.insert_type(
                                    tpl_id,
                                    InferenceCandidate::ordinary(LuaType::Nil),
                                    variance,
                                    false,
                                    priority,
                                ),
                                1 => context.insert_type(
                                    tpl_id,
                                    InferenceCandidate::ordinary(types[0].clone()),
                                    variance,
                                    false,
                                    priority,
                                ),
                                _ => context.insert_multi_types(
                                    tpl_id,
                                    types.to_vec(),
                                    InferenceCandidateView::Ordinary,
                                    false,
                                    priority,
                                ),
                            },
                            VariadicType::Base(base) => {
                                context.add_variadic_base(tpl_id, base.clone());
                            }
                        },
                        target => context.insert_type(
                            tpl_id,
                            InferenceCandidate::ordinary(target.clone()),
                            variance,
                            false,
                            priority,
                        ),
                    },
                    _ => context.insert_multi_types(
                        tpl_id,
                        target_rest_types.to_vec(),
                        InferenceCandidateView::Ordinary,
                        false,
                        priority,
                    ),
                }
            }
            LuaType::ConstTplRef(tpl_ref) => {
                let tpl_id = tpl_ref.get_tpl_id();
                match target_rest_types.len() {
                    0 => context.insert_type(
                        tpl_id,
                        InferenceCandidate::const_preserving(LuaType::Nil),
                        variance,
                        false,
                        priority,
                    ),
                    1 => context.insert_type(
                        tpl_id,
                        InferenceCandidate::const_preserving(target_rest_types[0].clone()),
                        variance,
                        false,
                        priority,
                    ),
                    _ => context.insert_multi_types(
                        tpl_id,
                        target_rest_types.to_vec(),
                        InferenceCandidateView::ConstPreserving,
                        false,
                        priority,
                    ),
                }
            }
            _ => {}
        },
        VariadicType::Multi(multi) => {
            for (i, ret_type) in multi.iter().enumerate() {
                match ret_type {
                    LuaType::Variadic(inner) => {
                        if i < target_rest_types.len() {
                            variadic_infer_types(
                                context,
                                inner,
                                &target_rest_types[i..],
                                original_target,
                                variance,
                                priority,
                            )?;
                        }
                        break;
                    }
                    _ => {
                        let Some(target) = target_rest_types.get(i) else {
                            break;
                        };
                        insert_tpl_ref_candidate(
                            context,
                            ret_type,
                            target.clone(),
                            variance,
                            target == original_target,
                            priority,
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

pub(in crate::semantic::generic) fn multi_param_infer_multi_return(
    context: &mut InferenceContext,
    source_params: &[LuaType],
    multi_return: &VariadicType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
) -> Result<(), InferFailReason> {
    match multi_return {
        VariadicType::Base(base) => {
            let mut target_types = Vec::with_capacity(source_params.len());
            for param in source_params {
                if param.is_variadic() {
                    target_types.push(LuaType::Variadic(multi_return.clone().into()));
                    break;
                } else {
                    target_types.push(base.clone());
                }
            }
            infer_type_list(
                context,
                source_params,
                &target_types,
                original_target,
                variance,
                priority,
            )?;
        }
        VariadicType::Multi(_) => {
            let mut target_types = Vec::with_capacity(source_params.len());
            for (i, param) in source_params.iter().enumerate() {
                let Some(return_type) = multi_return.get_type(i) else {
                    break;
                };
                if param.is_variadic() {
                    target_types.push(LuaType::Variadic(
                        multi_return.get_new_variadic_from(i).into(),
                    ));
                    break;
                } else {
                    target_types.push(return_type.clone());
                }
            }
            infer_type_list(
                context,
                source_params,
                &target_types,
                original_target,
                variance,
                priority,
            )?;
        }
    }

    Ok(())
}

pub(in crate::semantic::generic) fn infer_type_list(
    context: &mut InferenceContext,
    source_types: &[LuaType],
    target_types: &[LuaType],
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
) -> Result<(), InferFailReason> {
    for (start, (source, target)) in source_types.iter().zip(target_types).enumerate() {
        match (source, target) {
            (LuaType::Variadic(variadic), _) => {
                variadic_infer_types(
                    context,
                    variadic,
                    &target_types[start..],
                    original_target,
                    variance,
                    priority,
                )?;
                break;
            }
            (_, LuaType::Variadic(variadic)) => {
                multi_param_infer_multi_return(
                    context,
                    &source_types[start..],
                    variadic,
                    original_target,
                    variance,
                    priority,
                )?;
                break;
            }
            _ => infer_types(context, source, target, original_target, variance, priority)?,
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn tuple_infer_types(
    context: &mut InferenceContext,
    source_tuple: &LuaTupleType,
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    match target {
        LuaType::Tuple(target_tuple) => {
            for (i, source_type) in source_tuple.get_types().iter().enumerate() {
                if let LuaType::Variadic(inner) = source_type {
                    variadic_infer_types(
                        context,
                        inner,
                        &target_tuple.get_types()[i..],
                        original_target,
                        variance,
                        priority,
                    )?;
                    break;
                }
                let Some(target_type) = target_tuple.get_types().get(i) else {
                    break;
                };
                infer_types_inner(
                    context,
                    source_type,
                    target_type,
                    original_target,
                    variance,
                    priority,
                    arg_expr,
                    infer_guard,
                )?;
            }
        }
        LuaType::Array(target_array) => {
            let Some(last_type) = source_tuple.get_types().last() else {
                return Err(InferFailReason::None);
            };
            if let LuaType::Variadic(inner) = last_type
                && let Some((tpl_id, _)) = variadic_base_tpl_ref(inner.deref())
            {
                context.add_variadic_base(tpl_id, target_array.get_base().clone());
            }
        }
        _ => {}
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn object_infer_types(
    context: &mut InferenceContext,
    source_obj: &crate::LuaObjectType,
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    match target {
        LuaType::Object(target_obj) => {
            for (key, value) in source_obj
                .get_fields()
                .iter()
                .sorted_by_key(|(key, _)| *key)
            {
                if let Some(target_value) = target_obj.get_fields().get(key) {
                    infer_types_inner(
                        context,
                        value,
                        target_value,
                        original_target,
                        variance,
                        priority,
                        arg_expr,
                        infer_guard,
                    )?;
                }
            }
            for (source_key, value) in source_obj.get_index_access() {
                let target_access = target_obj
                    .get_index_access()
                    .iter()
                    .find(|(target_key, _)| {
                        check_type_compact(context.db, source_key, target_key).is_ok()
                    });
                if let Some((target_key, target_value)) = target_access {
                    infer_types_inner(
                        context,
                        source_key,
                        target_key,
                        original_target,
                        variance,
                        priority,
                        arg_expr,
                        infer_guard,
                    )?;
                    infer_types_inner(
                        context,
                        value,
                        target_value,
                        original_target,
                        variance,
                        priority,
                        arg_expr,
                        infer_guard,
                    )?;
                }
            }
        }
        LuaType::TableConst(inst) => {
            object_member_owner_infer_types(
                context,
                source_obj,
                LuaMemberOwner::Element(inst.clone()),
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        _ => {}
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn object_member_owner_infer_types(
    context: &mut InferenceContext,
    source_obj: &crate::LuaObjectType,
    owner: LuaMemberOwner,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    let owner_type = match &owner {
        LuaMemberOwner::Element(inst) => LuaType::TableConst(inst.clone()),
        LuaMemberOwner::Type(type_id) => LuaType::Ref(type_id.clone()),
        _ => return Err(InferFailReason::None),
    };

    let members = get_member_map(context.db, &owner_type).ok_or(InferFailReason::None)?;
    for (key, members) in members {
        let resolve_type = member_infos_type(members);
        if let Some(field_value) = source_obj.get_field(&key) {
            infer_types_inner(
                context,
                field_value,
                &resolve_type,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
    }

    Ok(())
}

fn member_infos_type(members: Vec<LuaMemberInfo>) -> LuaType {
    match members.len() {
        0 => LuaType::Any,
        1 => members[0].typ.clone(),
        _ => LuaType::from_vec(members.into_iter().map(|member| member.typ).collect()),
    }
}

fn is_pairs_call(context: &mut InferenceContext) -> Option<bool> {
    let call_expr = context.call_expr.as_ref()?;
    let prefix_expr = call_expr.get_prefix_expr()?;
    let semantic_decl = match prefix_expr.syntax().clone().into() {
        NodeOrToken::Node(node) => infer_node_semantic_decl(
            context.db,
            context.cache,
            node,
            SemanticDeclLevel::default(),
        ),
        _ => None,
    }?;

    let LuaSemanticDeclId::LuaDecl(decl_id) = semantic_decl else {
        return None;
    };
    let decl = context.db.get_decl_index().get_decl(&decl_id)?;
    if !context.db.get_module_index().is_std(&decl.get_file_id()) {
        return None;
    }
    if decl.get_name() != "pairs" {
        return None;
    }

    Some(true)
}

#[allow(clippy::too_many_arguments)]
fn try_handle_pairs_metamethod(
    context: &mut InferenceContext,
    table_params: &[LuaType],
    members: &StdHashMap<LuaMemberKey, Vec<LuaMemberInfo>>,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    let pairs_member = members
        .get(&LuaMemberKey::Name("__pairs".into()))
        .ok_or(InferFailReason::None)?
        .first()
        .ok_or(InferFailReason::None)?;

    let meta_return = match &pairs_member.typ {
        LuaType::Signature(signature_id) => context
            .db
            .get_signature_index()
            .get(signature_id)
            .map(|signature| signature.get_return_type()),
        LuaType::DocFunction(doc_func) => Some(doc_func.get_ret().clone()),
        _ => None,
    }
    .ok_or(InferFailReason::None)?;

    let iterator_func = meta_return.get_result_slot_type(0).unwrap_or(meta_return);
    let final_return_type = match iterator_func {
        LuaType::DocFunction(doc_func) => Some(doc_func.get_ret().clone()),
        LuaType::Signature(signature_id) => context
            .db
            .get_signature_index()
            .get(&signature_id)
            .map(|signature| signature.get_return_type()),
        _ => None,
    };

    if let Some(final_return_type) = &final_return_type {
        let key_type = final_return_type
            .get_result_slot_type(0)
            .ok_or(InferFailReason::None)?;
        let value_type = final_return_type
            .get_result_slot_type(1)
            .unwrap_or(LuaType::Nil);
        infer_types_inner(
            context,
            &table_params[0],
            &key_type,
            original_target,
            variance,
            priority,
            arg_expr,
            infer_guard,
        )?;
        infer_types_inner(
            context,
            &table_params[1],
            &value_type,
            original_target,
            variance,
            priority,
            arg_expr,
            infer_guard,
        )?;
        return Ok(());
    }

    Err(InferFailReason::None)
}

fn check_lambda_inference(
    context: &mut InferenceContext,
    signature_id: crate::LuaSignatureId,
) -> Result<(), InferFailReason> {
    let call_expr = context.call_expr.as_ref().ok_or(InferFailReason::None)?;
    let call_arg_list = call_expr.get_args_list().ok_or(InferFailReason::None)?;
    for arg in call_arg_list.get_args() {
        if let Ok(LuaType::Signature(arg_signature_id)) =
            crate::semantic::infer_expr(context.db, context.cache, arg.clone())
            && arg_signature_id == signature_id
        {
            return Ok(());
        }
    }

    Err(InferFailReason::UnResolveSignatureReturn(signature_id))
}
