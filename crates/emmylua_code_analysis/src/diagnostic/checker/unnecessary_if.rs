//! Unnecessary if checker — salsa-native.

use emmylua_parser::{LuaAstNode, LuaExpr, LuaIfStat};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for if_stat in root.descendants::<LuaIfStat>() {
        if let Some(cond) = if_stat.get_condition_expr() {
            check_condition(context, model, cond);
        }
        for clause in if_stat.get_else_if_clause_list() {
            if let Some(cond) = clause.get_condition_expr() {
                check_condition(context, model, cond);
            }
        }
    }
}

fn check_condition(context: &mut DiagnosticContext, model: &SemanticModel, cond: LuaExpr) {
    let Ok(ty) = model.infer_expr(cond.clone()) else {
        return;
    };
    if ty.is_always_truthy() {
        context.add_diagnostic(
            DiagnosticCode::UnnecessaryIf,
            cond.get_range(),
            t!("Unnecessary `if` statement: this condition is always truthy").to_string(),
            None,
        );
    } else if ty.is_always_falsy() {
        context.add_diagnostic(
            DiagnosticCode::UnnecessaryIf,
            cond.get_range(),
            t!("Impossible `if` statement: this condition is always falsy").to_string(),
            None,
        );
    }
}
