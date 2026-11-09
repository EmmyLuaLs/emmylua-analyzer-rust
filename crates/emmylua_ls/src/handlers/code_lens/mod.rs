mod build_code_lens;
mod resolve_code_lens;

use build_code_lens::build_code_lens;
use lsp_types::{
    ClientCapabilities, CodeLens, CodeLensOptions, CodeLensParams, ServerCapabilities,
};
use resolve_code_lens::resolve_code_lens;
use tokio_util::sync::CancellationToken;

use crate::context::{RequestOutcome, ServerContextSnapshot, analysis_query};

use super::RegisterCapabilities;

pub async fn on_code_lens_handler(
    context: ServerContextSnapshot,
    params: CodeLensParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<Vec<CodeLens>> {
    let uri = params.text_document.uri;
    let cache_key = format!("code_lens:{}", uri.as_str());
    analysis_query(
        context.analysis(),
        context.request_manager(),
        &cache_key,
        Some(cancel_token.clone()),
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            if !analysis.get_emmyrc().code_lens.enable {
                return None;
            }
            let model = analysis.semantic_model(file_id);
            let document = analysis.db.document(file_id)?;
            build_code_lens(&model, &document)
        },
    )
    .await
}

pub async fn on_resolve_code_lens_handler(
    context: ServerContextSnapshot,
    code_lens: CodeLens,
    _: CancellationToken,
) -> RequestOutcome<CodeLens> {
    let client_id = context
        .workspace_manager()
        .lock()
        .await
        .client_config
        .client_id;
    let fallback = code_lens.clone();
    let result = context
        .analysis()
        .run_blocking(move |analysis| resolve_code_lens(&analysis.db, code_lens, client_id))
        .await
        .unwrap_or(fallback);

    RequestOutcome::Ready(result)
}

pub struct CodeLensCapabilities;

impl RegisterCapabilities for CodeLensCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.code_lens_provider = Some(CodeLensOptions {
            resolve_provider: Some(true),
        });
    }
}
