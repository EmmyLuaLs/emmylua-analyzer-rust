mod bind_call_expr;

use emmylua_parser::{LuaAst, LuaAstNode, LuaCallExpr, LuaExpr, LuaNameExpr};

use crate::{
    compilation::analyzer::flow::{
        bind_analyze::{bind_each_child, exprs::bind_call_expr::bind_call_expr},
        binder::FlowBinder,
        flow_node::{FlowId, FlowNodeType, LuaFlowCondition},
    },
    LuaVarRefId,
};

pub fn bind_expr(binder: &mut FlowBinder, expr: LuaExpr) -> Option<()> {
    bind_condition_expr(binder, expr)?;
    Some(())
}

pub fn bind_condition_expr(binder: &mut FlowBinder, condition_expr: LuaExpr) -> Option<FlowId> {
    match condition_expr {
        LuaExpr::NameExpr(name_expr) => bind_name_expr(binder, name_expr),
        LuaExpr::CallExpr(call_expr) => bind_call_expr(binder, call_expr),
        LuaExpr::TableExpr(table_expr) => todo!(),
        LuaExpr::LiteralExpr(literal_expr) => todo!(),
        LuaExpr::BinaryExpr(binary_expr) => todo!(),
        LuaExpr::UnaryExpr(unary_expr) => todo!(),
        LuaExpr::ClosureExpr(closure_expr) => todo!(),
        LuaExpr::ParenExpr(paren_expr) => todo!(),
        LuaExpr::IndexExpr(index_expr) => todo!(),
    }
}

fn bind_name_expr(binder: &mut FlowBinder, name_expr: LuaNameExpr) -> Option<FlowId> {
    let name_text = name_expr.get_name_text()?;
    let position = name_expr.get_position();

    let decl_tree = binder.db.get_decl_index().get_decl_tree(&binder.file_id)?;
    let var_ref_id = match decl_tree.find_local_decl(&name_text, position) {
        Some(decl) => LuaVarRefId::DeclId(decl.get_id()),
        None => LuaVarRefId::Name(name_text.into()),
    };

    let flow_id = binder.create_node(FlowNodeType::Condition(
        LuaFlowCondition::Exist(var_ref_id).into(),
    ));

    Some(flow_id)
}
