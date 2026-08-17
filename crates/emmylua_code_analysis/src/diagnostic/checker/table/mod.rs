use emmylua_parser::{LuaAstNode, LuaExpr};

use crate::{DiagnosticCode, LuaType, SemanticModel};

use super::DiagnosticContext;

pub mod table_field_type_mismatch;
pub mod table_type_mismatch;

pub(crate) fn check_table_assignment_diagnostics(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    value_expr: &LuaExpr,
    source_type: &LuaType,
    target_type: &LuaType,
) -> bool {
    let LuaExpr::TableExpr(table_expr) = value_expr else {
        return false;
    };
    if context
        .has_diagnostic_codes_in_range(table_expr.get_range(), &[DiagnosticCode::MissingFields])
    {
        return true;
    }

    table_type_mismatch::check_table_type_mismatch(
        context,
        semantic_model,
        table_expr,
        source_type,
        target_type,
    )
}
