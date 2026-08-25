use emmylua_parser::LuaExpr;

use crate::{LuaType, SemanticModel};

use super::DiagnosticContext;

pub mod table_field_type_mismatch;
pub mod table_type_mismatch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TableAssignmentOutcome {
    NotTable,
    Fallback,
    Assignable,
    Reported,
    NoDiagnostic,
}

impl TableAssignmentOutcome {
    pub(crate) fn is_handled(self) -> bool {
        matches!(self, Self::Assignable | Self::Reported | Self::NoDiagnostic)
    }
}

pub(crate) fn check_table_assignment_diagnostics(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    value_expr: &LuaExpr,
    source_type: &LuaType,
    target_type: &LuaType,
) -> TableAssignmentOutcome {
    let LuaExpr::TableExpr(table_expr) = value_expr else {
        return TableAssignmentOutcome::NotTable;
    };

    table_type_mismatch::check_table_type_mismatch(
        context,
        semantic_model,
        table_expr,
        source_type,
        target_type,
    )
}
