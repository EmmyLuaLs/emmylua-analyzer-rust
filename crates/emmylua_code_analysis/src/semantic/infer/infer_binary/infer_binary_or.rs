use emmylua_parser::LuaExpr;

use crate::{
    DbIndex, LuaType, TypeOps, check_type_compact,
    semantic::infer::{InferResult, narrow::remove_false_or_nil},
};

pub fn special_or_rule(
    db: &DbIndex,
    left_type: &LuaType,
    right_type: &LuaType,
    left_expr: LuaExpr,
    right_expr: LuaExpr,
) -> Option<LuaType> {
    match right_expr {
        // workaround for x or error('')
        LuaExpr::CallExpr(call_expr) => {
            if call_expr.is_error() {
                return Some(remove_false_or_nil(left_type.clone()));
            }
        }
        LuaExpr::TableExpr(table_expr) => {
            if table_expr.is_empty() {
                // Remove nil/false from left type and check if result is table-compatible
                let left_without_nil = remove_false_or_nil(left_type.clone());
                if check_type_compact(db, &left_without_nil, &LuaType::Table).is_ok() {
                    return Some(left_without_nil);
                }
            }
        }
        LuaExpr::LiteralExpr(_) => {
            match left_expr {
                LuaExpr::CallExpr(_) | LuaExpr::NameExpr(_) | LuaExpr::IndexExpr(_) => {}
                _ => return None,
            }

            if right_type.is_nil() || left_type.is_const() {
                return None;
            }

            if check_type_compact(db, left_type, right_type).is_ok() {
                return Some(remove_false_or_nil(left_type.clone()));
            }
        }

        _ => {}
    }

    None
}

pub fn infer_binary_expr_or(db: &DbIndex, left: LuaType, right: LuaType) -> InferResult {
    if left.is_always_truthy() {
        return Ok(left);
    } else if left.is_always_falsy() {
        return Ok(right);
    }

    Ok(TypeOps::Union.apply(db, &remove_false_or_nil(left), &right))
}
