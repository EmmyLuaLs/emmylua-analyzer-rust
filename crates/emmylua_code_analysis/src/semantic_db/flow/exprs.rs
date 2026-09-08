use emmylua_parser::LuaExpr;

use super::FlowId;
use super::binder::FlowBinder;

pub use super::bind_binary_expr::is_binary_logical;

/// Bind an expression (explicit task stack engine; the result always equals the input current)
pub fn bind_expr(binder: &mut FlowBinder, expr: LuaExpr, current: FlowId) -> FlowId {
    super::engine::run_bind_expr(binder, expr, current)
}
