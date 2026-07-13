//! Check attribute usage — pure salsa.

use crate::semantic_model::{SemanticModel, TypeCheckFailReason, TypeCheckResult};
use crate::{DiagnosticCode, LuaType, diagnostic::checker::humanize_lint_type_salsa};
use emmylua_parser::{
    LuaAstNode, LuaDocAttributeUse, LuaDocTagAttributeUse, LuaExpr, LuaLiteralExpr,
};
use rowan::TextRange;

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for tag_use in root.descendants::<LuaDocTagAttributeUse>() {
        for attribute_use in tag_use.get_attribute_uses() {
            check_attribute_use(context, model, &attribute_use);
        }
    }
}

fn check_attribute_use(
    _context: &mut DiagnosticContext,
    _model: &SemanticModel,
    attribute_use: &LuaDocAttributeUse,
) -> Option<()> {
    // Get attribute type name from doc type
    let name_type = attribute_use.get_type()?;
    let _name_text = name_type.get_name_text()?;

    // // Resolve and check if it's an attribute type
    // let type_def = model.get_type_def(&name_text)?;
    // if !matches!(
    //     type_def.kind,
    //     SalsaDocTypeDefKindSummary::Attribute
    // ) {
    //     return None;
    // }

    // // Get attribute params from salsa
    // let def_params = model.get_attribute_params(&type_def)?;

    // let args = match attribute_use.get_arg_list() {
    //     Some(arg_list) => arg_list.get_args().collect::<Vec<_>>(),
    //     None => vec![],
    // };
    // check_param_count(context, &def_params, attribute_use, &args);
    // check_param(context, model, &def_params, &args, &[]);

    Some(())
}

fn infer_attribute_arg_types(
    semantic_model: &SemanticModel,
    args: &[LuaLiteralExpr],
) -> Vec<LuaType> {
    args.iter()
        .map(|arg| {
            semantic_model
                .infer_expr(LuaExpr::LiteralExpr(arg.clone()))
                .unwrap_or(LuaType::Unknown)
        })
        .collect()
}

/// 检查参数数量是否匹配
fn check_param_count(
    context: &mut DiagnosticContext,
    def_params: &[(String, Option<LuaType>)],
    attribute_use: &LuaDocAttributeUse,
    args: &[LuaLiteralExpr],
) -> Option<()> {
    let call_args_count = args.len();
    if call_args_count < def_params.len() {
        for def_param in def_params[call_args_count..].iter() {
            if def_param.0 == "..." {
                break;
            }
            if def_param.1.as_ref().is_some_and(LuaType::is_optional) {
                continue;
            }
            context.add_diagnostic(
                DiagnosticCode::AttributeMissingParameter,
                match args.last() {
                    Some(arg) => arg.get_range(),
                    None => attribute_use.get_range(),
                },
                t!(
                    "expected %{num} parameters but found %{found_num}",
                    num = def_params.len(),
                    found_num = call_args_count
                )
                .to_string(),
                None,
            );
        }
    } else if call_args_count > def_params.len() {
        if def_params.last().is_some_and(|(name, typ)| {
            name == "..." || typ.as_ref().is_some_and(|typ| typ.is_variadic())
        }) {
            return Some(());
        }
        for arg in args[def_params.len()..].iter() {
            context.add_diagnostic(
                DiagnosticCode::AttributeRedundantParameter,
                arg.get_range(),
                t!(
                    "expected %{num} parameters but found %{found_num}",
                    num = def_params.len(),
                    found_num = call_args_count
                )
                .to_string(),
                None,
            );
        }
    }

    Some(())
}

/// 检查参数类型是否匹配
fn check_param(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    def_params: &[(String, Option<LuaType>)],
    args: &[LuaLiteralExpr],
    _call_arg_types: &[LuaType],
) -> Option<()> {
    let mut call_arg_types = Vec::new();
    for arg in args {
        let arg_type = match model.infer_expr(LuaExpr::LiteralExpr(arg.clone())) {
            Ok(ty) => ty,
            Err(_) => return None,
        };
        call_arg_types.push(arg_type);
    }

    for (idx, param) in def_params.iter().enumerate() {
        if param.0 == "..." {
            if call_arg_types.len() < idx {
                break;
            }
            if let Some(variadic_type) = param.1.clone() {
                for arg_type in call_arg_types[idx..].iter() {
                    let result = model.type_check_detail(&variadic_type, arg_type);
                    if result.is_err() {
                        add_type_check_diagnostic(
                            context,
                            args.get(idx)?.get_range(),
                            &variadic_type,
                            arg_type,
                            &result,
                        );
                    }
                }
            }
            break;
        }
        if let Some(param_type) = param.1.as_ref() {
            let arg_type = call_arg_types.get(idx).unwrap_or(&LuaType::Any);
            let result = model.type_check_detail(&param_type, arg_type);
            if result.is_err() {
                add_type_check_diagnostic(
                    context,
                    args.get(idx)?.get_range(),
                    param_type,
                    arg_type,
                    &result,
                );
            }
        }
    }
    Some(())
}

fn add_type_check_diagnostic(
    context: &mut DiagnosticContext,
    range: TextRange,
    param_type: &LuaType,
    expr_type: &LuaType,
    result: &TypeCheckResult,
) {
    match result {
        Ok(_) => (),
        Err(reason) => {
            let reason_message = match reason {
                TypeCheckFailReason::TypeNotMatchWithReason(reason) => reason.clone(),
                TypeCheckFailReason::TypeNotMatch | TypeCheckFailReason::DoNotCheck => {
                    "".to_string()
                }
                TypeCheckFailReason::TypeRecursion => "type recursion".to_string(),
            };
            context.add_diagnostic(
                DiagnosticCode::AttributeParamTypeMismatch,
                range,
                t!(
                    "expected `%{source}` but found `%{found}`. %{reason}",
                    source = humanize_lint_type_salsa(
                        context.get_salsa_db(),
                        context.get_file_id(),
                        param_type
                    ),
                    found = humanize_lint_type_salsa(
                        context.get_salsa_db(),
                        context.get_file_id(),
                        expr_type
                    ),
                    reason = reason_message
                )
                .to_string(),
                None,
            );
        }
    }
}
