//! # unnecessary_if —— if conditions that are always truthy / always falsy

use emmylua_parser::{LuaAstNode, LuaIfStat};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct UnnecessaryIfChecker;

impl Checker for UnnecessaryIfChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::UnnecessaryIf];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for if_statement in root.descendants().filter_map(LuaIfStat::cast) {
            if let Some(condition) = if_statement.get_condition_expr() {
                check_condition(context, semantic_model, &condition);
            }
            for clause in if_statement.get_else_if_clause_list() {
                if let Some(condition) = clause.get_condition_expr() {
                    check_condition(context, semantic_model, &condition);
                }
            }
        }
    }
}

fn check_condition(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    condition: &emmylua_parser::LuaExpr,
) {
    let expr_type = semantic_model.type_of_expr(condition.get_syntax_id());
    if expr_type.is_always_truthy() {
        context.add_diagnostic(
            DiagnosticCode::UnnecessaryIf,
            condition.get_range(),
            t!("Unnecessary `if` statement: this condition is always truthy"),
        );
    } else if expr_type.is_always_falsy() {
        context.add_diagnostic(
            DiagnosticCode::UnnecessaryIf,
            condition.get_range(),
            t!("Impossible `if` statement: this condition is always falsy"),
        );
    }
}
