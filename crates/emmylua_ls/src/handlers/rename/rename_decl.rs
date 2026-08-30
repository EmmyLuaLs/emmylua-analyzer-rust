use std::collections::HashMap;

use emmylua_code_analysis::{DeclKind, SalsaDatabase, SalsaSemanticModel, SemanticId};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaClosureExpr, LuaCommentOwner, LuaDocTagParam, LuaStat,
    LuaTableField,
};
use lsp_types::Uri;

use crate::handlers::common::decl_reference_ranges;

#[allow(clippy::mutable_key_type)]
pub fn rename_decl_references(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    decl: &SemanticId,
    new_name: String,
    result: &mut HashMap<Uri, HashMap<lsp_types::Range, String>>,
) -> Option<()> {
    // Reference ranges (cross-file) + declaration name.
    let ranges = decl_reference_ranges(salsa, decl, true);
    for (file_id, range) in ranges {
        push_edit(salsa, file_id, range, new_name.clone(), result);
    }

    // Rename the matching `---@param` name for parameter declarations.
    if is_param(model, decl) {
        rename_doc_param(model, salsa, decl, new_name, result);
    }

    Some(())
}

fn is_param(model: &SalsaSemanticModel<'_>, decl: &SemanticId) -> bool {
    model
        .decls()
        .and_then(|decls| decls.iter().find(|d| &d.id == decl))
        .is_some_and(|d| matches!(d.kind, DeclKind::Param))
}

#[allow(clippy::mutable_key_type)]
fn rename_doc_param(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    decl: &SemanticId,
    new_name: String,
    result: &mut HashMap<Uri, HashMap<lsp_types::Range, String>>,
) -> Option<()> {
    let decl_info = model.decls()?.iter().find(|d| &d.id == decl)?.clone();
    let name = decl_info.name;
    let chunk = model.chunk()?;
    let token = chunk
        .syntax()
        .token_at_offset(decl_info.name_range.start())
        .right_biased()?;
    let param_node = LuaAst::cast(token.parent()?)?;
    let closure_expr = param_node.ancestors::<LuaClosureExpr>().next()?;

    let comments = if let Some(table_field) = closure_expr.get_parent::<LuaTableField>() {
        table_field.get_comments()
    } else {
        let stat = closure_expr.ancestors::<LuaStat>().next()?;
        stat.get_comments()
    };

    for comment in comments {
        for tag_doc in comment.get_doc_tags() {
            if let Some(doc_param) = LuaDocTagParam::cast(tag_doc.syntax().clone())
                && let Some(name_token) = doc_param.get_name_token()
            {
                if name_token.get_name_text() != name.as_str() {
                    continue;
                }
                push_edit(
                    salsa,
                    model.file_id(),
                    name_token.get_range(),
                    new_name.clone(),
                    result,
                );
            }
        }
    }

    Some(())
}

#[allow(clippy::mutable_key_type)]
pub(crate) fn push_edit(
    salsa: &SalsaDatabase,
    file_id: emmylua_code_analysis::FileId,
    range: rowan::TextRange,
    new_text: String,
    result: &mut HashMap<Uri, HashMap<lsp_types::Range, String>>,
) -> Option<()> {
    let document = salsa.document(file_id)?;
    let uri = document.get_uri()?;
    let lsp_range = document.to_lsp_range(range)?;
    result.entry(uri).or_default().insert(lsp_range, new_text);
    Some(())
}
