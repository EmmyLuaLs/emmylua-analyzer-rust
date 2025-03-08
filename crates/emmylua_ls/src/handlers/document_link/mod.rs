mod build_link;

use crate::context::ServerContextSnapshot;
use build_link::build_links;
pub use build_link::is_require_path;
use emmylua_parser::LuaAstNode;
use lsp_types::{
    ClientCapabilities, DocumentLink, DocumentLinkOptions, DocumentLinkParams, ServerCapabilities,
};
use tokio_util::sync::CancellationToken;

pub async fn on_document_link_handler(
    context: ServerContextSnapshot,
    params: DocumentLinkParams,
    _: CancellationToken,
) -> Option<Vec<DocumentLink>> {
    let uri = params.text_document.uri;
    let analysis = context.analysis.read().await;
    let file_id = analysis.get_file_id(&uri)?;
    let semantic_model = analysis.compilation.get_semantic_model(file_id)?;
    let root = semantic_model.get_root();
    let document = semantic_model.get_document();
    let db = semantic_model.get_db();
    let emmyrc = analysis.get_emmyrc();

    build_links(&db, root.syntax().clone(), &document, &emmyrc)
}

#[allow(unused_variables)]
pub async fn on_document_link_resolve_handler(
    _: ServerContextSnapshot,
    params: DocumentLink,
    _: CancellationToken,
) -> DocumentLink {
    params
}

pub fn register_capabilities(
    server_capabilities: &mut ServerCapabilities,
    _: &ClientCapabilities,
) -> Option<()> {
    server_capabilities.document_link_provider = Some(DocumentLinkOptions {
        resolve_provider: Some(false),
        work_done_progress_options: Default::default(),
    });
    Some(())
}
