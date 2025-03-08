use emmylua_parser::{LuaAstNode, LuaCallExprStat};
use rowan::NodeOrToken;

use crate::{DiagnosticCode, SemanticModel};

use super::DiagnosticContext;

pub const CODES: &[DiagnosticCode] = &[DiagnosticCode::DiscardReturns];

pub fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) -> Option<()> {
    let root = semantic_model.get_root().clone();
    for call_expr_stat in root.descendants::<LuaCallExprStat>() {
        check_call_expr(context, semantic_model, call_expr_stat);
    }

    Some(())
}

fn check_call_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr_stat: LuaCallExprStat,
) -> Option<()> {
    let call_expr = call_expr_stat.get_call_expr()?;
    let prefix_node = call_expr.get_prefix_expr()?.syntax().clone();
    let property_owner =
        semantic_model.get_property_owner_id(NodeOrToken::Node(prefix_node.clone()))?;

    let property = semantic_model
        .get_db()
        .get_property_index()
        .get_property(property_owner)?;
    if property.is_nodiscard {
        context.add_diagnostic(
            DiagnosticCode::DiscardReturns,
            prefix_node.text_range(),
            "discard returns".to_string(),
            None,
        );
    }

    Some(())
}
