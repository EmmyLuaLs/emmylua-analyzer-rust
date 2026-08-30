//! # goto_module_file — require path → module file (salsa `module_file_of`).

use emmylua_code_analysis::SalsaDatabase;
use emmylua_parser::LuaStringToken;
use lsp_types::{GotoDefinitionResponse, Location, Range};

use crate::handlers::document_link::is_require_path;

pub fn goto_module_file(
    salsa: &SalsaDatabase,
    string_token: LuaStringToken,
) -> Option<GotoDefinitionResponse> {
    if !is_require_path(string_token.clone()).unwrap_or(false) {
        return None;
    }

    let module_path = string_token.get_value();
    let file_id = salsa.module_file_of(&module_path)?;
    let document = salsa.document(file_id)?;
    let uri = document.get_uri()?;
    // Ensure the target file exists (mirrors old semantics).
    let file_path = document.path.as_ref()?;
    if !file_path.try_exists().unwrap_or(false) {
        return None;
    }

    Some(GotoDefinitionResponse::Scalar(Location {
        uri,
        range: Range::default(),
    }))
}
