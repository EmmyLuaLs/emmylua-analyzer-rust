//! 一元表达式推断 — `not a`, `#a`, `-a`, `~a`

use emmylua_parser::{LuaUnaryExpr, UnaryOperator};

use crate::LuaType;

use super::{InferFailReason, InferQuery, InferResult};

pub(super) fn infer_unary_expr(infer: &InferQuery, unary_expr: LuaUnaryExpr) -> InferResult {
    let op = unary_expr
        .get_op_token()
        .ok_or(InferFailReason::None)?
        .get_op();
    let inner_expr = unary_expr.get_expr().ok_or(InferFailReason::None)?;
    let inner_type = infer.infer_expr(inner_expr)?;

    match op {
        UnaryOperator::OpNot => infer_unary_not(inner_type),
        UnaryOperator::OpLen => Ok(LuaType::Integer),
        UnaryOperator::OpUnm => infer_unary_unm(inner_type),
        UnaryOperator::OpBNot => infer_unary_bnot(inner_type),
        UnaryOperator::OpNop => Ok(inner_type),
    }
}

fn infer_unary_not(inner_type: LuaType) -> InferResult {
    match inner_type {
        LuaType::BooleanConst(b) => Ok(LuaType::BooleanConst(!b)),
        LuaType::DocBooleanConst(b) => Ok(LuaType::DocBooleanConst(!b)),
        _ => Ok(LuaType::Boolean),
    }
}

fn infer_unary_unm(inner_type: LuaType) -> InferResult {
    match inner_type {
        LuaType::IntegerConst(i) => {
            if let Some(neg) = i.checked_neg() {
                Ok(LuaType::IntegerConst(neg))
            } else {
                Ok(LuaType::Integer)
            }
        }
        LuaType::DocIntegerConst(i) => {
            if let Some(neg) = i.checked_neg() {
                Ok(LuaType::DocIntegerConst(neg))
            } else {
                Ok(LuaType::Integer)
            }
        }
        LuaType::FloatConst(f) => Ok(LuaType::FloatConst(-f)),
        LuaType::Integer => Ok(LuaType::Integer),
        LuaType::Number => Ok(LuaType::Number),
        _ => Ok(LuaType::Number),
    }
}

fn infer_unary_bnot(inner_type: LuaType) -> InferResult {
    match inner_type {
        LuaType::IntegerConst(i) => Ok(LuaType::IntegerConst(!i)),
        LuaType::DocIntegerConst(i) => Ok(LuaType::DocIntegerConst(!i)),
        _ => Ok(LuaType::Integer),
    }
}
