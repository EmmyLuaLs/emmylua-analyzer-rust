mod bind_binary_expr;

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaCallExpr, LuaClosureExpr, LuaExpr, LuaIndexExpr, LuaNameExpr,
    LuaTableExpr, LuaUnaryExpr,
};

use crate::{
    compilation::analyzer::flow::{
        bind_analyze::{bind_each_child, exprs::bind_binary_expr::is_binary_logical},
        binder::FlowBinder,
    },
    FlowId, FlowNodeKind,
};
pub use bind_binary_expr::bind_binary_expr;

pub fn bind_condition_expr(
    binder: &mut FlowBinder,
    condition_expr: LuaExpr,
    current: FlowId,
    true_target: FlowId,
    false_target: FlowId,
) {
    let old_true_target = binder.true_target;
    let old_false_target = binder.false_target;

    binder.true_target = true_target;
    binder.false_target = false_target;
    bind_expr(binder, condition_expr.clone(), current);
    binder.true_target = old_true_target;
    binder.false_target = old_false_target;

    if !is_binary_logical(&condition_expr) {
        let true_condition =
            binder.create_node(FlowNodeKind::TrueCondition(condition_expr.to_ptr()));
        binder.add_antecedent(true_condition, current);
        binder.add_antecedent(true_target, true_condition);

        let false_condition =
            binder.create_node(FlowNodeKind::FalseCondition(condition_expr.to_ptr()));
        binder.add_antecedent(false_condition, current);
        binder.add_antecedent(false_target, false_condition);
    }
}

pub fn bind_expr(binder: &mut FlowBinder, expr: LuaExpr, current: FlowId) -> FlowId {
    match expr {
        LuaExpr::NameExpr(name_expr) => bind_name_expr(binder, name_expr, current),
        LuaExpr::CallExpr(call_expr) => bind_call_expr(binder, call_expr, current),
        LuaExpr::TableExpr(table_expr) => bind_table_expr(binder, table_expr, current),
        LuaExpr::LiteralExpr(_) => Some(()), // Literal expressions do not need binding
        LuaExpr::ClosureExpr(closure_expr) => bind_closure_expr(binder, closure_expr, current),
        LuaExpr::ParenExpr(paren_expr) => bind_paren_expr(binder, paren_expr, current),
        LuaExpr::IndexExpr(index_expr) => bind_index_expr(binder, index_expr, current),
        LuaExpr::BinaryExpr(binary_expr) => bind_binary_expr(binder, binary_expr, current),
        LuaExpr::UnaryExpr(unary_expr) => bind_unary_expr(binder, unary_expr, current),
    };

    current
}

pub fn bind_name_expr(
    binder: &mut FlowBinder,
    name_expr: LuaNameExpr,
    current: FlowId,
) -> Option<()> {
    binder.bind_syntax_node(name_expr.get_syntax_id(), current);
    Some(())
}

pub fn bind_table_expr(
    binder: &mut FlowBinder,
    table_expr: LuaTableExpr,
    current: FlowId,
) -> Option<()> {
    bind_each_child(binder, LuaAst::LuaTableExpr(table_expr), current);
    Some(())
}

pub fn bind_closure_expr(
    binder: &mut FlowBinder,
    closure_expr: LuaClosureExpr,
    current: FlowId,
) -> Option<()> {
    bind_each_child(binder, LuaAst::LuaClosureExpr(closure_expr), current);
    Some(())
}

pub fn bind_index_expr(
    binder: &mut FlowBinder,
    index_expr: LuaIndexExpr,
    current: FlowId,
) -> Option<()> {
    bind_each_child(binder, LuaAst::LuaIndexExpr(index_expr.clone()), current);
    Some(())
}

pub fn bind_paren_expr(
    binder: &mut FlowBinder,
    paren_expr: emmylua_parser::LuaParenExpr,
    current: FlowId,
) -> Option<()> {
    let Some(inner_expr) = paren_expr.get_expr() else {
        return None;
    };

    bind_expr(binder, inner_expr, current);
    Some(())
}

pub fn bind_unary_expr(
    binder: &mut FlowBinder,
    unary_expr: LuaUnaryExpr,
    current: FlowId,
) -> Option<()> {
    let Some(inner_expr) = unary_expr.get_expr() else {
        return None;
    };
    bind_expr(binder, inner_expr, current);
    Some(())
}

pub fn bind_call_expr(
    binder: &mut FlowBinder,
    call_expr: LuaCallExpr,
    current: FlowId,
) -> Option<()> {
    bind_each_child(binder, LuaAst::LuaCallExpr(call_expr.clone()), current);
    Some(())
}
