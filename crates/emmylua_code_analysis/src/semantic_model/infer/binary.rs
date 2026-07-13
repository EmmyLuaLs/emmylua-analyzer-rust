//! 二元表达式推断 — `a + b`, `a .. b`, `a < b`, `a and b`, `a or b`

use emmylua_parser::{BinaryOperator, LuaBinaryExpr};
use smol_str::SmolStr;

use crate::{LuaType, LuaUnionType};

use super::{InferFailReason, InferQuery, InferResult};

pub(super) fn infer_binary_expr(infer: &InferQuery, binary_expr: LuaBinaryExpr) -> InferResult {
    let op = binary_expr
        .get_op_token()
        .ok_or(InferFailReason::None)?
        .get_op();
    let (left_expr, right_expr) = binary_expr.get_exprs().ok_or(InferFailReason::None)?;
    let left_type = infer.infer_expr(left_expr.clone())?;
    let right_type = infer.infer_expr(right_expr.clone())?;

    // 逻辑运算符特殊处理
    match op {
        BinaryOperator::OpOr => {
            return infer_binary_or(&left_type, &right_type);
        }
        BinaryOperator::OpAnd => {
            return infer_binary_and(&left_type, &right_type);
        }
        _ => {}
    }

    // Union 分发
    if let Some(result) = infer_union_binary(&left_type, &right_type, op) {
        return result;
    }

    infer_binary_op(&left_type, &right_type, op)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Union 分发
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn infer_union_binary(left: &LuaType, right: &LuaType, op: BinaryOperator) -> Option<InferResult> {
    let (union_types, other, is_left) = if let LuaType::Union(u) = left {
        (u.into_vec(), right, true)
    } else if let LuaType::Union(u) = right {
        (u.into_vec(), left, false)
    } else {
        return None;
    };

    let mut result: Option<LuaType> = None;
    for ty in &union_types {
        let ty_result = if is_left {
            infer_binary_op(ty, other, op)
        } else {
            infer_binary_op(other, ty, op)
        };
        if let Ok(ty) = ty_result {
            result = Some(match result {
                Some(prev) => union_two_types(prev, ty),
                None => ty,
            });
        }
    }
    Some(result.ok_or(InferFailReason::None).or(Ok(LuaType::Unknown)))
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 运算符分发
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn infer_binary_op(left: &LuaType, right: &LuaType, op: BinaryOperator) -> InferResult {
    match op {
        BinaryOperator::OpAdd => infer_add(left, right),
        BinaryOperator::OpSub => infer_sub(left, right),
        BinaryOperator::OpMul => infer_mul(left, right),
        BinaryOperator::OpDiv => infer_div(left, right),
        BinaryOperator::OpIDiv => infer_idiv(left, right),
        BinaryOperator::OpMod => infer_mod(left, right),
        BinaryOperator::OpPow => infer_pow(left, right),
        BinaryOperator::OpBAnd => infer_band(left, right),
        BinaryOperator::OpBOr => infer_bor(left, right),
        BinaryOperator::OpBXor => infer_bxor(left, right),
        BinaryOperator::OpShl => infer_shl(left, right),
        BinaryOperator::OpShr => infer_shr(left, right),
        BinaryOperator::OpConcat => infer_concat(left, right),
        BinaryOperator::OpLt
        | BinaryOperator::OpLe
        | BinaryOperator::OpGt
        | BinaryOperator::OpGe
        | BinaryOperator::OpEq
        | BinaryOperator::OpNe => infer_cmp(left, right, op),
        BinaryOperator::OpOr => infer_binary_or(left, right),
        BinaryOperator::OpAnd => infer_binary_and(left, right),
        _ => Ok(left.clone()),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 算术运算符
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn infer_add(left: &LuaType, right: &LuaType) -> InferResult {
    if left.is_number() && right.is_number() {
        return infer_number_op(left, right, |a, b| a.checked_add(b), |a, b| a + b);
    }
    // nil/any/unknown propagation
    if left.is_nil() || left.is_any() || left.is_unknown() {
        return Ok(right.clone());
    }
    if right.is_nil() || right.is_any() || right.is_unknown() {
        return Ok(left.clone());
    }
    // concat: string + anything or anything + string
    if left.is_string() || right.is_string() {
        return infer_concat_strings(left, right);
    }
    Ok(LuaType::Number)
}

fn infer_sub(left: &LuaType, right: &LuaType) -> InferResult {
    if left.is_number() && right.is_number() {
        return infer_number_op(left, right, |a, b| a.checked_sub(b), |a, b| a - b);
    }
    Ok(LuaType::Number)
}

fn infer_mul(left: &LuaType, right: &LuaType) -> InferResult {
    if left.is_number() && right.is_number() {
        return infer_number_op(left, right, |a, b| a.checked_mul(b), |a, b| a * b);
    }
    Ok(LuaType::Number)
}

fn infer_div(left: &LuaType, right: &LuaType) -> InferResult {
    if left.is_number() && right.is_number() {
        return match (left, right) {
            (LuaType::IntegerConst(a), LuaType::IntegerConst(b)) => {
                if *b != 0 {
                    if a % b != 0 {
                        Ok(LuaType::FloatConst(*a as f64 / *b as f64))
                    } else {
                        Ok(LuaType::IntegerConst(a / b))
                    }
                } else {
                    Ok(LuaType::Number)
                }
            }
            (LuaType::FloatConst(a), LuaType::FloatConst(b)) => {
                if *b != 0.0 {
                    Ok(LuaType::FloatConst(a / b))
                } else {
                    Ok(LuaType::Number)
                }
            }
            (LuaType::IntegerConst(a), LuaType::FloatConst(b)) => {
                if *b != 0.0 {
                    Ok(LuaType::FloatConst(*a as f64 / b))
                } else {
                    Ok(LuaType::Number)
                }
            }
            (LuaType::FloatConst(a), LuaType::IntegerConst(b)) => {
                if *b != 0 {
                    Ok(LuaType::FloatConst(a / *b as f64))
                } else {
                    Ok(LuaType::Number)
                }
            }
            _ => Ok(LuaType::Number),
        };
    }
    Ok(LuaType::Number)
}

fn infer_idiv(left: &LuaType, right: &LuaType) -> InferResult {
    if left.is_integer() && right.is_integer() {
        return match (left, right) {
            (LuaType::IntegerConst(a), LuaType::IntegerConst(b)) => {
                if *b != 0 {
                    Ok(LuaType::IntegerConst(a / b))
                } else {
                    Ok(LuaType::Integer)
                }
            }
            _ => Ok(LuaType::Integer),
        };
    }
    Ok(LuaType::Integer)
}

fn infer_mod(left: &LuaType, right: &LuaType) -> InferResult {
    if left.is_number() && right.is_number() {
        return match (left, right) {
            (LuaType::IntegerConst(a), LuaType::IntegerConst(b)) => {
                if *b != 0 {
                    Ok(LuaType::IntegerConst(a % b))
                } else {
                    Ok(LuaType::Integer)
                }
            }
            _ => Ok(LuaType::Number),
        };
    }
    Ok(LuaType::Number)
}

fn infer_pow(left: &LuaType, right: &LuaType) -> InferResult {
    if left.is_number() && right.is_number() {
        return match (left, right) {
            (LuaType::IntegerConst(a), LuaType::IntegerConst(b)) => {
                if let Some(result) = a.checked_pow(*b as u32) {
                    Ok(LuaType::IntegerConst(result))
                } else {
                    Ok(LuaType::Number)
                }
            }
            (LuaType::FloatConst(a), LuaType::IntegerConst(b)) => {
                Ok(LuaType::FloatConst(a.powf(*b as f64)))
            }
            _ => Ok(LuaType::Number),
        };
    }
    Ok(LuaType::Number)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 位运算符
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn infer_band(left: &LuaType, right: &LuaType) -> InferResult {
    if left.is_integer() && right.is_integer() {
        return match (left, right) {
            (LuaType::IntegerConst(a), LuaType::IntegerConst(b)) => {
                Ok(LuaType::IntegerConst(a & b))
            }
            _ => Ok(LuaType::Integer),
        };
    }
    Ok(LuaType::Integer)
}

fn infer_bor(left: &LuaType, right: &LuaType) -> InferResult {
    if left.is_integer() && right.is_integer() {
        return match (left, right) {
            (LuaType::IntegerConst(a), LuaType::IntegerConst(b)) => {
                Ok(LuaType::IntegerConst(a | b))
            }
            _ => Ok(LuaType::Integer),
        };
    }
    Ok(LuaType::Integer)
}

fn infer_bxor(left: &LuaType, right: &LuaType) -> InferResult {
    if left.is_integer() && right.is_integer() {
        return match (left, right) {
            (LuaType::IntegerConst(a), LuaType::IntegerConst(b)) => {
                Ok(LuaType::IntegerConst(a ^ b))
            }
            _ => Ok(LuaType::Integer),
        };
    }
    Ok(LuaType::Integer)
}

fn infer_shl(left: &LuaType, right: &LuaType) -> InferResult {
    if left.is_integer() && right.is_integer() {
        return match (left, right) {
            (LuaType::IntegerConst(a), LuaType::IntegerConst(b)) => {
                Ok(LuaType::IntegerConst(a.checked_shl(*b as u32).unwrap_or(0)))
            }
            _ => Ok(LuaType::Integer),
        };
    }
    Ok(LuaType::Integer)
}

fn infer_shr(left: &LuaType, right: &LuaType) -> InferResult {
    if left.is_integer() && right.is_integer() {
        return match (left, right) {
            (LuaType::IntegerConst(a), LuaType::IntegerConst(b)) => {
                Ok(LuaType::IntegerConst(a.checked_shr(*b as u32).unwrap_or(0)))
            }
            _ => Ok(LuaType::Integer),
        };
    }
    Ok(LuaType::Integer)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 拼接运算符
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn infer_concat(left: &LuaType, right: &LuaType) -> InferResult {
    if left.is_number() || left.is_string() || right.is_number() || right.is_string() {
        return infer_concat_strings(left, right);
    }
    Ok(LuaType::String)
}

fn infer_concat_strings(left: &LuaType, right: &LuaType) -> InferResult {
    match (left, right) {
        (LuaType::StringConst(s1), LuaType::StringConst(s2)) => Ok(LuaType::StringConst(
            SmolStr::new(format!("{}{}", s1.as_str(), s2.as_str())).into(),
        )),
        (LuaType::StringConst(s1), LuaType::IntegerConst(i)) => Ok(LuaType::StringConst(
            SmolStr::new(format!("{}{}", s1.as_str(), i)).into(),
        )),
        (LuaType::IntegerConst(i), LuaType::StringConst(s2)) => Ok(LuaType::StringConst(
            SmolStr::new(format!("{}{}", i, s2.as_str())).into(),
        )),
        _ => Ok(LuaType::String),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 比较运算符
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn infer_cmp(left: &LuaType, right: &LuaType, op: BinaryOperator) -> InferResult {
    // 整数比较
    if let (Some(li), Some(lj)) = (get_int_const(left), get_int_const(right)) {
        return Ok(LuaType::BooleanConst(integer_cmp(li, lj, op)));
    }
    // 浮点比较
    if let (Some(fi), Some(fj)) = (get_float_const(left), get_float_const(right)) {
        return Ok(LuaType::BooleanConst(float_cmp(fi, fj, op)));
    }
    // Int + Float mixed
    if let (Some(fi), Some(fj)) = (as_float(left), as_float(right)) {
        return Ok(LuaType::BooleanConst(float_cmp(fi, fj, op)));
    }
    // 布尔常量 eq/ne
    match (left, right, op) {
        (LuaType::BooleanConst(a), LuaType::BooleanConst(b), _)
        | (LuaType::DocBooleanConst(a), LuaType::DocBooleanConst(b), _) => match op {
            BinaryOperator::OpEq => return Ok(LuaType::BooleanConst(a == b)),
            BinaryOperator::OpNe => return Ok(LuaType::BooleanConst(a != b)),
            _ => return Ok(LuaType::Boolean),
        },
        _ => {}
    }
    // 字符串常量 eq/ne
    if let (Some(a), Some(b)) = (get_str_const(left), get_str_const(right)) {
        return match op {
            BinaryOperator::OpEq => Ok(LuaType::BooleanConst(a == b)),
            BinaryOperator::OpNe => Ok(LuaType::BooleanConst(a != b)),
            _ => Ok(LuaType::Boolean),
        };
    }
    // Table 常量 eq/ne
    if let (LuaType::TableConst(a), LuaType::TableConst(b)) = (left, right) {
        match op {
            BinaryOperator::OpEq => return Ok(LuaType::BooleanConst(a == b)),
            BinaryOperator::OpNe => return Ok(LuaType::BooleanConst(a != b)),
            _ => return Ok(LuaType::Boolean),
        }
    }
    if left.is_const() && right.is_const() {
        return Ok(LuaType::BooleanConst(false));
    }
    Ok(LuaType::Boolean)
}

fn get_int_const(ty: &LuaType) -> Option<i64> {
    match ty {
        LuaType::IntegerConst(i) => Some(*i),
        LuaType::DocIntegerConst(i) => Some(*i),
        _ => None,
    }
}

fn get_float_const(ty: &LuaType) -> Option<f64> {
    match ty {
        LuaType::FloatConst(f) => Some(*f),
        _ => None,
    }
}

fn get_str_const(ty: &LuaType) -> Option<&SmolStr> {
    match ty {
        LuaType::StringConst(s) | LuaType::DocStringConst(s) => Some(s),
        _ => None,
    }
}

fn as_float(ty: &LuaType) -> Option<f64> {
    match ty {
        LuaType::FloatConst(f) => Some(*f),
        LuaType::IntegerConst(i) => Some(*i as f64),
        LuaType::DocIntegerConst(i) => Some(*i as f64),
        _ => None,
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

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 逻辑运算符
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `a and b`: 若 a 恒假 → a；若 a 恒真 → b；否则 → falsy_part(a) | b
fn infer_binary_and(left: &LuaType, right: &LuaType) -> InferResult {
    if left.is_always_falsy() {
        return Ok(left.clone());
    }
    if left.is_always_truthy() {
        return Ok(right.clone());
    }
    let falsy = narrow_false_or_nil(left);
    Ok(union_two_types(falsy, right.clone()))
}

/// `a or b`: 若 a 恒真 → a；若 a 恒假 → b；否则 → truthy_part(a) | b
fn infer_binary_or(left: &LuaType, right: &LuaType) -> InferResult {
    if left.is_always_truthy() {
        return Ok(left.clone());
    }
    if left.is_always_falsy() {
        return Ok(right.clone());
    }
    let truthy = remove_false_or_nil(left);
    Ok(union_two_types(truthy, right.clone()))
}

/// 提取类型的 falsy 部分（nil、false）。
fn narrow_false_or_nil(ty: &LuaType) -> LuaType {
    match ty {
        LuaType::Nil => LuaType::Nil,
        LuaType::BooleanConst(false) | LuaType::DocBooleanConst(false) => {
            LuaType::BooleanConst(false)
        }
        LuaType::Boolean => {
            LuaType::Union(LuaUnionType::Nullable(LuaType::BooleanConst(false)).into())
        }
        LuaType::Union(u) => {
            let parts: Vec<LuaType> = u
                .into_vec()
                .into_iter()
                .map(|t| narrow_false_or_nil(&t))
                .filter(|t| !t.is_never())
                .collect();
            LuaType::from_vec(parts)
        }
        _ => LuaType::Never,
    }
}

/// 移除类型的 falsy 部分（nil、false），只保留 truthy 部分。
fn remove_false_or_nil(ty: &LuaType) -> LuaType {
    match ty {
        LuaType::Nil => LuaType::Never,
        LuaType::BooleanConst(false) | LuaType::DocBooleanConst(false) => LuaType::Never,
        LuaType::BooleanConst(true) | LuaType::DocBooleanConst(true) => LuaType::BooleanConst(true),
        LuaType::Boolean => LuaType::BooleanConst(true),
        LuaType::Union(u) => {
            let parts: Vec<LuaType> = u
                .into_vec()
                .into_iter()
                .map(|t| remove_false_or_nil(&t))
                .filter(|t| !t.is_never())
                .collect();
            LuaType::from_vec(parts)
        }
        other => other.clone(),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 工具函数
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 二元数值运算的通用处理。
fn infer_number_op(
    left: &LuaType,
    right: &LuaType,
    int_op: fn(i64, i64) -> Option<i64>,
    float_op: fn(f64, f64) -> f64,
) -> InferResult {
    match (left, right) {
        (LuaType::IntegerConst(a), LuaType::IntegerConst(b)) => {
            if let Some(result) = int_op(*a, *b) {
                Ok(LuaType::IntegerConst(result))
            } else {
                Ok(LuaType::Number)
            }
        }
        (LuaType::FloatConst(a), LuaType::FloatConst(b)) => {
            Ok(LuaType::FloatConst(float_op(*a, *b)))
        }
        (LuaType::IntegerConst(a), LuaType::FloatConst(b)) => {
            Ok(LuaType::FloatConst(float_op(*a as f64, *b)))
        }
        (LuaType::FloatConst(a), LuaType::IntegerConst(b)) => {
            Ok(LuaType::FloatConst(float_op(*a, *b as f64)))
        }
        _ => {
            if left.is_integer() && right.is_integer() {
                Ok(LuaType::Integer)
            } else {
                Ok(LuaType::Number)
            }
        }
    }
}

/// 合并两个类型为 union，使用 `LuaType::from_vec` 处理去重和展平。
fn union_two_types(left: LuaType, right: LuaType) -> LuaType {
    if left == right {
        return left;
    }
    if left.is_any() || right.is_any() {
        return LuaType::Any;
    }
    if left.is_never() {
        return right;
    }
    if right.is_never() {
        return left;
    }
    // 合并两个 union
    let mut all = Vec::new();
    push_non_union(left, &mut all);
    push_non_union(right, &mut all);
    LuaType::from_vec(all)
}

fn push_non_union(ty: LuaType, out: &mut Vec<LuaType>) {
    match ty {
        LuaType::Union(u) => out.extend(u.into_vec()),
        other => out.push(other),
    }
}
