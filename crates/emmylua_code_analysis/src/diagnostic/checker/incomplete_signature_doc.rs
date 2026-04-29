use std::collections::HashSet;

use emmylua_parser::{
    LuaAstNode, LuaClosureExpr, LuaDocTagParam, LuaDocTagReturn, LuaDocTagReturnOverload, LuaStat,
};

use crate::{
    DiagnosticCode, LuaSemanticDeclId, LuaSignature, LuaSignatureId, LuaType, SemanticDeclLevel,
    SemanticModel, SignatureReturnStatus,
};

use super::{Checker, DiagnosticContext, get_closure_expr_comment, get_return_stats};

pub struct IncompleteSignatureDocChecker;

impl Checker for IncompleteSignatureDocChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::IncompleteSignatureDoc,
        DiagnosticCode::MissingGlobalDoc,
    ];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root();
        for closure_expr in root.descendants::<LuaClosureExpr>() {
            check_doc(context, semantic_model, &closure_expr);
        }
    }
}

fn check_doc(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    closure_expr: &LuaClosureExpr,
) -> Option<()> {
    let semantic_decl = semantic_model.find_decl(
        rowan::NodeOrToken::Node(closure_expr.syntax().clone()),
        SemanticDeclLevel::default(),
    )?;
    let (is_global, function_name) = match semantic_decl {
        LuaSemanticDeclId::LuaDecl(decl_id) => {
            let decl = semantic_model
                .get_db()
                .get_decl_index()
                .get_decl(&decl_id)?;
            (decl.is_global(), decl.get_name().to_string())
        }
        _ => (false, String::new()),
    };
    let signature_id = LuaSignatureId::from_closure(semantic_model.get_file_id(), closure_expr);
    let signature = semantic_model
        .get_db()
        .get_signature_index()
        .get(&signature_id)?;

    let comment = get_closure_expr_comment(closure_expr);

    let code = if is_global {
        DiagnosticCode::MissingGlobalDoc
    } else {
        DiagnosticCode::IncompleteSignatureDoc
    };

    if comment.is_none() {
        if !is_global && should_skip_incomplete_signature_doc(closure_expr, signature) {
            return Some(());
        }

        let message = if is_global {
            t!(
                "Missing comment for global function `%{name}`.",
                name = function_name
            )
        } else {
            t!(
                "Missing comment for function `%{name}`.",
                name = function_name
            )
        };
        if let Some(stat) = closure_expr.get_parent::<LuaStat>() {
            context.add_diagnostic(code, stat.get_range(), message.to_string(), None);
        }
        return Some(());
    }

    let Some(comment) = comment else {
        return Some(());
    };

    let doc_param_names: HashSet<String> = comment
        .children::<LuaDocTagParam>()
        .filter_map(|param| {
            param
                .get_name_token()
                .map(|token| token.get_name_text().to_string())
        })
        .collect();

    let doc_return_len = get_doc_return_max_len(signature).unwrap_or_else(|| {
        let doc_return_len: usize = comment
            .children::<LuaDocTagReturn>()
            .map(|return_doc| return_doc.get_types().count())
            .sum();
        let doc_return_overload_max_len = comment
            .children::<LuaDocTagReturnOverload>()
            .map(|return_doc| return_doc.get_types().count())
            .max()
            .unwrap_or(0);

        Some(doc_return_len.max(doc_return_overload_max_len))
    });

    check_params(
        context,
        closure_expr,
        &doc_param_names,
        code,
        is_global,
        &function_name,
    );

    check_returns(
        context,
        semantic_model,
        closure_expr,
        doc_return_len,
        code,
        is_global,
        &function_name,
    );

    Some(())
}

fn should_skip_incomplete_signature_doc(
    closure_expr: &LuaClosureExpr,
    signature: &LuaSignature,
) -> bool {
    let has_params = closure_expr
        .get_params_list()
        .map(|params_list| params_list.get_params().next().is_some())
        .unwrap_or(true);
    let skip_param = if has_params {
        !signature.param_docs.is_empty()
            && signature
                .param_docs
                .values()
                .all(|param_info| !matches!(param_info.type_ref, LuaType::Unknown))
    } else {
        true
    };

    skip_param && signature.is_resolve_return()
}

fn check_params(
    context: &mut DiagnosticContext,
    closure_expr: &LuaClosureExpr,
    doc_param_names: &HashSet<String>,
    code: DiagnosticCode,
    is_global: bool,
    function_name: &str,
) {
    let Some(params_list) = closure_expr.get_params_list() else {
        return;
    };

    for param in params_list.get_params() {
        let Some(name_token) = param.get_name_token() else {
            continue;
        };

        let name = name_token.get_name_text();
        if !doc_param_names.contains(name) && name != "_" {
            let message = if is_global {
                t!(
                    "Missing @param annotation for parameter `%{name}` in global function `%{function_name}`.",
                    name = name,
                    function_name = function_name
                )
            } else {
                t!(
                    "Incomplete signature. Missing @param annotation for parameter `%{name}`.",
                    name = name
                )
            };

            context.add_diagnostic(code, param.get_range(), message.to_string(), None);
        }
    }
}

fn check_returns(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    closure_expr: &LuaClosureExpr,
    doc_return_len: Option<usize>,
    code: DiagnosticCode,
    is_global: bool,
    function_name: &str,
) -> Option<()> {
    for return_stat in get_return_stats(closure_expr) {
        let mut return_stat_len: usize = 0;

        for (i, expr) in return_stat.get_expr_list().enumerate() {
            let Some(infer_type) = semantic_model.infer_expr(expr.clone()).ok() else {
                continue;
            };

            let expr_return_count = match infer_type {
                LuaType::Variadic(variadic) => variadic.get_min_len()?,
                _ => 1,
            };

            return_stat_len += expr_return_count;

            if let Some(doc_return_len) = doc_return_len
                && return_stat_len > doc_return_len
            {
                let message = if is_global {
                    t!(
                        "Missing @return annotation at index `%{index}` in global function `%{function_name}`.",
                        index = i + 1,
                        function_name = function_name
                    )
                } else {
                    t!(
                        "Incomplete signature. Missing @return annotation at index `%{index}`.",
                        index = i + 1
                    )
                };

                context.add_diagnostic(code, expr.get_range(), message.to_string(), None);
            }
        }
    }

    Some(())
}

fn get_doc_return_max_len(signature: &LuaSignature) -> Option<Option<usize>> {
    if signature.resolve_return != SignatureReturnStatus::DocResolve {
        return None;
    }
    let return_type = signature.get_return_type();

    Some(match return_type {
        LuaType::Variadic(variadic) => variadic.get_max_len(),
        LuaType::Any | LuaType::Unknown => Some(1),
        LuaType::Nil => Some(0),
        _ => Some(1),
    })
}
