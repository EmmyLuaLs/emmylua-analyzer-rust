use std::collections::HashSet;

use emmylua_parser::{LuaAstNode, LuaAstToken, LuaClosureExpr, LuaDocTagParam};

use crate::{DiagnosticCode, LuaSignatureId, SemanticModel};

use super::{Checker, DiagnosticContext, get_closure_expr_comment};

pub struct UndefinedDocParamChecker;

impl Checker for UndefinedDocParamChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::UndefinedDocParam];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for closure_expr in root.descendants::<LuaClosureExpr>() {
            check_doc_param(context, semantic_model, &closure_expr);
        }
    }
}

fn check_doc_param(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    closure_expr: &LuaClosureExpr,
) -> Option<()> {
    let signature_id = LuaSignatureId::from_closure(semantic_model.get_file_id(), closure_expr);
    let defined_params = context
        .get_signature(&signature_id)?
        .params
        .iter()
        .cloned()
        .collect::<HashSet<_>>();

    let undefined_params = get_closure_expr_comment(closure_expr)?
        .children::<LuaDocTagParam>()
        .filter_map(|tag| {
            let name_token = tag.get_name_token()?;
            (!defined_params.contains(name_token.get_name_text())).then(|| {
                (
                    name_token.get_range(),
                    name_token.get_name_text().to_string(),
                )
            })
        })
        .collect::<Vec<_>>();

    for (range, name) in undefined_params {
        context.add_diagnostic(
            DiagnosticCode::UndefinedDocParam,
            range,
            t!("Undefined doc param: `%{name}`", name = name).to_string(),
            None,
        );
    }
    Some(())
}
