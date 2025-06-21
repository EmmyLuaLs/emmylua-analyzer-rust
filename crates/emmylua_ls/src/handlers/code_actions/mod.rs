mod actions;
mod build_actions;

use build_actions::build_actions;
use emmylua_code_analysis::{EmmyLuaAnalysis, FileId};
use lsp_types::{
    ClientCapabilities, CodeActionParams, CodeActionProviderCapability, CodeActionResponse,
    Diagnostic, ServerCapabilities,
};
use tokio_util::sync::CancellationToken;

use crate::context::ServerContextSnapshot;

use super::RegisterCapabilities;

#[allow(unused_variables)]
pub async fn on_code_action_handler(
    context: ServerContextSnapshot,
    params: CodeActionParams,
    _: CancellationToken,
) -> Option<CodeActionResponse> {
    let uri = params.text_document.uri;
    let diagnostics = params.context.diagnostics;
    let analysis = context.analysis.read().await;
    let file_id = analysis.get_file_id(&uri)?;
    code_action(&analysis, file_id, diagnostics)
}

pub fn code_action(
    analysis: &EmmyLuaAnalysis,
    file_id: FileId,
    diagnostics: Vec<Diagnostic>,
) -> Option<CodeActionResponse> {
    let mut semantic_model = analysis.compilation.get_semantic_model(file_id)?;

    build_actions(&mut semantic_model, diagnostics)
}

pub struct CodeActionsCapabilities;

impl RegisterCapabilities for CodeActionsCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.code_action_provider = Some(CodeActionProviderCapability::Simple(true));
    }
}
