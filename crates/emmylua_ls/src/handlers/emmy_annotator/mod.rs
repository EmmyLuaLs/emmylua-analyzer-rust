mod build_annotator;
mod emmy_annotator_request;

use std::str::FromStr;

use build_annotator::build_annotators;
pub use emmy_annotator_request::*;
use lsp_types::Uri;
use tokio_util::sync::CancellationToken;

use crate::context::ServerContextSnapshot;

pub async fn on_emmy_annotator_handler(
    context: ServerContextSnapshot,
    params: EmmyAnnotatorParams,
    _: CancellationToken,
) -> Option<Vec<EmmyAnnotator>> {
    let uri = Uri::from_str(&params.uri).ok()?;
    let analysis = context.analysis.read().await;
    let file_id = analysis.get_file_id(&uri)?;
    let semantic_model = analysis.compilation.get_semantic_model(file_id)?;

    Some(build_annotators(&semantic_model))
}
