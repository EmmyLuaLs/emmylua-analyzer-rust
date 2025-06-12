use std::ops::Deref;

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr, LuaVarExpr};

use crate::{
    compilation::analyzer::flow::{
        bind_analyze::exprs::bind_expr,
        binder::FlowBinder,
        flow_node::{FlowAssertion, FlowCall, FlowId, FlowNodeKind},
    },
    LuaVarRefId,
};

pub fn bind_call_expr(binder: &mut FlowBinder, call_expr: LuaCallExpr, current: FlowId) -> FlowId {
    let mut flow_call_args = vec![];
    bind_call_arg_list(binder, call_expr.clone(), &mut flow_call_args, current);
    let self_var_ref = bind_call_prefix(binder, call_expr.clone(), current);
    let flow_call = FlowCall {
        call_expr: call_expr.get_syntax_id(),
        args: flow_call_args,
        self_var_ref,
    };

    let assertion = FlowAssertion::TruthyCall(flow_call.into());
    let flow_id = binder.create_node(FlowNodeKind::Assertion(assertion.into()).into());
    binder.add_antecedent(flow_id, current);
    flow_id
}

fn bind_call_arg_list(
    binder: &mut FlowBinder,
    call_expr: LuaCallExpr,
    flow_call_args: &mut Vec<Option<LuaVarRefId>>,
    current: FlowId,
) -> Option<()> {
    let arg_list = call_expr.get_args_list()?;
    for expr in arg_list.get_args() {
        let expr_flow_id = bind_expr(binder, expr.clone(), current);
        if LuaVarExpr::can_cast(expr.syntax().kind().into()) {
            if let Some(flow_node) = binder.get_flow(expr_flow_id) {
                match &flow_node.kind {
                    FlowNodeKind::Assertion(assertion) => {
                        if let FlowAssertion::Truthy(var_ref_id) = assertion.deref() {
                            flow_call_args.push(Some(var_ref_id.clone()));
                        } else {
                            flow_call_args.push(None);
                        }
                    }
                    _ => {
                        flow_call_args.push(None);
                    }
                }
            }
        } else {
            flow_call_args.push(None);
        }
    }

    Some(())
}

fn bind_call_prefix(
    binder: &mut FlowBinder,
    call_expr: LuaCallExpr,
    current: FlowId,
) -> Option<LuaVarRefId> {
    let prefix_expr = call_expr.get_prefix_expr()?;
    let flow_id = match prefix_expr {
        LuaExpr::IndexExpr(index_expr) => {
            bind_expr(binder, LuaExpr::IndexExpr(index_expr.clone()), current)
        }
        _ => {
            bind_expr(binder, prefix_expr, current);
            return None;
        }
    };

    if let Some(flow_node) = binder.get_flow(flow_id) {
        match &flow_node.kind {
            FlowNodeKind::Assertion(assertion) => {
                if let FlowAssertion::Truthy(var_ref_id) = assertion.deref() {
                    return Some(var_ref_id.clone());
                }
            }
            _ => {}
        }
    }

    None
}
