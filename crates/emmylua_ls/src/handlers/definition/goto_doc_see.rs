use crate::handlers::definition::goto_path::goto_path;
use emmylua_code_analysis::{
    LuaCompilation, LuaMemberKey, LuaSemanticDeclId, LuaType, SemanticModel,
};
use emmylua_parser::{LuaAstToken, LuaGeneralToken};
use emmylua_parser_desc::parse_ref_target;
use lsp_types::GotoDefinitionResponse;
use rowan::TextSize;

pub fn goto_doc_see(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    content_token: LuaGeneralToken,
    position_offset: TextSize,
) -> Option<GotoDefinitionResponse> {
    let text = content_token.get_text();
    let name_parts = text.split('#').collect::<Vec<_>>();

    match name_parts.len() {
        0 => {}
        // Legacy handler for format like `@see type#member`
        2 if !name_parts[1].is_empty() && !name_parts[1].starts_with([' ', '\t']) => {
            let type_name = &name_parts[0];
            let member_name = &name_parts[1];
            return goto_type_member(semantic_model, &type_name, &member_name);
        }
        _ => {
            let path = parse_ref_target(
                semantic_model.get_document().get_text(),
                content_token.get_range(),
                position_offset,
            )?;

            return goto_path(semantic_model, compilation, &path, content_token.syntax());
        }
    }

    None
}

fn goto_type_member(
    semantic_model: &SemanticModel,
    type_name: &str,
    member_name: &str,
) -> Option<GotoDefinitionResponse> {
    let file_id = semantic_model.get_file_id();
    let type_decl = semantic_model
        .get_db()
        .get_type_index()
        .find_type_decl(file_id, type_name)?;
    let type_id = type_decl.get_id();
    let typ = LuaType::Ref(type_id);
    let member_map = semantic_model.get_member_info_map(&typ)?;
    let member_infos = member_map.get(&LuaMemberKey::Name(member_name.to_string().into()))?;

    let mut result = Vec::new();
    for member_info in member_infos {
        if let Some(LuaSemanticDeclId::Member(member_id)) = &member_info.property_owner_id {
            let file_id = member_id.file_id;
            let member_range = member_id.get_syntax_id().get_range();
            let document = semantic_model.get_document_by_file_id(file_id)?;
            let lsp_location = document.to_lsp_location(member_range)?;
            result.push(lsp_location);
        }
    }

    Some(GotoDefinitionResponse::Array(result))
}
