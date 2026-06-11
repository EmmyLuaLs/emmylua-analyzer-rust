//! Call non-callable checker — pure salsa.

use emmylua_parser::{LuaAstNode, LuaCallExpr};

use crate::semantic_model::SemanticModel;
use crate::{DiagnosticCode, LuaType};

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    for call in model.get_root().descendants::<LuaCallExpr>() {
        check_call(context, model, call);
    }
}

fn check_call(context: &mut DiagnosticContext, model: &SemanticModel, call: LuaCallExpr) {
    let Some(prefix) = call.get_prefix_expr() else { return };
    if model.infer_call_expr_func(call, None).is_some() { return }
    let Ok(prefix_type) = model.infer_expr(prefix.clone()) else { return };
    if matches!(prefix_type, LuaType::Any | LuaType::Unknown) { return }
    context.add_diagnostic(DiagnosticCode::CallNonCallable, prefix.get_range(),
        t!("%{name} is not callable", name = prefix.syntax().text()).to_string(), None);
}
