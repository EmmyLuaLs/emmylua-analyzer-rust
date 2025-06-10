use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaBreakStat, LuaCallExprStat, LuaDoStat, LuaGotoStat, LuaIfStat, LuaLabelStat, LuaLocalStat, LuaRepeatStat, LuaVarExpr, LuaWhileStat, PathTrait
};

use crate::{
    compilation::analyzer::flow::{
        bind_analyze::{bind_block, bind_each_child, exprs::{bind_condition_expr, bind_expr}},
        binder::FlowBinder,
        flow_node::{FlowAssignment, FlowNodeKind},
    },
    LuaClosureId, LuaDeclId, LuaVarRefId,
};

pub fn bind_local_stat(binder: &mut FlowBinder, local_stat: LuaLocalStat) -> Option<()> {
    let current = binder.current;
    let local_names = local_stat.get_local_name_list().collect::<Vec<_>>();
    let values = local_stat.get_value_exprs().collect::<Vec<_>>();
    let min_len = local_names.len().min(values.len());
    for i in 0..min_len {
        let name = &local_names[i];
        let value = &values[i];
        let decl_id = LuaDeclId::new(binder.file_id, name.get_position());
        if let Some(flow_id) = bind_condition_expr(binder, value.clone()) {
            binder.decl_bind_flow_ref.insert(decl_id, flow_id);
        }
    }
    binder.current = current;

    Some(())
}

pub fn bind_assign_stat(binder: &mut FlowBinder, assign_stat: LuaAssignStat) -> Option<()> {
    let (vars, values) = assign_stat.get_var_and_expr_list();

    let current = binder.current;
    // First bind the right-hand side expressions
    for expr in &values {
        bind_each_child(binder, LuaAst::cast(expr.syntax().clone())?);
    }

    binder.current = current;

    let mut last_expr = values.first()?.clone();
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
                FlowAssignment {
                    var_ref_id,
                    expr,
                    idx: idx as u32,
                }
                .into(),
            ));

            binder.add_antecedent(flow_id, binder.current);
            binder.current = flow_id;
        }
    }

    Some(())
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

pub fn bind_call_expr_stat(binder: &mut FlowBinder, call_expr_stat: LuaCallExprStat) -> Option<()> {
    let current = binder.current;
    let call_expr = call_expr_stat.get_call_expr()?;
    bind_each_child(binder, LuaAst::cast(call_expr.syntax().clone())?);
    binder.current = current;
    if call_expr.is_assert() {
        let call_arg_list = call_expr.get_args_list()?;
        for call_arg in call_arg_list.get_args() {
            match bind_condition_expr(binder, call_arg.clone()) {
                Some(flow_id) => {
                    binder.add_antecedent(flow_id, binder.current);
                    binder.current = flow_id;
                }
                None => {}
            }
        }
    }

    Some(())
}

pub fn bind_label_stat(binder: &mut FlowBinder, label_stat: LuaLabelStat) -> Option<()> {
    let label_name_token = label_stat.get_label_name_token()?;
    let label_name = label_name_token.get_name_text();
    let closure_id = LuaClosureId::from_node(label_stat.syntax());
    let name_label = binder.create_name_label(label_name, closure_id);
    binder.add_antecedent(name_label, binder.current);
    binder.current = name_label;

    Some(())
}

pub fn bind_break_stat(binder: &mut FlowBinder, break_stat: LuaBreakStat) -> Option<()> {
    // let 
    // binder.add_antecedent(loop_label, current);
    // binder.current = loop_label;

    Some(())
}

pub fn bind_goto_stat(binder: &mut FlowBinder, goto_stat: LuaGotoStat) -> Option<()> {
    // Goto statements are handled separately in the flow analysis
    // They will be processed when we analyze the labels
    // For now, we just return None to indicate no flow node is created
    let closure_id = LuaClosureId::from_node(goto_stat.syntax());
    binder.add_goto_stat(goto_stat, closure_id);

    Some(())
}

pub fn bind_do_stat(binder: &mut FlowBinder, do_stat: LuaDoStat) -> Option<()> {
    // Do statements are typically used for blocks of code
    // We can treat them as a block and bind their contents
    bind_each_child(binder, LuaAst::cast(do_stat.syntax().clone())?);

    Some(())
}

pub fn bind_while_stat(binder: &mut FlowBinder, while_stat: LuaWhileStat) -> Option<()> {
    // While statements are typically used for loops
    // We can treat them as a loop and bind their contents
    // For now, we just return None to indicate no flow node is created
    let current = binder.current;
    let old_loop_label = binder.loop_label;
    let loop_label = binder.create_loop_label();
    binder.add_antecedent(loop_label, current);

    binder.loop_label = loop_label;
    binder.current = loop_label;
    let condition_expr = while_stat.get_condition_expr()?;
    if let Some(condition_flow_id) = bind_condition_expr(binder, condition_expr) {
        binder.add_antecedent(condition_flow_id, loop_label);
        binder.current = condition_flow_id;
        binder.loop_label = loop_label;
    } else {
        binder.current = loop_label;
        binder.loop_label = current;
    }

    if let Some(while_block) = while_stat.get_block() {
        bind_block(binder, while_block);
    }

    binder.current = current;
    binder.loop_label = old_loop_label;

    Some(())
}


pub fn bind_repeat_stat(binder: &mut FlowBinder, repeat_stat: LuaRepeatStat) -> Option<()> {    
    let current = binder.current;
    let old_loop_label = binder.loop_label;
    
    // Create a loop label for the repeat statement
    let loop_label = binder.create_loop_label();
    binder.add_antecedent(loop_label, current);
    
    binder.loop_label = loop_label;
    binder.current = loop_label;
    
    // Bind the block first (repeat-until executes the block at least once)
    if let Some(repeat_block) = repeat_stat.get_block() {
        bind_block(binder, repeat_block);
    }
    
    let current = binder.current;
    bind_expr(binder, repeat_stat.get_condition_expr()?);
    
    // Restore previous state
    binder.current = current;
    binder.loop_label = old_loop_label;
    
    Some(())
}

pub fn bind_if_stat(binder: &mut FlowBinder, if_stat: LuaIfStat) -> Option<()> {
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
    
    Some(())
}