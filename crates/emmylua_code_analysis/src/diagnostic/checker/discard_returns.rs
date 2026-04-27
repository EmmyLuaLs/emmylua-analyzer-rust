use emmylua_parser::{LuaAstNode, LuaCallExprStat};
use rowan::NodeOrToken;

use crate::{DiagnosticCode, LuaNoDiscard, SemanticDeclLevel, SemanticModel};

use super::{Checker, DiagnosticContext};

pub struct DiscardReturnsChecker;

impl Checker for DiscardReturnsChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::DiscardReturns];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for call_expr_stat in root.descendants::<LuaCallExprStat>() {
            check_call_expr(context, semantic_model, call_expr_stat);
        }
    }
}

fn check_call_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr_stat: LuaCallExprStat,
) -> Option<()> {
    let call_expr = call_expr_stat.get_call_expr()?;
    let prefix_node = call_expr.get_prefix_expr()?.syntax().clone();
    let semantic_decl = semantic_model.find_decl(
        NodeOrToken::Node(prefix_node.clone()),
        SemanticDeclLevel::default(),
    )?;

    let signature_id = semantic_model.signature_id_of_decl(semantic_decl)?;
    let signature = semantic_model.get_signature(&signature_id)?;
    if let Some(nodiscard) = &signature.nodiscard {
        let nodiscard_message = match nodiscard {
            LuaNoDiscard::NoDiscard => "no discard".to_string(),
            LuaNoDiscard::NoDiscardWithMessage(message) => message.to_string(),
        };

        context.add_diagnostic(
            DiagnosticCode::DiscardReturns,
            prefix_node.text_range(),
            nodiscard_message,
            None,
        );
    }

    Some(())
}
