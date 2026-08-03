mod bind_binary_expr;

use emmylua_parser::LuaExpr;

use crate::{FlowId, compilation::analyzer::flow::binder::FlowBinder};

pub use bind_binary_expr::is_binary_logical;

/// Bind an expression (explicit task stack engine; the result always equals the input current)
pub fn bind_expr(binder: &mut FlowBinder, expr: LuaExpr, current: FlowId) -> FlowId {
    super::engine::run_bind_expr(binder, expr, current)
}
