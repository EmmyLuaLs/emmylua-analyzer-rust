//! Check param count — pure salsa.

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaCallExpr, LuaClosureExpr, LuaExpr, LuaGeneralToken,
    LuaLiteralToken,
};

use crate::semantic_model::SemanticModel;
use crate::{DiagnosticCode, LuaSignatureId, LuaType};

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    for node in model.get_root().descendants::<LuaAst>() {
        match node {
            LuaAst::LuaCallExpr(call) => check_call(context, model, call),
            LuaAst::LuaClosureExpr(closure) => check_closure(context, model, &closure),
            _ => {}
        }
    }
}

fn check_closure(context: &mut DiagnosticContext, model: &SemanticModel, closure: &LuaClosureExpr) {
    let file_id = model.get_file_id();
    let sig_id = LuaSignatureId::from_closure(file_id, closure);
    let Some(current_sig) = model.get_signature(file_id, sig_id.get_position()) else {
        return;
    };
    let Some(source) = model.infer_bind_value_type(closure.clone().into()) else {
        return;
    };

    let source_params_len = match &source {
        LuaType::DocFunction(f) => param_len(f.get_params()),
        LuaType::Signature(s) => {
            let Some(sig) = model.get_signature(file_id, s.get_position()) else {
                return;
            };
            Some(sig.type_params().len())
        }
        _ => return,
    };
    let Some(source_len) = source_params_len else {
        return;
    };
    if source_len > current_sig.param_names().len() {
        return;
    }

    let Some(params) = closure.get_params_list() else {
        return;
    };
    let params: Vec<_> = params.get_params().collect();
    for p in &params[source_len..] {
        context.add_diagnostic(
            DiagnosticCode::RedundantParameter,
            p.get_range(),
            t!(
                "expected %{num} parameters but found %{found_num}",
                num = source_len,
                found_num = current_sig.param_names().len()
            )
            .to_string(),
            None,
        );
    }
}

fn check_call(context: &mut DiagnosticContext, model: &SemanticModel, call: LuaCallExpr) {
    let Some(func_info) = model.infer_call_expr_func(call.clone(), None) else {
        return;
    };
    let mut params = func_info.params.clone();
    let Some(args) = call.get_args_list() else {
        return;
    };
    let call_args: Vec<_> = args.get_args().collect();
    let mut call_count = call_args.len();
    let last_is_dots = call_args.last().is_some_and(is_dots);

    let colon_call = call.is_colon_call();
    let colon_def = func_info.is_colon_define;
    match (colon_call, colon_def) {
        (true, true) | (false, false) => {}
        (false, true) => {
            params.insert(0, ("self".into(), Some(LuaType::SelfInfer)));
        }
        (true, false) => {
            call_count += 1;
        }
    }

    if call_count < params.len() {
        if call_args.iter().any(is_dots) {
            return;
        }
        if let Some(last) = call_args.last() {
            if let Ok(LuaType::Variadic(v)) = model.infer_expr(last.clone()) {
                if let Some(len) = v.get_max_len() {
                    call_count = call_count + len - 1;
                    if call_count >= params.len() {
                        return;
                    }
                }
            }
        }
        let missing: Vec<String> = params[call_count..]
            .iter()
            .take_while(|(n, t)| {
                n.as_str() != "..." && !t.as_ref().is_some_and(|t| t.is_variadic())
            })
            .map(|(n, _)| t!("missing parameter: %{name}", name = n).to_string())
            .collect();
        if !missing.is_empty() {
            let Some(last_tk) = args.tokens::<LuaGeneralToken>().last() else {
                return;
            };
            context.add_diagnostic(
                DiagnosticCode::MissingParameter,
                last_tk.get_range(),
                t!(
                    "expected %{num} parameters but found %{found_num}. %{infos}",
                    num = params.len(),
                    found_num = call_count,
                    infos = missing.join(" \n ")
                )
                .to_string(),
                None,
            );
        }
    } else {
        if func_info.is_variadic {
            return;
        }
        let mut min_count = call_count;
        if last_is_dots {
            min_count = min_count.saturating_sub(1);
        }
        if min_count <= params.len() {
            return;
        }
        if params
            .last()
            .is_some_and(|(n, t)| n == "..." || t.as_ref().is_some_and(|t| t.is_variadic()))
        {
            return;
        }
        let adj: isize = if colon_def && !colon_call {
            -1
        } else if !colon_def && colon_call {
            1
        } else {
            0
        };
        for (i, arg) in call_args.iter().enumerate() {
            if last_is_dots && i + 1 == call_args.len() {
                continue;
            }
            let pi = i as isize + adj;
            if pi < 0 || pi < params.len() as isize {
                continue;
            }
            context.add_diagnostic(
                DiagnosticCode::RedundantParameter,
                arg.get_range(),
                t!(
                    "expected %{num} parameters but found %{found_num}",
                    num = params.len(),
                    found_num = min_count
                )
                .to_string(),
                None,
            );
        }
    }
}

fn param_len(params: &[(String, Option<LuaType>)]) -> Option<usize> {
    if params
        .last()
        .is_some_and(|(n, t)| n == "..." || t.as_ref().is_some_and(|t| t.is_variadic()))
    {
        None
    } else {
        Some(params.len())
    }
}

fn is_dots(expr: &LuaExpr) -> bool {
    matches!(expr, LuaExpr::LiteralExpr(l) if matches!(l.get_literal(), Some(LuaLiteralToken::Dots(_))))
}
