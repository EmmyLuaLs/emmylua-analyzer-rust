use std::error::Error;

use lsp_server::Response;

use crate::context::ServerContext;

pub async fn on_response_handler(
    response: Response,
    server_context: &mut ServerContext,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    server_context.send_response(response).await;
    Ok(())
}
