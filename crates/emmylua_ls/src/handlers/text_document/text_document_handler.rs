use emmylua_code_analysis::{Emmyrc, uri_to_file_path};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams,
};
use std::sync::Arc;

use crate::context::{ServerContextSnapshot, UpdateEvent, WorkspaceDiagnosticLevel};

pub async fn on_did_open_text_document(
    context: ServerContextSnapshot,
    params: DidOpenTextDocumentParams,
) -> Option<()> {
    let _ = context.update_tx().send(UpdateEvent::DidOpen(params));
    Some(())
}

pub async fn process_did_open_text_document(
    context: ServerContextSnapshot,
    params: DidOpenTextDocumentParams,
) -> Option<()> {
    let uri = params.text_document.uri;
    let text = params.text_document.text;

    // Check if file should be filtered before acquiring locks
    // Follow lock order: workspace_manager (read) -> analysis (write)
    let should_process = match context.analysis().try_with_snapshot(|analysis| {
        let old_file_id = analysis.get_file_id(&uri);
        if old_file_id.is_some() {
            Some(true)
        } else {
            None
        }
    }) {
        Some(true) => true,
        _ => {
            let workspace_manager = context.workspace_manager().lock().await;
            workspace_manager.is_workspace_file(&uri)
        }
    };

    {
        let mut workspace = context.workspace_manager().lock().await;
        workspace.sync_open_file(uri.clone(), text.clone());
    }

    if !should_process {
        return None;
    }

    // Update file and get diagnostic settings
    let (file_id, interval) = context
        .analysis()
        .update(|analysis| {
            let file_id = analysis.update_file_by_uri(&uri, Some(text));
            let interval = analysis
                .get_emmyrc()
                .diagnostics
                .diagnostic_interval
                .unwrap_or(500);
            (file_id, interval)
        })
        .await;
    let supports_pull = context.lsp_features().supports_pull_diagnostic();

    // Schedule diagnostic task without holding any locks
    if !supports_pull {
        if let Some(file_id) = file_id {
            context
                .file_diagnostic()
                .add_diagnostic_task(file_id, interval)
                .await;
        }
    }

    Some(())
}

pub async fn on_did_save_text_document(
    context: ServerContextSnapshot,
    _: DidSaveTextDocumentParams,
) -> Option<()> {
    let emmyrc = context
        .analysis()
        .with_snapshot(|analysis| analysis.get_emmyrc())
        .unwrap_or_else(|| Arc::new(Emmyrc::default()));
    if !emmyrc.workspace.enable_reindex {
        if context.lsp_features().supports_workspace_diagnostic() {
            context
                .file_diagnostic()
                .cancel_workspace_diagnostic()
                .await;
            let workspace_manager = context.workspace_manager().lock().await;
            workspace_manager.update_workspace_version(WorkspaceDiagnosticLevel::Slow, true);
        }

        return Some(());
    }

    // let mut duration = emmyrc.workspace.reindex_duration;
    // // if duration is less than 1000ms, set it to 1000ms
    // if duration < 1000 {
    //     duration = 1000;
    // }
    // let workspace = context.workspace_manager().lock().await;
    // workspace
    //     .reindex_workspace(Duration::from_millis(duration))
    //     .await;
    Some(())
}

pub async fn on_did_change_text_document(
    context: ServerContextSnapshot,
    params: DidChangeTextDocumentParams,
) -> Option<()> {
    // Only enqueue; don't block the notification loop. Worker processes serially to preserve order.
    let _ = context.update_tx().send(UpdateEvent::DidChange(params));
    Some(())
}

pub async fn process_did_change_text_document(
    context: ServerContextSnapshot,
    params: DidChangeTextDocumentParams,
) -> Option<()> {
    let uri = params.text_document.uri;
    let text = params.content_changes.first()?.text.clone();

    // Check if file should be filtered before acquiring locks
    // Follow lock order: workspace_manager (read) -> analysis (write)
    let should_process = match context.analysis().try_with_snapshot(|analysis| {
        let old_file_id = analysis.get_file_id(&uri);
        if old_file_id.is_some() {
            Some(true)
        } else {
            None
        }
    }) {
        Some(true) => true,
        _ => {
            let workspace_manager = context.workspace_manager().lock().await;
            workspace_manager.is_workspace_file(&uri)
        }
    };

    {
        let mut workspace = context.workspace_manager().lock().await;
        workspace.sync_open_file(uri.clone(), text.clone());
    }

    if !should_process {
        return None;
    }

    // Update file and get settings
    let (file_id, interval) = context
        .analysis()
        .update(|analysis| {
            let file_id = analysis.update_file_by_uri(&uri, Some(text));
            let emmyrc = analysis.get_emmyrc();
            (
                file_id,
                emmyrc.diagnostics.diagnostic_interval.unwrap_or(500),
            )
        })
        .await;

    let supports_pull = context.lsp_features().supports_pull_diagnostic();
    // Schedule diagnostic task
    if !supports_pull {
        if let Some(file_id) = file_id {
            context
                .file_diagnostic()
                .add_diagnostic_task(file_id, interval)
                .await;
        }
    }

    Some(())
}

pub async fn on_did_close_document(
    context: ServerContextSnapshot,
    params: DidCloseTextDocumentParams,
) -> Option<()> {
    let _ = context.update_tx().send(UpdateEvent::DidClose(params));
    Some(())
}

pub async fn process_did_close_document(
    context: ServerContextSnapshot,
    params: DidCloseTextDocumentParams,
) -> Option<()> {
    let uri = &params.text_document.uri;
    let mut workspace = context.workspace_manager().lock().await;
    workspace.close_open_file(&params.text_document.uri);
    drop(workspace);
    let lsp_features = context.lsp_features();

    // If the closed file no longer exists, remove it.
    if let Some(file_path) = uri_to_file_path(uri)
        && !file_path.exists()
    {
        context
            .analysis()
            .update(|analysis| analysis.remove_file_by_uri(uri))
            .await;

        if !lsp_features.supports_pull_diagnostic() {
            context
                .file_diagnostic()
                .clear_push_file_diagnostics(uri.clone());
        }

        return Some(());
    }

    // Non-workspace file (not registered in salsa) -> remove.
    let file_exists = context
        .analysis()
        .try_with_snapshot(|analysis| {
            let file_id = analysis.get_file_id(uri)?;
            Some(analysis.salsa.get_file_text(file_id).is_some())
        })
        .unwrap_or(false);
    if !file_exists {
        context
            .analysis()
            .update(|analysis| analysis.remove_file_by_uri(uri))
            .await;

        if !lsp_features.supports_pull_diagnostic() {
            context
                .file_diagnostic()
                .clear_push_file_diagnostics(uri.clone());
        }
    }

    Some(())
}
