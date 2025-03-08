use crate::{
    infer_call_expr_func, infer_expr, DbIndex, InferGuard, LuaDocParamInfo, LuaDocReturnInfo,
    LuaInferConfig, LuaPropertyOwnerId, LuaType, SignatureReturnStatus,
};

use super::{
    resolve::try_resolve_return_point, UnResolveClosureParams, UnResolveClosureReturn,
    UnResolveReturn,
};

pub fn try_resolve_closure_params(
    db: &mut DbIndex,
    config: &mut LuaInferConfig,
    closure_params: &UnResolveClosureParams,
) -> Option<bool> {
    let call_expr = closure_params.call_expr.clone();
    let prefix_expr = call_expr.get_prefix_expr()?;
    let call_expr_type = infer_expr(db, config, prefix_expr.into())?;

    let call_doc_func = infer_call_expr_func(
        db,
        config,
        call_expr.clone(),
        call_expr_type,
        &mut InferGuard::new(),
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
                return Some(true);
            }

            param_idx -= 1;
        }
        _ => {}
    }

    let expr_closure_params = if let Some(param_type) = call_doc_func.get_params().get(param_idx) {
        if let Some(LuaType::DocFunction(func)) = &param_type.1 {
            if func.is_async() {
                let file_id = closure_params.file_id;
                let property_owner = LuaPropertyOwnerId::Signature(closure_params.signature_id);
                db.get_property_index_mut()
                    .add_async(file_id, property_owner);
            }

            func.get_params()
        } else {
            return Some(true);
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

    Some(true)
}

pub fn try_resolve_closure_return(
    db: &mut DbIndex,
    config: &mut LuaInferConfig,
    closure_return: &UnResolveClosureReturn,
) -> Option<bool> {
    let call_expr = closure_return.call_expr.clone();
    let prefix_expr = call_expr.get_prefix_expr()?;
    let call_expr_type = infer_expr(db, config, prefix_expr.into())?;
    let param_idx = closure_return.param_idx;
    let call_doc_func = infer_call_expr_func(
        db,
        config,
        call_expr.clone(),
        call_expr_type,
        &mut InferGuard::new(),
        None,
    )?;

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
        return try_convert_to_func_body_infer(db, config, closure_return);
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
    config: &mut LuaInferConfig,
    closure_return: &UnResolveClosureReturn,
) -> Option<bool> {
    let unresolve = UnResolveReturn {
        file_id: closure_return.file_id,
        signature_id: closure_return.signature_id,
        return_points: closure_return.return_points.clone(),
    };

    try_resolve_return_point(db, config, &unresolve)
}
