use crate::{
    InferFailReason, InferGuard, InferGuardRef, LuaGenericType, LuaType, TplContext,
    TypeSubstitutor, instantiate_generic, instantiate_type_generic,
    semantic::generic::tpl_pattern::{
        TplPatternMatchResult, tpl_pattern_match, variadic_tpl_pattern_match,
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
                    if super_type.contain_tpl() {
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
        _ => {
            // 对于 @alias 类型, 我们能拿到的 target 实际上很有可能是实例化后的类型, 因此我们需要实例化后再进行匹配
            let substitutor = TypeSubstitutor::new();
            let typ = instantiate_generic(context.db, source_generic, &substitutor);
            if LuaType::from(source_generic.clone()) != typ {
                tpl_pattern_match(context, &typ, target)?;
            }
        }
    }

    Ok(())
}
