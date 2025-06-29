mod infer_binary_or;

use emmylua_parser::{BinaryOperator, LuaBinaryExpr};
use infer_binary_or::{infer_binary_expr_or, special_or_rule};
use smol_str::SmolStr;

use crate::{
    check_type_compact,
    db_index::{DbIndex, LuaOperatorMetaMethod, LuaType},
    get_real_type, LuaInferCache, TypeOps,
};

use super::{get_custom_type_operator, infer_expr, InferFailReason, InferResult};

pub fn infer_binary_expr(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    expr: LuaBinaryExpr,
) -> InferResult {
    let op = expr.get_op_token().ok_or(InferFailReason::None)?.get_op();
    let (left, right) = expr.get_exprs().ok_or(InferFailReason::None)?;
    let left_type = infer_expr(db, cache, left.clone())?;
    let right_type = infer_expr(db, cache, right.clone())?;
    let real_left_type = get_real_type(db, &left_type);
    let real_right_type = get_real_type(db, &right_type);
    let left_type_ref = real_left_type.unwrap_or(&left_type);
    let right_type_ref = real_right_type.unwrap_or(&right_type);

    if op == BinaryOperator::OpOr {
        if let Some(ty) = special_or_rule(db, left_type_ref, right_type_ref, left, right) {
            return Ok(ty);
        }
    } else if !matches!(op, BinaryOperator::OpAnd | BinaryOperator::OpOr) {
        if let Some(ty) = infer_union_binary_expr(db, op, left_type_ref, right_type_ref) {
            return Ok(ty);
        }
    }

    match (real_left_type.is_some(), real_right_type.is_some()) {
        (false, false) => infer_binary_expr_type(db, left_type, right_type, op),
        (true, false) => infer_binary_expr_type(db, left_type_ref.clone(), right_type, op),
        (false, true) => infer_binary_expr_type(db, left_type, right_type_ref.clone(), op),
        (true, true) => {
            infer_binary_expr_type(db, left_type_ref.clone(), right_type_ref.clone(), op)
        }
    }
}

fn infer_union_binary_expr(
    db: &DbIndex,
    op: BinaryOperator,
    left_type: &LuaType,
    right_type: &LuaType,
) -> Option<LuaType> {
    let (u, other, is_left_union) = if let LuaType::Union(u) = left_type {
        (u, right_type, true)
    } else if let LuaType::Union(u) = right_type {
        (u, left_type, false)
    } else {
        return None;
    };

    let mut result = LuaType::Unknown;
    let types = u.get_types();
    for ty in types.iter() {
        // 只在实际调用时才 clone，而不是预先 clone
        let ty_result = if is_left_union {
            infer_binary_expr_type(db, ty.clone(), other.clone(), op)
        } else {
            infer_binary_expr_type(db, other.clone(), ty.clone(), op)
        };

        if let Ok(ty) = ty_result {
            result = TypeOps::Union.apply(db, &result, &ty);
        }
    }
    Some(result)
}

fn infer_binary_expr_type(
    db: &DbIndex,
    left_type: LuaType,
    right_type: LuaType,
    op: BinaryOperator,
) -> InferResult {
    match op {
        BinaryOperator::OpAdd => infer_binary_expr_add(db, left_type, right_type),
        BinaryOperator::OpSub => infer_binary_expr_sub(db, left_type, right_type),
        BinaryOperator::OpMul => infer_binary_expr_mul(db, left_type, right_type),
        BinaryOperator::OpDiv => infer_binary_expr_div(db, left_type, right_type),
        BinaryOperator::OpIDiv => infer_binary_expr_idiv(db, left_type, right_type),
        BinaryOperator::OpMod => infer_binary_expr_mod(db, left_type, right_type),
        BinaryOperator::OpPow => infer_binary_expr_pow(db, left_type, right_type),
        BinaryOperator::OpBAnd => infer_binary_expr_band(db, left_type, right_type),
        BinaryOperator::OpBOr => infer_binary_expr_bor(db, left_type, right_type),
        BinaryOperator::OpBXor => infer_binary_expr_bxor(db, left_type, right_type),
        BinaryOperator::OpShl => infer_binary_expr_shl(db, left_type, right_type),
        BinaryOperator::OpShr => infer_binary_expr_shr(db, left_type, right_type),
        BinaryOperator::OpConcat => infer_binary_expr_concat(db, left_type, right_type),
        BinaryOperator::OpOr => infer_binary_expr_or(db, left_type, right_type),
        BinaryOperator::OpAnd => infer_binary_expr_and(db, left_type, right_type),
        BinaryOperator::OpLt
        | BinaryOperator::OpLe
        | BinaryOperator::OpGt
        | BinaryOperator::OpGe
        | BinaryOperator::OpEq
        | BinaryOperator::OpNe => infer_cmp_expr(db, left_type, right_type, op),
        _ => Ok(left_type),
    }
}

fn infer_binary_custom_operator(
    db: &DbIndex,
    left: &LuaType,
    right: &LuaType,
    op: LuaOperatorMetaMethod,
) -> InferResult {
    // 先检查 left 是否是自定义类型，避免不必要的 clone
    if left.is_custom_type() {
        if let Some(operators) = get_custom_type_operator(db, left.clone(), op) {
            for operator in operators {
                let operand = operator.get_operand(db);
                if check_type_compact(db, &operand, right).is_ok() {
                    return operator.get_result(db);
                }
            }
        }
    }

    // 再检查 right 是否是自定义类型，只在需要时 clone
    if right.is_custom_type() {
        if let Some(operators) = get_custom_type_operator(db, right.clone(), op) {
            for operator in operators {
                let operand = operator.get_operand(db);
                if check_type_compact(db, &operand, left).is_ok() {
                    return operator.get_result(db);
                }
            }
        }
    }

    match op {
        LuaOperatorMetaMethod::Add
        | LuaOperatorMetaMethod::Sub
        | LuaOperatorMetaMethod::Mul
        | LuaOperatorMetaMethod::Div
        | LuaOperatorMetaMethod::Mod
        | LuaOperatorMetaMethod::Pow => Ok(LuaType::Number),
        LuaOperatorMetaMethod::IDiv
        | LuaOperatorMetaMethod::BAnd
        | LuaOperatorMetaMethod::BOr
        | LuaOperatorMetaMethod::BXor
        | LuaOperatorMetaMethod::Shl
        | LuaOperatorMetaMethod::Shr => Ok(LuaType::Integer),

        LuaOperatorMetaMethod::Concat => Ok(LuaType::String),
        _ => Err(InferFailReason::None),
    }
}

fn infer_binary_expr_add(db: &DbIndex, left: LuaType, right: LuaType) -> InferResult {
    if left.is_number() && right.is_number() {
        return match (&left, &right) {
            (LuaType::IntegerConst(int1), LuaType::IntegerConst(int2)) => {
                Ok(LuaType::IntegerConst(int1 + int2))
            }
            (LuaType::FloatConst(num1), LuaType::FloatConst(num2)) => {
                Ok(LuaType::FloatConst(num1 + num2))
            }
            (LuaType::IntegerConst(int1), LuaType::FloatConst(num2)) => {
                Ok(LuaType::FloatConst((*int1 as f64 + *num2).into()))
            }
            (LuaType::FloatConst(num1), LuaType::IntegerConst(int2)) => {
                Ok(LuaType::FloatConst((*num1 + *int2 as f64).into()))
            }
            _ => {
                if left.is_integer() && right.is_integer() {
                    Ok(LuaType::Integer)
                } else {
                    Ok(LuaType::Number)
                }
            }
        };
    }

    match (
        left.is_nil() || left.is_any() || left.is_unknown(),
        right.is_nil() || right.is_any() || right.is_unknown(),
    ) {
        (true, false) => return Ok(right),
        (false, true) => return Ok(left),
        _ => {}
    }

    infer_binary_custom_operator(db, &left, &right, LuaOperatorMetaMethod::Add)
}

fn infer_binary_expr_sub(db: &DbIndex, left: LuaType, right: LuaType) -> InferResult {
    if left.is_number() && right.is_number() {
        return match (&left, &right) {
            (LuaType::IntegerConst(int1), LuaType::IntegerConst(int2)) => {
                Ok(LuaType::IntegerConst(int1 - int2))
            }
            (LuaType::FloatConst(num1), LuaType::FloatConst(num2)) => {
                Ok(LuaType::FloatConst(num1 - num2))
            }
            (LuaType::IntegerConst(int1), LuaType::FloatConst(num2)) => {
                Ok(LuaType::FloatConst((*int1 as f64 - *num2).into()))
            }
            (LuaType::FloatConst(num1), LuaType::IntegerConst(int2)) => {
                Ok(LuaType::FloatConst((*num1 - *int2 as f64).into()))
            }
            _ => {
                if left.is_integer() && right.is_integer() {
                    Ok(LuaType::Integer)
                } else {
                    Ok(LuaType::Number)
                }
            }
        };
    }

    infer_binary_custom_operator(db, &left, &right, LuaOperatorMetaMethod::Sub)
}

fn infer_binary_expr_mul(db: &DbIndex, left: LuaType, right: LuaType) -> InferResult {
    if left.is_number() && right.is_number() {
        return match (&left, &right) {
            (LuaType::IntegerConst(int1), LuaType::IntegerConst(int2)) => {
                Ok(LuaType::IntegerConst(int1 * int2))
            }
            (LuaType::FloatConst(num1), LuaType::FloatConst(num2)) => {
                Ok(LuaType::FloatConst(num1 * num2))
            }
            (LuaType::IntegerConst(int1), LuaType::FloatConst(num2)) => {
                Ok(LuaType::FloatConst((*int1 as f64 * *num2).into()))
            }
            (LuaType::FloatConst(num1), LuaType::IntegerConst(int2)) => {
                Ok(LuaType::FloatConst((*num1 * *int2 as f64).into()))
            }
            _ => {
                if left.is_integer() && right.is_integer() {
                    Ok(LuaType::Integer)
                } else {
                    Ok(LuaType::Number)
                }
            }
        };
    }

    infer_binary_custom_operator(db, &left, &right, LuaOperatorMetaMethod::Mul)
}

fn infer_binary_expr_div(db: &DbIndex, left: LuaType, right: LuaType) -> InferResult {
    if left.is_number() && right.is_number() {
        return match (&left, &right) {
            (LuaType::IntegerConst(int1), LuaType::IntegerConst(int2)) => {
                if *int2 != 0 {
                    return Ok(LuaType::FloatConst((*int1 as f64 / *int2 as f64).into()));
                }
                Ok(LuaType::Number)
            }
            (LuaType::FloatConst(num1), LuaType::FloatConst(num2)) => {
                if *num2 != 0.0 {
                    return Ok(LuaType::FloatConst(num1 / num2));
                }
                Ok(LuaType::Number)
            }
            (LuaType::IntegerConst(int1), LuaType::FloatConst(num2)) => {
                if *num2 != 0.0 {
                    return Ok(LuaType::FloatConst((*int1 as f64 / *num2).into()));
                }
                Ok(LuaType::Number)
            }
            (LuaType::FloatConst(num1), LuaType::IntegerConst(int2)) => {
                if *int2 != 0 {
                    return Ok(LuaType::FloatConst(*num1 / *int2 as f64));
                }
                Ok(LuaType::Number)
            }
            _ => Ok(LuaType::Number),
        };
    }

    infer_binary_custom_operator(db, &left, &right, LuaOperatorMetaMethod::Div)
}

fn infer_binary_expr_idiv(db: &DbIndex, left: LuaType, right: LuaType) -> InferResult {
    if left.is_integer() && right.is_integer() {
        return match (&left, &right) {
            (LuaType::IntegerConst(int1), LuaType::IntegerConst(int2)) => {
                if *int2 != 0 {
                    return Ok(LuaType::IntegerConst(int1 / int2));
                }
                Ok(LuaType::Integer)
            }
            _ => Ok(LuaType::Integer),
        };
    }

    infer_binary_custom_operator(db, &left, &right, LuaOperatorMetaMethod::IDiv)
}

fn infer_binary_expr_mod(db: &DbIndex, left: LuaType, right: LuaType) -> InferResult {
    if left.is_integer() && right.is_integer() {
        return match (&left, &right) {
            (LuaType::IntegerConst(int1), LuaType::IntegerConst(int2)) => {
                if *int2 != 0 {
                    return Ok(LuaType::IntegerConst(int1 % int2));
                }
                Ok(LuaType::Integer)
            }
            _ => Ok(LuaType::Integer),
        };
    }

    infer_binary_custom_operator(db, &left, &right, LuaOperatorMetaMethod::Mod)
}

fn infer_binary_expr_pow(db: &DbIndex, left: LuaType, right: LuaType) -> InferResult {
    if left.is_number() && right.is_number() {
        return match (&left, &right) {
            (LuaType::IntegerConst(int1), LuaType::IntegerConst(int2)) => {
                if let Some(int3) = int1.checked_pow(*int2 as u32) {
                    Ok(LuaType::IntegerConst(int3))
                } else {
                    Ok(LuaType::Number)
                }
            }
            (LuaType::FloatConst(num1), LuaType::IntegerConst(num2)) => {
                Ok(LuaType::FloatConst(num1.powf(*num2 as f64).into()))
            }
            _ => Ok(LuaType::Number),
        };
    }

    infer_binary_custom_operator(db, &left, &right, LuaOperatorMetaMethod::Pow)
}

fn infer_binary_expr_band(db: &DbIndex, left: LuaType, right: LuaType) -> InferResult {
    if left.is_integer() && right.is_integer() {
        return match (&left, &right) {
            (LuaType::IntegerConst(int1), LuaType::IntegerConst(int2)) => {
                Ok(LuaType::IntegerConst(int1 & int2))
            }
            _ => Ok(LuaType::Integer),
        };
    }

    infer_binary_custom_operator(db, &left, &right, LuaOperatorMetaMethod::BAnd)
}

fn infer_binary_expr_bor(db: &DbIndex, left: LuaType, right: LuaType) -> InferResult {
    if left.is_integer() && right.is_integer() {
        return match (&left, &right) {
            (LuaType::IntegerConst(int1), LuaType::IntegerConst(int2)) => {
                Ok(LuaType::IntegerConst(int1 | int2))
            }
            _ => Ok(LuaType::Integer),
        };
    }

    infer_binary_custom_operator(db, &left, &right, LuaOperatorMetaMethod::BOr)
}

fn infer_binary_expr_bxor(db: &DbIndex, left: LuaType, right: LuaType) -> InferResult {
    if left.is_integer() && right.is_integer() {
        return match (&left, &right) {
            (LuaType::IntegerConst(int1), LuaType::IntegerConst(int2)) => {
                Ok(LuaType::IntegerConst(int1 ^ int2))
            }
            _ => Ok(LuaType::Integer),
        };
    }

    infer_binary_custom_operator(db, &left, &right, LuaOperatorMetaMethod::BXor)
}

fn infer_binary_expr_shl(db: &DbIndex, left: LuaType, right: LuaType) -> InferResult {
    if left.is_integer() && right.is_integer() {
        return match (&left, &right) {
            (LuaType::IntegerConst(int1), LuaType::IntegerConst(int2)) => {
                Ok(LuaType::IntegerConst(int1 << int2))
            }
            _ => Ok(LuaType::Integer),
        };
    }

    infer_binary_custom_operator(db, &left, &right, LuaOperatorMetaMethod::Shl)
}

fn infer_binary_expr_shr(db: &DbIndex, left: LuaType, right: LuaType) -> InferResult {
    if left.is_integer() && right.is_integer() {
        return match (&left, &right) {
            (LuaType::IntegerConst(int1), LuaType::IntegerConst(int2)) => {
                Ok(LuaType::IntegerConst(int1 >> int2))
            }
            _ => Ok(LuaType::Integer),
        };
    }

    infer_binary_custom_operator(db, &left, &right, LuaOperatorMetaMethod::Shr)
}

fn infer_binary_expr_concat(db: &DbIndex, left: LuaType, right: LuaType) -> InferResult {
    if left.is_number() || left.is_string() || right.is_number() || right.is_string() {
        match (&left, &right) {
            (LuaType::StringConst(s1), LuaType::StringConst(s2)) => {
                return Ok(LuaType::StringConst(
                    SmolStr::new(format!("{}{}", s1.as_str(), s2.as_str())).into(),
                ));
            }
            (LuaType::StringConst(s1), LuaType::IntegerConst(i)) => {
                return Ok(LuaType::StringConst(
                    SmolStr::new(format!("{}{}", s1.as_str(), i)).into(),
                ));
            }
            (LuaType::IntegerConst(i), LuaType::StringConst(s2)) => {
                return Ok(LuaType::StringConst(
                    SmolStr::new(format!("{}{}", i, s2.as_str())).into(),
                ));
            }
            _ => return Ok(LuaType::String),
        }
    }

    infer_binary_custom_operator(db, &left, &right, LuaOperatorMetaMethod::Concat)
}

fn infer_binary_expr_and(db: &DbIndex, left: LuaType, right: LuaType) -> InferResult {
    if left.is_always_falsy() {
        return Ok(left);
    } else if left.is_always_truthy() {
        return Ok(right);
    }

    Ok(TypeOps::Union.apply(
        db,
        &TypeOps::NarrowFalseOrNil.apply_source(db, &left),
        &right,
    ))
}

fn infer_cmp_expr(_: &DbIndex, left: LuaType, right: LuaType, op: BinaryOperator) -> InferResult {
    match (left, right) {
        (LuaType::IntegerConst(i), LuaType::IntegerConst(j)) => {
            Ok(LuaType::BooleanConst(integer_cmp(i, j, op)))
        }
        (LuaType::IntegerConst(i), LuaType::DocIntegerConst(j)) => {
            Ok(LuaType::BooleanConst(integer_cmp(i, j, op)))
        }
        (LuaType::DocIntegerConst(i), LuaType::IntegerConst(j)) => {
            Ok(LuaType::BooleanConst(integer_cmp(i, j, op)))
        }
        (LuaType::DocIntegerConst(i), LuaType::DocIntegerConst(j)) => {
            Ok(LuaType::BooleanConst(integer_cmp(i, j, op)))
        }
        (LuaType::FloatConst(i), LuaType::FloatConst(j)) => {
            Ok(LuaType::BooleanConst(float_cmp(i, j, op)))
        }
        (LuaType::IntegerConst(i), LuaType::FloatConst(j)) => {
            Ok(LuaType::BooleanConst(float_cmp(i as f64, j, op)))
        }
        (LuaType::FloatConst(i), LuaType::IntegerConst(j)) => {
            Ok(LuaType::BooleanConst(float_cmp(i, j as f64, op)))
        }
        (LuaType::DocIntegerConst(i), LuaType::FloatConst(j)) => {
            Ok(LuaType::BooleanConst(float_cmp(i as f64, j, op)))
        }
        (LuaType::FloatConst(i), LuaType::DocIntegerConst(j)) => {
            Ok(LuaType::BooleanConst(float_cmp(i, j as f64, op)))
        }
        (LuaType::DocBooleanConst(i), LuaType::DocBooleanConst(j)) => match op {
            BinaryOperator::OpEq => Ok(LuaType::BooleanConst(i == j)),
            BinaryOperator::OpNe => Ok(LuaType::BooleanConst(i != j)),
            _ => Ok(LuaType::Boolean),
        },
        (LuaType::BooleanConst(i), LuaType::BooleanConst(j)) => match op {
            BinaryOperator::OpEq => Ok(LuaType::BooleanConst(i == j)),
            BinaryOperator::OpNe => Ok(LuaType::BooleanConst(i != j)),
            _ => Ok(LuaType::Boolean),
        },
        (LuaType::DocStringConst(i), LuaType::DocStringConst(j)) => match op {
            BinaryOperator::OpEq => Ok(LuaType::BooleanConst(i == j)),
            BinaryOperator::OpNe => Ok(LuaType::BooleanConst(i != j)),
            _ => Ok(LuaType::Boolean),
        },
        (LuaType::DocStringConst(i), LuaType::StringConst(j)) => match op {
            BinaryOperator::OpEq => Ok(LuaType::BooleanConst(i == j)),
            BinaryOperator::OpNe => Ok(LuaType::BooleanConst(i != j)),
            _ => Ok(LuaType::Boolean),
        },
        (LuaType::StringConst(i), LuaType::StringConst(j)) => match op {
            BinaryOperator::OpEq => Ok(LuaType::BooleanConst(i == j)),
            BinaryOperator::OpNe => Ok(LuaType::BooleanConst(i != j)),
            _ => Ok(LuaType::Boolean),
        },
        (LuaType::StringConst(i), LuaType::DocStringConst(j)) => match op {
            BinaryOperator::OpEq => Ok(LuaType::BooleanConst(i == j)),
            BinaryOperator::OpNe => Ok(LuaType::BooleanConst(i != j)),
            _ => Ok(LuaType::Boolean),
        },
        (LuaType::TableConst(i), LuaType::TableConst(j)) => match op {
            BinaryOperator::OpEq => Ok(LuaType::BooleanConst(i == j)),
            BinaryOperator::OpNe => Ok(LuaType::BooleanConst(i != j)),
            _ => Ok(LuaType::Boolean),
        },
        (left, right) if left.is_const() && right.is_const() => Ok(LuaType::BooleanConst(false)),
        _ => Ok(LuaType::Boolean),
    }
}

fn integer_cmp(left: i64, right: i64, op: BinaryOperator) -> bool {
    match op {
        BinaryOperator::OpGt => left > right,
        BinaryOperator::OpGe => left >= right,
        BinaryOperator::OpLt => left < right,
        BinaryOperator::OpLe => left <= right,
        BinaryOperator::OpEq => left == right,
        BinaryOperator::OpNe => left != right,
        _ => false,
    }
}

fn float_cmp(left: f64, right: f64, op: BinaryOperator) -> bool {
    match op {
        BinaryOperator::OpGt => left > right,
        BinaryOperator::OpGe => left >= right,
        BinaryOperator::OpLt => left < right,
        BinaryOperator::OpLe => left <= right,
        BinaryOperator::OpEq => left == right,
        BinaryOperator::OpNe => left != right,
        _ => false,
    }
}
