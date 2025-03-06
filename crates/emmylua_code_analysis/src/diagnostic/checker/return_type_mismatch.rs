use std::ops::Deref;

use emmylua_parser::{LuaAstNode, LuaBlock, LuaClosureExpr, LuaExpr, LuaReturnStat};
use rowan::TextRange;

use crate::{
    humanize_type, DiagnosticCode, LuaMultiReturn, LuaSignatureId, LuaType, RenderLevel,
    SemanticModel, SignatureReturnStatus, TypeCheckFailReason, TypeCheckResult,
};

use super::DiagnosticContext;

pub const CODES: &[DiagnosticCode] = &[DiagnosticCode::ReturnTypeMismatch];

pub fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) -> Option<()> {
    let root = semantic_model.get_root().clone();
    for return_stat in root.descendants::<LuaReturnStat>() {
        check_return_stat(context, semantic_model, &return_stat);
    }
    Some(())
}

fn check_return_stat(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    return_stat: &LuaReturnStat,
) -> Option<()> {
    let closure_expr = return_stat
        .get_parent::<LuaBlock>()?
        .ancestors::<LuaClosureExpr>()
        .next()?;

    let signature_id = LuaSignatureId::from_closure(semantic_model.get_file_id(), &closure_expr);
    let signature = context.db.get_signature_index().get(&signature_id)?;
    let return_types = signature.get_return_types();
    // 如果没有返回值注解, 则不检查
    if signature.resolve_return != SignatureReturnStatus::DocResolve {
        return Some(());
    }

    for (index, expr) in return_stat.get_expr_list().enumerate() {
        let return_type = return_types.get(index).unwrap_or(&LuaType::Any);
        if let LuaType::Variadic(variadic) = return_type {
            check_variadic_return_type_match(
                context,
                semantic_model,
                index,
                variadic,
                &return_stat.get_expr_list().skip(index).collect::<Vec<_>>(),
            );
            break;
        };

        let expr_type = semantic_model
            .infer_expr(expr.clone())
            .unwrap_or(LuaType::Any);

        let result = semantic_model.type_check(&return_type, &expr_type);
        if !result.is_ok() {
            add_type_check_diagnostic(
                context,
                semantic_model,
                index,
                expr.get_range(),
                &return_type,
                &expr_type,
                result,
            );
        }
    }
    Some(())
}

fn check_variadic_return_type_match(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    start_idx: usize,
    variadic_type: &LuaType,
    return_exprs: &[LuaExpr],
) -> Option<()> {
    for (idx, expr) in return_exprs.iter().enumerate() {
        let expr_type = semantic_model
            .infer_expr(expr.clone())
            .unwrap_or(LuaType::Any);
        match &expr_type {
            LuaType::MuliReturn(multi_return) => match multi_return.deref() {
                LuaMultiReturn::Base(base) => {
                    let result = semantic_model.type_check(&variadic_type, base);
                    if !result.is_ok() {
                        add_type_check_diagnostic(
                            context,
                            semantic_model,
                            start_idx + idx,
                            expr.get_range(),
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
                                start_idx + idx,
                                expr.get_range(),
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
                        start_idx + idx,
                        expr.get_range(),
                        &variadic_type,
                        &expr_type,
                        result,
                    );
                }
            }
        }
    }
    Some(())
}

fn add_type_check_diagnostic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    index: usize,
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
                    DiagnosticCode::ReturnTypeMismatch,
                    range,
                    t!(
                        "Annotations specify that return value %{index} has a type of `%{source}`, returning value of type `%{found}` here instead.",
                        index = index + 1,
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
