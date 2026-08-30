//! # incomplete_signature_doc - functions missing doc annotations / parameter annotations / return annotations
//!
//! Uses only salsa facts: `Signature.docs` is `None` = no comment (global functions report `MissingGlobalDoc`,
//! others report `IncompleteSignatureDoc`); additionally report missing `@param` and return values exceeding the documented count.

use emmylua_parser::{LuaAstNode, LuaClosureExpr, LuaReturnStat, LuaStat, LuaSyntaxKind};

use crate::DiagnosticCode;
use crate::salsa_builder::def::{DeclKind, Signature};
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct IncompleteSignatureDocChecker;

impl Checker for IncompleteSignatureDocChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::IncompleteSignatureDoc,
        DiagnosticCode::MissingGlobalDoc,
    ];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(signatures) = semantic_model.signatures() else {
            return;
        };
        let signatures = signatures.to_vec();
        for signature in &signatures {
            check_signature(context, semantic_model, signature);
        }
    }
}

fn check_signature(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    signature: &Signature,
) {
    let is_global = semantic_model
        .file_facts()
        .and_then(|facts| {
            signature
                .name
                .as_ref()
                .and_then(|name| facts.decl_named(name))
                .map(|decl| decl.kind == DeclKind::Global)
        })
        .unwrap_or(false);
    let code = if is_global {
        DiagnosticCode::MissingGlobalDoc
    } else {
        DiagnosticCode::IncompleteSignatureDoc
    };

    let Some(tree) = semantic_model.syntax_tree() else {
        return;
    };
    let root = tree.get_red_root();
    let Some(node) = signature.closure_syntax.to_node_from_root(&root) else {
        return;
    };
    let Some(closure) = LuaClosureExpr::cast(node) else {
        return;
    };

    let Some(docs) = &signature.docs else {
        // No comment: always report for global functions; for local functions only when there are parameters/return values.
        let has_signature_content = closure.get_params_list().is_some_and(|params| {
            params
                .get_params()
                .any(|param| param.get_name_token().is_some())
        }) || closure
            .descendants::<LuaReturnStat>()
            .any(|stat| stat.get_expr_list().count() > 0);
        // A leading pure `---` comment on a global function counts as documentation (no report when there are no parameters/return values).
        if is_global && !has_signature_content && has_comment_before(&closure) {
            return;
        }
        if is_global || has_signature_content {
            context.add_diagnostic(
                code,
                closure.get_range(),
                t!(
                    "Missing comment for function `%{name}`.",
                    name = signature.name.as_deref().unwrap_or("")
                ),
            );
        }
        return;
    };

    // Missing @param for parameters.
    let doc_param_names: Vec<&str> = docs
        .param_types
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    if let Some(params_list) = closure.get_params_list() {
        for param in params_list.get_params() {
            let Some(name_token) = param.get_name_token() else {
                continue;
            };
            let name = name_token.get_name_text();
            if name == "_" || doc_param_names.contains(&name) {
                continue;
            }
            context.add_diagnostic(
                code,
                param.get_range(),
                t!(
                    "Incomplete signature. Missing @param annotation for parameter `%{name}`.",
                    name = name
                ),
            );
        }
    }

    // The actual number of return values exceeds the documented count. return_overload is computed per line (taking the widest line);
    // If any line is variadic (`integer...`), there is no limit.
    let mut doc_return_len = docs.returns.len();
    for row_len in &docs.return_overload_rows {
        doc_return_len = doc_return_len.max(*row_len);
    }
    let variadic_overload = docs.return_overloads.iter().any(|(_, syntax)| {
        matches!(
            semantic_model.doc_type_lua(*syntax),
            crate::LuaType::Variadic(_)
        ) || syntax
            .to_node_from_root(&root)
            .is_some_and(|node| node.text().to_string().ends_with("..."))
    });
    if variadic_overload {
        return;
    }
    for return_stat in closure.descendants::<LuaReturnStat>() {
        if return_stat.get_expr_list().count() > doc_return_len {
            for (index, expr) in return_stat.get_expr_list().enumerate() {
                if index >= doc_return_len {
                    context.add_diagnostic(
                        code,
                        expr.get_range(),
                        t!(
                            "Incomplete signature. Missing @return annotation at index `%{index}`.",
                            index = index + 1
                        ),
                    );
                }
            }
        }
    }
}

/// Whether there is a comment directly before the function statement (a pure `---` also counts).
fn has_comment_before(closure: &LuaClosureExpr) -> bool {
    let Some(stat) = closure.ancestors::<LuaStat>().next() else {
        return false;
    };
    let Some(prev) = stat.syntax().prev_sibling() else {
        return false;
    };
    matches!(prev.kind().into(), LuaSyntaxKind::Comment)
}
