use std::collections::HashMap;

use emmylua_code_analysis::{
    LuaCompilation, LuaMemberId, LuaSemanticDeclId, SemanticDeclLevel, SemanticModel,
};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaIndexToken, LuaLiteralExpr, LuaNameToken, LuaNumberToken,
    LuaSyntaxNode, LuaTokenKind,
};
use lsp_types::Uri;

use crate::handlers::hover::find_member_origin_owner;

pub fn rename_member_references(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    member_id: LuaMemberId,
    new_name: String,
    result: &mut HashMap<Uri, HashMap<lsp_types::Range, String>>,
) -> Option<()> {
    let member = semantic_model
        .get_db()
        .get_member_index()
        .get_member(&member_id)?;
    let key = member.get_key();
    let index_references = semantic_model
        .get_db()
        .get_reference_index()
        .get_index_references(&key)?;

    let property_owner = find_member_origin_owner(compilation, semantic_model, member_id)
        .unwrap_or(LuaSemanticDeclId::Member(member_id));
    let mut semantic_cache = HashMap::new();
    for in_filed_syntax_id in index_references {
        let semantic_model =
            if let Some(semantic_model) = semantic_cache.get_mut(&in_filed_syntax_id.file_id) {
                semantic_model
            } else {
                let semantic_model = compilation.get_semantic_model(in_filed_syntax_id.file_id)?;
                semantic_cache.insert(in_filed_syntax_id.file_id, semantic_model);
                semantic_cache.get_mut(&in_filed_syntax_id.file_id)?
            };
        let root = semantic_model.get_root();
        let node = in_filed_syntax_id.value.to_node_from_root(root.syntax())?;
        if semantic_model.is_reference_to(
            node.clone(),
            property_owner.clone(),
            SemanticDeclLevel::NoTrace,
        ) {
            let range = get_member_name_token_lsp_range(semantic_model, node.clone());
            if let Some(range) = range {
                result
                    .entry(semantic_model.get_document().get_uri())
                    .or_insert_with(HashMap::new)
                    .insert(range, new_name.clone());
            }
        }
    }

    Some(())
}

fn get_member_name_token_lsp_range(
    semantic_model: &SemanticModel,
    node: LuaSyntaxNode,
) -> Option<lsp_types::Range> {
    let document = semantic_model.get_document();
    let node = LuaAst::cast(node)?;
    if let Some(token) = node.token::<LuaNameToken>() {
        return document.to_lsp_range(token.get_range());
    }

    // 此时可能是 [] 访问
    if let Some(_) = node.token::<LuaIndexToken>() {
        match node {
            LuaAst::LuaDocTagField(tag) => {
                if let Some(token) = tag.token::<LuaNumberToken>() {
                    if matches!(token.get_token_kind(), LuaTokenKind::TkInt) {
                        return document.to_lsp_range(token.get_range());
                    }
                    return document.to_lsp_range(token.get_range());
                }
            }
            _ => {
                let literal = node.child::<LuaLiteralExpr>()?.get_literal()?;
                if matches!(
                    literal.get_token_kind(),
                    LuaTokenKind::TkInt | LuaTokenKind::TkString
                ) {
                    return document.to_lsp_range(literal.get_range());
                }
            }
        }
    }

    None
}
