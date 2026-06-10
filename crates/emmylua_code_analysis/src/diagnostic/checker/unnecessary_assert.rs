//! Unnecessary assert checker — salsa-native.

use emmylua_parser::{LuaAstNode, LuaCallExpr};

use crate::semantic_model::SemanticModel;
use crate::{DiagnosticCode, LuaType};

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for call_expr in root.descendants::<LuaCallExpr>() {
        if call_expr.is_assert() {
            check_assert(context, model, call_expr);
        }
    }
}

fn check_assert(context: &mut DiagnosticContext, model: &SemanticModel, call_expr: LuaCallExpr) {
    let Some(args) = call_expr.get_args_list() else { return };
    let arg_exprs: Vec<_> = args.get_args().collect();
    let Some(first) = arg_exprs.first() else { return };

    let Ok(expr_type) = model.infer_expr(first.clone()) else { return };
    let first_type = match &expr_type {
        LuaType::Variadic(multi) => multi.get_type(0).cloned().unwrap_or(expr_type),
        _ => expr_type,
    };

    if first_type.is_always_truthy() {
        context.add_diagnostic(
            DiagnosticCode::UnnecessaryAssert,
            call_expr.get_range(),
            t!("Unnecessary assert: this expression is always truthy").to_string(),
            None,
        );
    } else if first_type.is_always_falsy() {
        context.add_diagnostic(
            DiagnosticCode::UnnecessaryAssert,
            call_expr.get_range(),
            t!("Impossible assert: this expression is always falsy; prefer `error()`").to_string(),
            None,
        );
    }
}
