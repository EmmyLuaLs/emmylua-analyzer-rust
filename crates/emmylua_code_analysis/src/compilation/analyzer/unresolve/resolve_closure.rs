use std::sync::Arc;

use emmylua_parser::{LuaAstNode, LuaIndexMemberExpr, LuaTableExpr, LuaVarExpr};

use crate::{
    DbIndex, InferFailReason, InferGuard, InferGuardRef, LuaDocParamInfo, LuaDocReturnInfo,
    LuaFunctionType, LuaInferCache, LuaSignature, LuaType, SignatureReturnStatus, TypeOps,
    get_real_type, infer_call_expr_func, infer_expr, infer_table_should_be,
};

use super::{
    ResolveResult, UnResolveCallClosureParams, UnResolveClosureReturn, UnResolveParentAst,
    UnResolveParentClosureParams, UnResolveReturn, find_decl_function::find_decl_function_type,
    resolve::try_resolve_return_point,
};

pub fn try_resolve_call_closure_params(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    closure_params: &mut UnResolveCallClosureParams,
) -> ResolveResult {
    let call_expr = closure_params.call_expr.clone();
    let prefix_expr = call_expr.get_prefix_expr().ok_or(InferFailReason::None)?;
    let call_expr_type = infer_expr(db, cache, prefix_expr)?;

    let call_doc_func = infer_call_expr_func(
        db,
        cache,
        call_expr.clone(),
        call_expr_type,
        &InferGuard::new(),
        None,
    )?;

    let colon_call = call_expr.is_colon_call();
    let colon_define = call_doc_func.is_colon_define();

    let mut param_idx = closure_params.param_idx;
    match (colon_call, colon_define) {
        (true, false) => {
            param_idx += 1;
        }
        (false, true) => {
            if param_idx == 0 {
                return Ok(());
            }

            param_idx -= 1;
        }
        _ => {}
    }

    let (async_state, params_to_insert) = if let Some(param_type) =
        call_doc_func.get_params().get(param_idx)
    {
        let Some(param_type) = get_real_type(db, param_type.1.as_ref().unwrap_or(&LuaType::Any))
        else {
            return Ok(());
        };
        match param_type {
            LuaType::DocFunction(func) => (func.get_async_state(), func.get_params().to_vec()),
            LuaType::Union(union_types) => {
                if let Some(LuaType::DocFunction(func)) = union_types
                    .into_vec()
                    .iter()
                    .find(|typ| matches!(typ, LuaType::DocFunction(_)))
                {
                    (func.get_async_state(), func.get_params().to_vec())
                } else {
                    return Ok(());
                }
            }
            _ => return Ok(()),
        }
    } else {
        return Ok(());
    };

    let signature = db
        .get_signature_index_mut()
        .get_mut(&closure_params.signature_id)
        .ok_or(InferFailReason::None)?;

    let signature_params = &mut signature.param_docs;
    for (idx, (name, type_ref)) in params_to_insert.iter().enumerate() {
        if signature_params.contains_key(&idx) {
            continue;
        }

        signature_params.insert(
            idx,
            LuaDocParamInfo {
                name: name.clone(),
                type_ref: type_ref.clone().unwrap_or(LuaType::Any),
                description: None,
                nullable: false,
                attributes: None,
            },
        );
    }

    signature.async_state = async_state;

    Ok(())
}

pub fn try_resolve_closure_return(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    closure_return: &mut UnResolveClosureReturn,
) -> ResolveResult {
    let call_expr = closure_return.call_expr.clone();
    let prefix_expr = call_expr.get_prefix_expr().ok_or(InferFailReason::None)?;
    let call_expr_type = infer_expr(db, cache, prefix_expr)?;
    let mut param_idx = closure_return.param_idx;
    let call_doc_func = infer_call_expr_func(
        db,
        cache,
        call_expr.clone(),
        call_expr_type,
        &InferGuard::new(),
        None,
    )?;

    let colon_define = call_doc_func.is_colon_define();
    let colon_call = call_expr.is_colon_call();
    match (colon_define, colon_call) {
        (true, false) => {
            if param_idx == 0 {
                return Ok(());
            }
            param_idx -= 1
        }
        (false, true) => {
            param_idx += 1;
        }
        _ => {}
    }

    let ret_type = if let Some(param_type) = call_doc_func.get_params().get(param_idx) {
        let Some(param_type) = get_real_type(db, param_type.1.as_ref().unwrap_or(&LuaType::Any))
        else {
            return Ok(());
        };
        if let LuaType::DocFunction(func) = param_type {
            func.get_ret().clone()
        } else {
            return Ok(());
        }
    } else {
        return Ok(());
    };

    let signature = db
        .get_signature_index_mut()
        .get_mut(&closure_return.signature_id)
        .ok_or(InferFailReason::None)?;

    if ret_type.contain_tpl() {
        return try_convert_to_func_body_infer(db, cache, closure_return);
    }

    match signature.resolve_return {
        SignatureReturnStatus::UnResolve => {}
        SignatureReturnStatus::InferResolve => {
            signature.return_docs.clear();
        }
        _ => return Ok(()),
    }

    signature.return_docs.push(LuaDocReturnInfo {
        name: None,
        type_ref: ret_type.clone(),
        description: None,
        attributes: None,
    });

    signature.resolve_return = SignatureReturnStatus::DocResolve;
    Ok(())
}

fn try_convert_to_func_body_infer(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    closure_return: &mut UnResolveClosureReturn,
) -> ResolveResult {
    let mut unresolve = UnResolveReturn {
        file_id: closure_return.file_id,
        signature_id: closure_return.signature_id,
        return_points: closure_return.return_points.clone(),
    };

    try_resolve_return_point(db, cache, &mut unresolve)
}

pub fn try_resolve_closure_parent_params(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    closure_params: &mut UnResolveParentClosureParams,
) -> ResolveResult {
    let signature = db
        .get_signature_index()
        .get(&closure_params.signature_id)
        .ok_or(InferFailReason::None)?;
    if !signature.param_docs.is_empty() {
        return Ok(());
    }
    let self_type;
    let member_type = match &closure_params.parent_ast {
        UnResolveParentAst::LuaFuncStat(func_stat) => {
            let func_name = func_stat.get_func_name().ok_or(InferFailReason::None)?;
            match func_name {
                LuaVarExpr::IndexExpr(index_expr) => {
                    let prefix_expr = index_expr.get_prefix_expr().ok_or(InferFailReason::None)?;
                    let prefix_type = infer_expr(db, cache, prefix_expr)?;
                    self_type = Some(prefix_type.clone());
                    find_best_function_type(
                        db,
                        cache,
                        &prefix_type,
                        LuaIndexMemberExpr::IndexExpr(index_expr),
                        signature,
                    )
                    .ok_or(InferFailReason::None)?
                }
                _ => return Ok(()),
            }
        }
        UnResolveParentAst::LuaTableField(table_field) => {
            let parnet_table_expr = table_field
                .get_parent::<LuaTableExpr>()
                .ok_or(InferFailReason::None)?;
            let parent_table_type = infer_table_should_be(db, cache, parnet_table_expr)?;
            self_type = Some(parent_table_type.clone());
            find_best_function_type(
                db,
                cache,
                &parent_table_type,
                LuaIndexMemberExpr::TableField(table_field.clone()),
                signature,
            )
            .ok_or(InferFailReason::None)?
        }
        UnResolveParentAst::LuaAssignStat(assign) => {
            let (vars, exprs) = assign.get_var_and_expr_list();
            let position = closure_params.signature_id.get_position();
            let idx = exprs
                .iter()
                .position(|expr| expr.get_position() == position)
                .ok_or(InferFailReason::None)?;
            let var = vars.get(idx).ok_or(InferFailReason::None)?;
            match var {
                LuaVarExpr::IndexExpr(index_expr) => {
                    let prefix_expr = index_expr.get_prefix_expr().ok_or(InferFailReason::None)?;
                    let prefix_expr_type = infer_expr(db, cache, prefix_expr)?;
                    self_type = Some(prefix_expr_type.clone());
                    find_best_function_type(
                        db,
                        cache,
                        &prefix_expr_type,
                        LuaIndexMemberExpr::IndexExpr(index_expr.clone()),
                        signature,
                    )
                    .ok_or(InferFailReason::None)?
                }
                _ => return Ok(()),
            }
        }
    };

    resolve_closure_member_type(
        db,
        closure_params,
        &member_type,
        self_type,
        &InferGuard::new(),
    )
}

fn resolve_closure_member_type(
    db: &mut DbIndex,
    closure_params: &UnResolveParentClosureParams,
    member_type: &LuaType,
    self_type: Option<LuaType>,
    infer_guard: &InferGuardRef,
) -> ResolveResult {
    match &member_type {
        LuaType::DocFunction(doc_func) => {
            resolve_doc_function(db, closure_params, doc_func, self_type)
        }
        LuaType::Signature(id) => {
            if id == &closure_params.signature_id {
                return Ok(());
            }
            let signature = db.get_signature_index().get(id);

            if let Some(signature) = signature {
                let fake_doc_function = signature.to_doc_func_type();
                resolve_doc_function(db, closure_params, &fake_doc_function, self_type)
            } else {
                Ok(())
            }
        }
        LuaType::Union(union_types) => {
            let signature = db
                .get_signature_index()
                .get(&closure_params.signature_id)
                .ok_or(InferFailReason::None)?;
            let mut final_params = signature.get_type_params().to_vec();
            let mut final_ret = LuaType::Unknown;

            let mut multi_function_type = Vec::new();
            for typ in union_types.into_vec() {
                match typ {
                    LuaType::DocFunction(func) => {
                        multi_function_type.push(func.clone());
                    }
                    LuaType::Ref(ref_id) => {
                        if infer_guard.check(&ref_id).is_err() {
                            continue;
                        }
                        let type_decl = db
                            .get_type_index()
                            .get_type_decl(&ref_id)
                            .ok_or(InferFailReason::None)?;

                        if let Some(origin) = type_decl.get_alias_origin(db, None)
                            && let LuaType::DocFunction(f) = origin
                        {
                            multi_function_type.push(f);
                        }
                    }
                    _ => {}
                };
            }

            let mut variadic_type = LuaType::Unknown;
            for doc_func in multi_function_type {
                let mut doc_params = doc_func.get_params().to_vec();
                match (doc_func.is_colon_define(), signature.is_colon_define) {
                    (true, false) => {
                        // 原始签名是冒号定义, 但未解析的签名不是冒号定义, 即要插入第一个参数
                        doc_params.insert(0, ("self".to_string(), Some(LuaType::SelfInfer)));
                    }
                    (false, true) => {
                        // 原始签名不是冒号定义, 但未解析的签名是冒号定义, 即要删除第一个参数
                        if !doc_params.is_empty() {
                            doc_params.remove(0);
                        }
                    }
                    _ => {}
                }

                for (idx, param) in doc_params.iter().enumerate() {
                    if let Some(final_param) = final_params.get(idx) {
                        if final_param.0 == "..." {
                            // 如果`doc_params`当前与之后的参数的类型不一致, 那么`variadic_type`为`Any`
                            for i in idx..doc_params.len() {
                                if let Some(param) = doc_params.get(i)
                                    && let Some(typ) = &param.1
                                {
                                    if variadic_type == LuaType::Unknown {
                                        variadic_type = typ.clone();
                                    } else if variadic_type != *typ {
                                        variadic_type = LuaType::Any;
                                    }
                                }
                            }

                            break;
                        }
                        let new_type = TypeOps::Union.apply(
                            db,
                            final_param.1.as_ref().unwrap_or(&LuaType::Unknown),
                            param.1.as_ref().unwrap_or(&LuaType::Unknown),
                        );
                        final_params[idx] = (final_param.0.clone(), Some(new_type));
                    } else {
                        final_params.push((param.0.clone(), param.1.clone()));
                    }
                }

                final_ret = TypeOps::Union.apply(db, &final_ret, doc_func.get_ret());
            }

            if !variadic_type.is_unknown()
                && let Some(param) = final_params.last_mut()
            {
                param.1 = Some(variadic_type);
            }

            resolve_doc_function(
                db,
                closure_params,
                &LuaFunctionType::new(
                    signature.async_state,
                    signature.is_colon_define,
                    signature.is_vararg,
                    final_params,
                    final_ret,
                ),
                self_type,
            )
        }
        LuaType::Ref(ref_id) => {
            infer_guard.check(ref_id)?;
            let type_decl = db
                .get_type_index()
                .get_type_decl(ref_id)
                .ok_or(InferFailReason::None)?;

            if type_decl.is_alias()
                && let Some(origin) = type_decl.get_alias_origin(db, None)
            {
                return resolve_closure_member_type(
                    db,
                    closure_params,
                    &origin,
                    self_type,
                    infer_guard,
                );
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn resolve_doc_function(
    db: &mut DbIndex,
    closure_params: &UnResolveParentClosureParams,
    doc_func: &LuaFunctionType,
    self_type: Option<LuaType>,
) -> ResolveResult {
    let signature = db
        .get_signature_index_mut()
        .get_mut(&closure_params.signature_id)
        .ok_or(InferFailReason::None)?;

    signature.async_state = doc_func.get_async_state();

    let mut doc_params = doc_func.get_params().to_vec();
    // doc_func 是往上追溯的有效签名, signature 是未解析的签名
    match (doc_func.is_colon_define(), signature.is_colon_define) {
        (true, false) => {
            // 原始签名是冒号定义, 但未解析的签名不是冒号定义, 即要插入第一个参数
            doc_params.insert(0, ("self".to_string(), Some(LuaType::SelfInfer)));
        }
        (false, true) => {
            if !doc_params.is_empty() {
                doc_params.remove(0);
            }
        }
        _ => {}
    }

    if let Some(self_type) = self_type
        && let Some((_, Some(typ))) = doc_params.first()
        && typ.is_self_infer()
    {
        doc_params[0].1 = Some(self_type);
    }

    for (index, param) in doc_params.iter().enumerate() {
        let name = signature.params.get(index).unwrap_or(&param.0);
        signature.param_docs.insert(
            index,
            LuaDocParamInfo {
                name: name.clone(),
                type_ref: param.1.clone().unwrap_or(LuaType::Any),
                description: None,
                nullable: false,
                attributes: None,
            },
        );
    }

    if signature.resolve_return == SignatureReturnStatus::UnResolve
        || signature.resolve_return == SignatureReturnStatus::InferResolve
    {
        signature.resolve_return = SignatureReturnStatus::DocResolve;
        signature.return_docs.clear();
        signature.return_docs.push(LuaDocReturnInfo {
            name: None,
            type_ref: doc_func.get_ret().clone(),
            description: None,
            attributes: None,
        });
    }

    Ok(())
}

fn filter_signature_type(db: &DbIndex, typ: &LuaType) -> Option<Vec<Arc<LuaFunctionType>>> {
    let mut result: Vec<Arc<LuaFunctionType>> = Vec::new();
    let mut stack = Vec::new();
    stack.push(typ.clone());
    let guard = InferGuard::new();
    while let Some(typ) = stack.pop() {
        match typ {
            LuaType::DocFunction(func) => {
                result.push(func.clone());
            }
            LuaType::Union(union) => {
                let types = union.into_vec();
                for typ in types.into_iter().rev() {
                    stack.push(typ);
                }
            }
            LuaType::Ref(type_ref_id) => {
                guard.check(&type_ref_id).ok()?;
                let type_decl = db.get_type_index().get_type_decl(&type_ref_id)?;
                if let Some(func) = type_decl.get_alias_origin(db, None) {
                    match func {
                        LuaType::DocFunction(f) => {
                            result.push(f.clone());
                        }
                        LuaType::Union(union) => {
                            let types = union.into_vec();
                            for typ in types.into_iter().rev() {
                                stack.push(typ);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

fn find_best_function_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type: &LuaType,
    index_member_expr: LuaIndexMemberExpr,
    origin_signature: &LuaSignature,
) -> Option<LuaType> {
    // 寻找非自身定义的签名
    if let Ok(result) = find_decl_function_type(db, cache, prefix_type, index_member_expr) {
        if result.is_current_owner {
            // 对应当前类型下的声明, 我们需要过滤掉所有`signature`类型
            if let Some(filtered_types) = filter_signature_type(db, &result.typ) {
                match filtered_types.len() {
                    0 => {}
                    1 => return Some(LuaType::DocFunction(filtered_types[0].clone())),
                    _ => {
                        return Some(LuaType::from_vec(
                            filtered_types
                                .into_iter()
                                .map(|func| LuaType::DocFunction(func.clone()))
                                .collect(),
                        ));
                    }
                }
            }
        } else {
            return Some(result.typ);
        }
    }

    match origin_signature.overloads.len() {
        0 => None,
        1 => origin_signature
            .overloads
            .clone()
            .into_iter()
            .next()
            .map(LuaType::DocFunction),
        _ => Some(LuaType::from_vec(
            origin_signature
                .overloads
                .clone()
                .into_iter()
                .map(LuaType::DocFunction)
                .collect(),
        )),
    }
}
