use emmylua_parser::{BinaryOperator, LuaExpr, UnaryOperator};

pub fn is_binary_logical(expr: &LuaExpr) -> bool {
    match expr {
        LuaExpr::BinaryExpr(binary_expr) => {
            let Some(op_token) = binary_expr.get_op_token() else {
                return false;
            };

            return matches!(
                op_token.get_op(),
                BinaryOperator::OpAnd | BinaryOperator::OpOr | BinaryOperator::OpNilCoalescing
            );
        }
        LuaExpr::ParenExpr(paren_expr) => {
            if let Some(inner_expr) = paren_expr.get_expr() {
                return is_binary_logical(&inner_expr);
            }
        }
        LuaExpr::UnaryExpr(unary_expr) => {
            let is_not = unary_expr
                .get_op_token()
                .is_some_and(|op| op.get_op() == UnaryOperator::OpNot);
            if is_not && let Some(inner_expr) = unary_expr.get_expr() {
                return is_binary_logical(&inner_expr);
            }
        }
        _ => {}
    }
    false
}
