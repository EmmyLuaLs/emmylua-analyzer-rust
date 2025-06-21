mod bind_binary_expr;
mod bind_call_expr;

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaClosureExpr, LuaExpr, LuaIndexExpr, LuaNameExpr, LuaTableExpr,
    LuaUnaryExpr, PathTrait, UnaryOperator,
};

use crate::{
    compilation::analyzer::flow::{
        bind_analyze::bind_each_child,
        binder::FlowBinder,
        flow_node::{FlowId, FlowNodeKind},
    },
    LuaVarRefId,
};
pub use bind_binary_expr::bind_binary_expr;
pub use bind_call_expr::bind_call_expr;

pub fn bind_condition_expr(
    binder: &mut FlowBinder,
    condition_expr: LuaExpr,
    current: FlowId,
    true_target: FlowId,
    false_target: FlowId,
) {
    let flow_id = bind_expr(binder, condition_expr, current);
    // todo
}

pub fn bind_expr(binder: &mut FlowBinder, expr: LuaExpr, current: FlowId) -> FlowId {
    match expr {
        LuaExpr::NameExpr(name_expr) => bind_name_expr(binder, name_expr, current),
        LuaExpr::CallExpr(call_expr) => bind_call_expr(binder, call_expr, current),
        LuaExpr::TableExpr(table_expr) => bind_table_expr(binder, table_expr, current),
        LuaExpr::LiteralExpr(_) => current,
        LuaExpr::ClosureExpr(closure_expr) => bind_closure_expr(binder, closure_expr, current),
        LuaExpr::ParenExpr(paren_expr) => bind_paren_expr(binder, paren_expr, current),
        LuaExpr::IndexExpr(index_expr) => bind_index_expr(binder, index_expr, current),
        LuaExpr::BinaryExpr(binary_expr) => bind_binary_expr(binder, binary_expr, current),
        LuaExpr::UnaryExpr(unary_expr) => bind_unary_expr(binder, unary_expr, current),
    }
}

pub fn bind_name_expr(binder: &mut FlowBinder, name_expr: LuaNameExpr, current: FlowId) -> FlowId {
    let name_node = binder.create_node(FlowNodeKind::Variable(name_expr.to_ptr()));
    binder.add_antecedent(name_node, current);
    binder.bind_syntax_node(name_expr.get_syntax_id(), name_node);
    name_node
}

pub fn bind_table_expr(
    binder: &mut FlowBinder,
    table_expr: LuaTableExpr,
    current: FlowId,
) -> FlowId {
    bind_each_child(binder, LuaAst::LuaTableExpr(table_expr), current);
    current
}

pub fn bind_closure_expr(
    binder: &mut FlowBinder,
    closure_expr: LuaClosureExpr,
    current: FlowId,
) -> FlowId {
    bind_each_child(binder, LuaAst::LuaClosureExpr(closure_expr), current);
    current
}

pub fn bind_index_expr(
    binder: &mut FlowBinder,
    index_expr: LuaIndexExpr,
    current: FlowId,
) -> FlowId {
    bind_each_child(binder, LuaAst::LuaIndexExpr(index_expr.clone()), current);

    let Some(access_path) = index_expr.get_access_path() else {
        return current;
    };
    let var_ref_id = LuaVarRefId::Name(access_path.into());
    let flow_id = binder.create_node(FlowNodeKind::Assertion(
        FlowAssertion::Truthy(var_ref_id).into(),
    ));

    binder.add_antecedent(flow_id, current);
    binder.bind_syntax_node(index_expr.get_syntax_id(), flow_id);

    flow_id
}

pub fn bind_paren_expr(
    binder: &mut FlowBinder,
    paren_expr: emmylua_parser::LuaParenExpr,
    current: FlowId,
) -> FlowId {
    let Some(inner_expr) = paren_expr.get_expr() else {
        return current;
    };

    bind_expr(binder, inner_expr, current)
}

pub fn bind_unary_expr(
    binder: &mut FlowBinder,
    unary_expr: LuaUnaryExpr,
    current: FlowId,
) -> FlowId {
    let Some(op_token) = unary_expr.get_op_token() else {
        return current;
    };

    let Some(expr) = unary_expr.get_expr() else {
        return current;
    };
    let flow_id = bind_expr(binder, expr, current);

    match op_token.get_op() {
        UnaryOperator::OpNot => {
            if let Some(flow_node) = binder.get_flow(flow_id) {
                match &flow_node.kind {
                    FlowNodeKind::Assertion(assertion) => {
                        let negated_assertion = assertion.get_negation();
                        let antecedent = binder.get_antecedents(flow_id).cloned();
                        let negated_flow_id = binder.create_node_with_antecedent(
                            FlowNodeKind::Assertion(negated_assertion.into()),
                            antecedent,
                        );
                        return negated_flow_id;
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    current
}
