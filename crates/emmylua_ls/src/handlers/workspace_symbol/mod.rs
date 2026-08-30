mod build_workspace_symbols;

use build_workspace_symbols::build_workspace_symbols;
use lsp_types::{
    ClientCapabilities, OneOf, ServerCapabilities, WorkspaceSymbolParams, WorkspaceSymbolResponse,
};
use tokio_util::sync::CancellationToken;

use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, snapshot_query};

use super::RegisterCapabilities;

pub async fn on_workspace_symbol_handler(
    context: ServerContextSnapshot,
    params: WorkspaceSymbolParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<WorkspaceSymbolResponse> {
    let query = params.query;
    snapshot_query(
        context.analysis(),
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        cancel_token.clone(),
        move |analysis| build_workspace_symbols(analysis, query.clone(), cancel_token.clone()),
    )
    .await
}

pub struct WorkspaceSymbolCapabilities;

impl RegisterCapabilities for WorkspaceSymbolCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.workspace_symbol_provider = Some(OneOf::Left(true));
    }
}
