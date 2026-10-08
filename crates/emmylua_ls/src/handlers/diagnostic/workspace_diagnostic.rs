use lsp_types::{
    FullDocumentDiagnosticReport, WorkspaceDiagnosticParams, WorkspaceDiagnosticReport,
    WorkspaceFullDocumentDiagnosticReport,
};
use tokio_util::sync::CancellationToken;

use std::time::Duration;

use crate::context::{
    CancelSource, RequestOutcome, ServerContextSnapshot, WorkspaceDiagnosticLevel,
};

pub async fn on_pull_workspace_diagnostic(
    context: ServerContextSnapshot,
    _: WorkspaceDiagnosticParams,
    token: CancellationToken,
) -> RequestOutcome<WorkspaceDiagnosticReport> {
    context
        .request_manager()
        .begin("workspace_diagnostic", token.clone())
        .await;

    let first = !context.file_diagnostic().has_workspace_diagnostic_done();
    let last_returned_version = context.file_diagnostic().get_last_workspace_version().await;

    loop {
        if token.is_cancelled() {
            return RequestOutcome::Cancelled(CancelSource::Client);
        }

        let (version, status) = {
            let workspace_manager = context.workspace_manager().lock().await;
            (
                workspace_manager.get_workspace_version(),
                workspace_manager.get_workspace_diagnostic_level(),
            )
        };

        let should_run = first
            || last_returned_version != Some(version)
            || status != WorkspaceDiagnosticLevel::None;

        if should_run {
            // Use Fast for the first pass to return quickly; use Slow for full diagnostics afterward.
            let level = if first {
                WorkspaceDiagnosticLevel::Fast
            } else {
                WorkspaceDiagnosticLevel::Slow
            };
            {
                let workspace_manager = context.workspace_manager().lock().await;
                workspace_manager.update_workspace_version(WorkspaceDiagnosticLevel::None, false);
            }

            let result = match level {
                WorkspaceDiagnosticLevel::None => Some(Vec::new()),
                WorkspaceDiagnosticLevel::Fast => {
                    context
                        .file_diagnostic()
                        .pull_workspace_diagnostics_fast(token.clone())
                        .await
                }
                WorkspaceDiagnosticLevel::Slow => {
                    context
                        .file_diagnostic()
                        .pull_workspace_diagnostics_slow(token.clone())
                        .await
                }
            };

            match result {
                Some(file_diagnostics) => {
                    context.file_diagnostic().mark_workspace_diagnostic_done();
                    context
                        .file_diagnostic()
                        .set_last_workspace_version(version)
                        .await;
                    return RequestOutcome::Ready(build_workspace_report(
                        file_diagnostics,
                        version,
                    ));
                }
                None => {
                    if token.is_cancelled() {
                        return RequestOutcome::Cancelled(CancelSource::Client);
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        } else {
            // Workspace is unchanged: keep the request pending and check again after 2 seconds.
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}

fn build_workspace_report(
    file_diagnostics: Vec<(lsp_types::Uri, Vec<lsp_types::Diagnostic>)>,
    version: i64,
) -> WorkspaceDiagnosticReport {
    WorkspaceDiagnosticReport {
        items: file_diagnostics
            .into_iter()
            .map(|(uri, diagnostics)| {
                WorkspaceFullDocumentDiagnosticReport {
                    uri,
                    version: Some(version),
                    full_document_diagnostic_report: FullDocumentDiagnosticReport {
                        items: diagnostics,
                        result_id: None,
                    },
                }
                .into()
            })
            .collect(),
    }
}
