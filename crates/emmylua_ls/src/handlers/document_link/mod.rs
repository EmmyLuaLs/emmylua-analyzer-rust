mod build_link;

use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, snapshot_query};
use build_link::build_links;
pub use build_link::is_require_path;
use emmylua_parser::LuaAstNode;
use lsp_types::{
    ClientCapabilities, DocumentLink, DocumentLinkOptions, DocumentLinkParams, ServerCapabilities,
};
use tokio_util::sync::CancellationToken;

use super::RegisterCapabilities;

pub async fn on_document_link_handler(
    context: ServerContextSnapshot,
    params: DocumentLinkParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<Vec<DocumentLink>> {
    let uri = params.text_document.uri;
    snapshot_query(
        context.analysis(),
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        cancel_token,
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            let semantic_model = analysis.semantic_model(file_id)?;
            let root = semantic_model.chunk()?;
            let document = analysis.salsa.document(file_id)?;
            let emmyrc = analysis.get_emmyrc();

            build_links(&analysis.salsa, root.syntax().clone(), &document, &emmyrc)
        },
    )
    .await
}

#[allow(unused_variables)]
pub async fn on_document_link_resolve_handler(
    _: ServerContextSnapshot,
    params: DocumentLink,
    _: CancellationToken,
) -> RequestOutcome<DocumentLink> {
    RequestOutcome::Ready(params)
}

pub struct DocumentLinkCapabilities;

impl RegisterCapabilities for DocumentLinkCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.document_link_provider = Some(DocumentLinkOptions {
            resolve_provider: Some(false),
            work_done_progress_options: Default::default(),
        });
    }
}
