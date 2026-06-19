use emmylua_parser::{
    BinaryOperator, LuaAssignStat, LuaAst, LuaAstNode, LuaAstToken, LuaBlock, LuaBreakStat,
    LuaCallArgList, LuaCallExprStat, LuaDoStat, LuaExpr, LuaForRangeStat, LuaForStat, LuaFuncStat,
    LuaGotoStat, LuaIfStat, LuaLabelStat, LuaLiteralToken, LuaLocalName, LuaLocalStat,
    LuaRepeatStat, LuaReturnStat, LuaVarExpr, LuaWhileStat, NumberResult, UnaryOperator,
};

use crate::{
    AnalyzeError, DeclMultiReturnRef, DeclMultiReturnRefAt, DiagnosticCode, FlowId, FlowNodeKind,
    LuaClosureId, LuaDeclId,
    compilation::analyzer::flow::{
        bind_analyze::{
            bind_block, bind_each_child, bind_node,
            exprs::{bind_condition_expr, bind_expr},
            finish_flow_label,
        },
        binder::FlowBinder,
    },
};

pub fn bind_local_stat(
    binder: &mut FlowBinder,
    local_stat: LuaLocalStat,
    current: FlowId,
) -> FlowId {
    let local_names = local_stat.get_local_name_list().collect::<Vec<_>>();
    let values = local_stat.get_value_exprs().collect::<Vec<_>>();
    let min_len = local_names.len().min(values.len());
    for i in 0..min_len {
        let name = &local_names[i];
        let value = &values[i];
        let decl_id = LuaDeclId::new(binder.file_id, name.get_position());
        if check_local_immutable(binder, decl_id) && check_value_expr_is_check_expr(value.clone()) {
            binder.decl_bind_expr_ref.insert(decl_id, value.to_ptr());
        }
    }

    for value in &values {
        // If there are more values than names, we still need to bind the values
        bind_expr(binder, value.clone(), current);
    }

    let local_flow_id = binder.create_decl(local_stat.get_position());
    binder.add_antecedent(local_flow_id, current);
    bind_multi_return_refs(
        binder,
        &get_local_decl_ids(binder, &local_names),
        &values,
        local_stat.get_position(),
        local_flow_id,
    );
    local_flow_id
}

fn check_local_immutable(binder: &mut FlowBinder, decl_id: LuaDeclId) -> bool {
    let Some(decl_ref) = binder
        .db
        .get_reference_index()
        .get_decl_references(&binder.file_id, &decl_id)
    else {
        return true;
    };

    !decl_ref.mutable
}

fn check_value_expr_is_check_expr(value_expr: LuaExpr) -> bool {
    match value_expr {
        LuaExpr::BinaryExpr(binary_expr) => {
            let Some(op) = binary_expr.get_op_token() else {
                return false;
            };

            matches!(op.get_op(), BinaryOperator::OpEq | BinaryOperator::OpNe)
        }
        LuaExpr::CallExpr(call) => call.is_type(),
        _ => false, // Other expressions can be checked
    }
}

fn get_local_decl_ids(
    binder: &FlowBinder<'_>,
    local_names: &[LuaLocalName],
) -> Vec<Option<LuaDeclId>> {
    local_names
        .iter()
        .map(|name| Some(LuaDeclId::new(binder.file_id, name.get_position())))
        .collect()
}

fn get_var_decl_ids(binder: &FlowBinder<'_>, vars: &[LuaVarExpr]) -> Vec<Option<LuaDeclId>> {
    vars.iter()
        .map(|var| {
            binder
                .db
                .get_reference_index()
                .get_var_reference_decl(&binder.file_id, var.get_range())
        })
        .collect()
}

pub fn bind_assign_stat(
    binder: &mut FlowBinder,
    assign_stat: LuaAssignStat,
    current: FlowId,
) -> FlowId {
    let (vars, values) = assign_stat.get_var_and_expr_list();
    // First bind the right-hand side expressions
    for expr in &values {
        if let Some(ast) = LuaAst::cast(expr.syntax().clone()) {
            bind_node(binder, ast, current);
        }
    }

    for var in &vars {
        if let Some(ast) = LuaAst::cast(var.syntax().clone()) {
            bind_node(binder, ast, current);
        }
    }

    let assignment_kind = FlowNodeKind::Assignment(assign_stat.to_ptr());
    let flow_id = binder.create_node(assignment_kind);
    binder.add_antecedent(flow_id, current);
    bind_multi_return_refs(
        binder,
        &get_var_decl_ids(binder, &vars),
        &values,
        assign_stat.get_position(),
        flow_id,
    );

    flow_id
}

fn bind_multi_return_refs(
    binder: &mut FlowBinder,
    decl_ids: &[Option<LuaDeclId>],
    values: &[LuaExpr],
    position: rowan::TextSize,
    flow_id: FlowId,
) {
    let tail_call = values.last().and_then(|value| match value {
        LuaExpr::CallExpr(call_expr) => Some((values.len() - 1, call_expr.to_ptr())),
        _ => None,
    });

    for (i, decl_id) in decl_ids.iter().enumerate() {
        let Some(decl_id) = decl_id else {
            continue;
        };

        let reference = tail_call.as_ref().and_then(|(last_value_idx, call_expr)| {
            if i < *last_value_idx {
                return None;
            }

            Some(DeclMultiReturnRef {
                call_expr: call_expr.clone(),
                return_index: i - *last_value_idx,
            })
        });

        binder
            .decl_multi_return_ref
            .entry(*decl_id)
            .or_default()
            .push(DeclMultiReturnRefAt {
                position,
                flow_id,
                reference,
            });
    }
}

pub fn bind_call_expr_stat(
    binder: &mut FlowBinder,
    call_expr_stat: LuaCallExprStat,
    current: FlowId,
) -> FlowId {
    let call_expr = match call_expr_stat.get_call_expr() {
        Some(expr) => expr,
        None => return current, // If there's no call expression, just return the current flow
    };

    if call_expr.is_assert() {
        let Some(arg_list) = call_expr.get_args_list() else {
            return current; // If there's no argument list, just return the current flow
        };

        bind_assert_stat(binder, arg_list, current)
    } else if call_expr.is_error() {
        if let Some(ast) = LuaAst::cast(call_expr.syntax().clone()) {
            bind_each_child(binder, ast, current);
        }
        let return_flow_id = binder.create_return();
        binder.add_antecedent(return_flow_id, current);
        return_flow_id
    } else {
        if let Some(ast) = LuaAst::cast(call_expr.syntax().clone()) {
            bind_each_child(binder, ast, current);
        }
        current
    }
}

fn bind_assert_stat(binder: &mut FlowBinder, arg_list: LuaCallArgList, current: FlowId) -> FlowId {
    let false_target = binder.unreachable;

    let mut pre_arg = current;
    for arg in arg_list.get_args() {
        let pre_next_arg = binder.create_branch_label();
        bind_condition_expr(binder, arg, pre_arg, pre_next_arg, false_target);
        pre_arg = finish_flow_label(binder, pre_next_arg, pre_arg);
    }

    pre_arg
}

pub fn bind_label_stat(
    binder: &mut FlowBinder,
    label_stat: LuaLabelStat,
    current: FlowId,
) -> FlowId {
    let Some(label_name_token) = label_stat.get_label_name_token() else {
        return current; // If there's no label token, just return the current flow
    };
    let label_name = label_name_token.get_name_text();
    let closure_id = LuaClosureId::from_node(label_stat.syntax());
    binder.db.get_reference_index_mut().add_label_declaration(
        binder.file_id,
        closure_id,
        label_name,
        label_name_token.get_range(),
    );
    let name_label = binder.create_name_label(label_name, closure_id);
    binder.add_antecedent(name_label, current);

    name_label
}

pub fn bind_break_stat(
    binder: &mut FlowBinder,
    break_stat: LuaBreakStat,
    current: FlowId,
) -> FlowId {
    let break_flow_id = binder.create_break();
    if let Some(loop_flow) = binder.get_flow(binder.loop_label)
        && loop_flow.kind.is_unreachable()
    {
        // report a error if we are trying to break outside a loop
        binder.report_error(AnalyzeError::new(
            DiagnosticCode::SyntaxError,
            &t!("Break outside loop"),
            break_stat.get_range(),
        ));
        return current;
    }

    binder.add_antecedent(break_flow_id, current);
    binder.add_antecedent(binder.break_target_label, break_flow_id);
    break_flow_id
}

pub fn bind_goto_stat(binder: &mut FlowBinder, goto_stat: LuaGotoStat, current: FlowId) -> FlowId {
    // Goto statements are handled separately in the flow analysis
    // They will be processed when we analyze the labels
    // For now, we just return None to indicate no flow node is created
    let closure_id = LuaClosureId::from_node(goto_stat.syntax());
    let Some(label_token) = goto_stat.get_label_name_token() else {
        return current; // If there's no label token, just return the current flow
    };

    let label_name = label_token.get_name_text();
    binder.db.get_reference_index_mut().add_label_reference(
        binder.file_id,
        closure_id,
        label_name,
        label_token.get_range(),
    );
    let return_flow_id = binder.create_return();
    binder.cache_goto_flow(closure_id, label_token.clone(), label_name, return_flow_id);
    binder.add_antecedent(return_flow_id, current);
    return_flow_id
}

pub fn bind_return_stat(
    binder: &mut FlowBinder,
    return_stat: LuaReturnStat,
    current: FlowId,
) -> FlowId {
    // If there are expressions in the return statement, bind them
    for expr in return_stat.get_expr_list() {
        bind_expr(binder, expr.clone(), current);
    }

    // Return statements are typically used to exit a function
    // We can treat them as a flow node that indicates the end of the current flow
    let return_flow_id = binder.create_return();
    binder.add_antecedent(return_flow_id, current);

    return_flow_id
}

pub fn bind_do_stat(binder: &mut FlowBinder, do_stat: LuaDoStat, mut current: FlowId) -> FlowId {
    // Do statements are typically used for blocks of code
    // We can treat them as a block and bind their contents
    if let Some(do_block) = do_stat.get_block() {
        current = bind_block(binder, do_block, current);
    }

    current
}

fn bind_iter_block(
    binder: &mut FlowBinder,
    iter_block: LuaBlock,
    current: FlowId,
    loop_label: FlowId,
    break_target_label: FlowId,
) -> FlowId {
    let old_loop_label = binder.loop_label;
    let old_loop_post_label = binder.break_target_label;

    binder.loop_label = loop_label;
    binder.break_target_label = break_target_label;
    // Bind the block of code inside the iterator
    let flow_id = bind_block(binder, iter_block, current);

    // Restore the previous loop labels
    binder.loop_label = old_loop_label;
    binder.break_target_label = old_loop_post_label;

    flow_id
}

pub fn bind_while_stat(
    binder: &mut FlowBinder,
    while_stat: LuaWhileStat,
    current: FlowId,
) -> FlowId {
    let pre_while_label = binder.create_loop_label();
    let after_while_label = binder.create_branch_label();
    let pre_block_label = binder.create_branch_label();
    binder.add_antecedent(pre_while_label, current);
    let Some(condition_expr) = while_stat.get_condition_expr() else {
        return current;
    };

    let loop_enters = match static_literal_truthiness(&condition_expr) {
        Some(true) => true,
        Some(false) => return current,
        None => {
            bind_condition_expr(
                binder,
                condition_expr.clone(),
                current,
                pre_block_label,
                after_while_label,
            );
            false
        }
    };
    let block_current = if loop_enters {
        current
    } else {
        finish_flow_label(binder, pre_block_label, current)
    };

    if let Some(iter_block) = while_stat.get_block() {
        // Bind the block of code inside the while loop
        let block_flow = bind_iter_block(
            binder,
            iter_block,
            block_current,
            pre_while_label,
            after_while_label,
        );
        if loop_enters {
            return finish_entered_loop_post_flow(binder, after_while_label, block_flow);
        }
    }

    current
}

pub fn bind_repeat_stat(
    binder: &mut FlowBinder,
    repeat_stat: LuaRepeatStat,
    current: FlowId,
) -> FlowId {
    let pre_repeat_label = binder.create_loop_label();
    let post_repeat_label = binder.create_branch_label();
    binder.add_antecedent(pre_repeat_label, current);

    let mut block_flow_id = pre_repeat_label;
    // Bind the block of code inside the repeat statement
    if let Some(iter_block) = repeat_stat.get_block() {
        block_flow_id = bind_iter_block(
            binder,
            iter_block,
            pre_repeat_label,
            pre_repeat_label,
            post_repeat_label,
        );
    }

    // Bind the condition expression
    if let Some(condition_expr) = repeat_stat.get_condition_expr() {
        bind_expr(binder, condition_expr, block_flow_id);
    }

    finish_flow_label(binder, post_repeat_label, block_flow_id)
}

pub fn bind_if_stat(binder: &mut FlowBinder, if_stat: LuaIfStat, current: FlowId) -> FlowId {
    let post_if_label = binder.create_branch_label();
    let mut else_label = binder.create_branch_label();
    let then_label = binder.create_branch_label();
    if let Some(condition_expr) = if_stat.get_condition_expr() {
        bind_condition_expr(binder, condition_expr, current, then_label, else_label);
    }

    if let Some(then_block) = if_stat.get_block() {
        let then_label = finish_flow_label(binder, then_label, current);
        let block_id = bind_block(binder, then_block, then_label);
        binder.add_antecedent(post_if_label, block_id);
    } else {
        let then_label = finish_flow_label(binder, then_label, current);
        // If there's no then block, we still need to add the antecedent
        binder.add_antecedent(post_if_label, then_label);
    }

    for elseif_clause in if_stat.get_else_if_clause_list() {
        let pre_elseif_label = finish_flow_label(binder, else_label, current);
        let elseif_then_label = binder.create_branch_label();
        let post_elseif_label = binder.create_branch_label();
        if let Some(condition_expr) = elseif_clause.get_condition_expr() {
            bind_condition_expr(
                binder,
                condition_expr,
                pre_elseif_label,
                elseif_then_label,
                post_elseif_label,
            );
        }
        // 后续 elseif/else 必须从当前 elseif 的 false 分支进入.
        // 这里保留 label, 让下一段条件回溯时还能看到当前条件为 false 的事实.
        else_label = post_elseif_label;
        if let Some(elseif_block) = elseif_clause.get_block() {
            let current = finish_flow_label(binder, elseif_then_label, current);
            let block_id = bind_block(binder, elseif_block, current);
            binder.add_antecedent(post_if_label, block_id);
        } else {
            let current = finish_flow_label(binder, elseif_then_label, current);
            binder.add_antecedent(post_if_label, current);
        }
    }

    if let Some(else_clause) = if_stat.get_else_clause() {
        let else_block = else_clause.get_block();
        if let Some(else_block) = else_block {
            let block_id = bind_block(binder, else_block, else_label);
            binder.add_antecedent(post_if_label, block_id);
        }
    } else {
        binder.add_antecedent(post_if_label, else_label);
    }

    if let Some(flow_node) = binder.get_flow(post_if_label)
        && flow_node.antecedent.is_none()
    {
        return binder.unreachable;
    }

    finish_flow_label(binder, post_if_label, else_label)
}

pub fn bind_func_stat(binder: &mut FlowBinder, func_stat: LuaFuncStat, current: FlowId) -> FlowId {
    let Some(func_name) = func_stat.get_func_name() else {
        return current; // If there's no function name, just return the current flow
    };

    bind_each_child(binder, LuaAst::LuaFuncStat(func_stat.clone()), current);
    let LuaVarExpr::NameExpr(_) = func_name else {
        return current; // If the function name is not a simple name, just return the current flow
    };

    let func_kind = FlowNodeKind::ImplFunc(func_stat.to_ptr());
    let flow_id = binder.create_node(func_kind);
    binder.add_antecedent(flow_id, current);

    flow_id
}

pub fn bind_local_func_stat(
    binder: &mut FlowBinder,
    local_func_stat: emmylua_parser::LuaLocalFuncStat,
    current: FlowId,
) -> FlowId {
    bind_each_child(binder, LuaAst::LuaLocalFuncStat(local_func_stat), current);
    current
}

pub fn bind_for_range_stat(
    binder: &mut FlowBinder,
    for_range_stat: LuaForRangeStat,
    current: FlowId,
) -> FlowId {
    let pre_for_range_label = binder.create_loop_label();
    let post_for_range_label = binder.create_branch_label();
    binder.add_antecedent(pre_for_range_label, current);

    for expr in for_range_stat.get_expr_list() {
        bind_expr(binder, expr.clone(), current);
    }

    let decl_flow = binder.create_decl(for_range_stat.get_position());
    binder.add_antecedent(decl_flow, pre_for_range_label);

    if let Some(iter_block) = for_range_stat.get_block() {
        // Bind the block of code inside the for loop
        bind_iter_block(
            binder,
            iter_block,
            decl_flow,
            pre_for_range_label,
            post_for_range_label,
        );
    }

    current
}

pub fn bind_for_stat(binder: &mut FlowBinder, for_stat: LuaForStat, current: FlowId) -> FlowId {
    let pre_for_label = binder.create_loop_label();
    let post_for_label = binder.create_branch_label();
    binder.add_antecedent(pre_for_label, current);

    let iter_exprs = for_stat.get_iter_expr().collect::<Vec<_>>();
    let loop_enters = match iter_exprs.as_slice() {
        [start_expr, stop_expr] => match (
            static_number_value(start_expr),
            static_number_value(stop_expr),
        ) {
            (Some(start), Some(stop)) => start <= stop,
            _ => false,
        },
        [start_expr, stop_expr, step_expr, ..] => match (
            static_number_value(start_expr),
            static_number_value(stop_expr),
            static_number_value(step_expr),
        ) {
            (Some(start), Some(stop), Some(step)) => {
                (step > 0.0 && start <= stop) || (step < 0.0 && start >= stop)
            }
            _ => false,
        },
        _ => false,
    };

    for var_expr in &iter_exprs {
        bind_expr(binder, var_expr.clone(), current);
    }

    let for_node = binder.create_node(FlowNodeKind::ForIStat(for_stat.to_ptr()));
    binder.add_antecedent(for_node, pre_for_label);

    if let Some(iter_block) = for_stat.get_block() {
        // Bind the block of code inside the for loop
        let block_flow =
            bind_iter_block(binder, iter_block, for_node, pre_for_label, post_for_label);
        if loop_enters {
            return finish_entered_loop_post_flow(binder, post_for_label, block_flow);
        }
    }

    current
}

fn finish_entered_loop_post_flow(
    binder: &mut FlowBinder,
    after_loop_label: FlowId,
    block_flow: FlowId,
) -> FlowId {
    // 这里使用悲观合流: 只有静态确认循环体会执行时, 才把循环体 flow 合到循环之后.
    binder.add_antecedent(after_loop_label, block_flow);
    if binder
        .get_flow(after_loop_label)
        .is_some_and(|flow_node| flow_node.antecedent.is_some())
    {
        after_loop_label
    } else {
        binder.unreachable
    }
}

/// 这里是循环可达性的静态判断, 只接受最直观的字面量真假值.
///
/// 它不是完整的常量求值或路径推断, 动态表达式和复杂常量表达式会返回 unknown,
/// 后续按不能确认进入循环处理.
fn static_literal_truthiness(expr: &LuaExpr) -> Option<bool> {
    match expr {
        LuaExpr::LiteralExpr(literal_expr) => match literal_expr.get_literal()? {
            LuaLiteralToken::Bool(bool_token) => Some(bool_token.is_true()),
            LuaLiteralToken::Nil(_) => Some(false),
            LuaLiteralToken::String(_) | LuaLiteralToken::Number(_) => Some(true),
            LuaLiteralToken::Dots(_) | LuaLiteralToken::Question(_) => None,
        },
        LuaExpr::ParenExpr(paren_expr) => static_literal_truthiness(&paren_expr.get_expr()?),
        LuaExpr::UnaryExpr(unary_expr)
            if unary_expr
                .get_op_token()
                .is_some_and(|op| op.get_op() == UnaryOperator::OpNot) =>
        {
            static_literal_truthiness(&unary_expr.get_expr()?).map(|truthy| !truthy)
        }
        _ => None,
    }
}

fn static_number_value(expr: &LuaExpr) -> Option<f64> {
    match expr {
        LuaExpr::LiteralExpr(literal_expr) => match literal_expr.get_literal()? {
            LuaLiteralToken::Number(number_token) => match number_token.get_number_value() {
                NumberResult::Int(value) => Some(value as f64),
                NumberResult::Uint(value) => Some(value as f64),
                NumberResult::Float(value) => Some(value),
                NumberResult::Number => None,
            },
            _ => None,
        },
        LuaExpr::ParenExpr(paren_expr) => static_number_value(&paren_expr.get_expr()?),
        _ => None,
    }
}
