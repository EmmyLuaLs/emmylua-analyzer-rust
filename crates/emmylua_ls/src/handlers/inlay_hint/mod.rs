mod build_function_hint;
mod build_inlay_hint;

use super::RegisterCapabilities;
use crate::context::{ClientId, ServerContextSnapshot};
use build_inlay_hint::build_inlay_hints;
pub use build_inlay_hint::{get_override_lsp_location, get_super_member_id};
use emmylua_code_analysis::{EmmyLuaAnalysis, FileId};
use lsp_types::{
    ClientCapabilities, InlayHint, InlayHintOptions, InlayHintParams, InlayHintServerCapabilities,
    OneOf, ServerCapabilities,
};
use tokio_util::sync::CancellationToken;

pub async fn on_inlay_hint_handler(
    context: ServerContextSnapshot,
    params: InlayHintParams,
    _: CancellationToken,
) -> Option<Vec<InlayHint>> {
    let uri = params.text_document.uri;
    let analysis = context.analysis().read().await;
    let client_id = context
        .workspace_manager()
        .read()
        .await
        .client_config
        .client_id;
    inlay_hint(&analysis, analysis.get_file_id(&uri)?, client_id)
}

pub fn inlay_hint(
    analysis: &EmmyLuaAnalysis,
    file_id: FileId,
    client_id: ClientId,
) -> Option<Vec<InlayHint>> {
    let semantic_model = analysis.compilation.get_semantic_model(file_id)?;
    if !semantic_model.get_emmyrc().hint.enable {
        return Some(vec![]);
    }

    build_inlay_hints(&semantic_model, client_id)
}

#[allow(unused_variables)]
pub async fn on_resolve_inlay_hint(
    context: ServerContextSnapshot,
    inlay_hint: InlayHint,
    _: CancellationToken,
) -> InlayHint {
    inlay_hint
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
