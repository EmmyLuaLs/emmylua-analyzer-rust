use emmylua_parser::{BinaryOperator, LuaAst, LuaBinaryExpr, LuaExpr};

use crate::{
    compilation::analyzer::flow::{
        bind_analyze::{bind_each_child, exprs::bind_condition_expr, finish_flow_label},
        binder::FlowBinder,
    },
    FlowId,
};

pub fn bind_binary_expr(
    binder: &mut FlowBinder,
    binary_expr: LuaBinaryExpr,
    current: FlowId,
) -> Option<()> {
    let op_token = binary_expr.get_op_token()?;

    match op_token.get_op() {
        BinaryOperator::OpAnd => bind_and_expr(binder, binary_expr, current),
        BinaryOperator::OpOr => bind_or_expr(binder, binary_expr, current),
        _ => {
            bind_each_child(binder, LuaAst::LuaBinaryExpr(binary_expr.clone()), current);
            Some(())
        }
    }
}

fn bind_and_expr(
    binder: &mut FlowBinder,
    binary_expr: LuaBinaryExpr,
    current: FlowId,
) -> Option<()> {
    let (left, right) = binary_expr.get_exprs()?;

    let pre_right = binder.create_branch_label();
    bind_condition_expr(binder, left, current, pre_right, binder.false_target);
    let current = finish_flow_label(binder, pre_right, current);
    bind_condition_expr(
        binder,
        right,
        current,
        binder.true_target,
        binder.false_target,
    );

    Some(())
}

fn bind_or_expr(
    binder: &mut FlowBinder,
    binary_expr: LuaBinaryExpr,
    current: FlowId,
) -> Option<()> {
    let (left, right) = binary_expr.get_exprs()?;
    let pre_right = binder.create_branch_label();
    bind_condition_expr(binder, left, current, binder.true_target, pre_right);
    let current = finish_flow_label(binder, pre_right, current);
    bind_condition_expr(
        binder,
        right,
        current,
        binder.true_target,
        binder.false_target,
    );
    Some(())
}

pub fn is_binary_logical(expr: &LuaExpr) -> bool {
    match expr {
        LuaExpr::BinaryExpr(binary_expr) => {
            let Some(op_token) = binary_expr.get_op_token() else {
                return false;
            };

            return matches!(
                op_token.get_op(),
                BinaryOperator::OpAnd | BinaryOperator::OpOr
            );
        }
        LuaExpr::ParenExpr(paren_expr) => {
            if let Some(inner_expr) = paren_expr.get_expr() {
                return is_binary_logical(&inner_expr);
            }
        }
        _ => {}
    }
    false
}
