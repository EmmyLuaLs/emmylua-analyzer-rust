use std::{ops::Deref, sync::Arc};

use emmylua_parser::LuaAstNode;
use itertools::Itertools;
use rowan::NodeOrToken;
use smol_str::SmolStr;

use crate::{
    GenericTplId, InferFailReason, LuaFunctionType, LuaMemberInfo, LuaMemberKey, LuaMemberOwner,
    LuaObjectType, LuaSemanticDeclId, LuaTupleType, LuaUnionType, SemanticDeclLevel, VariadicType,
    check_type_compact,
    db_index::{DbIndex, LuaGenericType, LuaType},
    infer_node_semantic_decl,
    semantic::{
        generic::{tpl_context::TplContext, type_substitutor::SubstitutorValue},
        member::{find_index_operations, get_member_map},
        type_check::is_sub_type_of,
    },
};

use super::type_substitutor::TypeSubstitutor;
use std::collections::HashMap;

type TplPatternMatchResult = Result<(), InferFailReason>;

pub fn tpl_pattern_match_args(
    context: &mut TplContext,
    func_param_types: &[LuaType],
    call_arg_types: &[LuaType],
) -> TplPatternMatchResult {
    for i in 0..func_param_types.len() {
        if i >= call_arg_types.len() {
            break;
        }

        let func_param_type = &func_param_types[i];
        let call_arg_type = &call_arg_types[i];

        match (func_param_type, call_arg_type) {
            (LuaType::Variadic(variadic), _) => {
                variadic_tpl_pattern_match(context, variadic, &call_arg_types[i..])?;
                break;
            }
            (_, LuaType::Variadic(variadic)) => {
                multi_param_tpl_pattern_match_multi_return(
                    context,
                    &func_param_types[i..],
                    variadic,
                )?;
                break;
            }
            _ => {
                tpl_pattern_match(context, func_param_type, call_arg_type)?;
            }
        }
    }

    Ok(())
}

pub fn multi_param_tpl_pattern_match_multi_return(
    context: &mut TplContext,
    func_param_types: &[LuaType],
    multi_return: &VariadicType,
) -> TplPatternMatchResult {
    match &multi_return {
        VariadicType::Base(base) => {
            let mut call_arg_types = Vec::new();
            for param in func_param_types {
                if param.is_variadic() {
                    call_arg_types.push(LuaType::Variadic(multi_return.clone().into()));
                    break;
                } else {
                    call_arg_types.push(base.clone());
                }
            }

            tpl_pattern_match_args(context, func_param_types, &call_arg_types)?;
        }
        VariadicType::Multi(_) => {
            let mut call_arg_types = Vec::new();
            for (i, param) in func_param_types.iter().enumerate() {
                let return_type = multi_return.get_type(i);
                if return_type.is_none() {
                    break;
                }

                if param.is_variadic() {
                    call_arg_types.push(LuaType::Variadic(
                        multi_return.get_new_variadic_from(i).into(),
                    ));
                    break;
                } else {
                    call_arg_types.push(return_type.unwrap().clone());
                }
            }

            tpl_pattern_match_args(context, func_param_types, &call_arg_types)?;
        }
    }

    Ok(())
}

pub fn tpl_pattern_match(
    context: &mut TplContext,
    pattern: &LuaType,
    target: &LuaType,
) -> TplPatternMatchResult {
    let target = escape_alias(context.db, target);
    match pattern {
        LuaType::TplRef(tpl) => {
            tpl_ref_pattern_match(context, tpl.get_tpl_id(), target)?;
        }
        LuaType::ConstTplRef(tpl) => {
            tpl_ref_pattern_match_const_tpl(context, tpl.get_tpl_id(), target)?;
        }
        LuaType::StrTplRef(str_tpl) => match target {
            LuaType::StringConst(s) => {
                let prefix = str_tpl.get_prefix();
                let suffix = str_tpl.get_suffix();
                let type_name = SmolStr::new(format!("{}{}{}", prefix, s, suffix));
                context
                    .substitutor
                    .insert_type(str_tpl.get_tpl_id(), type_name.into());
            }
            _ => {}
        },
        LuaType::Array(array_type) => {
            array_tpl_pattern_match(context, array_type.get_base(), &target)?;
        }
        LuaType::TableGeneric(table_generic_params) => {
            table_generic_tpl_pattern_match(context, table_generic_params, &target)?;
        }
        LuaType::Generic(generic) => {
            generic_tpl_pattern_match(context, generic, &target)?;
        }
        LuaType::Union(union) => {
            union_tpl_pattern_match(context, union, &target)?;
        }
        LuaType::DocFunction(doc_func) => {
            func_tpl_pattern_match(context, doc_func, &target)?;
        }
        LuaType::Tuple(tuple) => {
            tuple_tpl_pattern_match(context, tuple, &target)?;
        }
        LuaType::Object(obj) => {
            object_tpl_pattern_match(context, obj, &target)?;
        }
        _ => {}
    }

    Ok(())
}

fn constant_decay(typ: LuaType) -> LuaType {
    match &typ {
        LuaType::FloatConst(_) => LuaType::Number,
        LuaType::DocIntegerConst(_) | LuaType::IntegerConst(_) => LuaType::Integer,
        LuaType::DocStringConst(_) | LuaType::StringConst(_) => LuaType::String,
        LuaType::DocBooleanConst(_) | LuaType::BooleanConst(_) => LuaType::Boolean,
        _ => typ,
    }
}

fn object_tpl_pattern_match(
    context: &mut TplContext,
    origin_obj: &LuaObjectType,
    target: &LuaType,
) -> TplPatternMatchResult {
    match target {
        LuaType::Object(target_object) => {
            // 先匹配 fields
            for (k, v) in origin_obj.get_fields().iter().sorted_by_key(|(k, _)| *k) {
                let target_value = target_object.get_fields().get(k);
                if let Some(target_value) = target_value {
                    tpl_pattern_match(context, v, target_value)?;
                }
            }
            // 再匹配索引访问
            let target_index_access = target_object.get_index_access();
            for (origin_key, v) in origin_obj.get_index_access() {
                // 先匹配 key 类型进行转换
                let target_access = target_index_access.iter().find(|(target_key, _)| {
                    check_type_compact(context.db, origin_key, target_key).is_ok()
                });
                if let Some(target_access) = target_access {
                    tpl_pattern_match(context, origin_key, &target_access.0)?;
                    tpl_pattern_match(context, v, &target_access.1)?;
                }
            }
        }
        LuaType::TableConst(inst) => {
            let owner = LuaMemberOwner::Element(inst.clone());
            object_tpl_pattern_match_member_owner_match(context, origin_obj, owner)?;
        }
        _ => {}
    }

    Ok(())
}

fn object_tpl_pattern_match_member_owner_match(
    context: &mut TplContext,
    object: &LuaObjectType,
    owner: LuaMemberOwner,
) -> TplPatternMatchResult {
    let owner_type = match &owner {
        LuaMemberOwner::Element(inst) => LuaType::TableConst(inst.clone()),
        LuaMemberOwner::Type(type_id) => LuaType::Ref(type_id.clone()),
        _ => {
            return Err(InferFailReason::None);
        }
    };

    let members = get_member_map(context.db, &owner_type).ok_or(InferFailReason::None)?;
    for (k, v) in members {
        let resolve_key = match &k {
            LuaMemberKey::Integer(i) => Some(LuaType::IntegerConst(i.clone())),
            LuaMemberKey::Name(s) => Some(LuaType::StringConst(s.clone().into())),
            _ => None,
        };
        let resolve_type = match v.len() {
            0 => LuaType::Any,
            1 => v[0].typ.clone(),
            _ => {
                let mut types = Vec::new();
                for m in &v {
                    types.push(m.typ.clone());
                }
                LuaType::from_vec(types)
            }
        };

        // this is a workaround, I need refactor infer member map
        if resolve_type.is_unknown() && v.len() > 0 {
            if let Some(LuaSemanticDeclId::Member(member_id)) = &v[0].property_owner_id {
                return Err(InferFailReason::UnResolveMemberType(*member_id));
            }
        }

        if let Some(_) = resolve_key {
            if let Some(field_value) = object.get_field(&k) {
                tpl_pattern_match(context, field_value, &resolve_type)?;
            }
        }
    }

    Ok(())
}

fn tpl_ref_pattern_match(
    context: &mut TplContext,
    tpl_id: GenericTplId,
    target: LuaType,
) -> TplPatternMatchResult {
    // Non-variadic tpl ref can't become a variadic.
    let target = match target {
        LuaType::Variadic(variadic) => variadic.get_type(0).cloned().unwrap_or(LuaType::Nil),
        target => target,
    };

    if tpl_id.is_func() {
        context
            .substitutor
            .insert_type(tpl_id, constant_decay(target));
    }

    Ok(())
}

fn tpl_ref_pattern_match_const_tpl(
    context: &mut TplContext,
    tpl_id: GenericTplId,
    target: LuaType,
) -> TplPatternMatchResult {
    // Non-variadic tpl ref can't become a variadic.
    let target = match target {
        LuaType::Variadic(variadic) => variadic.get_type(0).cloned().unwrap_or(LuaType::Nil),
        target => target,
    };

    if tpl_id.is_func() {
        context
            .substitutor
            .insert_type(tpl_id, target);
    }

    Ok(())
}

fn array_tpl_pattern_match(
    context: &mut TplContext,
    base: &LuaType,
    target: &LuaType,
) -> TplPatternMatchResult {
    match target {
        LuaType::Array(target_array_type) => {
            tpl_pattern_match(context, base, target_array_type.get_base())?;
        }
        LuaType::Tuple(target_tuple) => {
            let target_base = target_tuple.cast_down_array_base(context.db);
            tpl_pattern_match(context, base, &target_base)?;
        }
        LuaType::Object(target_object) => {
            let target_base = target_object
                .cast_down_array_base(context.db)
                .ok_or(InferFailReason::None)?;
            tpl_pattern_match(context, base, &target_base)?;
        }
        _ => {}
    }

    Ok(())
}

fn table_generic_tpl_pattern_match(
    context: &mut TplContext,
    table_generic_params: &Vec<LuaType>,
    target: &LuaType,
) -> TplPatternMatchResult {
    if table_generic_params.len() != 2 {
        return Err(InferFailReason::None);
    }

    match target {
        LuaType::TableGeneric(target_table_generic_params) => {
            let min_len = table_generic_params
                .len()
                .min(target_table_generic_params.len());
            for i in 0..min_len {
                tpl_pattern_match(
                    context,
                    &table_generic_params[i],
                    &target_table_generic_params[i],
                )?;
            }
        }
        LuaType::Array(target_array_base) => {
            tpl_pattern_match(context, &table_generic_params[0], &LuaType::Integer)?;
            tpl_pattern_match(
                context,
                &table_generic_params[1],
                target_array_base.get_base(),
            )?;
        }
        LuaType::Tuple(target_tuple) => {
            let len = target_tuple.get_types().len();
            let mut keys = Vec::new();
            for i in 0..len {
                keys.push(LuaType::IntegerConst((i as i64) + 1));
            }

            let key_type = LuaType::Union(LuaUnionType::from_vec(keys).into());
            let target_base = target_tuple.cast_down_array_base(context.db);
            tpl_pattern_match(context, &table_generic_params[0], &key_type)?;
            tpl_pattern_match(context, &table_generic_params[1], &target_base)?;
        }
        LuaType::TableConst(inst) => {
            let owner = LuaMemberOwner::Element(inst.clone());
            table_generic_tpl_pattern_member_owner_match(
                context,
                table_generic_params,
                owner,
                &[],
            )?;
        }
        LuaType::Ref(type_id) => {
            let owner = LuaMemberOwner::Type(type_id.clone());
            table_generic_tpl_pattern_member_owner_match(
                context,
                table_generic_params,
                owner,
                &[],
            )?;
        }
        LuaType::Def(type_id) => {
            let owner = LuaMemberOwner::Type(type_id.clone());
            table_generic_tpl_pattern_member_owner_match(
                context,
                table_generic_params,
                owner,
                &[],
            )?;
        }
        LuaType::Generic(generic) => {
            let owner = LuaMemberOwner::Type(generic.get_base_type_id());
            let target_params = generic.get_params();
            table_generic_tpl_pattern_member_owner_match(
                context,
                table_generic_params,
                owner,
                &target_params,
            )?;
        }
        LuaType::Object(obj) => {
            let mut keys = Vec::new();
            let mut values = Vec::new();
            for (k, v) in obj.get_fields() {
                match k {
                    LuaMemberKey::Integer(i) => {
                        keys.push(LuaType::IntegerConst(i.clone()));
                    }
                    LuaMemberKey::Name(s) => {
                        keys.push(LuaType::StringConst(s.clone().into()));
                    }
                    _ => {}
                };
                values.push(v.clone());
            }
            for (k, v) in obj.get_index_access() {
                keys.push(k.clone());
                values.push(v.clone());
            }

            let key_type = LuaType::Union(LuaUnionType::from_vec(keys).into());
            let value_type = LuaType::Union(LuaUnionType::from_vec(values).into());
            tpl_pattern_match(context, &table_generic_params[0], &key_type)?;
            tpl_pattern_match(context, &table_generic_params[1], &value_type)?;
        }

        LuaType::Global | LuaType::Any | LuaType::Table | LuaType::Userdata => {
            // too many
            tpl_pattern_match(context, &table_generic_params[0], &LuaType::Any)?;
            tpl_pattern_match(context, &table_generic_params[1], &LuaType::Any)?;
        }
        _ => {}
    }

    Ok(())
}

// KV 表匹配 ref/def/tableconst
fn table_generic_tpl_pattern_member_owner_match(
    context: &mut TplContext,
    table_generic_params: &Vec<LuaType>,
    owner: LuaMemberOwner,
    target_params: &[LuaType],
) -> TplPatternMatchResult {
    if table_generic_params.len() != 2 {
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
        _ => {
            return Err(InferFailReason::None);
        }
    };

    let members = get_member_map(context.db, &owner_type).ok_or(InferFailReason::None)?;
    // 如果是 pairs 调用, 我们需要尝试寻找元方法, 但目前`__pairs` 被放进成员表中
    if is_pairs_call(context).unwrap_or(false) {
        if try_handle_pairs_metamethod(context, table_generic_params, &members).is_ok() {
            return Ok(());
        }
    }

    let target_key_type = table_generic_params[0].clone();
    let mut keys = Vec::new();
    let mut values = Vec::new();
    for (k, v) in members {
        let key_type = match k {
            LuaMemberKey::Integer(i) => LuaType::IntegerConst(i.clone()),
            LuaMemberKey::Name(s) => LuaType::StringConst(s.clone().into()),
            _ => continue,
        };

        if !target_key_type.is_generic() {
            if !check_type_compact(context.db, &target_key_type, &key_type).is_ok() {
                continue;
            }
        }

        keys.push(key_type);

        let resolve_type = match v.len() {
            0 => LuaType::Any,
            1 => v[0].typ.clone(),
            _ => {
                let mut types = Vec::new();
                for m in v {
                    types.push(m.typ.clone());
                }
                LuaType::from_vec(types)
            }
        };

        values.push(resolve_type);
    }

    if keys.is_empty() {
        find_index_operations(context.db, &owner_type)
            .ok_or(InferFailReason::None)?
            .iter()
            .for_each(|m| {
                if target_key_type.is_generic() {
                    return;
                }
                let key_type = match &m.key {
                    LuaMemberKey::ExprType(typ) => typ.clone(),
                    _ => return,
                };
                if check_type_compact(context.db, &target_key_type, &key_type).is_ok() {
                    keys.push(key_type);
                    values.push(m.typ.clone());
                }
            });
    }

    let key_type = match keys.len() {
        0 => return Err(InferFailReason::None),
        1 => keys.iter().next().cloned().unwrap(),
        _ => LuaType::Union(LuaUnionType::from_vec(keys).into()),
    };
    let value_type = match values.len() {
        1 => values.iter().next().cloned().unwrap(),
        _ => LuaType::Union(LuaUnionType::from_vec(values).into()),
    };

    tpl_pattern_match(context, &table_generic_params[0], &key_type)?;
    tpl_pattern_match(context, &table_generic_params[1], &value_type)?;

    Ok(())
}

fn generic_tpl_pattern_match(
    context: &mut TplContext,
    generic: &LuaGenericType,
    target: &LuaType,
) -> TplPatternMatchResult {
    match target {
        LuaType::Generic(target_generic) => {
            let base = generic.get_base_type_id_ref();
            let target_base = target_generic.get_base_type_id_ref();

            if !is_sub_type_of(context.db, &target_base, &base) {
                return Err(InferFailReason::None);
            }

            let params = generic.get_params();
            let target_params = target_generic.get_params();
            let min_len = params.len().min(target_params.len());
            for i in 0..min_len {
                tpl_pattern_match(context, &params[i], &target_params[i])?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn union_tpl_pattern_match(
    context: &mut TplContext,
    union: &LuaUnionType,
    target: &LuaType,
) -> TplPatternMatchResult {
    for u in union.into_vec() {
        tpl_pattern_match(context, &u, target)?;
    }

    Ok(())
}

fn func_tpl_pattern_match(
    context: &mut TplContext,
    tpl_func: &LuaFunctionType,
    target: &LuaType,
) -> TplPatternMatchResult {
    match target {
        LuaType::DocFunction(target_doc_func) => {
            func_tpl_pattern_match_doc_func(context, tpl_func, target_doc_func)?;
        }
        LuaType::Signature(signature_id) => {
            let signature = context
                .db
                .get_signature_index()
                .get(&signature_id)
                .ok_or(InferFailReason::None)?;
            if !signature.is_resolve_return() {
                return Err(InferFailReason::UnResolveSignatureReturn(*signature_id));
            }
            // TODO: find all tpl_refs that refer to signature's own types and replace them
            //       with their base types. I.e. if we're matching `fun<T>(x: T)`, this `T`
            //       shouldn't end up in our substitutor.
            let fake_doc_func = signature.to_doc_func_type();
            func_tpl_pattern_match_doc_func(context, tpl_func, &fake_doc_func)?;
        }
        _ => {}
    }

    Ok(())
}

fn func_tpl_pattern_match_doc_func(
    context: &mut TplContext,
    tpl_func: &LuaFunctionType,
    target_func: &LuaFunctionType,
) -> TplPatternMatchResult {
    let mut tpl_func_params = tpl_func.get_params().to_vec();
    if tpl_func.is_colon_define() {
        tpl_func_params.insert(0, ("self".to_string(), Some(LuaType::Any)));
    }

    let mut target_func_params = target_func.get_params().to_vec();

    if target_func.is_colon_define() {
        target_func_params.insert(0, ("self".to_string(), Some(LuaType::Any)));
    }

    param_type_list_pattern_match_type_list(context, &tpl_func_params, &target_func_params)?;

    let tpl_return = tpl_func.get_ret();
    let target_return = target_func.get_ret();
    return_type_pattern_match_target_type(context, &tpl_return, &target_return)?;

    Ok(())
}

fn param_type_list_pattern_match_type_list(
    context: &mut TplContext,
    sources: &[(String, Option<LuaType>)],
    targets: &[(String, Option<LuaType>)],
) -> TplPatternMatchResult {
    let type_len = sources.len();
    let mut target_offset = 0;
    for i in 0..type_len {
        let source = match sources.get(i) {
            Some(t) => t.1.clone().unwrap_or(LuaType::Any),
            None => break,
        };

        match &source {
            LuaType::Variadic(inner) => {
                let i = i + target_offset;
                if i >= targets.len() {
                    if let VariadicType::Base(LuaType::TplRef(tpl_ref)) = inner.deref() {
                        let tpl_id = tpl_ref.get_tpl_id();
                        context.substitutor.insert_type(tpl_id, LuaType::Nil);
                    }
                    break;
                }

                if let VariadicType::Base(LuaType::TplRef(generic_tpl)) = inner.deref() {
                    let tpl_id = generic_tpl.get_tpl_id();
                    if let Some(inferred_type_value) = context.substitutor.get(tpl_id) {
                        match inferred_type_value {
                            SubstitutorValue::Type(_) => {
                                continue;
                            }
                            SubstitutorValue::MultiTypes(types) => {
                                if types.len() > 1 {
                                    target_offset += types.len() - 1;
                                }
                                continue;
                            }
                            SubstitutorValue::Params(params) => {
                                if params.len() > 1 {
                                    target_offset += params.len() - 1;
                                }
                                continue;
                            }
                            _ => {}
                        }
                    }
                }

                let mut target_rest_params = &targets[i..];
                // If the variadic parameter is not the last one, then target_rest_params should exclude the parameters that come after it.
                if i + 1 < type_len {
                    let source_rest_len = type_len - i - 1;
                    if source_rest_len >= target_rest_params.len() {
                        continue;
                    }
                    let target_rest_len = target_rest_params.len() - source_rest_len;
                    target_rest_params = &target_rest_params[..target_rest_len];
                    if target_rest_len > 1 {
                        target_offset += target_rest_len - 1;
                    }
                }

                func_varargs_tpl_pattern_match(&inner, &target_rest_params, context.substitutor)?;
            }
            _ => {
                let target = match targets.get(i + target_offset) {
                    Some(t) => t.1.clone().unwrap_or(LuaType::Any),
                    None => break,
                };
                tpl_pattern_match(context, &source, &target)?;
            }
        }
    }

    Ok(())
}

fn return_type_pattern_match_target_type(
    context: &mut TplContext,
    source: &LuaType,
    target: &LuaType,
) -> TplPatternMatchResult {
    match (source, target) {
        // toooooo complex
        (LuaType::Variadic(variadic_source), LuaType::Variadic(variadic_target)) => {
            match variadic_target.deref() {
                VariadicType::Base(target_base) => match variadic_source.deref() {
                    VariadicType::Base(source_base) => {
                        if let LuaType::TplRef(type_ref) = source_base {
                            let tpl_id = type_ref.get_tpl_id();
                            context.substitutor.insert_type(tpl_id, target_base.clone());
                        }
                    }
                    VariadicType::Multi(source_multi) => {
                        for ret_type in source_multi {
                            match ret_type {
                                LuaType::Variadic(inner) => {
                                    if let VariadicType::Base(base) = inner.deref() {
                                        if let LuaType::TplRef(type_ref) = base {
                                            let tpl_id = type_ref.get_tpl_id();
                                            context
                                                .substitutor
                                                .insert_type(tpl_id, target_base.clone());
                                        }
                                    }

                                    break;
                                }
                                LuaType::TplRef(tpl_ref) => {
                                    let tpl_id = tpl_ref.get_tpl_id();
                                    context.substitutor.insert_type(tpl_id, target_base.clone());
                                }
                                _ => {}
                            }
                        }
                    }
                },
                VariadicType::Multi(target_types) => {
                    variadic_tpl_pattern_match(context, variadic_source, target_types)?;
                }
            }
        }
        (LuaType::Variadic(variadic), _) => {
            variadic_tpl_pattern_match(context, variadic, &[target.clone()])?;
        }
        (_, LuaType::Variadic(variadic)) => {
            multi_param_tpl_pattern_match_multi_return(context, &[source.clone()], variadic)?;
        }
        _ => {
            tpl_pattern_match(context, source, target)?;
        }
    }

    Ok(())
}

fn func_varargs_tpl_pattern_match(
    variadic: &VariadicType,
    target_rest_params: &[(String, Option<LuaType>)],
    substitutor: &mut TypeSubstitutor,
) -> TplPatternMatchResult {
    match variadic {
        VariadicType::Base(base) => {
            if let LuaType::TplRef(tpl_ref) = base {
                let tpl_id = tpl_ref.get_tpl_id();
                substitutor.insert_params(
                    tpl_id,
                    target_rest_params
                        .iter()
                        .map(|(n, t)| (n.clone(), t.clone()))
                        .collect(),
                );
            }
        }
        VariadicType::Multi(_) => {}
    }

    Ok(())
}

pub fn variadic_tpl_pattern_match(
    context: &mut TplContext,
    tpl: &VariadicType,
    target_rest_types: &[LuaType],
) -> TplPatternMatchResult {
    match tpl {
        VariadicType::Base(base) => {
            let tpl_ids: Vec<_> = base
                .find_all_tpl()
                .iter()
                .filter_map(LuaType::get_tpl_id)
                .collect();

            if tpl_ids.is_empty() {
                return Ok(());
            }

            let mut multi_types: HashMap<_, _> =
                tpl_ids.iter().map(|tpl_id| (*tpl_id, Vec::new())).collect();

            for (i, target) in target_rest_types.iter().enumerate() {
                match_variadic_element(
                    context,
                    base,
                    &mut multi_types,
                    target,
                    i == target_rest_types.len() - 1,
                )?;
                if multi_types.is_empty() {
                    // All generic parameters failed to infer, there's nothing more to do here.
                    break;
                }
            }

            for (tpl_id, types) in multi_types {
                context.substitutor.insert_multi_types(tpl_id, types);
            }
        }
        VariadicType::Multi(multi) => {
            // This branch only happens when matching function return type.
            // Return type of`fun(): A, B, C...` is encoded as `Multi([A, B, Base(C)])`.
            for (i, ret_type) in multi.iter().enumerate() {
                match ret_type {
                    LuaType::Variadic(inner) => {
                        // Found tailing variadic return, i.e. `C` from example above.
                        if i < target_rest_types.len() {
                            variadic_tpl_pattern_match(context, inner, &target_rest_types[i..])?;
                        }

                        break;
                    }
                    tpl => {
                        // Found regular return type, i.e. `A` or `B` from example above.
                        match target_rest_types.get(i) {
                            Some(typ) => {
                                tpl_pattern_match(context, tpl, typ)?;
                            }
                            None => {
                                break;
                            }
                        };
                    }
                }
            }
        }
    }

    Ok(())
}

fn match_variadic_element(
    context: &mut TplContext,
    base: &LuaType,
    multi_types: &mut HashMap<GenericTplId, Vec<LuaType>>,
    target: &LuaType,
    is_last: bool,
) -> TplPatternMatchResult {
    let mut new_substitutor = context.substitutor.clone();
    for tpl_id in multi_types.keys() {
        new_substitutor.reset_type(*tpl_id);
    }

    let mut new_context = TplContext {
        db: context.db,
        cache: context.cache,
        substitutor: &mut new_substitutor,
        root: context.root.clone(),
        call_expr: context.call_expr.clone(),
    };

    let target_is_base = is_last && target.is_variadic_base();
    if target_is_base {
        let LuaType::Variadic(target) = target else {
            unreachable!();
        };
        let VariadicType::Base(target) = target.deref() else {
            unreachable!();
        };

        tpl_pattern_match(&mut new_context, base, target)?;
    } else {
        tpl_pattern_match(&mut new_context, base, target)?;
    }

    for tpl_id in multi_types.keys().cloned().collect::<Vec<_>>() {
        let matched_type = match new_context.substitutor.get(tpl_id) {
            Some(SubstitutorValue::Type(t)) => t.clone(),
            Some(SubstitutorValue::Params(params)) => {
                let ts = params
                    .iter()
                    .map(|(_, t)| t.clone().unwrap_or(LuaType::Unknown))
                    .collect();
                LuaType::Variadic(VariadicType::Multi(ts).into())
            }
            Some(SubstitutorValue::MultiTypes(ts)) => {
                LuaType::Variadic(VariadicType::Multi(ts.clone()).into())
            }
            Some(SubstitutorValue::MultiBase(t)) => {
                LuaType::Variadic(VariadicType::Base(t.clone()).into())
            }
            None | Some(SubstitutorValue::None) => {
                // Failed to infer type for this generic parameter; abandon it, maybe
                // it will be inferred from another type expression.
                multi_types.remove(&tpl_id);
                continue;
            }
        };

        if target_is_base {
            multi_types
                .get_mut(&tpl_id)
                .unwrap()
                .push(LuaType::Variadic(VariadicType::Base(matched_type).into()));
        } else {
            multi_types.get_mut(&tpl_id).unwrap().push(matched_type);
        }
    }

    Ok(())
}

fn tuple_tpl_pattern_match(
    context: &mut TplContext,
    tpl_tuple: &LuaTupleType,
    target: &LuaType,
) -> TplPatternMatchResult {
    match target {
        LuaType::Tuple(target_tuple) => {
            let tpl_tuple_types = tpl_tuple.get_types();
            let target_tuple_types = target_tuple.get_types();
            let tpl_tuple_len = tpl_tuple_types.len();
            for i in 0..tpl_tuple_len {
                let tpl_type = &tpl_tuple_types[i];

                if let LuaType::Variadic(inner) = tpl_type {
                    let target_rest_types = &target_tuple_types[i..];
                    variadic_tpl_pattern_match(context, inner, target_rest_types)?;
                    break;
                }

                let target_type = match target_tuple_types.get(i) {
                    Some(t) => t,
                    None => break,
                };

                tpl_pattern_match(context, tpl_type, target_type)?;
            }
        }
        LuaType::Array(target_array_base) => {
            let tupl_tuple_types = tpl_tuple.get_types();
            let last_type = tupl_tuple_types.last().ok_or(InferFailReason::None)?;
            if let LuaType::Variadic(inner) = last_type {
                match inner.deref() {
                    VariadicType::Base(base) => {
                        if let LuaType::TplRef(tpl_ref) = base {
                            let tpl_id = tpl_ref.get_tpl_id();
                            context
                                .substitutor
                                .insert_multi_base(tpl_id, target_array_base.get_base().clone());
                        }
                    }
                    VariadicType::Multi(_) => {}
                }
            }
        }
        // LuaType::TableConst() // TODO!
        _ => {}
    }

    Ok(())
}

fn escape_alias(db: &DbIndex, may_alias: &LuaType) -> LuaType {
    match may_alias {
        LuaType::Ref(type_id) => {
            if let Some(type_decl) = db.get_type_index().get_type_decl(type_id) {
                if type_decl.is_alias() {
                    if let Some(origin_type) = type_decl.get_alias_origin(db, None) {
                        return origin_type.clone();
                    }
                }
            }
        }
        _ => {}
    }

    may_alias.clone()
}

fn is_pairs_call(context: &mut TplContext) -> Option<bool> {
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
    let name = decl.get_name();
    if name != "pairs" {
        return None;
    }
    Some(true)
}

fn try_handle_pairs_metamethod(
    context: &mut TplContext,
    table_generic_params: &[LuaType],
    members: &HashMap<LuaMemberKey, Vec<LuaMemberInfo>>,
) -> TplPatternMatchResult {
    let pairs_member = members
        .get(&LuaMemberKey::Name("__pairs".into()))
        .ok_or(InferFailReason::None)?
        .get(0)
        .ok_or(InferFailReason::None)?;
    // 获取迭代函数返回类型
    let meta_return = match &pairs_member.typ {
        LuaType::Signature(signature_id) => context
            .db
            .get_signature_index()
            .get(signature_id)
            .map(|s| s.get_return_type()),
        LuaType::DocFunction(doc_func) => Some(doc_func.get_ret().clone()),
        _ => None,
    }
    .ok_or(InferFailReason::None)?;

    // 解析出迭代函数返回类型
    let final_return_type = match meta_return {
        LuaType::DocFunction(doc_func) => Some(doc_func.get_ret().clone()),
        LuaType::Signature(signature_id) => context
            .db
            .get_signature_index()
            .get(&signature_id)
            .map(|s| s.get_return_type()),
        _ => None,
    };

    if let Some(LuaType::Variadic(variadic)) = &final_return_type {
        let key_type = variadic.get_type(0).ok_or(InferFailReason::None)?;
        let value_type = variadic.get_type(1).ok_or(InferFailReason::None)?;
        tpl_pattern_match(context, &table_generic_params[0], &key_type)?;
        tpl_pattern_match(context, &table_generic_params[1], &value_type)?;
        return Ok(());
    }
    Err(InferFailReason::None)
}
