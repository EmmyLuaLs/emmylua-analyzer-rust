//! Incomplete signature doc — pure salsa.

use emmylua_parser::{LuaAstNode, LuaAstToken, LuaClosureExpr, LuaDocTagParam, LuaDocTagReturn};

use crate::semantic_model::SemanticModel;
use crate::{DiagnosticCode, LuaSignatureId};

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for closure in root.descendants::<LuaClosureExpr>() {
        check_closure(context, model, &closure);
    }
}

fn check_closure(context: &mut DiagnosticContext, model: &SemanticModel, closure: &LuaClosureExpr) {
    let file_id = model.get_file_id();
    let sig_id = LuaSignatureId::from_closure(file_id, closure);
    let Some(sig) = model.get_signature(file_id, sig_id.get_position()) else {
        return;
    };
    let actual_params: std::collections::HashSet<String> = sig.param_names().into_iter().collect();

    let Some(comment) = super::get_closure_expr_comment(closure) else {
        return;
    };

    let mut has_doc_param = false;
    let mut missing_param = false;

    for tag in comment.children::<LuaDocTagParam>() {
        has_doc_param = true;
        if let Some(name_tk) = tag.get_name_token() {
            if !actual_params.contains(name_tk.get_name_text()) {
                missing_param = true;
            }
        }
    }

    let has_return_doc = comment.children::<LuaDocTagReturn>().next().is_some();

    let is_global = false; // TODO: determine from decl context
    let code = if is_global {
        DiagnosticCode::MissingGlobalDoc
    } else {
        DiagnosticCode::IncompleteSignatureDoc
    };

    if !has_doc_param || missing_param {
        let range = closure
            .token_by_kind(emmylua_parser::LuaTokenKind::TkEnd)
            .map(|t| t.get_range())
            .unwrap_or(closure.get_range());
        context.add_diagnostic(
            code,
            range,
            t!("Incomplete signature documentation.").to_string(),
            None,
        );
    }
    let _ = has_return_doc; // suppress unused warning
}
