//! Undefined doc param — pure salsa.

use emmylua_parser::{LuaAstNode, LuaAstToken, LuaClosureExpr, LuaDocTagParam};

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
    let Some(sig) = model.get_signature(file_id, sig_id.get_position()) else { return };
    let actual_params: Vec<String> = sig.param_names();

    let Some(comment) = super::get_closure_expr_comment(closure) else { return };
    for tag in comment.children::<LuaDocTagParam>() {
        let Some(name_tk) = tag.get_name_token() else { continue };
        let name = name_tk.get_name_text();
        if !actual_params.contains(&name.to_string()) {
            context.add_diagnostic(
                DiagnosticCode::UndefinedDocParam,
                name_tk.get_range(),
                t!("Undefined doc param: `%{name}`", name = name).to_string(),
                None,
            );
        }
    }
}
