use crate::handlers::definition::goto_def_definition;
use crate::util::resolve_ref;
use emmylua_code_analysis::{LuaCompilation, SemanticModel};
use emmylua_parser::LuaSyntaxToken;
use emmylua_parser_desc::LuaDescRefPathItem;
use lsp_types::GotoDefinitionResponse;
use rowan::TextRange;

pub fn goto_path(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    path: &[(LuaDescRefPathItem, TextRange)],
    trigger_token: &LuaSyntaxToken,
) -> Option<GotoDefinitionResponse> {
    let semantic_infos = resolve_ref(
        semantic_model.get_db(),
        semantic_model.get_file_id(),
        path,
        trigger_token,
    );

    let locations = semantic_infos
        .into_iter()
        .filter_map(|semantic_info| {
            goto_def_definition(
                &semantic_model,
                compilation,
                semantic_info.semantic_decl?,
                trigger_token,
            )
        })
        .flat_map(|response| match response {
            GotoDefinitionResponse::Scalar(location) => vec![location],
            GotoDefinitionResponse::Array(locations) => locations,
            GotoDefinitionResponse::Link(_) => Vec::new(),
        })
        .collect::<Vec<_>>();

    if locations.is_empty() {
        None
    } else {
        Some(GotoDefinitionResponse::Array(locations))
    }
}
