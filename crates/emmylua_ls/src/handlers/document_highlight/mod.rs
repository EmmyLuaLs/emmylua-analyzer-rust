mod highlight_tokens;

use emmylua_parser::{LuaAstNode, LuaTokenKind};
use highlight_tokens::highlight_tokens;
use lsp_types::{
    ClientCapabilities, DocumentHighlight, DocumentHighlightParams, OneOf, ServerCapabilities,
};
use rowan::TokenAtOffset;
use tokio_util::sync::CancellationToken;

use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, analysis_query};

use super::RegisterCapabilities;

pub async fn on_document_highlight_handler(
    context: ServerContextSnapshot,
    params: DocumentHighlightParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<Vec<DocumentHighlight>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    let cache_key = format!("highlight:{}", uri.as_str());
    analysis_query(
        context.analysis(),
        context.request_manager(),
        &cache_key,
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        Some(cancel_token.clone()),
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            let model = analysis.semantic_model(file_id)?;
            let document = analysis.salsa.document(file_id)?;
            let root = model.chunk()?;
            let position_offset =
                document.get_offset(position.line as usize, position.character as usize)?;

            if position_offset > root.syntax().text_range().end() {
                return None;
            }

            let token = match root.syntax().token_at_offset(position_offset) {
                TokenAtOffset::Single(token) => token,
                TokenAtOffset::Between(left, right) => {
                    if left.kind() == LuaTokenKind::TkName.into() {
                        left
                    } else {
                        right
                    }
                }
                TokenAtOffset::None => {
                    return None;
                }
            };

            highlight_tokens(&model, &analysis.salsa, token)
        },
    )
    .await
}

pub struct DocumentHighlightCapabilities;

impl RegisterCapabilities for DocumentHighlightCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.document_highlight_provider = Some(OneOf::Left(true));
    }
}
