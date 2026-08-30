use std::time::Duration;

use lsp_types::{
    DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, RelatedFullDocumentDiagnosticReport,
};
use tokio_util::sync::CancellationToken;

use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, analysis_query};

pub async fn on_pull_document_diagnostic(
    context: ServerContextSnapshot,
    params: DocumentDiagnosticParams,
    token: CancellationToken,
) -> RequestOutcome<DocumentDiagnosticReportResult> {
    let uri = params.text_document.uri;
    let cache_key = format!("diagnostic:{}", uri.as_str());

    analysis_query(
        context.analysis(),
        context.request_manager(),
        &cache_key,
        CancelStrategy::RetryAfter(Duration::from_millis(200)),
        Some(token.clone()),
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            analysis.diagnose_file(file_id, token.clone())
        },
    )
    .await
    .map(|diagnostics| {
        DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
            related_documents: None,
            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                result_id: None,
                items: diagnostics,
            },
        })
        .into()
    })
}
