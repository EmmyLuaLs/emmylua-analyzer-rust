mod build_code_lens;
mod resolve_code_lens;

use build_code_lens::build_code_lens;
use emmylua_code_analysis::{LuaDeclId, LuaMemberId};
use lsp_types::{
    ClientCapabilities, CodeLens, CodeLensOptions, CodeLensParams, ServerCapabilities,
};
use resolve_code_lens::resolve_code_lens;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::context::ServerContextSnapshot;

use super::RegisterCapabilities;

pub async fn on_code_lens_handler(
    context: ServerContextSnapshot,
    params: CodeLensParams,
    _: CancellationToken,
) -> Option<Vec<CodeLens>> {
    let uri = params.text_document.uri;
    let analysis = context.analysis().read().await;
    let file_id = analysis.get_file_id(&uri)?;
    let semantic_model = analysis.compilation.get_semantic_model(file_id)?;

    if !semantic_model.get_emmyrc().code_lens.enable {
        return None;
    }

    build_code_lens(&semantic_model)
}

pub async fn on_resolve_code_lens_handler(
    context: ServerContextSnapshot,
    code_lens: CodeLens,
    _: CancellationToken,
) -> CodeLens {
    let client_id = context
        .workspace_manager()
        .read()
        .await
        .client_config
        .client_id;
    let analysis = context.analysis().read().await;
    let compilation = &analysis.compilation;

    resolve_code_lens(compilation, code_lens.clone(), client_id).unwrap_or(code_lens)
}

#[derive(Debug, Serialize, Deserialize)]
pub enum CodeLensData {
    Member(LuaMemberId),
    DeclId(LuaDeclId),
}

pub struct CodeLensCapabilities;

impl RegisterCapabilities for CodeLensCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.code_lens_provider = Some(CodeLensOptions {
            resolve_provider: Some(true),
        });
    }
}
