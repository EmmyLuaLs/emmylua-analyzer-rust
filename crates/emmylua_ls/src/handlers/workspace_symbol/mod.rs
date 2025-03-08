mod build_workspace_symbols;

use build_workspace_symbols::build_workspace_symbols;
use lsp_types::{
    ClientCapabilities, OneOf, ServerCapabilities, WorkspaceSymbolParams, WorkspaceSymbolResponse,
};
use tokio_util::sync::CancellationToken;

use crate::context::ServerContextSnapshot;

pub async fn on_workspace_symbol_handler(
    context: ServerContextSnapshot,
    params: WorkspaceSymbolParams,
    cancel_token: CancellationToken,
) -> Option<WorkspaceSymbolResponse> {
    let query = params.query;
    let analysis = context.analysis.read().await;
    let compilation = &analysis.compilation;

    build_workspace_symbols(compilation, query, cancel_token)
}

pub fn register_capabilities(
    server_capabilities: &mut ServerCapabilities,
    _: &ClientCapabilities,
) -> Option<()> {
    server_capabilities.workspace_symbol_provider = Some(OneOf::Left(true));

    Some(())
}
