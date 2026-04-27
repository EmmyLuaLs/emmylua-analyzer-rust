use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaClosureExpr};

use crate::{AsyncState, DiagnosticCode, LuaSignatureId, LuaType, SemanticModel};

use super::{Checker, DiagnosticContext};

pub struct AwaitInSyncChecker;

impl Checker for AwaitInSyncChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::AwaitInSync];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for call_expr in root.descendants::<LuaCallExpr>() {
            check_call_in_async(context, semantic_model, call_expr.clone());
            check_call_as_arg(context, semantic_model, call_expr);
        }
    }
}

fn check_call_in_async(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
) -> Option<()> {
    let function_type = semantic_model.infer_call_expr_func(call_expr.clone(), None)?;
    let async_state = function_type.get_async_state();

    if async_state == AsyncState::Async
        && let Some(prefix_expr) = call_expr.get_prefix_expr()
        && check_async_func_in_sync_call(semantic_model, call_expr).is_err()
    {
        context.add_diagnostic(
            DiagnosticCode::AwaitInSync,
            prefix_expr.get_range(),
            t!("Async function can only be called in async function.").to_string(),
            None,
        );
    }

    Some(())
}

fn check_call_as_arg(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
) -> Option<()> {
    let func = semantic_model.infer_call_expr_func(call_expr.clone(), None)?;
    let colon_define = func.is_colon_define();
    let colon_call = call_expr.is_colon_call();
    for (i, arg_type) in func.get_params().iter().enumerate() {
        if let Some(LuaType::DocFunction(f)) = &arg_type.1 {
            let async_state = f.get_async_state();
            if async_state == AsyncState::Sync {
                let arg_list = call_expr.get_args_list()?;
                let arg_idx = match (colon_define, colon_call) {
                    (true, false) => i + 1,
                    (false, true) => {
                        if i == 0 {
                            return None; // colon call should not have a self argument
                        }
                        i - 1
                    }
                    _ => i,
                };
                let arg = arg_list.get_args().nth(arg_idx)?;
                let arg_type = semantic_model
                    .infer_expr(arg.clone())
                    .unwrap_or(LuaType::Any);
                let async_state = match &arg_type {
                    LuaType::DocFunction(f) => f.get_async_state(),
                    LuaType::Signature(sig) => {
                        let signature = semantic_model.get_signature(sig)?;
                        signature.async_state
                    }
                    _ => continue,
                };

                if async_state == AsyncState::Async
                    && check_async_func_in_sync_call(semantic_model, call_expr.clone()).is_err()
                {
                    context.add_diagnostic(
                        DiagnosticCode::AwaitInSync,
                        arg.get_range(),
                        t!("Async function can only be called in async function.").to_string(),
                        None,
                    );
                }
            }
        }
    }

    Some(())
}

fn check_async_func_in_sync_call(
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
) -> Result<(), ()> {
    let file_id = semantic_model.get_file_id();
    let closures = call_expr.ancestors::<LuaClosureExpr>();
    for closure in closures {
        let signature_id = LuaSignatureId::from_closure(file_id, &closure);
        let Some(signature) = semantic_model.get_signature(&signature_id) else {
            return Ok(());
        };

        match signature.async_state {
            AsyncState::Sync => continue,
            AsyncState::None => {
                return Err(());
            }
            _ => return Ok(()),
        }
    }

    Err(())
}
