use std::sync::Arc;

use emmylua_parser::{LuaAstNode, LuaTableExpr, LuaVarExpr};

use crate::{
    infer_call_expr_func, infer_expr, infer_member_map, infer_table_should_be, DbIndex,
    InferFailReason, InferGuard, LuaDocParamInfo, LuaDocReturnInfo, LuaFunctionType, LuaInferCache,
    LuaMemberInfo, LuaSemanticDeclId, LuaSignatureId, LuaType, LuaTypeDeclId, LuaUnionType,
    SignatureReturnStatus, TypeOps,
};

use super::{
    check_reason::check_reach_reason, resolve::try_resolve_return_point,
    UnResolveCallClosureParams, UnResolveClosureReturn, UnResolveParentAst,
    UnResolveParentClosureParams, UnResolveReturn,
};

pub fn try_resolve_closure_params(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    closure_params: &mut UnResolveCallClosureParams,
) -> Option<bool> {
    if !check_reach_reason(db, cache, &closure_params.reason).unwrap_or(false) {
        return None;
    }

    let call_expr = closure_params.call_expr.clone();
    let prefix_expr = call_expr.get_prefix_expr()?;
    let call_expr_type = infer_expr(db, cache, prefix_expr.into()).ok()?;

    let call_doc_func = match infer_call_expr_func(
        db,
        cache,
        call_expr.clone(),
        call_expr_type,
        &mut InferGuard::new(),
        None,
    ) {
        Ok(call_doc_func) => call_doc_func,
        Err(reason) => {
            closure_params.reason = reason;
            return None;
        }
    };

    let colon_call = call_expr.is_colon_call();
    let colon_define = call_doc_func.is_colon_define();

    let mut param_idx = closure_params.param_idx;
    match (colon_call, colon_define) {
        (true, false) => {
            param_idx += 1;
        }
        (false, true) => {
            if param_idx == 0 {
                return Some(true);
            }

            param_idx -= 1;
        }
        _ => {}
    }

    let is_async;
    let expr_closure_params = if let Some(param_type) = call_doc_func.get_params().get(param_idx) {
        match &param_type.1 {
            Some(LuaType::DocFunction(func)) => {
                is_async = func.is_async();
                func.get_params()
            }
            Some(LuaType::Union(union_types)) => {
                if let Some(LuaType::DocFunction(func)) = union_types
                    .get_types()
                    .iter()
                    .find(|typ| matches!(typ, LuaType::DocFunction(_)))
                {
                    is_async = func.is_async();
                    func.get_params()
                } else {
                    return Some(true);
                }
            }
            _ => return Some(true),
        }
    } else {
        return Some(true);
    };

    let signature = db
        .get_signature_index_mut()
        .get_mut(&closure_params.signature_id)?;

    let signature_params = &mut signature.param_docs;
    for (idx, (name, type_ref)) in expr_closure_params.iter().enumerate() {
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
            },
        );
    }

    signature.is_async = is_async;

    Some(true)
}

pub fn try_resolve_closure_return(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    closure_return: &mut UnResolveClosureReturn,
) -> Option<bool> {
    if !check_reach_reason(db, cache, &closure_return.reason).unwrap_or(false) {
        return None;
    }

    let call_expr = closure_return.call_expr.clone();
    let prefix_expr = call_expr.get_prefix_expr()?;
    let call_expr_type = infer_expr(db, cache, prefix_expr.into()).ok()?;
    let mut param_idx = closure_return.param_idx;
    let call_doc_func = match infer_call_expr_func(
        db,
        cache,
        call_expr.clone(),
        call_expr_type,
        &mut InferGuard::new(),
        None,
    ) {
        Ok(call_doc_func) => call_doc_func,
        Err(reason) => {
            closure_return.reason = reason;
            return None;
        }
    };

    let colon_define = call_doc_func.is_colon_define();
    let colon_call = call_expr.is_colon_call();
    match (colon_define, colon_call) {
        (true, false) => {
            if param_idx == 0 {
                return Some(true);
            }
            param_idx -= 1
        }
        (false, true) => {
            param_idx += 1;
        }
        _ => {}
    }

    let expr_closure_return = if let Some(param_type) = call_doc_func.get_params().get(param_idx) {
        if let Some(LuaType::DocFunction(func)) = &param_type.1 {
            func.get_ret()
        } else {
            return Some(true);
        }
    } else {
        return Some(true);
    };

    let signature = db
        .get_signature_index_mut()
        .get_mut(&closure_return.signature_id)?;

    if expr_closure_return.iter().any(|it| it.contain_tpl()) {
        return try_convert_to_func_body_infer(db, cache, closure_return);
    }

    for ret_type in expr_closure_return {
        signature.return_docs.push(LuaDocReturnInfo {
            name: None,
            type_ref: ret_type.clone(),
            description: None,
        });
    }

    signature.resolve_return = SignatureReturnStatus::DocResolve;
    Some(true)
}

fn try_convert_to_func_body_infer(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    closure_return: &mut UnResolveClosureReturn,
) -> Option<bool> {
    let mut unresolve = UnResolveReturn {
        file_id: closure_return.file_id,
        signature_id: closure_return.signature_id,
        return_points: closure_return.return_points.clone(),
        reason: closure_return.reason.clone(),
    };

    try_resolve_return_point(db, cache, &mut unresolve)
}

pub fn try_resolve_closure_parent_params(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    closure_params: &mut UnResolveParentClosureParams,
) -> Option<bool> {
    if !check_reach_reason(db, cache, &closure_params.reason).unwrap_or(false) {
        return None;
    }

    let signature = db.get_signature_index().get(&closure_params.signature_id)?;

    if !signature.param_docs.is_empty() {
        return Some(true);
    }
    let self_type;
    let member_type = match &closure_params.parent_ast {
        UnResolveParentAst::LuaFuncStat(func_stat) => {
            let func_name = func_stat.get_func_name()?;
            match func_name {
                LuaVarExpr::IndexExpr(index_expr) => {
                    let typ = match infer_expr(db, cache, index_expr.get_prefix_expr()?) {
                        Ok(typ) => typ,
                        Err(reason) => {
                            closure_params.reason = reason;
                            return None;
                        }
                    };
                    self_type = Some(typ.clone());
                    find_best_function_type(db, cache, &typ, &closure_params.signature_id)
                }
                _ => return Some(true),
            }
        }
        UnResolveParentAst::LuaTableField(table_field) => {
            let parnet_table_expr = table_field
                .get_parent::<LuaTableExpr>()
                .ok_or(InferFailReason::None)
                .ok()?;
            let typ = match infer_table_should_be(db, cache, parnet_table_expr) {
                Ok(typ) => typ,
                Err(reason) => {
                    closure_params.reason = reason;
                    return None;
                }
            };
            self_type = Some(typ.clone());
            find_best_function_type(db, cache, &typ, &closure_params.signature_id)
        }
        UnResolveParentAst::LuaAssignStat(assign) => {
            let (vars, exprs) = assign.get_var_and_expr_list();
            let position = closure_params.signature_id.get_position();
            let idx = exprs
                .iter()
                .position(|expr| expr.get_position() == position)?;
            let var = vars.get(idx)?;
            match var {
                LuaVarExpr::IndexExpr(index_expr) => {
                    let typ = match infer_expr(db, cache, index_expr.get_prefix_expr()?) {
                        Ok(typ) => typ,
                        Err(reason) => {
                            closure_params.reason = reason;
                            return None;
                        }
                    };
                    self_type = Some(typ.clone());
                    find_best_function_type(db, cache, &typ, &closure_params.signature_id)
                }
                _ => return Some(true),
            }
        }
    };

    let member_type = match member_type {
        Some(member_type) => member_type,
        None => return Some(true),
    };

    match &member_type {
        LuaType::DocFunction(doc_func) => {
            resolve_doc_function(db, closure_params, doc_func, self_type)
        }
        LuaType::Signature(id) => {
            if id == &closure_params.signature_id {
                return Some(true);
            }
            let signature = db.get_signature_index().get(id);

            if let Some(signature) = signature {
                let fake_doc_function = LuaFunctionType::new(
                    signature.is_async,
                    signature.is_colon_define,
                    signature.get_type_params(),
                    signature.get_return_types(),
                );
                resolve_doc_function(db, closure_params, &fake_doc_function, self_type)
            } else {
                Some(true)
            }
        }
        LuaType::Union(union_types) => {
            let mut final_params = signature.get_type_params().to_vec();
            for typ in union_types.get_types() {
                let LuaType::DocFunction(doc_func) = typ else {
                    continue;
                };
                let mut doc_params = doc_func.get_params().to_vec();
                match (doc_func.is_colon_define(), signature.is_colon_define) {
                    (true, true) | (false, false) => {}
                    (true, false) => {
                        // 原始签名是冒号定义, 但未解析的签名不是冒号定义, 即要插入第一个参数
                        doc_params.insert(0, ("self".to_string(), Some(LuaType::SelfInfer)));
                    }
                    (false, true) => {
                        // 原始签名不是冒号定义, 但未解析的签名是冒号定义, 即要删除第一个参数
                        doc_params.remove(0);
                    }
                }
                // 如果第一个参数是 self, 则需要将 self 的类型设置为 self_type
                if doc_params.get(0).map_or(false, |(_, typ)| match typ {
                    Some(LuaType::SelfInfer) => true,
                    _ => false,
                }) {
                    if let Some(self_type) = &self_type {
                        doc_params[0].1 = Some(self_type.clone());
                    }
                }
                for (idx, param) in doc_params.iter().enumerate() {
                    if let Some(final_param) = final_params.get(idx) {
                        if final_param.0 == "..." {
                            continue;
                        }
                        let new_type = TypeOps::Union.apply(
                            db,
                            final_param.1.as_ref().unwrap_or(&LuaType::Unknown),
                            param.1.as_ref().unwrap_or(&LuaType::Unknown),
                        );
                        final_params[idx] = (final_param.0.clone(), Some(new_type));
                    }
                }
            }
            resolve_doc_function(
                db,
                closure_params,
                &LuaFunctionType::new(
                    signature.is_async,
                    signature.is_colon_define,
                    final_params,
                    signature.get_return_types(),
                ),
                self_type,
            )
        }
        _ => Some(true),
    }
}

fn resolve_doc_function(
    db: &mut DbIndex,
    closure_params: &UnResolveParentClosureParams,
    doc_func: &LuaFunctionType,
    self_type: Option<LuaType>,
) -> Option<bool> {
    let signature = db
        .get_signature_index_mut()
        .get_mut(&closure_params.signature_id)?;

    if doc_func.is_async() {
        signature.is_async = true;
    }

    let mut doc_params = doc_func.get_params().to_vec();
    // doc_func 是往上追溯的有效签名, signature 是未解析的签名
    match (doc_func.is_colon_define(), signature.is_colon_define) {
        (true, true) | (false, false) => {}
        (true, false) => {
            // 原始签名是冒号定义, 但未解析的签名不是冒号定义, 即要插入第一个参数
            doc_params.insert(0, ("self".to_string(), Some(LuaType::SelfInfer)));
        }
        (false, true) => {
            // 原始签名不是冒号定义, 但未解析的签名是冒号定义, 即要删除第一个参数
            doc_params.remove(0);
        }
    }
    // 如果第一个参数是 self, 则需要将 self 的类型设置为 self_type
    if doc_params.get(0).map_or(false, |(_, typ)| match typ {
        Some(LuaType::SelfInfer) => true,
        _ => false,
    }) {
        if let Some(self_type) = self_type {
            doc_params[0].1 = Some(self_type);
        }
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
            },
        );
    }

    if signature.resolve_return == SignatureReturnStatus::UnResolve
        || signature.resolve_return == SignatureReturnStatus::InferResolve
    {
        signature.return_docs.clear();
        if !doc_func.get_ret().is_empty() {
            signature.resolve_return = SignatureReturnStatus::DocResolve;
            for ret in doc_func.get_ret() {
                signature.return_docs.push(LuaDocReturnInfo {
                    name: None,
                    type_ref: ret.clone(),
                    description: None,
                });
            }
        }
    }
    Some(true)
}

fn get_owner_type_id(db: &DbIndex, info: &LuaMemberInfo) -> Option<LuaTypeDeclId> {
    match &info.property_owner_id {
        Some(LuaSemanticDeclId::Member(member_id)) => {
            if let Some(owner) = db.get_member_index().get_current_owner(member_id) {
                return owner.get_type_id().cloned();
            }
            None
        }
        _ => None,
    }
}

fn find_best_function_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type: &LuaType,
    signature_id: &LuaSignatureId,
) -> Option<LuaType> {
    let member_info_map = infer_member_map(db, prefix_type)?;
    let mut current_type_id = None;

    let target_infos = member_info_map.into_values().find(|infos| {
        infos.iter().any(|info| {
            if let LuaType::Signature(id) = &info.typ {
                if id == signature_id {
                    current_type_id = get_owner_type_id(db, info);
                    return true;
                }
            }
            false
        })
    })?;

    let mut current_function_types = Vec::with_capacity(target_infos.len());
    // 父类或许也应该返回联合类型
    let mut parent_function_type = None;

    for info in target_infos {
        let function_type =
            get_final_function_type(db, cache, &info.typ).unwrap_or_else(|| info.typ.clone());

        // 所有者类型一致, 不是父类
        if get_owner_type_id(db, &info) == current_type_id {
            match &function_type {
                LuaType::Signature(id) => {
                    if let Some(cur_signature) = db.get_signature_index().get(id) {
                        // 只需要重载声明
                        if cur_signature.param_docs.is_empty() {
                            current_function_types.extend(cur_signature.overloads.iter().cloned());
                        }
                    }
                }
                LuaType::DocFunction(doc_func) => {
                    // 使用迭代器优化参数计数
                    if doc_func.get_params().iter().any(|(_, typ)| typ.is_some()) {
                        current_function_types.push(doc_func.clone());
                    }
                }
                _ => {}
            }
            continue;
        }

        // 父类处理
        let has_params = match &function_type {
            LuaType::Signature(id) => db
                .get_signature_index()
                .get(id)
                .map_or(false, |sig| !sig.param_docs.is_empty()),
            LuaType::DocFunction(doc_func) => {
                doc_func.get_params().iter().any(|(_, typ)| typ.is_some())
            }
            _ => false,
        };

        if has_params {
            parent_function_type = Some(function_type);
        }
    }
    match current_function_types.len() {
        0 => parent_function_type,
        1 => current_function_types
            .into_iter()
            .next()
            .map(LuaType::DocFunction),
        _ => Some(LuaType::Union(Arc::new(LuaUnionType::new(
            current_function_types
                .into_iter()
                .map(LuaType::DocFunction)
                .collect(),
        )))),
    }
}

fn get_final_function_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    origin: &LuaType,
) -> Option<LuaType> {
    match origin {
        LuaType::Signature(_) => Some(origin.clone()),
        LuaType::DocFunction(_) => Some(origin.clone()),
        LuaType::Ref(decl_id) => {
            let decl = db.get_type_index().get_type_decl(decl_id)?;
            if decl.is_alias() {
                let origin_type = decl.get_alias_origin(db, None)?;
                get_final_function_type(db, cache, &origin_type)
            } else {
                Some(origin.clone())
            }
        }
        LuaType::Union(union_types) => {
            for typ in union_types.get_types() {
                let final_type = get_final_function_type(db, cache, typ);
                if final_type.is_some() {
                    return final_type;
                }
            }
            None
        }
        _ => None,
    }
}
