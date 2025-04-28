mod implementation_searcher;
mod test;

use crate::context::ServerContextSnapshot;
use emmylua_code_analysis::{EmmyLuaAnalysis, FileId};
use emmylua_parser::LuaAstNode;
use implementation_searcher::search_implementations;
use lsp_types::{
    request::GotoImplementationParams, ClientCapabilities, GotoDefinitionResponse,
    ImplementationProviderCapability, Position, ServerCapabilities,
};
use rowan::TokenAtOffset;
use tokio_util::sync::CancellationToken;

use super::RegisterCapabilities;

pub async fn on_implementation_handler(
    context: ServerContextSnapshot,
    params: GotoImplementationParams,
    _: CancellationToken,
) -> Option<GotoDefinitionResponse> {
    let uri = params.text_document_position_params.text_document.uri;
    let analysis = context.analysis.read().await;
    let file_id = analysis.get_file_id(&uri)?;
    let position = params.text_document_position_params.position;

    implementation(&analysis, file_id, position)
}

pub fn implementation(
    analysis: &EmmyLuaAnalysis,
    file_id: FileId,
    position: Position,
) -> Option<GotoDefinitionResponse> {
    let mut semantic_model = analysis.compilation.get_semantic_model(file_id)?;

    let root = semantic_model.get_root();
    let position_offset = {
        let document = semantic_model.get_document();
        document.get_offset(position.line as usize, position.character as usize)?
    };

    if position_offset > root.syntax().text_range().end() {
        return None;
    }

    let token = match root.syntax().token_at_offset(position_offset) {
        TokenAtOffset::None => return None,
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(token, _) => token,
    };

    let implementations =
        search_implementations(&mut semantic_model, &analysis.compilation, token)?;

    if implementations.is_empty() {
        return None;
    }

    Some(GotoDefinitionResponse::Array(implementations))
}

pub struct ImplementationCapabilities;

impl RegisterCapabilities for ImplementationCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.implementation_provider =
            Some(ImplementationProviderCapability::Simple(true));
    }
}
