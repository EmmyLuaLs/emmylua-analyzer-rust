use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaBinaryExpr, LuaChunk, LuaExpr, LuaIndexMemberExpr,
    LuaLiteralToken, UnaryOperator,
};

use crate::{
    DbIndex, FlowNode, FlowTree, InferFailReason, InferGuard, LuaArrayLen, LuaArrayType,
    LuaInferCache, LuaType, TypeOps, infer_expr,
    semantic::infer::{
        VarRefId,
        infer_index::infer_member_by_member_key,
        narrow::{
            ResultTypeOrContinue,
            condition_flow::{
                ConditionFlowAction, InferConditionFlow, PendingConditionNarrow,
                always_literal_equal, call_flow::get_type_at_call_expr,
                correlated_flow::prepare_var_from_return_overload_condition,
            },
            get_single_antecedent,
            get_type_at_flow::get_type_at_flow,
            get_var_ref_type, narrow_down_type,
            var_ref_id::get_var_expr_var_ref_id,
        },
    },
};

#[allow(clippy::too_many_arguments)]
pub fn get_type_at_binary_expr(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    binary_expr: LuaBinaryExpr,
    condition_flow: InferConditionFlow,
) -> Result<ConditionFlowAction, InferFailReason> {
    let Some(op_token) = binary_expr.get_op_token() else {
        return Ok(ConditionFlowAction::Continue);
    };

    let Some((left_expr, right_expr)) = binary_expr.get_exprs() else {
        return Ok(ConditionFlowAction::Continue);
    };

    match op_token.get_op() {
        BinaryOperator::OpEq => try_get_at_eq_or_neq_expr(
            db,
            tree,
            cache,
            root,
            var_ref_id,
            flow_node,
            left_expr,
            right_expr,
            condition_flow,
        ),
        BinaryOperator::OpNe => try_get_at_eq_or_neq_expr(
            db,
            tree,
            cache,
            root,
            var_ref_id,
            flow_node,
            left_expr,
            right_expr,
            condition_flow.get_negated(),
        ),
        BinaryOperator::OpGt => try_get_at_gt_or_ge_expr(
            db,
            tree,
            cache,
            root,
            var_ref_id,
            flow_node,
            left_expr,
            right_expr,
            condition_flow,
            true,
        )
        .map(Into::into),
        BinaryOperator::OpGe => try_get_at_gt_or_ge_expr(
            db,
            tree,
            cache,
            root,
            var_ref_id,
            flow_node,
            left_expr,
            right_expr,
            condition_flow,
            false,
        )
        .map(Into::into),
        _ => Ok(ConditionFlowAction::Continue),
    }
}

#[allow(clippy::too_many_arguments)]
fn try_get_at_eq_or_neq_expr(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    left_expr: LuaExpr,
    right_expr: LuaExpr,
    condition_flow: InferConditionFlow,
) -> Result<ConditionFlowAction, InferFailReason> {
    if let Some(action) = maybe_type_guard_binary_action(
        db,
        tree,
        cache,
        root,
        var_ref_id,
        flow_node,
        left_expr.clone(),
        right_expr.clone(),
        condition_flow,
    )? {
        return Ok(action);
    }

    let (left_expr, right_expr) = if !matches!(
        left_expr,
        LuaExpr::NameExpr(_) | LuaExpr::CallExpr(_) | LuaExpr::IndexExpr(_) | LuaExpr::UnaryExpr(_)
    ) && matches!(
        right_expr,
        LuaExpr::NameExpr(_) | LuaExpr::CallExpr(_) | LuaExpr::IndexExpr(_) | LuaExpr::UnaryExpr(_)
    ) {
        (right_expr, left_expr)
    } else {
        (left_expr, right_expr)
    };

    if let Some(action) = maybe_field_literal_eq_action(
        db,
        tree,
        cache,
        root,
        var_ref_id,
        flow_node,
        left_expr.clone(),
        right_expr.clone(),
        condition_flow,
    )? {
        return Ok(action);
    }

    get_var_eq_condition_action(
        db,
        tree,
        cache,
        root,
        var_ref_id,
        flow_node,
        left_expr,
        right_expr,
        condition_flow,
    )
}

#[allow(clippy::too_many_arguments)]
fn try_get_at_gt_or_ge_expr(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    left_expr: LuaExpr,
    right_expr: LuaExpr,
    condition_flow: InferConditionFlow,
    gt: bool,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    match left_expr {
        LuaExpr::UnaryExpr(unary_expr) => {
            let Some(op) = unary_expr.get_op_token() else {
                return Ok(ResultTypeOrContinue::Continue);
            };

            match op.get_op() {
                UnaryOperator::OpLen => {}
                _ => return Ok(ResultTypeOrContinue::Continue),
            };

            let Some(expr) = unary_expr.get_expr() else {
                return Ok(ResultTypeOrContinue::Continue);
            };

            let Some(maybe_ref_id) = get_var_expr_var_ref_id(db, cache, expr) else {
                return Ok(ResultTypeOrContinue::Continue);
            };

            if maybe_ref_id != *var_ref_id {
                // If the reference declaration ID does not match, we cannot narrow it
                return Ok(ResultTypeOrContinue::Continue);
            }

            let right_expr_type = infer_expr(db, cache, right_expr)?;
            let antecedent_flow_id = get_single_antecedent(flow_node)?;
            let antecedent_type =
                get_type_at_flow(db, tree, cache, root, var_ref_id, antecedent_flow_id)?;
            match (&antecedent_type, &right_expr_type) {
                (
                    LuaType::Array(array_type),
                    LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i),
                ) => {
                    let add = if gt { 1 } else { 0 };
                    if condition_flow.is_true() {
                        let new_array_type = LuaArrayType::new(
                            array_type.get_base().clone(),
                            LuaArrayLen::Max(*i + add),
                        );
                        return Ok(ResultTypeOrContinue::Result(LuaType::Array(
                            new_array_type.into(),
                        )));
                    }
                }
                _ => return Ok(ResultTypeOrContinue::Continue),
            }

            Ok(ResultTypeOrContinue::Continue)
        }
        _ => Ok(ResultTypeOrContinue::Continue),
    }
}

#[allow(clippy::too_many_arguments)]
fn maybe_type_guard_binary_action(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    left_expr: LuaExpr,
    right_expr: LuaExpr,
    condition_flow: InferConditionFlow,
) -> Result<Option<ConditionFlowAction>, InferFailReason> {
    let (candidate_expr, literal_expr) = match (left_expr, right_expr) {
        // If either side is a literal expression and the other side is a type guard call expression
        // (or ref), we can narrow it
        (candidate_expr, LuaExpr::LiteralExpr(literal_expr))
        | (LuaExpr::LiteralExpr(literal_expr), candidate_expr) => {
            (Some(candidate_expr), Some(literal_expr))
        }
        _ => (None, None),
    };

    let (Some(candidate_expr), Some(LuaLiteralToken::String(literal_string))) =
        (candidate_expr, literal_expr.and_then(|e| e.get_literal()))
    else {
        return Ok(None);
    };

    let candidate_expr = match candidate_expr {
        // may ref a type value
        LuaExpr::NameExpr(name_expr) => db
            .get_reference_index()
            .get_var_reference_decl(&cache.get_file_id(), name_expr.get_range())
            .and_then(|decl_id| tree.get_decl_ref_expr(&decl_id))
            .and_then(|expr_ptr| expr_ptr.to_node(root)),
        expr => Some(expr),
    };

    let Some(type_guard_expr) = candidate_expr.and_then(|expr| match expr {
        LuaExpr::CallExpr(call_expr) if call_expr.is_type() => Some(call_expr),
        _ => None,
    }) else {
        return Ok(None);
    };

    let Some(narrow) = type_call_name_to_type(&literal_string.get_value()) else {
        return Ok(None);
    };

    let Some(arg) = type_guard_expr
        .get_args_list()
        .and_then(|arg_list| arg_list.get_args().next())
    else {
        return Ok(None);
    };

    let Some(maybe_var_ref_id) = get_var_expr_var_ref_id(db, cache, arg) else {
        // If we cannot find a reference declaration ID, we cannot narrow it
        return Ok(None);
    };

    if maybe_var_ref_id == *var_ref_id {
        return Ok(Some(ConditionFlowAction::pending(
            PendingConditionNarrow::TypeGuard {
                narrow,
                condition_flow,
            },
        )));
    }

    let Some(discriminant_decl_id) = maybe_var_ref_id.get_decl_id_ref() else {
        return Ok(None);
    };
    let Some(target_decl_id) = var_ref_id.get_decl_id_ref() else {
        return Ok(None);
    };
    if !tree.has_decl_multi_return_refs(&discriminant_decl_id)
        || !tree.has_decl_multi_return_refs(&target_decl_id)
    {
        return Ok(None);
    }

    let antecedent_flow_id = get_single_antecedent(flow_node)?;
    let antecedent_type =
        get_type_at_flow(db, tree, cache, root, &maybe_var_ref_id, antecedent_flow_id)?;
    let narrowed_discriminant_type = match condition_flow {
        InferConditionFlow::TrueCondition => {
            narrow_down_type(db, antecedent_type, narrow.clone(), None).unwrap_or(narrow.clone())
        }
        InferConditionFlow::FalseCondition => TypeOps::Remove.apply(db, &antecedent_type, &narrow),
    };

    Ok(prepare_var_from_return_overload_condition(
        db,
        tree,
        cache,
        root,
        var_ref_id,
        flow_node,
        discriminant_decl_id,
        type_guard_expr.get_position(),
        &narrowed_discriminant_type,
    )?
    .map(PendingConditionNarrow::Correlated)
    .map(ConditionFlowAction::pending))
}

/// Maps the string result of Lua's builtin `type()` call to the corresponding `LuaType`.
fn type_call_name_to_type(literal_string: &str) -> Option<LuaType> {
    Some(match literal_string {
        "number" => LuaType::Number,
        "string" => LuaType::String,
        "boolean" => LuaType::Boolean,
        "table" => LuaType::Table,
        "function" => LuaType::Function,
        "thread" => LuaType::Thread,
        "userdata" => LuaType::Userdata,
        "nil" => LuaType::Nil,
        _ => return None,
    })
}

fn narrow_eq_condition(
    db: &DbIndex,
    antecedent_type: LuaType,
    right_expr_type: LuaType,
    condition_flow: InferConditionFlow,
    allow_literal_equivalence: bool,
) -> LuaType {
    match condition_flow {
        InferConditionFlow::TrueCondition => {
            let left_maybe_type = TypeOps::Intersect.apply(db, &antecedent_type, &right_expr_type);

            if left_maybe_type.is_never() {
                if allow_literal_equivalence {
                    let literal_matches = match &antecedent_type {
                        LuaType::Union(union) => union
                            .into_vec()
                            .into_iter()
                            .filter(|candidate| always_literal_equal(candidate, &right_expr_type))
                            .collect::<Vec<_>>(),
                        _ if always_literal_equal(&antecedent_type, &right_expr_type) => {
                            vec![antecedent_type.clone()]
                        }
                        _ => Vec::new(),
                    };

                    if !literal_matches.is_empty() {
                        return LuaType::from_vec(literal_matches);
                    }
                }

                antecedent_type
            } else {
                left_maybe_type
            }
        }
        InferConditionFlow::FalseCondition => {
            TypeOps::Remove.apply(db, &antecedent_type, &right_expr_type)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn get_var_eq_condition_action(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    left_expr: LuaExpr,
    right_expr: LuaExpr,
    condition_flow: InferConditionFlow,
) -> Result<ConditionFlowAction, InferFailReason> {
    // only check left as need narrow
    match left_expr {
        LuaExpr::NameExpr(left_name_expr) => {
            let Some(maybe_ref_id) =
                get_var_expr_var_ref_id(db, cache, LuaExpr::NameExpr(left_name_expr.clone()))
            else {
                return Ok(ConditionFlowAction::Continue);
            };

            if maybe_ref_id != *var_ref_id {
                let Some(discriminant_decl_id) = maybe_ref_id.get_decl_id_ref() else {
                    return Ok(ConditionFlowAction::Continue);
                };
                let Some(target_decl_id) = var_ref_id.get_decl_id_ref() else {
                    return Ok(ConditionFlowAction::Continue);
                };
                if !tree.has_decl_multi_return_refs(&discriminant_decl_id)
                    || !tree.has_decl_multi_return_refs(&target_decl_id)
                {
                    return Ok(ConditionFlowAction::Continue);
                }
                let antecedent_flow_id = get_single_antecedent(flow_node)?;
                let right_expr_type = infer_expr(db, cache, right_expr)?;
                let antecedent_type =
                    get_type_at_flow(db, tree, cache, root, &maybe_ref_id, antecedent_flow_id)?;
                let narrowed_discriminant_type =
                    narrow_eq_condition(db, antecedent_type, right_expr_type, condition_flow, true);
                return Ok(prepare_var_from_return_overload_condition(
                    db,
                    tree,
                    cache,
                    root,
                    var_ref_id,
                    flow_node,
                    discriminant_decl_id,
                    left_name_expr.get_position(),
                    &narrowed_discriminant_type,
                )?
                .map(PendingConditionNarrow::Correlated)
                .map(ConditionFlowAction::pending)
                .unwrap_or(ConditionFlowAction::Continue));
            }

            let right_expr_type = infer_expr(db, cache, right_expr)?;
            let result_type = match condition_flow {
                InferConditionFlow::TrueCondition => {
                    // self 是特殊的, 我们删除其 nil 类型
                    if var_ref_id.is_self_ref() && !right_expr_type.is_nil() {
                        TypeOps::Remove.apply(db, &right_expr_type, &LuaType::Nil)
                    } else {
                        return Ok(ConditionFlowAction::pending(PendingConditionNarrow::Eq {
                            right_expr_type,
                            condition_flow,
                        }));
                    }
                }
                InferConditionFlow::FalseCondition => {
                    return Ok(ConditionFlowAction::pending(PendingConditionNarrow::Eq {
                        right_expr_type,
                        condition_flow,
                    }));
                }
            };
            Ok(ConditionFlowAction::Result(result_type))
        }
        LuaExpr::CallExpr(left_call_expr) => {
            if let LuaExpr::LiteralExpr(literal_expr) = right_expr {
                match literal_expr.get_literal() {
                    Some(LuaLiteralToken::Bool(b)) => {
                        let flow = if b.is_true() {
                            condition_flow
                        } else {
                            condition_flow.get_negated()
                        };

                        return get_type_at_call_expr(db, cache, var_ref_id, left_call_expr, flow);
                    }
                    _ => return Ok(ConditionFlowAction::Continue),
                }
            };

            Ok(ConditionFlowAction::Continue)
        }
        LuaExpr::IndexExpr(left_index_expr) => {
            let Some(maybe_ref_id) =
                get_var_expr_var_ref_id(db, cache, LuaExpr::IndexExpr(left_index_expr.clone()))
            else {
                return Ok(ConditionFlowAction::Continue);
            };

            if maybe_ref_id != *var_ref_id {
                // If the reference declaration ID does not match, we cannot narrow it
                return Ok(ConditionFlowAction::Continue);
            }

            let right_expr_type = infer_expr(db, cache, right_expr)?;
            if condition_flow.is_false() {
                return Ok(ConditionFlowAction::pending(PendingConditionNarrow::Eq {
                    right_expr_type,
                    condition_flow,
                }));
            }

            Ok(ConditionFlowAction::Result(right_expr_type))
        }
        LuaExpr::UnaryExpr(unary_expr) => {
            let Some(op) = unary_expr.get_op_token() else {
                return Ok(ConditionFlowAction::Continue);
            };

            match op.get_op() {
                UnaryOperator::OpLen => {}
                _ => return Ok(ConditionFlowAction::Continue),
            };

            let Some(expr) = unary_expr.get_expr() else {
                return Ok(ConditionFlowAction::Continue);
            };

            let Some(maybe_ref_id) = get_var_expr_var_ref_id(db, cache, expr) else {
                return Ok(ConditionFlowAction::Continue);
            };

            if maybe_ref_id != *var_ref_id {
                // If the reference declaration ID does not match, we cannot narrow it
                return Ok(ConditionFlowAction::Continue);
            }

            let right_expr_type = infer_expr(db, cache, right_expr)?;
            let antecedent_flow_id = get_single_antecedent(flow_node)?;
            let antecedent_type =
                get_type_at_flow(db, tree, cache, root, var_ref_id, antecedent_flow_id)?;
            match (&antecedent_type, &right_expr_type) {
                (
                    LuaType::Array(array_type),
                    LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i),
                ) => {
                    if condition_flow.is_true() {
                        let new_array_type =
                            LuaArrayType::new(array_type.get_base().clone(), LuaArrayLen::Max(*i));
                        return Ok(ConditionFlowAction::Result(LuaType::Array(
                            new_array_type.into(),
                        )));
                    }
                }
                _ => return Ok(ConditionFlowAction::Continue),
            }

            Ok(ConditionFlowAction::Continue)
        }
        _ => {
            // If the left expression is not a name or call expression, we cannot narrow it
            Ok(ConditionFlowAction::Continue)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn maybe_field_literal_eq_action(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    left_expr: LuaExpr,
    right_expr: LuaExpr,
    condition_flow: InferConditionFlow,
) -> Result<Option<ConditionFlowAction>, InferFailReason> {
    // only check left as need narrow
    let syntax_id = left_expr.get_syntax_id();
    let (index_expr, literal_expr) = match (left_expr, right_expr) {
        (LuaExpr::IndexExpr(index_expr), LuaExpr::LiteralExpr(literal_expr)) => {
            (index_expr, literal_expr)
        }
        (LuaExpr::LiteralExpr(literal_expr), LuaExpr::IndexExpr(index_expr)) => {
            (index_expr, literal_expr)
        }
        _ => return Ok(None),
    };

    let Some(prefix_expr) = index_expr.get_prefix_expr() else {
        return Ok(None);
    };

    let Some(maybe_var_ref_id) = get_var_expr_var_ref_id(db, cache, prefix_expr.clone()) else {
        // If we cannot find a reference declaration ID, we cannot narrow it
        return Ok(None);
    };

    if maybe_var_ref_id != *var_ref_id {
        if cache
            .narrow_by_literal_stop_position_cache
            .contains(&syntax_id)
            && var_ref_id.start_with(&maybe_var_ref_id)
        {
            return Ok(Some(ConditionFlowAction::Result(get_var_ref_type(
                db, cache, var_ref_id,
            )?)));
        }

        return Ok(None);
    }

    let antecedent_flow_id = get_single_antecedent(flow_node)?;
    let left_type = get_type_at_flow(db, tree, cache, root, var_ref_id, antecedent_flow_id)?;
    let LuaType::Union(union_type) = left_type else {
        return Ok(None);
    };

    cache
        .narrow_by_literal_stop_position_cache
        .insert(syntax_id);

    let right_type = infer_expr(db, cache, LuaExpr::LiteralExpr(literal_expr))?;
    let index = LuaIndexMemberExpr::IndexExpr(index_expr);
    let mut opt_result = None;
    let mut union_types = union_type.into_vec();
    for (i, sub_type) in union_types.iter().enumerate() {
        let member_type = match infer_member_by_member_key(
            db,
            cache,
            sub_type,
            index.clone(),
            &InferGuard::new(),
        ) {
            Ok(member_type) => member_type,
            Err(_) => continue, // If we cannot infer the member type, skip this type
        };
        if always_literal_equal(&member_type, &right_type) {
            // If the right type matches the member type, we can narrow it
            opt_result = Some(i);
        }
    }

    match condition_flow {
        InferConditionFlow::TrueCondition => {
            if let Some(i) = opt_result {
                return Ok(Some(ConditionFlowAction::Result(union_types[i].clone())));
            }
        }
        InferConditionFlow::FalseCondition => {
            if let Some(i) = opt_result {
                union_types.remove(i);
                return Ok(Some(ConditionFlowAction::Result(LuaType::from_vec(
                    union_types,
                ))));
            }
        }
    }

    Ok(None)
}
