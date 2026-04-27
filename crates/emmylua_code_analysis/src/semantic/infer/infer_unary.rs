use emmylua_parser::{LuaUnaryExpr, UnaryOperator};

use crate::{
    LuaInferCache,
    db_index::{DbIndex, LuaOperatorMetaMethod, LuaType},
};

use super::{InferFailReason, InferResult, get_custom_type_operator, infer_expr_root};

pub fn infer_unary_expr(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    unary_expr: LuaUnaryExpr,
) -> InferResult {
    let op = unary_expr
        .get_op_token()
        .ok_or(InferFailReason::None)?
        .get_op();
    let inner_expr = unary_expr.get_expr().ok_or(InferFailReason::None)?;
    let inner_type = infer_expr_root(db, cache, inner_expr)?;
    match op {
        UnaryOperator::OpNot => infer_unary_expr_not(inner_type),
        UnaryOperator::OpLen => Ok(LuaType::Integer),
        UnaryOperator::OpUnm => infer_unary_expr_unm(db, inner_type),
        UnaryOperator::OpBNot => infer_unary_expr_bnot(db, inner_type),
        UnaryOperator::OpNop => Ok(inner_type),
    }
}

fn infer_unary_custom_operator(
    db: &DbIndex,
    inner: &LuaType,
    op: LuaOperatorMetaMethod,
) -> InferResult {
    let operators = get_custom_type_operator(db, inner.clone(), op);
    if let Some(operators) = operators {
        for operator in operators {
            if let Ok(res) = operator.get_result(db) {
                return Ok(res);
            }
        }
    }

    match op {
        LuaOperatorMetaMethod::Unm => Ok(LuaType::Number),
        LuaOperatorMetaMethod::BNot => Ok(LuaType::Integer),
        _ => Ok(LuaType::Nil),
    }
}

fn infer_unary_expr_not(inner_type: LuaType) -> InferResult {
    match inner_type {
        LuaType::BooleanConst(b) => Ok(LuaType::BooleanConst(!b)),
        _ => Ok(LuaType::Boolean),
    }
}

fn infer_unary_expr_unm(db: &DbIndex, inner_type: LuaType) -> InferResult {
    match inner_type {
        LuaType::IntegerConst(i) => Ok(LuaType::IntegerConst(-i)),
        LuaType::DocIntegerConst(i) => Ok(LuaType::DocIntegerConst(-i)),
        LuaType::FloatConst(f) => Ok(LuaType::FloatConst(-f)),
        LuaType::Integer => Ok(LuaType::Integer),
        _ => infer_unary_custom_operator(db, &inner_type, LuaOperatorMetaMethod::Unm),
    }
}

fn infer_unary_expr_bnot(db: &DbIndex, inner_type: LuaType) -> InferResult {
    match inner_type {
        LuaType::IntegerConst(i) => Ok(LuaType::IntegerConst(!i)),
        _ => infer_unary_custom_operator(db, &inner_type, LuaOperatorMetaMethod::BNot),
    }
}
