use emmylua_code_analysis::SemanticModel;
use emmylua_parser::LuaStringToken;
use lsp_types::{GotoDefinitionResponse, Location};

use crate::handlers::document_link::is_require_path;

pub fn goto_module_file(
    semantic_model: &SemanticModel,
    string_token: LuaStringToken,
) -> Option<GotoDefinitionResponse> {
    if !is_require_path(string_token.clone()).unwrap_or(false) {
        return None;
    }

    let module_path = string_token.get_value();
    let module_index = semantic_model.get_db().get_module_index();
    let founded_module =
        module_index.find_module_from(&module_path, semantic_model.get_file_id())?;
    let file_id = founded_module.file_id;
    let document = semantic_model.get_document_by_file_id(file_id)?;
    let uri = document.get_uri();
    // 确保目标文件存在
    let file_path = document.get_file_path();
    if !file_path.try_exists().unwrap_or(false) {
        return None;
    }

    let lsp_range = document.get_document_lsp_range();

    Some(GotoDefinitionResponse::Scalar(Location {
        uri,
        range: lsp_range,
    }))
}
