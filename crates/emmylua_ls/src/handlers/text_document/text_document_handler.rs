use std::time::Duration;

use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams,
};

use crate::context::ServerContextSnapshot;

pub async fn on_did_open_text_document(
    context: ServerContextSnapshot,
    params: DidOpenTextDocumentParams,
) -> Option<()> {
    let mut analysis = context.analysis.write().await;
    let uri = params.text_document.uri;
    let text = params.text_document.text;

    let file_id = analysis.update_file_by_uri(&uri, Some(text));
    let emmyrc = analysis.get_emmyrc();
    let interval = emmyrc.diagnostics.diagnostic_interval.unwrap_or(500);
    if let Some(file_id) = file_id {
        context
            .file_diagnostic
            .add_diagnostic_task(file_id, interval)
            .await;
    }

    Some(())
}

pub async fn on_did_save_text_document(
    context: ServerContextSnapshot,
    _: DidSaveTextDocumentParams,
) -> Option<()> {
    let emmyrc = context.analysis.read().await.get_emmyrc();
    let mut duration = emmyrc.workspace.reindex_duration;
    // if duration is less than 1000ms, set it to 1000ms
    if duration < 1000 {
        duration = 1000;
    }
    let workspace = context.workspace_manager.read().await;
    workspace
        .reindex_workspace(Duration::from_millis(duration))
        .await;
    Some(())
}

pub async fn on_did_change_text_document(
    context: ServerContextSnapshot,
    params: DidChangeTextDocumentParams,
) -> Option<()> {
    let mut analysis = context.analysis.write().await;
    let uri = params.text_document.uri;
    let text = params.content_changes.first()?.text.clone();
    let file_id = analysis.update_file_by_uri(&uri, Some(text));
    let emmyrc = analysis.get_emmyrc();
    let interval = emmyrc.diagnostics.diagnostic_interval.unwrap_or(500);
    drop(analysis);

    let workspace = context.workspace_manager.read().await;
    workspace.extend_reindex_delay().await;
    drop(workspace);
    if let Some(file_id) = file_id {
        context
            .file_diagnostic
            .add_diagnostic_task(file_id, interval)
            .await;
    }

    Some(())
}

pub async fn on_did_close_document(
    _: ServerContextSnapshot,
    _: DidCloseTextDocumentParams,
) -> Option<()> {
    Some(())
}
