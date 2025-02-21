use emmylua_code_analysis::{LuaMemberKey, LuaMemberOwner, SemanticModel};
use emmylua_parser::{LuaAstToken, LuaDocTagSee, LuaNameToken};
use lsp_types::GotoDefinitionResponse;

pub fn goto_doc_see(
    semantic_model: &SemanticModel,
    doc_see: LuaDocTagSee,
    see_name: LuaNameToken,
) -> Option<GotoDefinitionResponse> {
    let name_tokens = doc_see.get_names();
    let mut name_parts = Vec::new();
    for name_token in name_tokens {
        let name = name_token.get_name_text();
        name_parts.push(name.to_string());
        if name_token.get_position() == see_name.get_position() {
            break;
        }
    }

    match name_parts.len() {
        1 => {
            let name = &name_parts[0];
            return goto_type(semantic_model, &name);
        }
        2 => {
            let type_name = &name_parts[0];
            let member_name = &name_parts[1];
            return goto_type_member(semantic_model, &type_name, &member_name);
        }
        _ => {}
    }

    None
}

fn goto_type(semantic_model: &SemanticModel, type_name: &str) -> Option<GotoDefinitionResponse> {
    let file_id = semantic_model.get_file_id();
    let type_decl = semantic_model
        .get_db()
        .get_type_index()
        .find_type_decl(file_id, type_name)?;
    let locations = type_decl.get_locations();
    let mut result = Vec::new();
    for location in locations {
        let document = semantic_model.get_document_by_file_id(location.file_id)?;
        let lsp_location = document.to_lsp_location(location.range)?;
        result.push(lsp_location);
    }

    Some(GotoDefinitionResponse::Array(result))
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
    let member_owner = LuaMemberOwner::Type(type_id);
    let member_map = semantic_model
        .get_db()
        .get_member_index()
        .get_member_map(member_owner)?;
    let member_id = member_map.get(&LuaMemberKey::Name(member_name.to_string().into()))?.get_member_id();
    let member = semantic_model
        .get_db()
        .get_member_index()
        .get_member(member_id)?;
    let file_id = member.get_file_id();
    let member_range = member.get_range();
    let document = semantic_model.get_document_by_file_id(file_id)?;
    let lsp_location = document.to_lsp_location(member_range)?;

    Some(GotoDefinitionResponse::Scalar(lsp_location))
}
