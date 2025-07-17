use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaBinaryExpr, LuaCallExpr, LuaChunk, LuaExpr, LuaIndexMemberExpr,
    LuaLiteralToken, UnaryOperator,
};

use crate::{
    infer_expr,
    semantic::infer::{
        infer_index::infer_member_by_member_key,
        narrow::{
            condition_flow::{call_flow::get_type_at_call_expr, InferConditionFlow},
            get_single_antecedent,
            get_type_at_flow::get_type_at_flow,
            get_var_ref_type, narrow_down_type,
            var_ref_id::get_var_expr_var_ref_id,
            ResultTypeOrContinue,
        },
        VarRefId,
    },
    DbIndex, FlowNode, FlowTree, InferFailReason, InferGuard, LuaArrayLen, LuaArrayType,
    LuaInferCache, LuaType, TypeOps,
};

pub fn get_type_at_binary_expr(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    binary_expr: LuaBinaryExpr,
    condition_flow: InferConditionFlow,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    let Some(op_token) = binary_expr.get_op_token() else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let Some((left_expr, right_expr)) = binary_expr.get_exprs() else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let condition_flow = match op_token.get_op() {
        BinaryOperator::OpEq => condition_flow,
        BinaryOperator::OpNe => condition_flow.get_negated(),
        _ => {
            return Ok(ResultTypeOrContinue::Continue);
        }
    };

    let mut result_type = maybe_type_guard_binary(
        db,
        tree,
        cache,
        root,
        var_ref_id,
        flow_node,
        left_expr.clone(),
        right_expr.clone(),
        condition_flow,
    )?;
    if let ResultTypeOrContinue::Result(result_type) = result_type {
        return Ok(ResultTypeOrContinue::Result(result_type));
    }

    result_type = maybe_field_literal_eq_narrow(
        db,
        tree,
        cache,
        root,
        var_ref_id,
        flow_node,
        left_expr.clone(),
        right_expr.clone(),
        condition_flow,
    )?;

    if let ResultTypeOrContinue::Result(result_type) = result_type {
        return Ok(ResultTypeOrContinue::Result(result_type));
    }

    return maybe_var_eq_narrow(
        db,
        tree,
        cache,
        root,
        var_ref_id,
        flow_node,
        left_expr,
        right_expr,
        condition_flow,
    );
}

fn maybe_type_guard_binary(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    left_expr: LuaExpr,
    right_expr: LuaExpr,
    condition_flow: InferConditionFlow,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    let mut type_guard_expr: Option<LuaCallExpr> = None;
    let mut literal_string = String::new();
    if let LuaExpr::CallExpr(call_expr) = left_expr {
        if call_expr.is_type() {
            type_guard_expr = Some(call_expr);
            if let LuaExpr::LiteralExpr(literal_expr) = right_expr {
                match literal_expr.get_literal() {
                    Some(LuaLiteralToken::String(s)) => {
                        literal_string = s.get_value();
                    }
                    _ => return Ok(ResultTypeOrContinue::Continue),
                }
            }
        }
    } else if let LuaExpr::CallExpr(call_expr) = right_expr {
        if call_expr.is_type() {
            type_guard_expr = Some(call_expr);
            if let LuaExpr::LiteralExpr(literal_expr) = left_expr {
                match literal_expr.get_literal() {
                    Some(LuaLiteralToken::String(s)) => {
                        literal_string = s.get_value();
                    }
                    _ => return Ok(ResultTypeOrContinue::Continue),
                }
            }
        }
        // may ref a type value
    } else if let LuaExpr::NameExpr(name_expr) = left_expr {
        if let LuaExpr::LiteralExpr(literal_expr) = right_expr {
            let Some(decl_id) = db
                .get_reference_index()
                .get_var_reference_decl(&cache.get_file_id(), name_expr.get_range())
            else {
                return Ok(ResultTypeOrContinue::Continue);
            };

            let Some(expr_ptr) = tree.get_decl_ref_expr(&decl_id) else {
                return Ok(ResultTypeOrContinue::Continue);
            };

            let Some(expr) = expr_ptr.to_node(root) else {
                return Ok(ResultTypeOrContinue::Continue);
            };

            if let LuaExpr::CallExpr(call_expr) = expr {
                if call_expr.is_type() {
                    type_guard_expr = Some(call_expr);
                    match literal_expr.get_literal() {
                        Some(LuaLiteralToken::String(s)) => {
                            literal_string = s.get_value();
                        }
                        _ => return Ok(ResultTypeOrContinue::Continue),
                    }
                }
            } else {
                return Ok(ResultTypeOrContinue::Continue);
            }
        }
    }

    if type_guard_expr.is_none() || literal_string.is_empty() {
        return Ok(ResultTypeOrContinue::Continue);
    }

    let Some(arg_list) = type_guard_expr.unwrap().get_args_list() else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let Some(arg) = arg_list.get_args().next() else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let Some(maybe_var_ref_id) = get_var_expr_var_ref_id(db, cache, arg) else {
        // If we cannot find a reference declaration ID, we cannot narrow it
        return Ok(ResultTypeOrContinue::Continue);
    };

    if maybe_var_ref_id != *var_ref_id {
        return Ok(ResultTypeOrContinue::Continue);
    }

    let anatecedent_flow_id = get_single_antecedent(tree, flow_node)?;
    let antecedent_type = get_type_at_flow(db, tree, cache, root, var_ref_id, anatecedent_flow_id)?;

    let narrow = match literal_string.as_str() {
        "number" => LuaType::Number,
        "string" => LuaType::String,
        "boolean" => LuaType::Boolean,
        "table" => LuaType::Table,
        "function" => LuaType::Function,
        "thread" => LuaType::Thread,
        "userdata" => LuaType::Userdata,
        "nil" => LuaType::Nil,
        _ => {
            // If the type is not recognized, we cannot narrow it
            return Ok(ResultTypeOrContinue::Continue);
        }
    };

    let result_type = match condition_flow {
        InferConditionFlow::TrueCondition => {
            narrow_down_type(db, antecedent_type.clone(), narrow.clone()).unwrap_or(narrow)
        }
        InferConditionFlow::FalseCondition => TypeOps::Remove.apply(db, &antecedent_type, &narrow),
    };

    Ok(ResultTypeOrContinue::Result(result_type))
}

fn maybe_var_eq_narrow(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    left_expr: LuaExpr,
    right_expr: LuaExpr,
    condition_flow: InferConditionFlow,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    // only check left as need narrow
    match left_expr {
        LuaExpr::NameExpr(left_name_expr) => {
            let Some(maybe_ref_id) =
                get_var_expr_var_ref_id(db, cache, LuaExpr::NameExpr(left_name_expr.clone()))
            else {
                return Ok(ResultTypeOrContinue::Continue);
            };

            if maybe_ref_id != *var_ref_id {
                // If the reference declaration ID does not match, we cannot narrow it
                return Ok(ResultTypeOrContinue::Continue);
            }

            let right_expr_type = infer_expr(db, cache, right_expr)?;

            let result_type = match condition_flow {
                InferConditionFlow::TrueCondition => {
                    // self 是特殊的, 我们删除其 nil 类型
                    if var_ref_id.is_self_ref() && !right_expr_type.is_nil() {
                        TypeOps::Remove.apply(db, &right_expr_type, &LuaType::Nil)
                    } else {
                        right_expr_type
                    }
                }
                InferConditionFlow::FalseCondition => {
                    let antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
                    let antecedent_type =
                        get_type_at_flow(db, tree, cache, root, &var_ref_id, antecedent_flow_id)?;
                    TypeOps::Remove.apply(db, &antecedent_type, &right_expr_type)
                }
            };
            Ok(ResultTypeOrContinue::Result(result_type))
        }
        LuaExpr::CallExpr(left_call_expr) => {
            match right_expr {
                LuaExpr::LiteralExpr(literal_expr) => match literal_expr.get_literal() {
                    Some(LuaLiteralToken::Bool(b)) => {
                        let flow = if b.is_true() {
                            condition_flow
                        } else {
                            condition_flow.get_negated()
                        };

                        return get_type_at_call_expr(
                            db,
                            tree,
                            cache,
                            root,
                            &var_ref_id,
                            flow_node,
                            left_call_expr,
                            flow,
                        );
                    }
                    _ => return Ok(ResultTypeOrContinue::Continue),
                },
                _ => {}
            };

            Ok(ResultTypeOrContinue::Continue)
        }
        LuaExpr::IndexExpr(left_index_expr) => {
            let Some(maybe_ref_id) =
                get_var_expr_var_ref_id(db, cache, LuaExpr::IndexExpr(left_index_expr.clone()))
            else {
                return Ok(ResultTypeOrContinue::Continue);
            };

            if maybe_ref_id != *var_ref_id {
                // If the reference declaration ID does not match, we cannot narrow it
                return Ok(ResultTypeOrContinue::Continue);
            }

            let right_expr_type = infer_expr(db, cache, right_expr)?;
            let result_type = match condition_flow {
                InferConditionFlow::TrueCondition => right_expr_type,
                InferConditionFlow::FalseCondition => {
                    let antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
                    let antecedent_type =
                        get_type_at_flow(db, tree, cache, root, &var_ref_id, antecedent_flow_id)?;
                    TypeOps::Remove.apply(db, &antecedent_type, &right_expr_type)
                }
            };
            Ok(ResultTypeOrContinue::Result(result_type))
        }
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
            let antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
            let antecedent_type =
                get_type_at_flow(db, tree, cache, root, &var_ref_id, antecedent_flow_id)?;
            match (&antecedent_type, &right_expr_type) {
                (
                    LuaType::Array(array_type),
                    LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i),
                ) => {
                    if condition_flow.is_true() {
                        let new_array_type =
                            LuaArrayType::new(array_type.get_base().clone(), LuaArrayLen::Max(*i));
                        return Ok(ResultTypeOrContinue::Result(LuaType::Array(
                            new_array_type.into(),
                        )));
                    }
                }
                _ => return Ok(ResultTypeOrContinue::Continue),
            }

            Ok(ResultTypeOrContinue::Continue)
        }
        _ => {
            // If the left expression is not a name or call expression, we cannot narrow it
            Ok(ResultTypeOrContinue::Continue)
        }
    }
}

fn maybe_field_literal_eq_narrow(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    left_expr: LuaExpr,
    right_expr: LuaExpr,
    condition_flow: InferConditionFlow,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    // only check left as need narrow
    let syntax_id = left_expr.get_syntax_id();
    let (index_expr, literal_expr) = match (left_expr, right_expr) {
        (LuaExpr::IndexExpr(index_expr), LuaExpr::LiteralExpr(literal_expr)) => {
            (index_expr, literal_expr)
        }
        (LuaExpr::LiteralExpr(literal_expr), LuaExpr::IndexExpr(index_expr)) => {
            (index_expr, literal_expr)
        }
        _ => return Ok(ResultTypeOrContinue::Continue),
    };

    let Some(prefix_expr) = index_expr.get_prefix_expr() else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let Some(maybe_var_ref_id) = get_var_expr_var_ref_id(db, cache, prefix_expr.clone()) else {
        // If we cannot find a reference declaration ID, we cannot narrow it
        return Ok(ResultTypeOrContinue::Continue);
    };

    if maybe_var_ref_id != *var_ref_id {
        if cache
            .narrow_by_literal_stop_position_cache
            .contains(&syntax_id)
        {
            if var_ref_id.start_with(&maybe_var_ref_id) {
                return Ok(ResultTypeOrContinue::Result(get_var_ref_type(
                    db,
                    cache,
                    &var_ref_id,
                )?));
            }
        }

        return Ok(ResultTypeOrContinue::Continue);
    }

    let antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
    let left_type = get_type_at_flow(db, tree, cache, root, &var_ref_id, antecedent_flow_id)?;
    let LuaType::Union(union_type) = left_type else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    cache
        .narrow_by_literal_stop_position_cache
        .insert(syntax_id);

    let right_type = infer_expr(db, cache, LuaExpr::LiteralExpr(literal_expr))?;
    let mut guard = InferGuard::new();
    let index = LuaIndexMemberExpr::IndexExpr(index_expr);
    let mut opt_result = None;
    let mut union_types = union_type.into_vec();
    for (i, sub_type) in union_types.iter().enumerate() {
        let member_type =
            match infer_member_by_member_key(db, cache, &sub_type, index.clone(), &mut guard) {
                Ok(member_type) => member_type,
                Err(_) => continue, // If we cannot infer the member type, skip this type
            };
        if const_type_eq(&member_type, &right_type) {
            // If the right type matches the member type, we can narrow it
            opt_result = Some(i);
        }
    }

    match condition_flow {
        InferConditionFlow::TrueCondition => {
            if let Some(i) = opt_result {
                return Ok(ResultTypeOrContinue::Result(union_types[i].clone()));
            }
        }
        InferConditionFlow::FalseCondition => {
            if let Some(i) = opt_result {
                union_types.remove(i);
                return Ok(ResultTypeOrContinue::Result(LuaType::from_vec(union_types)));
            }
        }
    }

    Ok(ResultTypeOrContinue::Continue)
}

fn const_type_eq(left_type: &LuaType, right_type: &LuaType) -> bool {
    if left_type == right_type {
        return true;
    }

    match (left_type, right_type) {
        (
            LuaType::StringConst(l) | LuaType::DocStringConst(l),
            LuaType::StringConst(r) | LuaType::DocStringConst(r),
        ) => l == r,
        (LuaType::FloatConst(l), LuaType::FloatConst(r)) => l == r,
        (LuaType::BooleanConst(l), LuaType::BooleanConst(r)) => l == r,
        (
            LuaType::IntegerConst(l) | LuaType::DocIntegerConst(l),
            LuaType::IntegerConst(r) | LuaType::DocIntegerConst(r),
        ) => l == r,
        _ => false,
    }
}
