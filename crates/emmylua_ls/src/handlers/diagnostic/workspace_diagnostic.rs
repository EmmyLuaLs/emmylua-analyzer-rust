use lsp_types::{
    FullDocumentDiagnosticReport, WorkspaceDiagnosticParams, WorkspaceDiagnosticReport,
    WorkspaceFullDocumentDiagnosticReport,
};
use tokio_util::sync::CancellationToken;

use crate::context::{ServerContextSnapshot, WorkspaceDiagnosticLevel};

pub async fn on_pull_workspace_diagnostic(
    context: ServerContextSnapshot,
    _: WorkspaceDiagnosticParams,
    token: CancellationToken,
) -> WorkspaceDiagnosticReport {
    let workspace_manager = context.workspace_manager().read().await;
    let status = workspace_manager.get_workspace_diagnostic_level();
    if status == WorkspaceDiagnosticLevel::None {
        return WorkspaceDiagnosticReport { items: vec![] };
    }
    let version = workspace_manager.get_workspace_version();
    workspace_manager.update_workspace_version(WorkspaceDiagnosticLevel::None, false);
    drop(workspace_manager);

    // let emmyrc = context.analysis().read().await.get_emmyrc();
    let file_diagnostics = match status {
        WorkspaceDiagnosticLevel::None => Vec::new(),
        WorkspaceDiagnosticLevel::Fast => {
            context
                .file_diagnostic()
                .pull_workspace_diagnostics_fast(token)
                .await
        }
        WorkspaceDiagnosticLevel::Slow => {
            context
                .file_diagnostic()
                .pull_workspace_diagnostics_slow(token)
                .await
        }
    };

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
