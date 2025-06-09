use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaCallExprStat, LuaLabelStat, LuaLocalStat, LuaVarExpr, PathTrait
};

use crate::{
    compilation::analyzer::flow::{
        bind_analyze::{bind_each_child, exprs::bind_condition_expr},
        binder::FlowBinder,
        flow_node::{FlowNodeType, LuaFlowAssignment},
    },
    LuaDeclId, LuaVarRefId,
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
            let flow_id = binder.create_node(
                FlowNodeType::Assignment(
                    LuaFlowAssignment {
                        var_ref_id,
                        expr,
                        idx: idx as u32,
                    }
                    .into(),
                ),
            );

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
    


    Some(())
}