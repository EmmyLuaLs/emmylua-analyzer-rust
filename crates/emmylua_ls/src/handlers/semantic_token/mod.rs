mod build_semantic_tokens;
mod function_string_highlight;
mod language_injector;
mod semantic_token_builder;

use crate::context::{
    CancelStrategy, ClientId, RequestOutcome, ServerContextSnapshot, analysis_query,
};
use build_semantic_tokens::build_semantic_tokens;
use emmylua_code_analysis::{EmmyLuaAnalysis, FileId};
use lsp_types::{
    ClientCapabilities, SemanticTokens, SemanticTokensFullOptions, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensParams, SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities,
};

pub use semantic_token_builder::{SemanticTokenModifierKind, SemanticTokenTypeKind};
use tokio_util::sync::CancellationToken;

use super::RegisterCapabilities;

pub async fn on_semantic_token_handler(
    context: ServerContextSnapshot,
    params: SemanticTokensParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<SemanticTokensResult> {
    let uri = params.text_document.uri;
    let client_id = {
        let workspace_manager = context.workspace_manager().lock().await;
        workspace_manager.client_config.client_id
    };
    let cache_key = format!("semantic:{}", uri.as_str());
    let supports_multiline_tokens = context.lsp_features().supports_multiline_tokens();
    analysis_query(
        context.analysis(),
        context.request_manager(),
        &cache_key,
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(50)),
        Some(cancel_token.clone()),
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            semantic_token(analysis, file_id, supports_multiline_tokens, client_id)
        },
    )
    .await
}

pub fn semantic_token(
    analysis: &EmmyLuaAnalysis,
    file_id: FileId,
    supports_multiline_tokens: bool,
    client_id: ClientId,
) -> Option<SemanticTokensResult> {
    if !analysis.get_emmyrc().semantic_tokens.enable {
        return None;
    }
    let model = analysis.semantic_model(file_id)?;
    let document = analysis.salsa.document(file_id)?;
    let emmyrc = analysis.get_emmyrc();

    let result = build_semantic_tokens(
        &model,
        &document,
        supports_multiline_tokens,
        client_id,
        &emmyrc,
    )?;

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: result,
    }))
}

pub struct SemanticTokenCapabilities;

impl RegisterCapabilities for SemanticTokenCapabilities {
    fn register_capabilities(
        server_capabilities: &mut ServerCapabilities,
        _client_capabilities: &ClientCapabilities,
    ) {
        server_capabilities.semantic_tokens_provider = Some(
            SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_modifiers: SemanticTokenModifierKind::all_modifiers(),
                    token_types: SemanticTokenTypeKind::all_types(),
                },
                full: Some(SemanticTokensFullOptions::Bool(true)),
                ..Default::default()
            }),
        );
    }
}
