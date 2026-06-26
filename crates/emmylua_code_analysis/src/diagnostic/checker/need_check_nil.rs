//! Need-check-nil checker — salsa-native.

use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaBinaryExpr, LuaCallExpr, LuaExpr, LuaIndexExpr,
};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for expr in root.descendants::<LuaExpr>() {
        match expr {
            LuaExpr::CallExpr(call) => check_call(context, model, call),
            LuaExpr::BinaryExpr(binary) => check_binary(context, model, binary),
            LuaExpr::IndexExpr(index) => check_index(context, model, index),
            _ => {}
        }
    }
}

fn check_call(context: &mut DiagnosticContext, model: &SemanticModel, call: LuaCallExpr) {
    let Some(prefix) = call.get_prefix_expr() else {
        return;
    };
    let Ok(func) = model.infer_expr(prefix.clone()) else {
        return;
    };
    if func.is_nullable() {
        context.add_diagnostic(
            DiagnosticCode::NeedCheckNil,
            prefix.get_range(),
            t!("function %{name} may be nil", name = prefix.syntax().text()).to_string(),
            None,
        );
    }
}

fn check_index(context: &mut DiagnosticContext, model: &SemanticModel, index: LuaIndexExpr) {
    let Some(prefix) = index.get_prefix_expr() else {
        return;
    };
    let Ok(prefix_type) = model.infer_expr(prefix.clone()) else {
        return;
    };
    if prefix_type.is_nullable() {
        context.add_diagnostic(
            DiagnosticCode::NeedCheckNil,
            prefix.get_range(),
            t!("%{name} may be nil", name = prefix.syntax().text()).to_string(),
            None,
        );
    }
}

fn check_binary(context: &mut DiagnosticContext, model: &SemanticModel, binary: LuaBinaryExpr) {
    let Some(op_token) = binary.get_op_token() else {
        return;
    };
    if !matches!(
        op_token.get_op(),
        BinaryOperator::OpAdd
            | BinaryOperator::OpSub
            | BinaryOperator::OpMul
            | BinaryOperator::OpDiv
            | BinaryOperator::OpMod
    ) {
        return;
    }
    let Some((left, right)) = binary.get_exprs() else {
        return;
    };

    let Ok(left_type) = model.infer_expr(left.clone()) else {
        return;
    };
    if left_type.is_nullable() {
        context.add_diagnostic(
            DiagnosticCode::NeedCheckNil,
            left.get_range(),
            t!("%{name} value may be nil", name = left.syntax().text()).to_string(),
            None,
        );
    }

    let Ok(right_type) = model.infer_expr(right.clone()) else {
        return;
    };
    if right_type.is_nullable() {
        context.add_diagnostic(
            DiagnosticCode::NeedCheckNil,
            right.get_range(),
            t!("%{name} value may be nil", name = right.syntax().text()).to_string(),
            None,
        );
    }
}
