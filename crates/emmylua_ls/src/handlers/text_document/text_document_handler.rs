use emmylua_code_analysis::uri_to_file_path;
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams,
};
use std::time::Duration;

use crate::context::{ServerContextSnapshot, WorkspaceDiagnosticLevel};

pub async fn on_did_open_text_document(
    context: ServerContextSnapshot,
    params: DidOpenTextDocumentParams,
) -> Option<()> {
    let uri = params.text_document.uri;
    let text = params.text_document.text;

    // Check if file should be filtered before acquiring locks
    // Follow lock order: workspace_manager (read) -> analysis (write)
    let should_process = {
        let analysis = context.analysis().read().await;
        let old_file_id = analysis.get_file_id(&uri);
        if old_file_id.is_some() {
            true
        } else {
            drop(analysis);
            let workspace_manager = context.workspace_manager().read().await;
            workspace_manager.is_workspace_file(&uri)
        }
    };

    if !should_process {
        return None;
    }

    // Update file and get diagnostic settings
    let (file_id, supports_pull, interval) = {
        let mut analysis = context.analysis().write().await;
        let file_id = analysis.update_file_by_uri(&uri, Some(text));
        let emmyrc = analysis.get_emmyrc();
        let interval = emmyrc.diagnostics.diagnostic_interval.unwrap_or(500);
        let supports_pull = context.lsp_features().supports_pull_diagnostic();
        (file_id, supports_pull, interval)
    };

    // Schedule diagnostic task without holding any locks
    if !supports_pull {
        if let Some(file_id) = file_id {
            context
                .file_diagnostic()
                .add_diagnostic_task(file_id, interval)
                .await;
        }
    }

    // Update open files list
    {
        let mut workspace = context.workspace_manager().write().await;
        workspace.current_open_files.insert(uri);
    }

    Some(())
}

pub async fn on_did_save_text_document(
    context: ServerContextSnapshot,
    _: DidSaveTextDocumentParams,
) -> Option<()> {
    let emmyrc = context.analysis().read().await.get_emmyrc();
    if !emmyrc.workspace.enable_reindex {
        if context.lsp_features().supports_workspace_diagnostic() {
            context
                .file_diagnostic()
                .cancel_workspace_diagnostic()
                .await;
            let workspace_manager = context.workspace_manager().write().await;
            workspace_manager.update_workspace_version(WorkspaceDiagnosticLevel::Slow, true);
            workspace_manager.check_schema_update().await;
        }

        return Some(());
    }

    let mut duration = emmyrc.workspace.reindex_duration;
    // if duration is less than 1000ms, set it to 1000ms
    if duration < 1000 {
        duration = 1000;
    }
    let workspace = context.workspace_manager().read().await;
    workspace
        .reindex_workspace(Duration::from_millis(duration))
        .await;
    workspace.check_schema_update().await;
    Some(())
}

pub async fn on_did_change_text_document(
    context: ServerContextSnapshot,
    params: DidChangeTextDocumentParams,
) -> Option<()> {
    let uri = params.text_document.uri;
    let text = params.content_changes.first()?.text.clone();

    // Check if file should be filtered before acquiring locks
    // Follow lock order: workspace_manager (read) -> analysis (write)
    let should_process = {
        let analysis = context.analysis().read().await;
        let old_file_id = analysis.get_file_id(&uri);
        if old_file_id.is_some() {
            true
        } else {
            drop(analysis);
            let workspace_manager = context.workspace_manager().read().await;
            workspace_manager.is_workspace_file(&uri)
        }
    };

    if !should_process {
        return None;
    }

    // Update file and get settings
    let (file_id, emmyrc, supports_pull) = {
        let mut analysis = context.analysis().write().await;
        let file_id = analysis.update_file_by_uri(&uri, Some(text));
        let emmyrc = analysis.get_emmyrc();
        let supports_pull = context.lsp_features().supports_pull_diagnostic();
        (file_id, emmyrc, supports_pull)
    };

    let interval = emmyrc.diagnostics.diagnostic_interval.unwrap_or(500);

    // Handle reindex without holding locks
    if emmyrc.workspace.enable_reindex {
        let workspace = context.workspace_manager().read().await;
        workspace.extend_reindex_delay().await;
    }

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
    let uri = &params.text_document.uri;
    let mut workspace = context.workspace_manager().write().await;
    workspace
        .current_open_files
        .remove(&params.text_document.uri);
    drop(workspace);
    let lsp_features = context.lsp_features();

    // 如果关闭后文件不存在, 则移除
    if let Some(file_path) = uri_to_file_path(uri)
        && !file_path.exists()
    {
        let mut mut_analysis = context.analysis().write().await;
        mut_analysis.remove_file_by_uri(uri);
        drop(mut_analysis);

        if !lsp_features.supports_pull_diagnostic() {
            context
                .file_diagnostic()
                .clear_push_file_diagnostics(uri.clone());
        }

        return Some(());
    }

    let analysis = context.analysis().read().await;
    let file_id = analysis.get_file_id(uri)?;
    let module_info = analysis
        .compilation
        .get_db()
        .get_module_index()
        .get_module(file_id);
    if module_info.is_none() {
        drop(analysis);
        let mut mut_analysis = context.analysis().write().await;
        mut_analysis.remove_file_by_uri(uri);
        drop(mut_analysis);

        if !lsp_features.supports_pull_diagnostic() {
            context
                .file_diagnostic()
                .clear_push_file_diagnostics(uri.clone());
        }
    }

    Some(())
}
