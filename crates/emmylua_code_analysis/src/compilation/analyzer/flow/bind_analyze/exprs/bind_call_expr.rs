use emmylua_parser::{LuaAst, LuaCallExpr};

use crate::compilation::analyzer::flow::{bind_analyze::bind_each_child, binder::FlowBinder, flow_node::{FlowId, FlowNodeKind}};


pub fn bind_call_expr(binder: &mut FlowBinder, call_expr: LuaCallExpr) -> Option<FlowId> {
    bind_each_child(binder, LuaAst::LuaCallExpr(call_expr.clone()));
    let prefix_expr = call_expr.get_prefix_expr()?;
    // let multi_flow_id = binder.create_node(FlowNodeType::Call);

    None
}
