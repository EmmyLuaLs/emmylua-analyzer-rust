mod build_semantic_tokens;
mod semantic_token_builder;

use crate::context::ServerContextSnapshot;
use build_semantic_tokens::build_semantic_tokens;
use lsp_types::{
    ClientCapabilities, SemanticTokens, SemanticTokensFullOptions, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensParams, SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities,
};
pub use semantic_token_builder::{SEMANTIC_TOKEN_MODIFIERS, SEMANTIC_TOKEN_TYPES};
use tokio_util::sync::CancellationToken;

static mut SEMANTIC_MULTILINE_SUPPORT: bool = false;

pub async fn on_semantic_token_handler(
    context: ServerContextSnapshot,
    params: SemanticTokensParams,
    _: CancellationToken,
) -> Option<SemanticTokensResult> {
    let uri = params.text_document.uri;
    let analysis = context.analysis.read().await;
    let config_manager = context.workspace_manager.read().await;
    let client_id = config_manager.client_config.client_id;
    let _ = config_manager;
    let file_id = analysis.get_file_id(&uri)?;
    let mut semantic_model = analysis.compilation.get_semantic_model(file_id)?;

    if !semantic_model.get_emmyrc().semantic_tokens.enable {
        return None;
    }

    let result = build_semantic_tokens(
        &mut semantic_model,
        unsafe { SEMANTIC_MULTILINE_SUPPORT },
        client_id,
    )?;

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: result,
    }))
}

pub fn register_capabilities(
    server_capabilities: &mut ServerCapabilities,
    client_capabilities: &ClientCapabilities,
) -> Option<()> {
    server_capabilities.semantic_tokens_provider = Some(
        SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
            legend: SemanticTokensLegend {
                token_modifiers: SEMANTIC_TOKEN_MODIFIERS.iter().cloned().collect(),
                token_types: SEMANTIC_TOKEN_TYPES.iter().cloned().collect(),
            },
            full: Some(SemanticTokensFullOptions::Bool(true)),
            ..Default::default()
        }),
    );

    if is_support_muliline_tokens(client_capabilities) {
        unsafe { SEMANTIC_MULTILINE_SUPPORT = true };
    }

    Some(())
}

fn is_support_muliline_tokens(client_capability: &ClientCapabilities) -> bool {
    if let Some(text_document) = &client_capability.text_document {
        if let Some(support) = &text_document.semantic_tokens {
            if let Some(support) = &support.multiline_token_support {
                return *support;
            }
        }
    }

    false
}
