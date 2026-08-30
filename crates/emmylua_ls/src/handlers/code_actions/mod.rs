mod actions;
mod build_actions;

use build_actions::build_actions;
use emmylua_code_analysis::{EmmyLuaAnalysis, FileId};
use lsp_types::{
    ClientCapabilities, CodeActionParams, CodeActionProviderCapability, CodeActionResponse,
    Diagnostic, ServerCapabilities,
};
use tokio_util::sync::CancellationToken;

use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, analysis_query};

use super::RegisterCapabilities;

#[allow(unused_variables)]
pub async fn on_code_action_handler(
    context: ServerContextSnapshot,
    params: CodeActionParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<CodeActionResponse> {
    let uri = params.text_document.uri;
    let diagnostics = params.context.diagnostics;
    let cache_key = format!("code_action:{}", uri.as_str());
    let external_cancel = cancel_token.clone();
    analysis_query(
        context.analysis(),
        context.request_manager(),
        &cache_key,
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        Some(external_cancel),
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            code_action(analysis, file_id, diagnostics.clone())
        },
    )
    .await
}

pub fn code_action(
    analysis: &EmmyLuaAnalysis,
    file_id: FileId,
    diagnostics: Vec<Diagnostic>,
) -> Option<CodeActionResponse> {
    let model = analysis.semantic_model(file_id)?;
    let document = analysis.salsa.document(file_id)?;
    let emmyrc = analysis.get_emmyrc();

    build_actions(&model, &document, &emmyrc, file_id, diagnostics)
}

pub struct CodeActionsCapabilities;

impl RegisterCapabilities for CodeActionsCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.code_action_provider = Some(CodeActionProviderCapability::Simple(true));
    }
}
