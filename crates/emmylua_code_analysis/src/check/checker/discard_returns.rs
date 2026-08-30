//! # discard_returns — return values of `---@nodiscard` functions are discarded
//!
//! M0: the callee of a call statement (`LuaCallExprStat`) resolves to a signature whose doc has `@nodiscard`
//! -> report `DiscardReturns` (the message is taken from the annotation description).

use emmylua_parser::{LuaAstNode, LuaCallExprStat};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct DiscardReturnsChecker;

impl Checker for DiscardReturnsChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::DiscardReturns];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for call_expr_stat in root.descendants().filter_map(LuaCallExprStat::cast) {
            check_call(context, semantic_model, &call_expr_stat);
        }
    }
}

fn check_call(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    call_expr_stat: &LuaCallExprStat,
) {
    let Some(call_expr) = call_expr_stat.get_call_expr() else {
        return;
    };
    let Some(prefix) = call_expr.get_prefix_expr() else {
        return;
    };
    let range = prefix.get_range();
    // callee name -> declaration -> closure signature -> nodiscard.
    let emmylua_parser::LuaExpr::NameExpr(name_expr) = prefix else {
        return;
    };
    let Some(decl_id) = semantic_model.resolve_name(name_expr.get_position()) else {
        return;
    };
    let Some(facts) = semantic_model.file_facts() else {
        return;
    };
    let Some(decl) = facts.decl_by_id(&decl_id) else {
        return;
    };
    let Some(value_syntax) = decl.value_expr_syntax else {
        return;
    };
    let Some(signatures) = semantic_model.signatures() else {
        return;
    };
    let Some(signature) = signatures
        .iter()
        .find(|sig| sig.closure_syntax == value_syntax)
    else {
        return;
    };
    let Some(docs) = &signature.docs else {
        return;
    };
    let Some(nodiscard) = &docs.nodiscard else {
        return;
    };
    context.add_diagnostic(
        DiagnosticCode::DiscardReturns,
        range,
        if nodiscard.is_empty() {
            "no discard".to_string()
        } else {
            nodiscard.to_string()
        },
    );
}
