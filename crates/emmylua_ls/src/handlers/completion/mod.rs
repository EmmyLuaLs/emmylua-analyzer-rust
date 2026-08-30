mod completion_builder;
mod completion_data;
pub(crate) mod data;
pub(crate) mod providers;
mod resolve_completion;

use completion_builder::CompletionBuilder;
use completion_data::CompletionData;
use emmylua_code_analysis::{EmmyLuaAnalysis, FileId};
use emmylua_parser::LuaAstNode;
use log::error;
use lsp_types::{
    ClientCapabilities, CompletionItem, CompletionOptions, CompletionOptionsCompletionItem,
    CompletionParams, CompletionResponse, CompletionTriggerKind, Position, ServerCapabilities,
};
use providers::add_completions;
use resolve_completion::resolve_completion;
use rowan::TokenAtOffset;
use tokio_util::sync::CancellationToken;

use crate::context::{
    CancelStrategy, ClientId, RequestOutcome, ServerContextSnapshot, analysis_query,
};

use super::RegisterCapabilities;

pub async fn on_completion_handler(
    context: ServerContextSnapshot,
    params: CompletionParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<CompletionResponse> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    let trigger_kind = params
        .context
        .map(|context| context.trigger_kind)
        .unwrap_or(CompletionTriggerKind::INVOKED);
    let cache_key = format!("completion:{}", uri.as_str());
    let external_cancel = cancel_token.clone();
    analysis_query(
        context.analysis(),
        context.request_manager(),
        &cache_key,
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        Some(external_cancel),
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            if !analysis.get_emmyrc().completion.enable {
                return None;
            }

            completion(
                analysis,
                file_id,
                position,
                trigger_kind,
                cancel_token.clone(),
            )
        },
    )
    .await
}

pub fn completion(
    analysis: &EmmyLuaAnalysis,
    file_id: FileId,
    position: Position,
    trigger_kind: CompletionTriggerKind,
    cancel_token: CancellationToken,
) -> Option<CompletionResponse> {
    if !analysis.get_emmyrc().completion.enable {
        return None;
    }
    let model = analysis.semantic_model(file_id)?;
    let document = analysis.salsa.document(file_id)?;
    let emmyrc = analysis.get_emmyrc();
    let root = model.chunk()?;
    let position_offset =
        document.get_offset(position.line as usize, position.character as usize)?;

    if position_offset > root.syntax().text_range().end() {
        return None;
    }

    let token = match root.syntax().token_at_offset(position_offset) {
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(left, _) => left,
        TokenAtOffset::None => {
            return None;
        }
    };

    let mut builder = CompletionBuilder::new(
        token,
        model,
        document,
        emmyrc,
        trigger_kind,
        position_offset,
        cancel_token,
    );
    add_completions(&mut builder);
    Some(CompletionResponse::Array(builder.get_completion_items()))
}

pub async fn on_completion_resolve_handler(
    context: ServerContextSnapshot,
    params: CompletionItem,
    _: CancellationToken,
) -> RequestOutcome<CompletionItem> {
    let client_id = {
        let workspace_manager = context.workspace_manager().lock().await;
        workspace_manager.client_config.client_id
    };
    let result = context
        .analysis()
        .with_snapshot(|analysis| completion_resolve(analysis, params.clone(), client_id))
        .unwrap_or_else(|| params.clone());
    RequestOutcome::Ready(result)
}

pub fn completion_resolve(
    analysis: &EmmyLuaAnalysis,
    params: CompletionItem,
    client_id: ClientId,
) -> CompletionItem {
    let completion_item = params;
    if let Some(data) = completion_item.data.clone() {
        let completion_data = match serde_json::from_value::<CompletionData>(data.clone()) {
            Ok(data) => data,
            Err(err) => {
                error!("Failed to deserialize completion data: {:?}", err);
                return completion_item;
            }
        };
        resolve_completion(analysis, completion_item, completion_data, client_id)
    } else {
        completion_item
    }
}

pub struct CompletionCapabilities;

impl RegisterCapabilities for CompletionCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.completion_provider = Some(CompletionOptions {
            resolve_provider: Some(true),
            trigger_characters: Some(
                [
                    '.', ':', '(', '[', '"', '\'', ' ', '@', '\\', '/', '|', '#', '?',
                ]
                .into_iter()
                .map(|s| s.to_string())
                .collect(),
            ),
            work_done_progress_options: Default::default(),
            completion_item: Some(CompletionOptionsCompletionItem {
                label_details_support: Some(true),
            }),
            all_commit_characters: Default::default(),
        });
    }
}
