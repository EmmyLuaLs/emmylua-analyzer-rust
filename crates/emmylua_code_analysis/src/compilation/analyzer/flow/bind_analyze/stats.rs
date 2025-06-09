use emmylua_parser::{LuaAssignStat, LuaAst, LuaAstNode, LuaCallExprStat, LuaLocalStat};

use crate::compilation::analyzer::flow::{bind_analyze::bind_each_child, binder::FlowBinder};

pub fn bind_local_stat(binder: &mut FlowBinder, local_stat: LuaLocalStat) -> Option<()> {
    let saved_current = binder.current;
    bind_each_child(binder, LuaAst::LuaLocalStat(local_stat));
    binder.current = saved_current;

    Some(())
}

pub fn bind_assign_stat(binder: &mut FlowBinder, assign_stat: LuaAssignStat) -> Option<()> {
    let mut current = binder.current;
    let (vars, values) = assign_stat.get_var_and_expr_list();

    // First bind the right-hand side expressions
    for expr in values {
        binder.current = current;
        bind_each_child(binder, LuaAst::cast(expr.syntax().clone())?);
    }

    // For each variable being assigned, create a separate assignment flow node
    for (_, var) in vars.iter().enumerate() {
        // Create assignment flow node for this specific variable
        let assignment_node =
            binder.create_flow_mutation(Some(LuaAst::cast(var.syntax().clone())?), Some(current));
        binder.add_antecedent(assignment_node, current);
        current = assignment_node;
    }

    binder.current = current;
    Some(())
}

pub fn bind_call_expr_stat(binder: &mut FlowBinder, call_expr_stat: LuaCallExprStat) -> Option<()> {
    let saved_current = binder.current;
    let call_expr = call_expr_stat.get_call_expr()?;
    bind_each_child(binder, LuaAst::cast(call_expr.syntax().clone())?);

    Some(())
}