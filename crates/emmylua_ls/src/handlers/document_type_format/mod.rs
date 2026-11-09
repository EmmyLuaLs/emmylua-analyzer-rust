use lsp_types::{ClientCapabilities, DocumentOnTypeFormattingParams, ServerCapabilities, TextEdit};
use tokio_util::sync::CancellationToken;

use crate::context::{RequestOutcome, ServerContextSnapshot};

use super::RegisterCapabilities;

/// should I support this?
pub async fn on_type_formatting_handler(
    _: ServerContextSnapshot,
    _: DocumentOnTypeFormattingParams,
    _: CancellationToken,
) -> RequestOutcome<Vec<TextEdit>> {
    RequestOutcome::Missing
}

#[allow(unused)]
pub struct DocumentTypeFormattingCapabilities;

impl RegisterCapabilities for DocumentTypeFormattingCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.document_on_type_formatting_provider =
            Some(lsp_types::DocumentOnTypeFormattingOptions {
                first_trigger_character: "\n".to_string(),
                more_trigger_character: None,
            });
    }
}
