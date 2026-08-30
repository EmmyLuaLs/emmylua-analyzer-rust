//! non_literal_expressions_in_assert — the `msg` in `local x = assert(v, msg)` should be a literal /
//! member access / local variable (neovim-code-style).

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr, LuaLocalStat};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::super::{CheckContext, Checker};

pub struct NonLiteralExpressionsInAssertChecker;

impl Checker for NonLiteralExpressionsInAssertChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::NonLiteralExpressionsInAssert];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for call_expr in root.descendants().filter_map(LuaCallExpr::cast) {
            if call_expr.is_assert() {
                check_assert_rule(context, semantic_model, &call_expr);
            }
        }
    }
}

fn check_assert_rule(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    call_expr: &LuaCallExpr,
) {
    // Only check `local a = assert(b, msg)`.
    if call_expr.get_parent::<LuaLocalStat>().is_none() {
        return;
    }
    let Some(args) = call_expr.get_args_list() else {
        return;
    };
    let arg_exprs = args.get_args().collect::<Vec<_>>();
    if arg_exprs.len() <= 1 {
        return;
    }
    let second_expr = &arg_exprs[1];
    match second_expr {
        // Literal / member access → OK.
        LuaExpr::LiteralExpr(_) | LuaExpr::IndexExpr(_) => return,
        // Local variable → OK.
        LuaExpr::NameExpr(name_expr) => {
            if name_expr.get_name_text().is_none() {
                return;
            }
            if let Some(decl_id) = semantic_model.resolve_name(name_expr.get_position())
                && let Some(facts) = semantic_model.file_facts()
                && let Some(decl) = facts.decl_by_id(&decl_id)
                && decl.kind.is_local()
            {
                return;
            }
        }
        _ => {}
    }

    context.add_diagnostic(
        DiagnosticCode::NonLiteralExpressionsInAssert,
        second_expr.get_range(),
        t!("assert message should be a literal or local variable"),
    );
}
