use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaBlock, LuaBreakStat, LuaCallExprStat, LuaDoStat,
    LuaForRangeStat, LuaForStat, LuaFuncStat, LuaGotoStat, LuaIfStat, LuaLabelStat, LuaLocalStat,
    LuaRepeatStat, LuaReturnStat, LuaWhileStat,
};

use crate::{
    compilation::analyzer::flow::{
        bind_analyze::{
            bind_block, bind_each_child,
            exprs::{bind_condition_expr, bind_expr},
            finish_flow_label,
        },
        binder::FlowBinder,
    },
    AnalyzeError, DiagnosticCode, FlowId, FlowNodeKind, LuaClosureId, LuaDeclId,
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
        let flow_id = bind_expr(binder, value.clone(), current);
        binder.decl_bind_flow_ref.insert(decl_id, flow_id);
    }

    let local_flow_id = binder.create_decl(local_stat.get_position());
    binder.add_antecedent(local_flow_id, current);
    local_flow_id
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
            bind_each_child(binder, ast, current);
        }
    }

    for var in &vars {
        if let Some(ast) = LuaAst::cast(var.syntax().clone()) {
            bind_each_child(binder, ast, current);
        }
    }

    let assignment_kind = FlowNodeKind::Assignment(assign_stat.to_ptr());
    let flow_id = binder.create_node(assignment_kind);
    binder.add_antecedent(flow_id, current);

    flow_id
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
        let post_assert_label = binder.create_branch_label();
        if let Some(call_arg_list) = call_expr.get_args_list() {
            for call_arg in call_arg_list.get_args() {
                bind_condition_expr(
                    binder,
                    call_arg,
                    current,
                    post_assert_label,
                    binder.unreachable,
                );
            }
        }

        let current = finish_flow_label(binder, post_assert_label, current);
        current
    } else {
        if let Some(ast) = LuaAst::cast(call_expr.syntax().clone()) {
            bind_each_child(binder, ast, current);
        }

        current
    }
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
    if let Some(loop_flow) = binder.get_flow(binder.loop_label) {
        if loop_flow.kind.is_unreachable() {
            // report a error if we are trying to break outside a loop
            binder.report_error(AnalyzeError::new(
                DiagnosticCode::SyntaxError,
                &t!("Break outside loop"),
                break_stat.get_range(),
            ));
            return current;
        }
    }

    binder.add_antecedent(break_flow_id, current);
    binder.add_antecedent(binder.loop_post_label, break_flow_id);
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
    let return_flow_id = binder.create_return();
    binder.cache_goto_flow(closure_id, label_name, return_flow_id);
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

    current
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
    loop_post_label: FlowId,
) -> FlowId {
    let old_loop_label = binder.loop_label;
    let old_loop_post_label = binder.loop_post_label;

    binder.loop_label = loop_label;
    binder.loop_post_label = loop_post_label;
    // Bind the block of code inside the iterator
    let flow_id = bind_block(binder, iter_block, current);

    // Restore the previous loop labels
    binder.loop_label = old_loop_label;
    binder.loop_post_label = old_loop_post_label;

    flow_id
}

pub fn bind_while_stat(
    binder: &mut FlowBinder,
    while_stat: LuaWhileStat,
    current: FlowId,
) -> FlowId {
    let loop_label = binder.create_loop_label();
    let loop_post_label = binder.create_branch_label();
    let pre_block_label = binder.create_branch_label();
    binder.add_antecedent(loop_label, current);
    binder.add_antecedent(pre_block_label, loop_label);
    binder.add_antecedent(loop_post_label, loop_label);

    let Some(condition_expr) = while_stat.get_condition_expr() else {
        return loop_post_label;
    };

    bind_condition_expr(
        binder,
        condition_expr,
        current,
        pre_block_label,
        loop_post_label,
    );

    if let Some(iter_block) = while_stat.get_block() {
        // Bind the block of code inside the while loop
        bind_iter_block(
            binder,
            iter_block,
            pre_block_label,
            loop_label,
            loop_post_label,
        );
    }

    loop_post_label
}

pub fn bind_repeat_stat(
    binder: &mut FlowBinder,
    repeat_stat: LuaRepeatStat,
    current: FlowId,
) -> FlowId {
    let loop_label = binder.create_loop_label();
    let loop_post_label = binder.create_branch_label();
    binder.add_antecedent(loop_label, current);
    binder.add_antecedent(loop_post_label, loop_label);

    let mut block_flow_id = loop_label;
    // Bind the block of code inside the repeat statement
    if let Some(iter_block) = repeat_stat.get_block() {
        block_flow_id =
            bind_iter_block(binder, iter_block, loop_label, loop_label, loop_post_label);
    }

    // Bind the condition expression
    if let Some(condition_expr) = repeat_stat.get_condition_expr() {
        bind_expr(binder, condition_expr, block_flow_id);
    }

    loop_post_label
}

pub fn bind_if_stat(binder: &mut FlowBinder, if_stat: LuaIfStat, current: FlowId) -> FlowId {
    let post_if_label = binder.create_branch_label();
    let mut else_label = binder.create_branch_label();
    let then_label = binder.create_branch_label();
    if let Some(condition_expr) = if_stat.get_condition_expr() {
        bind_condition_expr(binder, condition_expr, current, then_label, else_label);
    }

    if let Some(then_block) = if_stat.get_block() {
        let block_id = bind_block(binder, then_block, then_label);
        binder.add_antecedent(post_if_label, block_id);
    }

    for elseif_clause in if_stat.get_else_if_clause_list() {
        let post_elseif_label = binder.create_branch_label();
        let elseif_then_label = binder.create_branch_label();
        if let Some(condition_expr) = elseif_clause.get_condition_expr() {
            bind_condition_expr(
                binder,
                condition_expr,
                else_label,
                elseif_then_label,
                post_elseif_label,
            );
        }
        else_label = post_elseif_label;
        if let Some(elseif_block) = elseif_clause.get_block() {
            let block_id = bind_block(binder, elseif_block, elseif_then_label);
            binder.add_antecedent(post_if_label, block_id);
        }
    }

    if let Some(else_clause) = if_stat.get_else_clause() {
        let else_block = else_clause.get_block();
        if let Some(else_block) = else_block {
            let block_id = bind_block(binder, else_block, else_label);
            binder.add_antecedent(post_if_label, block_id);
        }
    } else {
        // If there's no else clause, we still need to connect the else_label to the post_if_label
        binder.add_antecedent(post_if_label, else_label);
    }

    post_if_label
}

pub fn bind_func_stat(binder: &mut FlowBinder, func_stat: LuaFuncStat, current: FlowId) -> FlowId {
    bind_each_child(binder, LuaAst::LuaFuncStat(func_stat), current);
    current
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
    let loop_label = binder.create_loop_label();
    let loop_post_label = binder.create_branch_label();
    binder.add_antecedent(loop_label, current);
    binder.add_antecedent(loop_post_label, loop_label);
    for expr in for_range_stat.get_expr_list() {
        bind_expr(binder, expr.clone(), loop_label);
    }

    let decl_flow = binder.create_decl(for_range_stat.get_position());
    binder.add_antecedent(decl_flow, loop_label);

    if let Some(iter_block) = for_range_stat.get_block() {
        // Bind the block of code inside the for loop
        bind_iter_block(binder, iter_block, decl_flow, loop_label, loop_post_label);
    }

    loop_post_label
}

pub fn bind_for_stat(binder: &mut FlowBinder, for_stat: LuaForStat, current: FlowId) -> FlowId {
    let loop_label = binder.create_loop_label();
    let loop_post_label = binder.create_branch_label();
    binder.add_antecedent(loop_label, current);
    binder.add_antecedent(loop_post_label, loop_label);

    for var_expr in for_stat.get_iter_expr() {
        bind_expr(binder, var_expr.clone(), loop_label);
    }

    let for_node = binder.create_node(FlowNodeKind::ForIStat(for_stat.to_ptr()));
    binder.add_antecedent(for_node, loop_label);

    if let Some(iter_block) = for_stat.get_block() {
        // Bind the block of code inside the for loop
        bind_iter_block(binder, iter_block, loop_label, for_node, loop_post_label);
    }

    loop_post_label
}
