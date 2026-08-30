mod build_inlay_hint;

use super::RegisterCapabilities;
use crate::context::{
    CancelStrategy, ClientId, RequestOutcome, ServerContextSnapshot, analysis_query,
};
use build_inlay_hint::build_inlay_hints;
use emmylua_code_analysis::{EmmyLuaAnalysis, FileId};
use lsp_types::{
    ClientCapabilities, InlayHint, InlayHintOptions, InlayHintParams, InlayHintServerCapabilities,
    OneOf, ServerCapabilities,
};
use tokio_util::sync::CancellationToken;
// Old DbIndex-based inlay_hint (temporarily reused override-related functions before the emmy_gutter migration);

pub async fn on_inlay_hint_handler(
    context: ServerContextSnapshot,
    params: InlayHintParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<Vec<InlayHint>> {
    let uri = params.text_document.uri;
    let client_id = {
        let workspace_manager = context.workspace_manager().lock().await;
        workspace_manager.client_config.client_id
    };
    let cache_key = format!("inlay:{}", uri.as_str());
    analysis_query(
        context.analysis(),
        context.request_manager(),
        &cache_key,
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        Some(cancel_token.clone()),
        move |analysis| inlay_hint(analysis, analysis.get_file_id(&uri)?, client_id),
    )
    .await
}

pub fn inlay_hint(
    analysis: &EmmyLuaAnalysis,
    file_id: FileId,
    client_id: ClientId,
) -> Option<Vec<InlayHint>> {
    if !analysis.get_emmyrc().hint.enable {
        return Some(vec![]);
    }
    let model = analysis.semantic_model(file_id)?;
    let enum_param_hint = analysis.get_emmyrc().hint.enum_param_hint;

    build_inlay_hints(&model, &analysis.salsa, client_id, enum_param_hint)
}

#[allow(unused_variables)]
pub async fn on_resolve_inlay_hint(
    context: ServerContextSnapshot,
    inlay_hint: InlayHint,
    cancel_token: CancellationToken,
) -> RequestOutcome<InlayHint> {
    RequestOutcome::Ready(inlay_hint)
}

pub struct InlayHintCapabilities;

impl RegisterCapabilities for InlayHintCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.inlay_hint_provider = Some(OneOf::Right(
            InlayHintServerCapabilities::Options(InlayHintOptions {
                resolve_provider: Some(false),
                work_done_progress_options: Default::default(),
            }),
        ));
    }
}
