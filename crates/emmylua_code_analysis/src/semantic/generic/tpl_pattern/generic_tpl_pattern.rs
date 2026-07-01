use crate::{
    InferFailReason, InferGuard, InferGuardRef, LuaFunctionType, LuaGenericType, LuaType,
    LuaTypeNode, SignatureReturnStatus, TplContext, TypeSubstitutor, instantiate_type_generic,
    semantic::{
        generic::tpl_pattern::{
            TplPatternMatchResult, tpl_pattern_match, variadic_tpl_pattern_match,
        },
        member::{find_members_with_key, get_member_map},
    },
};

pub fn generic_tpl_pattern_match(
    context: &mut TplContext,
    generic: &LuaGenericType,
    target: &LuaType,
) -> TplPatternMatchResult {
    generic_tpl_pattern_match_inner(context, generic, target, &InferGuard::new())
}

fn generic_tpl_pattern_match_inner(
    context: &mut TplContext,
    source_generic: &LuaGenericType,
    target: &LuaType,
    infer_guard: &InferGuardRef,
) -> TplPatternMatchResult {
    match target {
        LuaType::Generic(target_generic) => {
            let base = source_generic.get_base_type_id_ref();
            let target_base = target_generic.get_base_type_id_ref();
            if base == target_base {
                let params = source_generic.get_params();
                let target_params = target_generic.get_params();
                let min_len = params.len().min(target_params.len());
                for i in 0..min_len {
                    match (&params[i], &target_params[i]) {
                        (LuaType::Variadic(variadict), _) => {
                            variadic_tpl_pattern_match(context, variadict, &target_params[i..])?;
                            break;
                        }
                        _ => {
                            tpl_pattern_match(context, &params[i], &target_params[i])?;
                        }
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
                let substitutor = TypeSubstitutor::from_alias(
                    target_generic.get_params().clone(),
                    target_base.clone(),
                );
                if let Some(origin_type) =
                    target_decl.get_alias_origin(context.db, Some(&substitutor))
                {
                    return generic_tpl_pattern_match_inner(
                        context,
                        source_generic,
                        &origin_type,
                        infer_guard,
                    );
                }
            } else if let Some(super_types) =
                context.db.get_type_index().get_super_types(target_base)
            {
                for mut super_type in super_types {
                    if super_type.contains_tpl_node() {
                        let substitutor =
                            TypeSubstitutor::from_type_array(target_generic.get_params().clone());
                        super_type =
                            instantiate_type_generic(context.db, &super_type, &substitutor);
                    }

                    generic_tpl_pattern_match_inner(
                        context,
                        source_generic,
                        &super_type,
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
                return generic_tpl_pattern_match_inner(
                    context,
                    source_generic,
                    &origin_type,
                    infer_guard,
                );
            }

            for super_type in context
                .db
                .get_type_index()
                .get_super_types(type_id)
                .unwrap_or_default()
            {
                generic_tpl_pattern_match_inner(
                    context,
                    source_generic,
                    &super_type,
                    &infer_guard.fork(),
                )?;
            }
        }
        LuaType::Union(union_type) => {
            for union_sub_type in &union_type.into_vec() {
                generic_tpl_pattern_match_inner(
                    context,
                    source_generic,
                    union_sub_type,
                    &infer_guard.fork(),
                )?;
            }
        }
        LuaType::TableConst(_) => {
            match_generic_members_with_table_literal(context, source_generic, target)?;
        }
        _ => {
            // 对于 @alias 类型, 我们能拿到的 target 实际上很有可能是实例化后的类型, 因此我们需要实例化后再进行匹配
            let substitutor = TypeSubstitutor::new();
            let source_type = LuaType::from(source_generic.clone());
            let typ = instantiate_type_generic(context.db, &source_type, &substitutor);
            if source_type != typ {
                tpl_pattern_match(context, &typ, target)?;
            }
        }
    }

    Ok(())
}

fn match_generic_members_with_table_literal(
    context: &mut TplContext,
    source_generic: &LuaGenericType,
    table_type: &LuaType,
) -> TplPatternMatchResult {
    if context.substitutor.is_infer_all_tpl() {
        return Ok(());
    }

    let Some(target_member_map) = get_member_map(context.db, table_type) else {
        return Ok(());
    };

    let source_type = LuaType::Generic(source_generic.clone().into());
    for (member_key, target_members) in target_member_map {
        if context.substitutor.is_infer_all_tpl() {
            break;
        }

        let Some(source_members) =
            find_members_with_key(context.db, &source_type, member_key, true)
        else {
            continue;
        };

        for source_member in source_members {
            if !source_member.typ.contain_tpl() {
                continue;
            }

            for target_member in &target_members {
                let target_type = erase_implicit_signature_types(context, &target_member.typ);
                tpl_pattern_match_ignoring_unknown_target(
                    context,
                    &source_member.typ,
                    &target_type,
                )?;
                if context.substitutor.is_infer_all_tpl() {
                    break;
                }
            }

            if context.substitutor.is_infer_all_tpl() {
                break;
            }
        }
    }

    Ok(())
}

fn erase_implicit_signature_types(context: &TplContext, target: &LuaType) -> LuaType {
    let LuaType::Signature(signature_id) = target else {
        return target.clone();
    };
    let Some(signature) = context.db.get_signature_index().get(signature_id) else {
        return target.clone();
    };

    let params = signature
        .params
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            (
                name.clone(),
                Some(
                    signature
                        .param_docs
                        .get(&idx)
                        .map(|param| param.type_ref.clone())
                        .unwrap_or(LuaType::Unknown),
                ),
            )
        })
        .collect();
    let ret = if signature.resolve_return == SignatureReturnStatus::DocResolve {
        signature.get_return_type()
    } else {
        LuaType::Unknown
    };

    LuaType::DocFunction(
        LuaFunctionType::new(
            signature.async_state,
            signature.is_colon_define,
            signature.is_vararg,
            params,
            ret,
            Some(signature.get_function_generic_params()),
        )
        .into(),
    )
}

fn tpl_pattern_match_ignoring_unknown_target(
    context: &mut TplContext,
    pattern: &LuaType,
    target: &LuaType,
) -> TplPatternMatchResult {
    if pattern.contain_tpl() && (target.is_any() || target.is_unknown()) {
        return Ok(());
    }

    match (pattern, target) {
        (LuaType::DocFunction(pattern_func), LuaType::DocFunction(target_func)) => {
            for ((_, pattern_param), (_, target_param)) in pattern_func
                .get_params()
                .iter()
                .zip(target_func.get_params().iter())
            {
                let pattern_param = pattern_param.clone().unwrap_or(LuaType::Any);
                let target_param = target_param.clone().unwrap_or(LuaType::Unknown);
                tpl_pattern_match_ignoring_unknown_target(context, &pattern_param, &target_param)?;
            }

            tpl_pattern_match_ignoring_unknown_target(
                context,
                pattern_func.get_ret(),
                target_func.get_ret(),
            )
        }
        _ => tpl_pattern_match(context, pattern, target),
    }
}
