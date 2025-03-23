use emmylua_parser::{LuaAstNode, LuaClosureExpr, LuaReturnStat};
use rowan::TextRange;

use crate::{
    humanize_type, DiagnosticCode, LuaSignatureId, LuaType, RenderLevel, SemanticModel,
    SignatureReturnStatus, TypeCheckFailReason, TypeCheckResult,
};

use super::{get_own_return_stats, DiagnosticContext};

pub const CODES: &[DiagnosticCode] = &[DiagnosticCode::ReturnTypeMismatch];

pub fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) -> Option<()> {
    let root = semantic_model.get_root().clone();
    for closure_expr in root.descendants::<LuaClosureExpr>() {
        check_closure_expr(context, semantic_model, &closure_expr);
    }
    Some(())
}

fn check_closure_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    closure_expr: &LuaClosureExpr,
) {
    let signature_id = LuaSignatureId::from_closure(semantic_model.get_file_id(), closure_expr);
    let Some(signature) = context.db.get_signature_index().get(&signature_id) else {
        return;
    };
    if signature.resolve_return != SignatureReturnStatus::DocResolve {
        return;
    }
    let return_types = signature.get_return_types();
    for return_stat in get_own_return_stats(closure_expr) {
        check_return_stat(context, semantic_model, &return_types, &return_stat);
    }
}

fn check_return_stat(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    return_types: &[LuaType],
    return_stat: &LuaReturnStat,
) {
    let (return_expr_types, return_expr_ranges) = {
        let Some(infos) = semantic_model.infer_multi_value_adjusted_expression_types(
            &return_stat.get_expr_list().collect::<Vec<_>>(),
            None,
        ) else {
            return;
        };
        let return_expr_types = infos.iter().map(|(typ, _)| typ.clone()).collect::<Vec<_>>();
        let return_expr_ranges = infos.iter().map(|(_, range)| *range).collect::<Vec<_>>();
        (return_expr_types, return_expr_ranges)
    };

    for (index, return_type) in return_types.iter().enumerate() {
        if let LuaType::Variadic(variadic) = return_type {
            if return_expr_types.len() < index {
                break;
            }

            check_variadic_return_type_match(
                context,
                semantic_model,
                index,
                variadic,
                &return_expr_types[index..],
                &return_expr_ranges[index..],
            );
            break;
        };

        let return_expr_type = return_expr_types.get(index).unwrap_or(&LuaType::Any);
        let result = semantic_model.type_check(return_type, return_expr_type);
        if result.is_err() {
            add_type_check_diagnostic(
                context,
                semantic_model,
                index,
                *return_expr_ranges
                    .get(index)
                    .unwrap_or(&return_stat.get_range()),
                return_type,
                return_expr_type,
                result,
            );
        }
    }
}

fn check_variadic_return_type_match(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    start_idx: usize,
    variadic_type: &LuaType,
    return_expr_types: &[LuaType],
    return_expr_ranges: &[TextRange],
) {
    let mut idx = start_idx;
    for (return_expr_type, return_expr_range) in
        return_expr_types.iter().zip(return_expr_ranges.iter())
    {
        let result = semantic_model.type_check(variadic_type, return_expr_type);
        if result.is_err() {
            add_type_check_diagnostic(
                context,
                semantic_model,
                start_idx + idx,
                *return_expr_range,
                variadic_type,
                return_expr_type,
                result,
            );
        }
        idx += 1;
    }
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
        Ok(_) => (),
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
                        source = humanize_type(db, param_type, RenderLevel::Simple),
                        found = humanize_type(db, expr_type, RenderLevel::Simple)
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
