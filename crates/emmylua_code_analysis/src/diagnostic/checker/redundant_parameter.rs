use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaLiteralExpr, LuaLiteralToken};

use crate::{DiagnosticCode, SemanticModel};

use super::DiagnosticContext;

pub const CODES: &[DiagnosticCode] = &[DiagnosticCode::RedundantParameter];

pub fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) -> Option<()> {
    let root = semantic_model.get_root().clone();
    for call_expr in root.descendants::<LuaCallExpr>() {
        check_call_expr(context, semantic_model, call_expr);
    }

    Some(())
}
fn check_call_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
) -> Option<()> {
    let func = semantic_model.infer_call_expr_func(call_expr.clone(), None)?;
    let params = func.get_params();
    let args_list = call_expr.get_args_list()?.get_args();
    let mut args_count = args_list.clone().count();
    let colon_call = call_expr.is_colon_call();
    let colon_define = func.is_colon_define();

    match (colon_call, colon_define) {
        (true, true) | (false, false) => {}
        (false, true) => {
            if args_count > 0 {
                args_count -= 1;
            }
        }
        (true, false) => {
            args_count += 1;
        }
    }

    if args_count > params.len() {
        // 调用参数是 `...`
        if let Some(last_arg) = call_expr.get_args_list()?.child::<LuaLiteralExpr>() {
            if let Some(literal_token) = last_arg.get_literal() {
                if let LuaLiteralToken::Dots(_) = literal_token {
                    return Some(());
                }
            }
        }
        // 参数定义中最后一个参数是 `...`
        if params.last().map_or(false, |(name, _)| name == "...") {
            return Some(());
        }

        let mut adjusted_index = 0;
        if colon_call != colon_define {
            adjusted_index = if colon_define && !colon_call { -1 } else { 1 };
        }

        for (i, arg) in args_list.enumerate() {
            let param_index = i as isize + adjusted_index;

            if param_index < 0 || param_index < params.len() as isize {
                continue;
            }

            context.add_diagnostic(
                DiagnosticCode::RedundantParameter,
                arg.get_range(),
                t!(
                    "expected %{num} parameters but found %{found_num}",
                    num = params.len(),
                    found_num = args_count,
                )
                .to_string(),
                None,
            );
        }
    }

    Some(())
}
