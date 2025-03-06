use std::ops::Deref;

use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaCallExpr, LuaExpr};
use rowan::TextRange;

use crate::{
    humanize_type, DiagnosticCode, LuaMultiReturn, LuaType, RenderLevel, SemanticModel,
    TypeCheckFailReason, TypeCheckResult,
};

use super::DiagnosticContext;

pub const CODES: &[DiagnosticCode] = &[DiagnosticCode::ParamTypeNotMatch];

/// a simple implementation of param type check, later we will do better
pub fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) -> Option<()> {
    let root = semantic_model.get_root().clone();
    for node in root.descendants::<LuaAst>() {
        match node {
            LuaAst::LuaCallExpr(call_expr) => {
                check_call_expr(context, semantic_model, call_expr);
            }
            _ => {}
        }
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
    let mut args = call_expr
        .get_args_list()?
        .get_args()
        .map(|arg| Some(arg))
        .collect::<Vec<_>>();
    let colon_call = call_expr.is_colon_call();
    let colon_define = func.is_colon_define();
    match (colon_call, colon_define) {
        (true, true) | (false, false) => {}
        (false, true) => {
            if args.len() > 0 {
                args.remove(0);
            }
        }
        (true, false) => {
            args.insert(0, None);

            if let Some((_, Some(self_type))) = params.first() {
                let result =
                    check_first_param_colon_call(semantic_model, call_expr.clone(), self_type);
                if !result.is_ok() {
                    add_type_check_diagnostic(
                        context,
                        semantic_model,
                        call_expr.get_colon_token()?.get_range(),
                        self_type,
                        &LuaType::SelfInfer,
                        result,
                    );
                }
            }
        }
    }

    for (idx, param) in params.iter().enumerate() {
        let arg = match args.get(idx) {
            Some(arg) => match arg {
                Some(arg) => arg,
                None => continue,
            },
            None => break,
        };

        if param.0 == "..." {
            if let Some(variadic_type) = param.1.clone() {
                check_variadic_param_match_args(
                    context,
                    semantic_model,
                    &variadic_type,
                    &args[idx..],
                );
            }

            break;
        }

        if let Some(param_type) = param.1.clone() {
            let expr_type = semantic_model
                .infer_expr(arg.clone())
                .unwrap_or(LuaType::Any);

            match &expr_type {
                LuaType::MuliReturn(multi) => {
                    for (idx, param_type) in params[idx..].iter().map(|p| p.1.clone()).enumerate() {
                        match (param_type, multi.get_type(idx)) {
                            (Some(param_type), Some(expr_type)) => {
                                let result = semantic_model.type_check(&param_type, &expr_type);
                                if !result.is_ok() {
                                    add_type_check_diagnostic(
                                        context,
                                        semantic_model,
                                        arg.get_range(),
                                        &param_type,
                                        &expr_type,
                                        result,
                                    );
                                }
                            }
                            (None, _) => continue,
                            _ => break,
                        }
                    }

                    break;
                }
                _ => {
                    let result = semantic_model.type_check(&param_type, &expr_type);
                    if !result.is_ok() {
                        add_type_check_diagnostic(
                            context,
                            semantic_model,
                            arg.get_range(),
                            &param_type,
                            &expr_type,
                            result,
                        );
                    }
                }
            }
        }
    }

    Some(())
}

fn check_variadic_param_match_args(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    variadic_type: &LuaType,
    args: &[Option<LuaExpr>],
) {
    for arg in args {
        if let Some(arg) = arg {
            let expr_type = semantic_model
                .infer_expr(arg.clone())
                .unwrap_or(LuaType::Any);

            match &expr_type {
                LuaType::MuliReturn(multi_return) => match multi_return.deref() {
                    LuaMultiReturn::Base(base) => {
                        let result = semantic_model.type_check(&variadic_type, base);
                        if !result.is_ok() {
                            add_type_check_diagnostic(
                                context,
                                semantic_model,
                                arg.get_range(),
                                &variadic_type,
                                base,
                                result,
                            );
                        }
                    }
                    LuaMultiReturn::Multi(types) => {
                        for expr_type in types {
                            let result = semantic_model.type_check(&variadic_type, expr_type);
                            if !result.is_ok() {
                                add_type_check_diagnostic(
                                    context,
                                    semantic_model,
                                    arg.get_range(),
                                    &variadic_type,
                                    expr_type,
                                    result,
                                );
                            }
                        }
                    }
                },
                _ => {
                    let result = semantic_model.type_check(&variadic_type, &expr_type);
                    if !result.is_ok() {
                        add_type_check_diagnostic(
                            context,
                            semantic_model,
                            arg.get_range(),
                            &variadic_type,
                            &expr_type,
                            result,
                        );
                    }
                }
            }
        }
    }
}

fn add_type_check_diagnostic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    param_type: &LuaType,
    expr_type: &LuaType,
    result: TypeCheckResult,
) {
    let db = semantic_model.get_db();
    match result {
        Ok(_) => return,
        Err(reason) => match reason {
            TypeCheckFailReason::TypeNotMatchWithReason(reason) => {
                context.add_diagnostic(DiagnosticCode::ParamTypeNotMatch, range, reason, None);
            }
            TypeCheckFailReason::TypeNotMatch => {
                context.add_diagnostic(
                    DiagnosticCode::ParamTypeNotMatch,
                    range,
                    t!(
                        "expected `%{source}` but found `%{found}`",
                        source = humanize_type(db, &param_type, RenderLevel::Simple),
                        found = humanize_type(db, &expr_type, RenderLevel::Simple)
                    )
                    .to_string(),
                    None,
                );
            }
            TypeCheckFailReason::TypeRecursion => {
                context.add_diagnostic(
                    DiagnosticCode::ParamTypeNotMatch,
                    range,
                    "type recursion".into(),
                    None,
                );
            }
        },
    }
}

/// Check if colon call is possible. This check can only be performed
/// when it's a colon call but not a colon definition.
fn check_first_param_colon_call(
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
    self_type: &LuaType,
) -> TypeCheckResult {
    if !matches!(self_type, LuaType::SelfInfer | LuaType::Any) {
        if let Some(LuaExpr::IndexExpr(index_expr)) = call_expr.get_prefix_expr() {
            // We need to narrow `SelfInfer` to the actual type
            return if let Some(prefix_expr) = index_expr.get_prefix_expr() {
                let expr_type = semantic_model
                    .infer_expr(prefix_expr.clone())
                    .unwrap_or(LuaType::SelfInfer);
                semantic_model.type_check(self_type, &expr_type)
            } else {
                Err(TypeCheckFailReason::TypeNotMatch)
            };
        }
    }
    Ok(())
}
