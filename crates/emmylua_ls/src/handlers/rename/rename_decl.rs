use std::collections::HashMap;

use emmylua_code_analysis::{LuaCompilation, LuaDeclId, SemanticModel};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaClosureExpr, LuaCommentOwner, LuaDocTagParam, LuaStat,
    LuaTableField,
};
use lsp_types::Uri;

#[allow(clippy::mutable_key_type)]
pub fn rename_decl_references(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    decl_id: LuaDeclId,
    new_name: String,
    result: &mut HashMap<Uri, HashMap<lsp_types::Range, String>>,
) -> Option<()> {
    let decl = semantic_model
        .get_db()
        .get_decl_index()
        .get_decl(&decl_id)?;
    if decl.is_local() {
        let local_references = semantic_model
            .get_db()
            .get_reference_index()
            .get_decl_references(&decl_id.file_id, &decl_id);
        let document = semantic_model.get_document();
        let uri = document.get_uri();
        if let Some(decl_refs) = local_references {
            for decl_ref in &decl_refs.cells {
                let range = document.to_lsp_range(decl_ref.range)?;
                result
                    .entry(uri.clone())
                    .or_default()
                    .insert(range, new_name.clone());
            }
        }

        let decl_range = get_decl_name_token_lsp_range(semantic_model, decl_id)?;
        result
            .entry(uri)
            .or_default()
            .insert(decl_range, new_name.clone());

        if decl.is_param() {
            rename_doc_param(semantic_model, decl_id, new_name, result);
        }

        return Some(());
    } else {
        let name = decl.get_name();
        let global_references = semantic_model
            .get_db()
            .get_reference_index()
            .get_global_references(name)?;

        let mut semantic_cache = HashMap::new();
        for in_filed_syntax_id in global_references {
            let semantic_model = if let Some(semantic_model) =
                semantic_cache.get_mut(&in_filed_syntax_id.file_id)
            {
                semantic_model
            } else {
                let semantic_model = compilation.get_semantic_model(in_filed_syntax_id.file_id)?;
                semantic_cache.insert(in_filed_syntax_id.file_id, semantic_model);
                semantic_cache.get_mut(&in_filed_syntax_id.file_id)?
            };
            let document = semantic_model.get_document();
            let uri = document.get_uri();
            let range = document.to_lsp_range(in_filed_syntax_id.value.get_range())?;
            result
                .entry(uri)
                .or_default()
                .insert(range, new_name.clone());
        }
    }

    Some(())
}

fn get_decl_name_token_lsp_range(
    semantic_model: &SemanticModel,
    decl_id: LuaDeclId,
) -> Option<lsp_types::Range> {
    let decl = semantic_model
        .get_db()
        .get_decl_index()
        .get_decl(&decl_id)?;
    let document = semantic_model.get_document_by_file_id(decl_id.file_id)?;
    document.to_lsp_range(decl.get_range())
}

#[allow(clippy::mutable_key_type)]
fn rename_doc_param(
    semantic_model: &SemanticModel,
    decl_id: LuaDeclId,
    new_name: String,
    result: &mut HashMap<Uri, HashMap<lsp_types::Range, String>>,
) -> Option<()> {
    let decl = semantic_model
        .get_db()
        .get_decl_index()
        .get_decl(&decl_id)?;
    let name = decl.get_name();
    let syntax_id = decl.get_syntax_id();
    let root = semantic_model.get_root();
    let param_node = LuaAst::cast(syntax_id.to_node_from_root(root.syntax())?)?;
    let closure_expr = param_node.ancestors::<LuaClosureExpr>().next()?;
    let comments = if let Some(table_field) = closure_expr.get_parent::<LuaTableField>() {
        table_field.get_comments()
    } else {
        let stat = closure_expr.ancestors::<LuaStat>().next()?;
        stat.get_comments()
    };

    let document = semantic_model.get_document();
    let uri = document.get_uri();
    for comment in comments {
        for tag_doc in comment.get_doc_tags() {
            if let Some(doc_param) = LuaDocTagParam::cast(tag_doc.syntax().clone())
                && let Some(name_token) = doc_param.get_name_token()
            {
                if name_token.get_text() != name {
                    continue;
                }

                let range = document.to_lsp_range(name_token.get_range())?;
                result
                    .entry(uri.clone())
                    .or_default()
                    .insert(range, new_name.clone());
            }
        }
    }

    Some(())
}
