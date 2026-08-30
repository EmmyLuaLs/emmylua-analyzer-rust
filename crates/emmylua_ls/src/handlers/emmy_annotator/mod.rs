mod build_annotator;
mod emmy_annotator_request;

use std::str::FromStr;

use build_annotator::build_annotators;
pub use emmy_annotator_request::*;
use lsp_types::Uri;
use tokio_util::sync::CancellationToken;

use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, analysis_query};

pub async fn on_emmy_annotator_handler(
    context: ServerContextSnapshot,
    params: EmmyAnnotatorParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<Vec<EmmyAnnotator>> {
    let Ok(uri) = Uri::from_str(&params.uri) else {
        return RequestOutcome::Missing;
    };
    let cache_key = format!("annotator:{}", uri.as_str());
    analysis_query(
        context.analysis(),
        context.request_manager(),
        &cache_key,
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        Some(cancel_token.clone()),
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            let semantic_model = analysis.semantic_model(file_id)?;
            let document = analysis.salsa.document(file_id)?;
            let emmyrc = analysis.get_emmyrc();
            Some(build_annotators(&semantic_model, &document, &emmyrc))
        },
    )
    .await
}
