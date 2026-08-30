mod build_code_lens;
mod resolve_code_lens;

use build_code_lens::build_code_lens;
use lsp_types::{
    ClientCapabilities, CodeLens, CodeLensOptions, CodeLensParams, ServerCapabilities,
};
use resolve_code_lens::resolve_code_lens;
use tokio_util::sync::CancellationToken;

use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, analysis_query};

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
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(50)),
        Some(cancel_token.clone()),
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            if !analysis.get_emmyrc().code_lens.enable {
                return None;
            }
            let model = analysis.semantic_model(file_id)?;
            let document = analysis.salsa.document(file_id)?;
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
    let result = context
        .analysis()
        .with_snapshot(|analysis| {
            resolve_code_lens(&analysis.salsa, code_lens.clone(), client_id)
                .unwrap_or(code_lens.clone())
        })
        .unwrap_or_else(|| code_lens.clone());
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
