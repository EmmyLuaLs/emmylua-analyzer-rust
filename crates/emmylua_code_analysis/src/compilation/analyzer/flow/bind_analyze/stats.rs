use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaBreakStat, LuaCallExprStat, LuaDoStat, LuaExpr,
    LuaForRangeStat, LuaForStat, LuaFuncStat, LuaGotoStat, LuaIfStat, LuaLabelStat, LuaLocalStat,
    LuaRepeatStat, LuaReturnStat, LuaUnaryExpr, LuaVarExpr, LuaWhileStat, PathTrait, UnaryOperator,
};

use crate::{
    compilation::analyzer::flow::{
        bind_analyze::{bind_block, bind_each_child, exprs::bind_expr},
        binder::FlowBinder,
        flow_node::{FlowAssertion, FlowAssignment, FlowId, FlowNodeKind},
    },
    LuaClosureId, LuaDeclId, LuaVarRefId,
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

    current
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

    let mut current = current;
    let mut last_expr = match values.first() {
        Some(e) => e.clone(),
        None => {
            return current; // If there are no values, just return the current flow
        }
    };
    // For each variable being assigned, create a separate assignment flow node
    for (i, var) in vars.iter().enumerate() {
        let (expr, idx) = match values.get(i) {
            Some(e) => {
                last_expr = e.clone();
                (e.clone(), 0)
            }
            None => (last_expr.clone(), i),
        };

        if let Some(var_ref_id) = get_var_ref_id(binder, var.clone()) {
            // Create a flow node for the assignment
            let flow_id = binder.create_node(FlowNodeKind::Assignment(
                FlowAssignment::new(var_ref_id, expr.get_syntax_id(), idx as u32).into(),
            ));

            binder.add_antecedent(flow_id, current);
            current = flow_id;
        }
    }

    current
}

pub fn get_var_ref_id(binder: &mut FlowBinder, var_expr: LuaVarExpr) -> Option<LuaVarRefId> {
    match var_expr {
        LuaVarExpr::NameExpr(name_expr) => {
            let name_text = name_expr.get_name_text()?;
            let position = name_expr.get_position();
            let decl_tree = binder.db.get_decl_index().get_decl_tree(&binder.file_id)?;
            match decl_tree.find_local_decl(&name_text, position) {
                Some(decl) => Some(LuaVarRefId::DeclId(decl.get_id())),
                None => Some(LuaVarRefId::Name(name_text.into())),
            }
        }
        _ => {
            let path = var_expr.get_access_path()?;
            Some(LuaVarRefId::Name(path.into()))
        }
    }
}

pub fn bind_call_expr_stat(
    binder: &mut FlowBinder,
    call_expr_stat: LuaCallExprStat,
    mut current: FlowId,
) -> FlowId {
    let call_expr = match call_expr_stat.get_call_expr() {
        Some(expr) => expr,
        None => return current, // If there's no call expression, just return the current flow
    };

    if let Some(ast) = LuaAst::cast(call_expr.syntax().clone()) {
        bind_each_child(binder, ast, current);
    }

    if call_expr.is_assert() {
        if let Some(call_arg_list) = call_expr.get_args_list() {
            for call_arg in call_arg_list.get_args() {
                let flow_id = bind_expr(binder, call_arg.clone(), current);
                current = flow_id;
            }
        }
    }

    current
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

pub fn bind_break_stat(binder: &mut FlowBinder, _: LuaBreakStat, current: FlowId) -> FlowId {
    let break_flow_id = binder.create_break();
    // TODO: check if we are inside a loop

    binder.add_antecedent(break_flow_id, current);
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

pub fn bind_while_stat(
    binder: &mut FlowBinder,
    while_stat: LuaWhileStat,
    current: FlowId,
) -> FlowId {
    // While statements are typically used for loops
    // We can treat them as a loop and bind their contents
    // For now, we just return None to indicate no flow node is created
    let old_loop_label = binder.loop_label;
    let loop_label = binder.create_loop_label();
    binder.add_antecedent(loop_label, current);

    binder.loop_label = loop_label;
    let Some(condition_expr) = while_stat.get_condition_expr() else {
        return current;
    };

    let condition_flow_id = bind_expr(binder, condition_expr, loop_label);
    if let Some(while_block) = while_stat.get_block() {
        bind_block(binder, while_block, condition_flow_id);
    }

    binder.loop_label = old_loop_label;

    current
}

pub fn bind_repeat_stat(
    binder: &mut FlowBinder,
    repeat_stat: LuaRepeatStat,
    current: FlowId,
) -> FlowId {
    let old_loop_label = binder.loop_label;

    // Create a loop label for the repeat statement
    let loop_label = binder.create_loop_label();
    binder.add_antecedent(loop_label, current);
    binder.loop_label = loop_label;

    let mut block_flow_id = loop_label;
    // Bind the block first (repeat-until executes the block at least once)
    if let Some(repeat_block) = repeat_stat.get_block() {
        block_flow_id = bind_block(binder, repeat_block, loop_label);
    }

    if let Some(condition_expr) = repeat_stat.get_condition_expr() {
        // Bind the condition expression
        bind_expr(binder, condition_expr, block_flow_id);
    }

    // Restore previous state
    binder.loop_label = old_loop_label;
    current
}

pub fn bind_if_stat(binder: &mut FlowBinder, if_stat: LuaIfStat, current: FlowId) -> FlowId {
    // let current = binder.current;
    // let mut branch_endings = Vec::new();

    // // Process the main if condition
    // let condition = if_stat.get_condition_expr()?;
    // if let Some(condition_flow_id) = bind_condition_expr(binder, condition) {
    //     binder.add_antecedent(condition_flow_id, current);
    //     binder.current = condition_flow_id;
    // } else {
    //     binder.current = current;
    // }

    // // Process the if block (true branch)
    // if let Some(if_block) = if_stat.get_block() {
    //     bind_block(binder, if_block);
    //     branch_endings.push(binder.current);
    // }

    // // Process elseif clauses
    // for elseif_clause in if_stat.get_else_if_clause_list() {
    //     binder.current = current; // Reset to the beginning for each elseif

    //     if let Some(elseif_condition) = elseif_clause.get_condition_expr() {
    //         if let Some(condition_flow_id) = bind_condition_expr(binder, elseif_condition) {
    //             binder.add_antecedent(condition_flow_id, current);
    //             binder.current = condition_flow_id;
    //         }
    //     }

    //     if let Some(elseif_block) = elseif_clause.get_block() {
    //         bind_block(binder, elseif_block);
    //         branch_endings.push(binder.current);
    //     }
    // }

    // // Process else clause if it exists
    // if let Some(else_clause) = if_stat.get_else_clause() {
    //     binder.current = current; // Reset to the beginning for else

    //     if let Some(else_block) = else_clause.get_block() {
    //         bind_block(binder, else_block);
    //         branch_endings.push(binder.current);
    //     }
    // } else {
    //     // If there's no else clause, the original flow continues
    //     branch_endings.push(current);
    // }

    // // Create a branch label to merge all branches
    // if !branch_endings.is_empty() {
    //     let branch_label = binder.create_branch_label();
    //     for ending in branch_endings {
    //         binder.add_antecedent(branch_label, ending);
    //     }
    //     binder.current = branch_label;
    // } else {
    //     binder.current = current;
    // }

    current
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
    binder.add_antecedent(loop_label, current);
    let old_loop_label = binder.loop_label;
    binder.loop_label = loop_label;
    bind_each_child(binder, LuaAst::LuaForRangeStat(for_range_stat), loop_label);
    binder.loop_label = old_loop_label;
    current
}

pub fn bind_for_stat(binder: &mut FlowBinder, for_stat: LuaForStat, current: FlowId) -> FlowId {
    let loop_label = binder.create_loop_label();
    binder.add_antecedent(loop_label, current);
    let old_loop_label = binder.loop_label;
    binder.loop_label = loop_label;

    let Some(var_name_token) = for_stat.get_var_name() else {
        binder.loop_label = old_loop_label;
        return current;
    };

    let var_name = var_name_token.get_name_text();
    let var_list = for_stat.get_iter_expr().collect::<Vec<_>>();
    for expr in &var_list {
        bind_expr(binder, expr.clone(), loop_label);
    }

    if var_list.len() < 2 {
        binder.loop_label = old_loop_label;
        return current;
    }

    let mut parent_flow_id = loop_label;
    let second_expr = var_list[1].clone();
    if let LuaExpr::UnaryExpr(unary_expr) = second_expr {
        if let Some(length_var_name) = get_for_length_name(unary_expr) {
            let var_ref_id =
                LuaVarRefId::Name(format!("{}.[{}]", length_var_name, var_name).into());
            let flow_id = binder.create_node(FlowNodeKind::Assertion(
                FlowAssertion::Truthy(var_ref_id).into(),
            ));
            binder.add_antecedent(flow_id, loop_label);
            parent_flow_id = flow_id;
        }
    }

    if let Some(block) = for_stat.get_block() {
        bind_block(binder, block, parent_flow_id);
    }

    binder.loop_label = old_loop_label;
    current
}

fn get_for_length_name(unary_expr: LuaUnaryExpr) -> Option<String> {
    let op_token = unary_expr.get_op_token()?;
    if op_token.get_op() == UnaryOperator::OpLen {
        if let Some(inner) = unary_expr.get_expr() {
            match inner {
                LuaExpr::IndexExpr(index_expr) => {
                    if let Some(access_path) = index_expr.get_access_path() {
                        return Some(access_path.into());
                    }
                }
                LuaExpr::NameExpr(name_expr) => {
                    return Some(name_expr.get_name_text()?);
                }
                _ => {}
            }
        }
    }

    None
}
