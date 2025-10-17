use std::{ops::Deref, sync::Arc};

use emmylua_parser::{LuaCallExpr, LuaChunk, LuaExpr};

use crate::{
    DbIndex, FlowNode, FlowTree, InferFailReason, InferGuard, LuaAliasCallKind, LuaAliasCallType,
    LuaFunctionType, LuaInferCache, LuaSignatureCast, LuaSignatureId, LuaType, TypeOps,
    infer_call_expr_func, infer_expr,
    semantic::infer::{
        VarRefId,
        narrow::{
            ResultTypeOrContinue, condition_flow::InferConditionFlow, get_single_antecedent,
            get_type_at_cast_flow::cast_type, get_type_at_flow::get_type_at_flow,
            narrow_false_or_nil, remove_false_or_nil, var_ref_id::get_var_expr_var_ref_id,
        },
    },
};

#[allow(clippy::too_many_arguments)]
pub fn get_type_at_call_expr(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    call_expr: LuaCallExpr,
    condition_flow: InferConditionFlow,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    let Some(prefix_expr) = call_expr.get_prefix_expr() else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let maybe_func = infer_expr(db, cache, prefix_expr.clone())?;
    match maybe_func {
        LuaType::DocFunction(f) => {
            let return_type = f.get_ret();
            match return_type {
                LuaType::TypeGuard(_) => get_type_at_call_expr_by_type_guard(
                    db,
                    tree,
                    cache,
                    root,
                    var_ref_id,
                    flow_node,
                    call_expr,
                    f,
                    condition_flow,
                ),
                _ => {
                    // If the return type is not a type guard, we cannot infer the type cast.
                    Ok(ResultTypeOrContinue::Continue)
                }
            }
        }
        LuaType::Signature(signature_id) => {
            let Some(signature) = db.get_signature_index().get(&signature_id) else {
                return Ok(ResultTypeOrContinue::Continue);
            };

            let ret = signature.get_return_type();
            match ret {
                LuaType::TypeGuard(_) => {
                    return get_type_at_call_expr_by_type_guard(
                        db,
                        tree,
                        cache,
                        root,
                        var_ref_id,
                        flow_node,
                        call_expr,
                        signature.to_doc_func_type(),
                        condition_flow,
                    );
                }
                LuaType::Call(call) => {
                    return get_type_at_call_expr_by_call(
                        db,
                        tree,
                        cache,
                        root,
                        var_ref_id,
                        flow_node,
                        call_expr,
                        &call,
                        condition_flow,
                    );
                }
                _ => {}
            }

            let Some(signature_cast) = db.get_flow_index().get_signature_cast(&signature_id) else {
                return Ok(ResultTypeOrContinue::Continue);
            };

            match signature_cast.name.as_str() {
                "self" => get_type_at_call_expr_by_signature_self(
                    db,
                    tree,
                    cache,
                    root,
                    var_ref_id,
                    flow_node,
                    prefix_expr,
                    signature_cast,
                    signature_id,
                    condition_flow,
                ),
                name => get_type_at_call_expr_by_signature_param_name(
                    db,
                    tree,
                    cache,
                    root,
                    var_ref_id,
                    flow_node,
                    call_expr,
                    signature_cast,
                    signature_id,
                    name,
                    condition_flow,
                ),
            }
        }
        _ => {
            // If the prefix expression is not a function, we cannot infer the type cast.
            Ok(ResultTypeOrContinue::Continue)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn get_type_at_call_expr_by_type_guard(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    call_expr: LuaCallExpr,
    func_type: Arc<LuaFunctionType>,
    condition_flow: InferConditionFlow,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    let Some(arg_list) = call_expr.get_args_list() else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let Some(first_arg) = arg_list.get_args().next() else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let Some(maybe_ref_id) = get_var_expr_var_ref_id(db, cache, first_arg) else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    if maybe_ref_id != *var_ref_id {
        return Ok(ResultTypeOrContinue::Continue);
    }

    let mut return_type = func_type.get_ret().clone();
    if return_type.contain_tpl() {
        let call_expr_type = LuaType::DocFunction(func_type);
        let inst_func = infer_call_expr_func(
            db,
            cache,
            call_expr,
            call_expr_type,
            &InferGuard::new(),
            None,
        )?;

        return_type = inst_func.get_ret().clone();
    }

    let guard_type = match return_type {
        LuaType::TypeGuard(guard) => guard.deref().clone(),
        _ => return Ok(ResultTypeOrContinue::Continue),
    };

    match condition_flow {
        InferConditionFlow::TrueCondition => Ok(ResultTypeOrContinue::Result(guard_type)),
        InferConditionFlow::FalseCondition => {
            let antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
            let antecedent_type =
                get_type_at_flow(db, tree, cache, root, var_ref_id, antecedent_flow_id)?;
            Ok(ResultTypeOrContinue::Result(TypeOps::Remove.apply(
                db,
                &antecedent_type,
                &guard_type,
            )))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn get_type_at_call_expr_by_signature_self(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    call_prefix: LuaExpr,
    signature_cast: &LuaSignatureCast,
    signature_id: LuaSignatureId,
    condition_flow: InferConditionFlow,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    let LuaExpr::IndexExpr(call_prefix_index) = call_prefix else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let Some(self_expr) = call_prefix_index.get_prefix_expr() else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let Some(name_var_ref_id) = get_var_expr_var_ref_id(db, cache, self_expr) else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    if name_var_ref_id != *var_ref_id {
        return Ok(ResultTypeOrContinue::Continue);
    }

    let antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
    let antecedent_type = get_type_at_flow(db, tree, cache, root, var_ref_id, antecedent_flow_id)?;

    let Some(syntax_tree) = db.get_vfs().get_syntax_tree(&signature_id.get_file_id()) else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let signature_root = syntax_tree.get_chunk_node();

    // Choose the appropriate cast based on condition_flow and whether fallback exists
    let result_type = match condition_flow {
        InferConditionFlow::TrueCondition => {
            let Some(cast_op_type) = signature_cast.cast.to_node(&signature_root) else {
                return Ok(ResultTypeOrContinue::Continue);
            };
            cast_type(
                db,
                signature_id.get_file_id(),
                cast_op_type,
                antecedent_type,
                condition_flow,
            )?
        }
        InferConditionFlow::FalseCondition => {
            // Use fallback_cast if available, otherwise use the default behavior
            if let Some(fallback_cast_ptr) = &signature_cast.fallback_cast {
                let Some(fallback_op_type) = fallback_cast_ptr.to_node(&signature_root) else {
                    return Ok(ResultTypeOrContinue::Continue);
                };
                cast_type(
                    db,
                    signature_id.get_file_id(),
                    fallback_op_type,
                    antecedent_type.clone(),
                    InferConditionFlow::TrueCondition, // Apply fallback as force cast
                )?
            } else {
                // Original behavior: remove the true type from antecedent
                let Some(cast_op_type) = signature_cast.cast.to_node(&signature_root) else {
                    return Ok(ResultTypeOrContinue::Continue);
                };
                cast_type(
                    db,
                    signature_id.get_file_id(),
                    cast_op_type,
                    antecedent_type,
                    condition_flow,
                )?
            }
        }
    };

    Ok(ResultTypeOrContinue::Result(result_type))
}

#[allow(clippy::too_many_arguments)]
fn get_type_at_call_expr_by_signature_param_name(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    call_expr: LuaCallExpr,
    signature_cast: &LuaSignatureCast,
    signature_id: LuaSignatureId,
    name: &str,
    condition_flow: InferConditionFlow,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    let colon_call = call_expr.is_colon_call();
    let Some(arg_list) = call_expr.get_args_list() else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let Some(signature) = db.get_signature_index().get(&signature_id) else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let Some(mut param_idx) = signature.find_param_idx(name) else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let colon_define = signature.is_colon_define;
    match (colon_call, colon_define) {
        (true, false) => {
            if param_idx == 0 {
                return Ok(ResultTypeOrContinue::Continue);
            }

            param_idx -= 1;
        }
        (false, true) => {
            param_idx += 1;
        }
        _ => {}
    }

    let Some(expr) = arg_list.get_args().nth(param_idx) else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let Some(name_var_ref_id) = get_var_expr_var_ref_id(db, cache, expr) else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    if name_var_ref_id != *var_ref_id {
        return Ok(ResultTypeOrContinue::Continue);
    }

    let antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
    let antecedent_type = get_type_at_flow(db, tree, cache, root, var_ref_id, antecedent_flow_id)?;

    let Some(syntax_tree) = db.get_vfs().get_syntax_tree(&signature_id.get_file_id()) else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let signature_root = syntax_tree.get_chunk_node();

    // Choose the appropriate cast based on condition_flow and whether fallback exists
    let result_type = match condition_flow {
        InferConditionFlow::TrueCondition => {
            let Some(cast_op_type) = signature_cast.cast.to_node(&signature_root) else {
                return Ok(ResultTypeOrContinue::Continue);
            };
            cast_type(
                db,
                signature_id.get_file_id(),
                cast_op_type,
                antecedent_type,
                condition_flow,
            )?
        }
        InferConditionFlow::FalseCondition => {
            // Use fallback_cast if available, otherwise use the default behavior
            if let Some(fallback_cast_ptr) = &signature_cast.fallback_cast {
                let Some(fallback_op_type) = fallback_cast_ptr.to_node(&signature_root) else {
                    return Ok(ResultTypeOrContinue::Continue);
                };
                cast_type(
                    db,
                    signature_id.get_file_id(),
                    fallback_op_type,
                    antecedent_type.clone(),
                    InferConditionFlow::TrueCondition, // Apply fallback as force cast
                )?
            } else {
                // Original behavior: remove the true type from antecedent
                let Some(cast_op_type) = signature_cast.cast.to_node(&signature_root) else {
                    return Ok(ResultTypeOrContinue::Continue);
                };
                cast_type(
                    db,
                    signature_id.get_file_id(),
                    cast_op_type,
                    antecedent_type,
                    condition_flow,
                )?
            }
        }
    };

    Ok(ResultTypeOrContinue::Result(result_type))
}

#[allow(unused, clippy::too_many_arguments)]
fn get_type_at_call_expr_by_call(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    call_expr: LuaCallExpr,
    alias_call_type: &Arc<LuaAliasCallType>,
    condition_flow: InferConditionFlow,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    let Some(maybe_ref_id) =
        get_var_expr_var_ref_id(db, cache, LuaExpr::CallExpr(call_expr.clone()))
    else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    if maybe_ref_id != *var_ref_id {
        return Ok(ResultTypeOrContinue::Continue);
    }

    if alias_call_type.get_call_kind() == LuaAliasCallKind::RawGet {
        let antecedent_type = infer_expr(db, cache, LuaExpr::CallExpr(call_expr))?;
        let result_type = match condition_flow {
            InferConditionFlow::FalseCondition => narrow_false_or_nil(db, antecedent_type),
            InferConditionFlow::TrueCondition => remove_false_or_nil(antecedent_type),
        };
        return Ok(ResultTypeOrContinue::Result(result_type));
    };

    Ok(ResultTypeOrContinue::Continue)
}
