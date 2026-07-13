//! Param type check — pure salsa.

use emmylua_parser::{LuaAst, LuaAstNode, LuaCallExpr, LuaExpr};

use crate::diagnostic::checker::humanize_lint_type_salsa;
use crate::semantic_model::SemanticModel;
use crate::{DiagnosticCode, LuaType};

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for node in root.descendants::<LuaAst>() {
        if let LuaAst::LuaCallExpr(call) = node {
            check_call(context, model, call);
        }
    }
}

fn check_call(context: &mut DiagnosticContext, model: &SemanticModel, call: LuaCallExpr) {
    let Some(func_info) = model.infer_call_expr_func(call.clone(), None) else {
        return;
    };
    let mut params = func_info.params.clone();
    let Some(args_list) = call.get_args_list() else {
        return;
    };
    let arg_exprs: Vec<_> = args_list.get_args().collect();
    let Ok(value_types) = model.infer_expr_list_types(&arg_exprs, None) else {
        return;
    };
    let mut arg_types: Vec<LuaType> = value_types.into_iter().map(|(t, _)| t).collect();

    let colon_call = call.is_colon_call();
    let colon_def = func_info.is_colon_define;
    match (colon_call, colon_def) {
        (true, true) => {
            // Both sides handle self implicitly — skip param[0] (self).
            if !params.is_empty() {
                params.remove(0);
            }
        }
        (true, false) => {
            // Colon call but callee doesn't expect self: insert receiver type
            if let Some(receiver_type) = infer_colon_call_receiver(model, &call) {
                arg_types.insert(0, receiver_type);
            } else {
                return;
            }
        }
        (false, true) => {
            // Regular call + colon define: callee expects self but caller didn't pass it.
            // Insert a self param so we skip it during comparison.
            params.insert(0, ("self".into(), Some(LuaType::SelfInfer)));
        }
        (false, false) => {}
    }

    for (idx, param) in params.iter().enumerate() {
        if param.0 == "..." {
            if arg_types.len() > idx {
                if let Some(var_type) = &param.1 {
                    check_variadic(context, model, var_type, &arg_types[idx..]);
                }
            }
            break;
        }
        let Some(param_type) = &param.1 else { continue };
        let Some(arg_type) = arg_types.get(idx) else {
            continue;
        };
        if model.type_check_detail(arg_type, param_type).is_err() {
            let Some(arg_pos) = call.get_args_list().and_then(|a| a.get_args().nth(idx)) else {
                continue;
            };
            context.add_diagnostic(
                DiagnosticCode::ParamTypeMismatch,
                arg_pos.get_range(),
                t!(
                    "expected `%{expected}` but found `%{found}`",
                    expected = humanize_lint_type_salsa(
                        context.get_salsa_db(),
                        context.get_file_id(),
                        param_type
                    ),
                    found = humanize_lint_type_salsa(
                        context.get_salsa_db(),
                        context.get_file_id(),
                        arg_type
                    )
                )
                .to_string(),
                None,
            );
        }
    }
}

/// For colon calls, extract the RECEIVER type (the `obj` in `obj:method()`),
/// not the member/function type.
fn infer_colon_call_receiver(model: &SemanticModel, call: &LuaCallExpr) -> Option<LuaType> {
    let prefix = call.get_prefix_expr()?;
    // The prefix for colon calls is a member/index expression like `obj.method`.
    // Get the inner prefix (the receiver `obj`).
    if let LuaExpr::IndexExpr(idx) = &prefix {
        let receiver = idx.get_prefix_expr()?;
        model.infer_expr(receiver).ok()
    } else {
        // Fallback: the prefix itself might be the receiver (simple cases)
        model.infer_expr(prefix).ok()
    }
}

fn check_variadic(
    _context: &mut DiagnosticContext,
    model: &SemanticModel,
    var_type: &LuaType,
    arg_types: &[LuaType],
) {
    for arg in arg_types {
        if model.type_check_detail(arg, var_type).is_err() {
            return;
        }
    }
}
