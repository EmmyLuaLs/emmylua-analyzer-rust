use emmylua_parser::UnaryOperator;

use crate::db_index::{DbIndex, LuaOperatorMetaMethod, LuaType};

use super::{InferResult, get_custom_type_operator};

/// 操作数类型已知后的一元运算推断(纯函数, 无表达式推断)
pub(super) fn infer_unary_expr_result(
    db: &DbIndex,
    op: UnaryOperator,
    inner_type: LuaType,
) -> InferResult {
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
