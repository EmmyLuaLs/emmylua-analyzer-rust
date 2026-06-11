//! Await in sync checker — pure salsa.

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaClosureExpr};

use crate::semantic_model::SemanticModel;
use crate::{AsyncState, DiagnosticCode, LuaSignatureId, LuaType};

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for call_expr in root.descendants::<LuaCallExpr>() {
        check_call(context, model, call_expr.clone());
        check_call_as_arg(context, model, call_expr);
    }
}

fn check_call(context: &mut DiagnosticContext, model: &SemanticModel, call: LuaCallExpr) {
    let Some(func_info) = model.infer_call_expr_func(call.clone(), None) else { return };
    if !func_info.is_async() { return }
    let Some(prefix) = call.get_prefix_expr() else { return };

    if !is_in_async_context(model, call) {
        context.add_diagnostic(DiagnosticCode::AwaitInSync, prefix.get_range(),
            t!("Async function can only be called in async function.").to_string(), None);
    }
}

fn check_call_as_arg(context: &mut DiagnosticContext, model: &SemanticModel, call: LuaCallExpr) {
    let Some(func_info) = model.infer_call_expr_func(call.clone(), None) else { return };
    let colon_def = func_info.is_colon_define;
    let colon_call = call.is_colon_call();

    for (i, (_, param_ty)) in func_info.params.iter().enumerate() {
        let Some(LuaType::DocFunction(f)) = param_ty else { continue };
        if f.get_async_state() != AsyncState::Async { continue }

        let Some(arg_list) = call.get_args_list() else { continue };
        let arg_idx = match (colon_def, colon_call) {
            (true, false) => i + 1,
            (false, true) => { if i == 0 { continue } i - 1 }
            _ => i,
        };
        let Some(arg) = arg_list.get_args().nth(arg_idx) else { continue };

        let Ok(arg_type) = model.infer_expr(arg.clone()) else { continue };
        let arg_is_async = match &arg_type {
            LuaType::DocFunction(f) => f.get_async_state() == AsyncState::Async,
            LuaType::Signature(sig) => {
                model.get_signature(model.get_file_id(), sig.get_position())
                    .is_some_and(|s| s.is_async())
            }
            _ => false,
        };

        if arg_is_async && !is_in_async_context(model, call.clone()) {
            context.add_diagnostic(DiagnosticCode::AwaitInSync, arg.get_range(),
                t!("Async function can only be called in async function.").to_string(), None);
        }
    }
}

fn is_in_async_context(model: &SemanticModel, call: LuaCallExpr) -> bool {
    let file_id = model.get_file_id();
    for closure in call.ancestors::<LuaClosureExpr>() {
        let sig_id = LuaSignatureId::from_closure(file_id, &closure);
        if let Some(sig) = model.get_signature(file_id, sig_id.get_position()) {
            if sig.is_async() { return true }
        }
    }
    false
}
