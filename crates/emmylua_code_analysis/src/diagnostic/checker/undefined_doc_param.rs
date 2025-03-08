use emmylua_parser::{LuaAstNode, LuaAstToken, LuaClosureExpr, LuaDocTagParam};

use crate::{DiagnosticCode, LuaSignatureId, SemanticModel};

use super::{get_closure_expr_comment, DiagnosticContext};

pub const CODES: &[DiagnosticCode] = &[DiagnosticCode::UndefinedDocParam];

pub fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) -> Option<()> {
    let root = semantic_model.get_root().clone();
    for return_stat in root.descendants::<LuaClosureExpr>() {
        check_doc_param(context, semantic_model, &return_stat);
    }
    Some(())
}

fn check_doc_param(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    closure_expr: &LuaClosureExpr,
) -> Option<()> {
    let signature_id = LuaSignatureId::from_closure(semantic_model.get_file_id(), &closure_expr);
    let signature = context.db.get_signature_index().get(&signature_id)?;

    get_closure_expr_comment(closure_expr)?
        .children::<LuaDocTagParam>()
        .for_each(|tag| {
            if let Some(name_token) = tag.get_name_token() {
                let info = signature.get_param_info_by_name(&name_token.get_name_text());
                if info.is_none() {
                    context.add_diagnostic(
                        DiagnosticCode::UndefinedDocParam,
                        name_token.get_range(),
                        t!(
                            "Undefined doc param: `%{name}`",
                            name = name_token.get_name_text()
                        )
                        .to_string(),
                        None,
                    );
                }
            }
        });
    Some(())
}
