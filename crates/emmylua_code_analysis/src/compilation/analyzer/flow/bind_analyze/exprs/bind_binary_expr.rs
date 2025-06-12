use std::ops::Deref;

use emmylua_parser::{
    BinaryOperator, LuaAst, LuaBinaryExpr, LuaCallExpr, LuaExpr, LuaLiteralExpr, LuaLiteralToken,
};
use smol_str::SmolStr;

use crate::{
    compilation::analyzer::flow::{
        bind_analyze::{bind_each_child, exprs::bind_expr},
        binder::FlowBinder,
        flow_node::{FlowAssertion, FlowId, FlowNodeKind},
    },
    LuaType,
};

pub fn bind_binary_expr(
    binder: &mut FlowBinder,
    binary_expr: LuaBinaryExpr,
    current: FlowId,
) -> FlowId {
    let Some(op_token) = binary_expr.get_op_token() else {
        return current;
    };

    match op_token.get_op() {
        BinaryOperator::OpAnd => bind_and_expr(binder, binary_expr, current),
        BinaryOperator::OpOr => bind_or_expr(binder, binary_expr, current),
        BinaryOperator::OpEq => bind_eq_expr(binder, binary_expr, current),
        BinaryOperator::OpNe => bind_ne_expr(binder, binary_expr, current),
        _ => {
            bind_each_child(binder, LuaAst::LuaBinaryExpr(binary_expr.clone()), current);
            current
        }
    }
}

fn bind_and_expr(binder: &mut FlowBinder, binary_expr: LuaBinaryExpr, current: FlowId) -> FlowId {
    let Some((left, right)) = binary_expr.get_exprs() else {
        return current;
    };

    let left_flow_id = bind_expr(binder, left, current);
    let right_flow_id = bind_expr(binder, right, left_flow_id);
    right_flow_id
}

fn bind_eq_expr(binder: &mut FlowBinder, binary_expr: LuaBinaryExpr, current: FlowId) -> FlowId {
    let Some((left, right)) = binary_expr.get_exprs() else {
        return current;
    };

    if let LuaExpr::CallExpr(call_expr) = &left {
        if call_expr.is_type() {
            if let LuaExpr::LiteralExpr(literal) = &right {
                if let Some(flow_assertion) =
                    get_type_guard_assertion(binder, call_expr.clone(), literal.clone(), current)
                {
                    let flow_id =
                        binder.create_node(FlowNodeKind::Assertion(flow_assertion.into()));
                    binder.add_antecedent(flow_id, current);
                    return flow_id;
                }
            }
        }
    } else if let LuaExpr::CallExpr(call_expr) = &right {
        if call_expr.is_type() {
            if let LuaExpr::LiteralExpr(literal) = &left {
                if let Some(flow_assertion) =
                    get_type_guard_assertion(binder, call_expr.clone(), literal.clone(), current)
                {
                    let flow_id =
                        binder.create_node(FlowNodeKind::Assertion(flow_assertion.into()));
                    binder.add_antecedent(flow_id, current);
                    return flow_id;
                }
            }
        }
    }

    if let LuaExpr::LiteralExpr(left_literal) = &left {
        return bind_eq_literal_expr(binder, right.clone(), left_literal.clone(), current);
    } else if let LuaExpr::LiteralExpr(right_literal) = &right {
        return bind_eq_literal_expr(binder, left.clone(), right_literal.clone(), current);
    }

    current
}

fn bind_eq_literal_expr(
    binder: &mut FlowBinder,
    expr: LuaExpr,
    literal_expr: LuaLiteralExpr,
    current: FlowId,
) -> FlowId {
    let literal_token = match literal_expr.get_literal() {
        Some(token) => token,
        None => return current,
    };
    let expr_flow_id = bind_expr(binder, expr, current);
    if expr_flow_id == current {
        return current;
    }

    let mut var_ref_id = None;
    if let Some(flow_node) = binder.get_flow(expr_flow_id) {
        match &flow_node.kind {
            FlowNodeKind::Assertion(assertion) => match assertion.deref() {
                FlowAssertion::Truthy(ref_id) => {
                    var_ref_id = Some(ref_id.clone());
                }
                _ => {}
            },
            _ => {}
        }
    };

    let Some(var_ref_id) = var_ref_id else {
        return current;
    };

    let flow_assertion = match literal_token {
        LuaLiteralToken::Nil(_) => FlowAssertion::TypeGuard(var_ref_id, LuaType::Nil),
        LuaLiteralToken::Bool(value) => {
            FlowAssertion::TypeForce(var_ref_id, LuaType::BooleanConst(value.is_true()))
        }
        LuaLiteralToken::Number(value) => {
            if value.is_int() {
                FlowAssertion::TypeForce(var_ref_id, LuaType::IntegerConst(value.get_int_value()))
            } else {
                FlowAssertion::TypeForce(var_ref_id, LuaType::FloatConst(value.get_float_value()))
            }
        }
        LuaLiteralToken::String(value) => FlowAssertion::TypeForce(
            var_ref_id,
            LuaType::StringConst(SmolStr::new(value.get_value()).into()),
        ),
        _ => return current,
    };

    let flow_id = binder.create_node(FlowNodeKind::Assertion(flow_assertion.into()));
    binder.add_antecedent(flow_id, current);

    flow_id
}

fn bind_ne_expr(binder: &mut FlowBinder, binary_expr: LuaBinaryExpr, current: FlowId) -> FlowId {
    let flow_id = bind_eq_expr(binder, binary_expr, current);
    if flow_id == current {
        return current;
    }

    if let Some(flow_node) = binder.get_flow(flow_id) {
        if let FlowNodeKind::Assertion(assertion) = &flow_node.kind {
            let negated_assertion = assertion.get_negation();
            let antecedent = binder.get_antecedents(flow_id).cloned();
            let negated_flow_id = binder.create_node_with_antecedent(
                FlowNodeKind::Assertion(negated_assertion.into()),
                antecedent,
            );
            return negated_flow_id;
        }
    }

    current
}

fn get_type_guard_assertion(
    binder: &mut FlowBinder,
    call_expr: LuaCallExpr,
    literal_expr: LuaLiteralExpr,
    current: FlowId,
) -> Option<FlowAssertion> {
    let first_arg = call_expr.get_args_list()?.get_args().next()?;
    let flow_id = bind_expr(binder, first_arg, current);
    let flow_node = binder.get_flow(flow_id)?;
    let var_ref_id = match &flow_node.kind {
        FlowNodeKind::Assertion(cond) => {
            let var_ref_id = match cond.deref() {
                FlowAssertion::Truthy(var_ref_id) => var_ref_id,
                _ => return None,
            };

            var_ref_id.clone()
        }
        _ => {
            return None;
        }
    };

    let type_literal = match literal_expr.get_literal()? {
        LuaLiteralToken::String(string) => string.get_value(),
        _ => return None,
    };

    let flow_assertion: FlowAssertion = match type_literal.as_str() {
        "number" => FlowAssertion::TypeGuard(var_ref_id, LuaType::Number),
        "string" => FlowAssertion::TypeGuard(var_ref_id, LuaType::String),
        "boolean" => FlowAssertion::TypeGuard(var_ref_id, LuaType::Boolean),
        "table" => FlowAssertion::TypeGuard(var_ref_id, LuaType::Table),
        "function" => FlowAssertion::TypeGuard(var_ref_id, LuaType::Function),
        "userdata" => FlowAssertion::TypeGuard(var_ref_id, LuaType::Userdata),
        "thread" => FlowAssertion::TypeGuard(var_ref_id, LuaType::Thread),
        "nil" => FlowAssertion::TypeGuard(var_ref_id, LuaType::Nil),
        _ => return None,
    };

    Some(flow_assertion)
}

fn bind_or_expr(binder: &mut FlowBinder, binary_expr: LuaBinaryExpr, current: FlowId) -> FlowId {
    let Some((left, right)) = binary_expr.get_exprs() else {
        return current;
    };

    let left_flow_id = bind_expr(binder, left, current);
    let mut left_ne_flow_id = None;
    if let Some(flow_node) = binder.get_flow(left_flow_id) {
        if let FlowNodeKind::Assertion(assertion) = &flow_node.kind {
            let ne_assertion = assertion.get_negation();
            let antecedent = binder.get_antecedents(left_flow_id).cloned();
            left_ne_flow_id = Some(binder.create_node_with_antecedent(
                FlowNodeKind::Assertion(ne_assertion.into()),
                antecedent,
            ));
        }
    }

    let Some(left_ne_flow_id) = left_ne_flow_id else {
        return current
    };

    let right_flow_id = bind_expr(binder, right, left_ne_flow_id);
    right_flow_id
}
