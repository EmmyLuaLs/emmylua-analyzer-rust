use emmylua_code_analysis::{FileId, LuaClosureId, SemanticModel};
use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaGotoStat, LuaLabelStat, LuaNameToken, LuaSyntaxToken,
};
use lsp_types::GotoDefinitionResponse;

pub(super) fn goto_label_definition(
    semantic_model: &SemanticModel,
    file_id: FileId,
    token: &LuaSyntaxToken,
) -> Option<GotoDefinitionResponse> {
    let name_token = LuaNameToken::cast(token.clone())?;
    let parent = token.parent()?;
    if LuaGotoStat::cast(parent.clone()).is_none() && LuaLabelStat::cast(parent.clone()).is_none() {
        return None;
    }

    let closure_id = LuaClosureId::from_node(&parent);
    let label_name = name_token.get_name_text();
    let label_range = semantic_model
        .get_db()
        .get_reference_index()
        .get_label_definition(&file_id, closure_id, label_name)?;
    let document = semantic_model.get_document_by_file_id(file_id)?;
    let location = document.to_lsp_location(label_range)?;
    Some(GotoDefinitionResponse::Scalar(location))
}
