use std::{ops::Deref, sync::Arc};

use emmylua_parser::{LuaCallExpr, LuaChunk, LuaExpr, LuaIndexMemberExpr};

use crate::{
    DbIndex, FlowNode, FlowTree, InferFailReason, InferGuard, LuaAliasCallKind, LuaAliasCallType,
    LuaFunctionType, LuaInferCache, LuaSignatureId, LuaType,
    semantic::infer::{
        VarRefId,
        infer_index::infer_member_by_member_key,
        narrow::{
            condition_flow::{ConditionFlowAction, InferConditionFlow, PendingConditionNarrow},
            get_var_ref_type, infer_expr_at_antecedent_flow, narrow_false_or_nil,
            remove_false_or_nil,
            var_ref_id::get_var_expr_var_ref_id,
        },
    },
    semantic::instantiate_func_generic,
};

pub fn get_type_at_call_expr(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    call_expr: LuaCallExpr,
    condition_flow: InferConditionFlow,
) -> Result<ConditionFlowAction, InferFailReason> {
    let Some(prefix_expr) = call_expr.get_prefix_expr() else {
        return Ok(ConditionFlowAction::Continue);
    };

    let maybe_func = if call_expr.is_colon_call()
        && let LuaExpr::IndexExpr(index_expr) = &prefix_expr
        && let Some(self_expr) = index_expr.get_prefix_expr()
        && let Some(self_var_ref_id) = get_var_expr_var_ref_id(db, cache, self_expr)
        && self_var_ref_id == *var_ref_id
    {
        let self_type = get_var_ref_type(db, cache, var_ref_id)?;
        let member_type = infer_member_by_member_key(
            db,
            cache,
            &self_type,
            LuaIndexMemberExpr::IndexExpr(index_expr.clone()),
            &InferGuard::new(),
        )?;

        if needs_antecedent_same_var_colon_lookup(&member_type) {
            // Keep the dedicated pending case here: replay needs the antecedent type
            // for member lookup itself, not just for applying a cast after lookup.
            return Ok(ConditionFlowAction::Pending(
                PendingConditionNarrow::SameVarColonCall {
                    idx: LuaIndexMemberExpr::IndexExpr(index_expr.clone()),
                    condition_flow,
                },
            ));
        }
        member_type
    } else {
        let Some(maybe_func) =
            infer_expr_at_antecedent_flow(db, tree, cache, root, flow_node, prefix_expr.clone())?
        else {
            return Ok(ConditionFlowAction::Continue);
        };
        maybe_func
    };

    match maybe_func {
        LuaType::DocFunction(f) => {
            let return_type = f.get_ret();
            match return_type {
                LuaType::TypeGuard(_) => get_type_at_call_expr_by_type_guard(
                    db,
                    cache,
                    var_ref_id,
                    call_expr,
                    f,
                    condition_flow,
                ),
                _ => Ok(ConditionFlowAction::Continue),
            }
        }
        LuaType::Signature(signature_id) => {
            let Some(signature) = db.get_signature_index().get(&signature_id) else {
                return Ok(ConditionFlowAction::Continue);
            };

            let ret = signature.get_return_type();
            match ret {
                LuaType::TypeGuard(_) => {
                    return get_type_at_call_expr_by_type_guard(
                        db,
                        cache,
                        var_ref_id,
                        call_expr,
                        signature.to_doc_func_type(),
                        condition_flow,
                    );
                }
                LuaType::Call(call) => {
                    return get_type_at_call_expr_by_call(
                        db,
                        cache,
                        var_ref_id,
                        call_expr,
                        &call,
                        condition_flow,
                    );
                }
                _ => {}
            }

            let Some(signature_cast) = db.get_flow_index().get_signature_cast(&signature_id) else {
                return Ok(ConditionFlowAction::Continue);
            };

            match signature_cast.name.as_str() {
                "self" => get_type_at_call_expr_by_signature_self(
                    db,
                    cache,
                    var_ref_id,
                    prefix_expr,
                    signature_id,
                    condition_flow,
                ),
                name => get_type_at_call_expr_by_signature_param_name(
                    db,
                    cache,
                    var_ref_id,
                    call_expr,
                    signature_id,
                    name,
                    condition_flow,
                ),
            }
        }
        _ => Ok(ConditionFlowAction::Continue),
    }
}

fn needs_antecedent_same_var_colon_lookup(member_type: &LuaType) -> bool {
    let candidate_members = match member_type {
        LuaType::Union(union_type) => union_type.into_vec(),
        LuaType::MultiLineUnion(multi_union) => match multi_union.to_union() {
            LuaType::Union(union_type) => union_type.into_vec(),
            _ => return false,
        },
        _ => return false,
    };

    candidate_members.len() > 1
        && candidate_members.iter().any(|ty| {
            matches!(
                ty,
                LuaType::DocFunction(_) | LuaType::Signature(_) | LuaType::Call(_)
            )
        })
}

fn get_type_guard_call_info(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    call_expr: LuaCallExpr,
    func_type: Arc<LuaFunctionType>,
) -> Result<Option<(VarRefId, LuaType)>, InferFailReason> {
    let Some(arg_list) = call_expr.get_args_list() else {
        return Ok(None);
    };

    let Some(first_arg) = arg_list.get_args().next() else {
        return Ok(None);
    };

    let Some(maybe_ref_id) = get_var_expr_var_ref_id(db, cache, first_arg) else {
        return Ok(None);
    };

    let mut return_type = func_type.get_ret().clone();
    if return_type.contain_tpl() {
        let Ok(inst_func) = cache.with_no_flow(|cache| {
            instantiate_func_generic(db, cache, func_type.as_ref(), call_expr)
        }) else {
            return Ok(None);
        };
        return_type = inst_func.get_ret().clone();
    }

    let LuaType::TypeGuard(guard) = return_type else {
        return Ok(None);
    };

    Ok(Some((maybe_ref_id, guard.deref().clone())))
}

fn get_type_at_call_expr_by_type_guard(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    var_ref_id: &VarRefId,
    call_expr: LuaCallExpr,
    func_type: Arc<LuaFunctionType>,
    condition_flow: InferConditionFlow,
) -> Result<ConditionFlowAction, InferFailReason> {
    let Some((maybe_ref_id, guard_type)) =
        get_type_guard_call_info(db, cache, call_expr, func_type)?
    else {
        return Ok(ConditionFlowAction::Continue);
    };

    if maybe_ref_id != *var_ref_id {
        return Ok(ConditionFlowAction::Continue);
    }

    Ok(ConditionFlowAction::Pending(
        PendingConditionNarrow::TypeGuard {
            narrow: guard_type,
            condition_flow,
        },
    ))
}

fn get_type_at_call_expr_by_signature_self(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    var_ref_id: &VarRefId,
    call_prefix: LuaExpr,
    signature_id: LuaSignatureId,
    condition_flow: InferConditionFlow,
) -> Result<ConditionFlowAction, InferFailReason> {
    let LuaExpr::IndexExpr(call_prefix_index) = call_prefix else {
        return Ok(ConditionFlowAction::Continue);
    };

    let Some(self_expr) = call_prefix_index.get_prefix_expr() else {
        return Ok(ConditionFlowAction::Continue);
    };

    let Some(name_var_ref_id) = get_var_expr_var_ref_id(db, cache, self_expr) else {
        return Ok(ConditionFlowAction::Continue);
    };

    if name_var_ref_id != *var_ref_id {
        return Ok(ConditionFlowAction::Continue);
    }

    Ok(ConditionFlowAction::Pending(
        PendingConditionNarrow::SignatureCast {
            signature_id,
            condition_flow,
        },
    ))
}

#[allow(clippy::too_many_arguments)]
fn get_type_at_call_expr_by_signature_param_name(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    var_ref_id: &VarRefId,
    call_expr: LuaCallExpr,
    signature_id: LuaSignatureId,
    name: &str,
    condition_flow: InferConditionFlow,
) -> Result<ConditionFlowAction, InferFailReason> {
    let colon_call = call_expr.is_colon_call();
    let Some(arg_list) = call_expr.get_args_list() else {
        return Ok(ConditionFlowAction::Continue);
    };

    let Some(signature) = db.get_signature_index().get(&signature_id) else {
        return Ok(ConditionFlowAction::Continue);
    };

    let Some(mut param_idx) = signature.find_param_idx(name) else {
        return Ok(ConditionFlowAction::Continue);
    };

    let colon_define = signature.is_colon_define;
    match (colon_call, colon_define) {
        (true, false) => {
            if param_idx == 0 {
                return Ok(ConditionFlowAction::Continue);
            }

            param_idx -= 1;
        }
        (false, true) => {
            param_idx += 1;
        }
        _ => {}
    }

    let Some(expr) = arg_list.get_args().nth(param_idx) else {
        return Ok(ConditionFlowAction::Continue);
    };

    let Some(name_var_ref_id) = get_var_expr_var_ref_id(db, cache, expr) else {
        return Ok(ConditionFlowAction::Continue);
    };

    if name_var_ref_id != *var_ref_id {
        return Ok(ConditionFlowAction::Continue);
    }

    Ok(ConditionFlowAction::Pending(
        PendingConditionNarrow::SignatureCast {
            signature_id,
            condition_flow,
        },
    ))
}

fn get_type_at_call_expr_by_call(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    var_ref_id: &VarRefId,
    call_expr: LuaCallExpr,
    alias_call_type: &Arc<LuaAliasCallType>,
    condition_flow: InferConditionFlow,
) -> Result<ConditionFlowAction, InferFailReason> {
    let Some(maybe_ref_id) =
        get_var_expr_var_ref_id(db, cache, LuaExpr::CallExpr(call_expr.clone()))
    else {
        return Ok(ConditionFlowAction::Continue);
    };

    if maybe_ref_id != *var_ref_id {
        return Ok(ConditionFlowAction::Continue);
    }

    if alias_call_type.get_call_kind() == LuaAliasCallKind::RawGet {
        let antecedent_type = get_var_ref_type(db, cache, var_ref_id)?;
        let result_type = match condition_flow {
            InferConditionFlow::FalseCondition => narrow_false_or_nil(db, antecedent_type),
            InferConditionFlow::TrueCondition => remove_false_or_nil(antecedent_type),
        };
        return Ok(ConditionFlowAction::Result(result_type));
    };

    Ok(ConditionFlowAction::Continue)
}
