use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaCallExpr, LuaGeneralToken, LuaLiteralExpr, LuaLiteralToken,
};

use crate::{DiagnosticCode, LuaType, SemanticModel};

use super::DiagnosticContext;

pub const CODES: &[DiagnosticCode] = &[DiagnosticCode::MissingParameter];

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
    let mut args_count = call_expr.get_args_list()?.get_args().count();
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

    if args_count < params.len() {
        // fix last arg is `...`
        if args_count != 0 {
            if let Some(last_arg) = call_expr.get_args_list()?.child::<LuaLiteralExpr>() {
                if let Some(literal_token) = last_arg.get_literal() {
                    if let LuaLiteralToken::Dots(_) = literal_token {
                        return Some(());
                    }
                }
            }
        }
        let mut miss_parameter_info = Vec::new();

        // 参数调用中最后一个参数是多返回值
        if let Some(last_arg) = call_expr.get_args_list()?.get_args().last() {
            if let Some(LuaType::MuliReturn(types)) = semantic_model.infer_expr(last_arg.clone()) {
                let len = types.get_len().unwrap_or(0);
                args_count = args_count + len as usize - 1;
                if args_count >= params.len() {
                    return Some(());
                }
            }
        }

        for i in args_count..params.len() {
            let param_info = params.get(i)?;
            if param_info.0 == "..." {
                break;
            }

            let typ = param_info.1.clone();
            if let Some(typ) = typ {
                if !typ.is_any() && !typ.is_unknown() && !typ.is_optional() {
                    miss_parameter_info
                        .push(t!("missing parameter: %{name}", name = param_info.0,));
                }
            }
        }

        let right_paren = call_expr
            .get_args_list()?
            .tokens::<LuaGeneralToken>()
            .last()?;
        if !miss_parameter_info.is_empty() {
            context.add_diagnostic(
                DiagnosticCode::MissingParameter,
                right_paren.get_range(),
                t!(
                    "expected %{num} parameters but found %{found_num}. %{infos}",
                    num = params.len(),
                    found_num = args_count,
                    infos = miss_parameter_info.join(" \n ")
                )
                .to_string(),
                None,
            );
        }
    }

    Some(())
}
