//! Param type check — pure salsa.

use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaCallExpr, LuaExpr};
use rowan::TextRange;

use crate::semantic_model::{CallFunctionInfo, SemanticModel};
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
    let Some(func_info) = model.infer_call_expr_func(call.clone(), None) else { return };
    let mut params = func_info.params.clone();
    let Some(args_list) = call.get_args_list() else { return };
    let arg_exprs: Vec<_> = args_list.get_args().collect();
    let Ok(value_types) = model.infer_expr_list_types(&arg_exprs, None) else { return };
    let mut arg_types: Vec<LuaType> = value_types.into_iter().map(|(t, _)| t).collect();

    let colon_call = call.is_colon_call();
    let colon_def = func_info.is_colon_define;
    match (colon_call, colon_def) {
        (true, true) | (false, false) => {}
        (false, true) => { params.insert(0, ("self".into(), Some(LuaType::SelfInfer))); }
        (true, false) => {
            let Some(prefix) = call.get_prefix_expr() else { return };
            let Ok(src_type) = model.infer_expr(prefix.clone()) else { return };
            arg_types.insert(0, src_type);
        }
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
        let Some(arg_type) = arg_types.get(idx) else { continue };
        let db = context.db;
        if crate::check_type_compact(db, arg_type, param_type).is_err() {
            let Some(arg_pos) = call.get_args_list().and_then(|a| a.get_args().nth(idx)) else { continue };
            context.add_diagnostic(DiagnosticCode::ParamTypeMismatch, arg_pos.get_range(),
                t!("expected `%{expected}` but found `%{found}`",
                    expected = super::humanize_lint_type(db, param_type),
                    found = super::humanize_lint_type(db, arg_type)).to_string(), None);
        }
    }
}

fn check_variadic(context: &mut DiagnosticContext, model: &SemanticModel, var_type: &LuaType, arg_types: &[LuaType]) {
    let db = context.db;
    for arg in arg_types {
        if crate::check_type_compact(db, arg, var_type).is_err() {
            return; // Don't report individual arg errors for variadic
        }
    }
}
