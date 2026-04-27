use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaCallExpr, LuaDocTagType};
use rowan::TextRange;

use crate::diagnostic::{checker::Checker, lua_diagnostic::DiagnosticContext};
use crate::semantic::{
    CallConstraintContext, build_call_constraint_context, normalize_constraint_type,
};
use crate::{
    DiagnosticCode, DocTypeInferContext, LuaStringTplType, LuaType, RenderLevel, SemanticModel,
    TypeCheckFailReason, TypeCheckResult, TypeSubstitutor, VariadicType, humanize_type,
    infer_doc_type, instantiate_type_generic,
};

pub struct GenericConstraintMismatchChecker;

impl Checker for GenericConstraintMismatchChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::GenericConstraintMismatch];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for node in root.descendants::<LuaAst>() {
            match node {
                LuaAst::LuaCallExpr(call_expr) => {
                    check_call_expr(context, semantic_model, call_expr);
                }
                LuaAst::LuaDocTagType(doc_tag_type) => {
                    check_doc_tag_type(context, semantic_model, doc_tag_type);
                }
                _ => {}
            }
        }
    }
}

fn check_call_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
) -> Option<()> {
    let Some((
        CallConstraintContext {
            params,
            arg_infos,
            substitutor,
        },
        doc_func,
    )) = build_call_constraint_context(semantic_model, &call_expr)
    else {
        return Some(());
    };

    let mut arg_ranges = collect_arg_ranges(semantic_model, &call_expr);
    if call_expr.is_colon_call() && !doc_func.is_colon_define() {
        let colon_range = call_expr.get_colon_token()?.get_range();
        arg_ranges.insert(0, colon_range);
    }

    for (i, (_, param_type)) in params.iter().enumerate() {
        let param_type = if let Some(param_type) = param_type {
            param_type
        } else {
            continue;
        };

        check_param(
            context,
            semantic_model,
            &call_expr,
            i,
            param_type,
            &arg_infos,
            &arg_ranges,
            false,
            &substitutor,
        );
    }

    Some(())
}

fn collect_arg_ranges(semantic_model: &SemanticModel, call_expr: &LuaCallExpr) -> Vec<TextRange> {
    let Some(arg_list) = call_expr.get_args_list() else {
        return Vec::new();
    };
    let arg_exprs = arg_list.get_args().collect::<Vec<_>>();
    let mut ranges = Vec::new();
    for expr in arg_exprs {
        let expr_type = semantic_model
            .infer_expr(expr.clone())
            .unwrap_or(LuaType::Unknown);
        match expr_type {
            LuaType::Variadic(variadic) => match variadic.as_ref() {
                VariadicType::Base(_) => ranges.push(expr.get_range()),
                VariadicType::Multi(values) => {
                    for _ in values {
                        ranges.push(expr.get_range());
                    }
                }
            },
            _ => ranges.push(expr.get_range()),
        }
    }
    ranges
}

fn check_doc_tag_type(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    doc_tag_type: LuaDocTagType,
) -> Option<()> {
    let type_list = doc_tag_type.get_type_list();
    let doc_ctx = DocTypeInferContext::new(
        semantic_model.get_compilation().legacy_db(),
        semantic_model.get_file_id(),
    );
    for doc_type in type_list {
        let type_ref = infer_doc_type(doc_ctx, &doc_type);
        let generic_type = match type_ref {
            LuaType::Generic(generic_type) => generic_type,
            _ => continue,
        };

        let generic_params = semantic_model
            .get_compilation()
            .get_generic_params(&generic_type.get_base_type_id())?;
        for (i, param_type) in generic_type.get_params().iter().enumerate() {
            let extend_type = generic_params.get(i)?.type_constraint.clone()?;
            let result = semantic_model.type_check_detail(&extend_type, param_type);
            if result.is_err() {
                add_type_check_diagnostic(
                    context,
                    semantic_model,
                    doc_type.get_range(),
                    &extend_type,
                    param_type,
                    result,
                );
            }
        }
    }
    Some(())
}

#[allow(clippy::too_many_arguments)]
fn check_param(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
    param_index: usize,
    param_type: &LuaType,
    arg_infos: &[LuaType],
    arg_ranges: &[TextRange],
    from_union: bool,
    substitutor: &TypeSubstitutor,
) -> Option<()> {
    // 应该先通过泛型体操约束到唯一类型再进行检查
    match param_type {
        LuaType::StrTplRef(str_tpl_ref) => {
            let extend_type = str_tpl_ref.get_constraint().cloned().map(|ty| {
                normalize_constraint_type(instantiate_type_generic(
                    semantic_model.get_compilation().legacy_db(),
                    &ty,
                    substitutor,
                ))
            });
            let arg_expr = call_expr.get_args_list()?.get_args().nth(param_index)?;
            let arg_type = semantic_model.infer_expr(arg_expr.clone()).ok()?;

            if from_union && !arg_type.is_string() {
                return None;
            }

            validate_str_tpl_ref(
                context,
                semantic_model,
                str_tpl_ref,
                &arg_type,
                arg_expr.get_range(),
                extend_type,
            );
        }
        LuaType::TplRef(tpl_ref) | LuaType::ConstTplRef(tpl_ref) => {
            let extend_type = tpl_ref.get_constraint().cloned().map(|ty| {
                normalize_constraint_type(instantiate_type_generic(
                    semantic_model.get_compilation().legacy_db(),
                    &ty,
                    substitutor,
                ))
            });
            let arg_type = arg_infos.get(param_index);
            let arg_range = arg_ranges.get(param_index).copied();
            validate_tpl_ref(context, semantic_model, &extend_type, arg_type, arg_range);
        }
        LuaType::Union(union_type) => {
            // 如果不是来自 union, 才展开 union 中的每个类型进行检查
            if !from_union {
                for union_member_type in union_type.into_vec().iter() {
                    check_param(
                        context,
                        semantic_model,
                        call_expr,
                        param_index,
                        union_member_type,
                        arg_infos,
                        arg_ranges,
                        true,
                        substitutor,
                    );
                }
            }
        }
        _ => {}
    }
    Some(())
}

fn validate_str_tpl_ref(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    str_tpl_ref: &LuaStringTplType,
    arg_type: &LuaType,
    range: TextRange,
    extend_type: Option<LuaType>,
) -> Option<()> {
    match arg_type {
        LuaType::StringConst(str) | LuaType::DocStringConst(str) => {
            let full_type_name = format!(
                "{}{}{}",
                str_tpl_ref.get_prefix(),
                str,
                str_tpl_ref.get_suffix()
            );
            let founded_type_decl = semantic_model.find_type_decl(&full_type_name);
            if founded_type_decl.is_none() {
                context.add_diagnostic(
                    DiagnosticCode::GenericConstraintMismatch,
                    range,
                    t!("the string template type does not match any type declaration").to_string(),
                    None,
                );
            }

            if let Some(extend_type) = extend_type
                && let Some(type_decl) = founded_type_decl
            {
                let type_id = type_decl.get_id();
                let ref_type = LuaType::Ref(type_id);
                let result = semantic_model.type_check_detail(&extend_type, &ref_type);
                if result.is_err() {
                    add_type_check_diagnostic(
                        context,
                        semantic_model,
                        range,
                        &extend_type,
                        &ref_type,
                        result,
                    );
                }
            }
        }
        LuaType::String | LuaType::Any | LuaType::Unknown | LuaType::StrTplRef(_) => {}
        _ => {
            context.add_diagnostic(
                DiagnosticCode::GenericConstraintMismatch,
                range,
                t!("the string template type must be a string constant").to_string(),
                None,
            );
        }
    }
    Some(())
}

fn validate_tpl_ref(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    extend_type: &Option<LuaType>,
    arg_type: Option<&LuaType>,
    range: Option<TextRange>,
) -> Option<()> {
    let extend_type = extend_type.clone()?;
    let arg_type = arg_type?;
    let range = range?;
    let result = semantic_model.type_check_detail(&extend_type, arg_type);
    if result.is_err() {
        add_type_check_diagnostic(
            context,
            semantic_model,
            range,
            &extend_type,
            arg_type,
            result,
        );
    }
    Some(())
}

fn add_type_check_diagnostic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    extend_type: &LuaType,
    expr_type: &LuaType,
    result: TypeCheckResult,
) {
    let db = semantic_model.get_compilation().legacy_db();
    match result {
        Ok(_) => (),
        Err(reason) => {
            let reason_message = match reason {
                TypeCheckFailReason::TypeNotMatchWithReason(reason) => reason,
                TypeCheckFailReason::TypeNotMatch | TypeCheckFailReason::DonotCheck => {
                    "".to_string()
                }
                TypeCheckFailReason::TypeRecursion => "type recursion".to_string(),
            };
            context.add_diagnostic(
                DiagnosticCode::GenericConstraintMismatch,
                range,
                t!(
                    "type `%{found}` does not satisfy the constraint `%{source}`. %{reason}",
                    source = humanize_type(db, extend_type, RenderLevel::Simple),
                    found = humanize_type(db, expr_type, RenderLevel::Simple),
                    reason = reason_message
                )
                .to_string(),
                None,
            );
        }
    }
}
