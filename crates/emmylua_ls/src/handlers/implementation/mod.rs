mod implementation_searcher;

use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, snapshot_query};
use emmylua_code_analysis::{EmmyLuaAnalysis, FileId};
use emmylua_parser::LuaAstNode;
use implementation_searcher::search_implementations;
use lsp_types::{
    ClientCapabilities, GotoDefinitionResponse, ImplementationProviderCapability, Position,
    ServerCapabilities, request::GotoImplementationParams,
};
use rowan::TokenAtOffset;
use tokio_util::sync::CancellationToken;

use super::RegisterCapabilities;

pub async fn on_implementation_handler(
    context: ServerContextSnapshot,
    params: GotoImplementationParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<GotoDefinitionResponse> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    snapshot_query(
        context.analysis(),
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        cancel_token,
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            implementation(analysis, file_id, position)
        },
    )
    .await
}

pub fn implementation(
    analysis: &EmmyLuaAnalysis,
    file_id: FileId,
    position: Position,
) -> Option<GotoDefinitionResponse> {
    let model = analysis.semantic_model(file_id)?;
    let document = analysis.salsa.document(file_id)?;
    let root = model.chunk()?;
    let position_offset =
        document.get_offset(position.line as usize, position.character as usize)?;

    if position_offset > root.syntax().text_range().end() {
        return None;
    }

    let token = match root.syntax().token_at_offset(position_offset) {
        TokenAtOffset::None => return None,
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(token, _) => token,
    };

    let implementations = search_implementations(&model, &analysis.salsa, token)?;

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
