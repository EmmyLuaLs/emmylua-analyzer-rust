use emmylua_code_analysis::SemanticModel;
use emmylua_parser::LuaStringToken;
use lsp_types::{GotoDefinitionResponse, Location};

use crate::handlers::document_link::is_require_path;

pub fn goto_module_file(
    semantic_model: &SemanticModel,
    string_token: LuaStringToken,
) -> Option<GotoDefinitionResponse> {
    let emmyrc = semantic_model.get_emmyrc();
    if !is_require_path(string_token.clone(), &emmyrc).unwrap_or(false) {
        return None;
    }

    let module_path = string_token.get_value();
    let module_index = semantic_model.get_db().get_module_index();
    let founded_module = module_index.find_module(&module_path)?;
    let file_id = founded_module.file_id;
    let document = semantic_model.get_document_by_file_id(file_id)?;
    let uri = document.get_uri();
    let lsp_range = document.get_document_lsp_range();

    Some(GotoDefinitionResponse::Scalar(Location {
        uri,
        range: lsp_range,
    }))
}
