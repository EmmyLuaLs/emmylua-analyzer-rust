use std::collections::{HashMap, HashSet};

use emmylua_code_analysis::{
    LuaCompilation, LuaDeclId, LuaMemberId, LuaPropertyOwnerId, SemanticModel,
};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaNameToken, LuaSyntaxNode, LuaSyntaxToken,
};
use lsp_types::{Uri, WorkspaceEdit};

pub fn rename_references(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    token: LuaSyntaxToken,
    new_name: String,
) -> Option<WorkspaceEdit> {
    let mut result = HashMap::new();
    let semantic_info = semantic_model.get_semantic_info(token.into())?;
    match semantic_info.property_owner? {
        LuaPropertyOwnerId::LuaDecl(decl_id) => {
            rename_decl_references(semantic_model, compilation, decl_id, &mut result);
        }
        LuaPropertyOwnerId::Member(member_id) => {
            rename_member_references(semantic_model, compilation, member_id, &mut result);
        }
        _ => {}
    }

    let changes = result
        .into_iter()
        .map(|(uri, ranges)| {
            let text_edits = ranges
                .into_iter()
                .map(|range| lsp_types::TextEdit {
                    range,
                    new_text: new_name.clone(),
                })
                .collect();
            (uri, text_edits)
        })
        .collect();

    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

fn rename_decl_references(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    decl_id: LuaDeclId,
    result: &mut HashMap<Uri, HashSet<lsp_types::Range>>,
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
            for decl_ref in decl_refs {
                let range = document.to_lsp_range(decl_ref.range.clone())?;
                result
                    .entry(uri.clone())
                    .or_insert_with(HashSet::new)
                    .insert(range);
            }
        }

        let decl_range = get_decl_name_token_lsp_range(semantic_model, decl_id)?;
        result
            .entry(uri)
            .or_insert_with(HashSet::new)
            .insert(decl_range);

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
            result.entry(uri).or_insert_with(HashSet::new).insert(range);
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
    let syntax_id = decl.get_syntax_id();
    let root = semantic_model.get_root();
    let node = LuaAst::cast(syntax_id.to_node_from_root(root.syntax())?)?;
    let token = node.token::<LuaNameToken>()?;
    document.to_lsp_range(token.get_range())
}

fn rename_member_references(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    member_id: LuaMemberId,
    result: &mut HashMap<Uri, HashSet<lsp_types::Range>>,
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

    let property_owner = LuaPropertyOwnerId::Member(member_id);
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
        if semantic_model.is_reference_to(node.clone(), property_owner.clone()) {
            let range = get_member_name_token_lsp_range(semantic_model, node.clone())?;
            result
                .entry(semantic_model.get_document().get_uri())
                .or_insert_with(HashSet::new)
                .insert(range);
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
    // todo
    let token = node.token::<LuaNameToken>()?;
    document.to_lsp_range(token.get_range())
}
