mod build_inline_values;

use build_inline_values::build_inline_values;
use lsp_types::{ClientCapabilities, InlineValue, InlineValueParams, OneOf, ServerCapabilities};
use tokio_util::sync::CancellationToken;

use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, snapshot_query};

use super::RegisterCapabilities;

pub async fn on_inline_values_handler(
    context: ServerContextSnapshot,
    params: InlineValueParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<Vec<InlineValue>> {
    let uri = params.text_document.uri;
    let stop_location = params.context.stopped_location;
    let stop_position = stop_location.start;
    snapshot_query(
        context.analysis(),
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        cancel_token,
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            if !analysis.get_emmyrc().inline_values.enable {
                return None;
            }
            let model = analysis.semantic_model(file_id)?;
            let document = analysis.salsa.document(file_id)?;
            build_inline_values(&model, &document, stop_position)
        },
    )
    .await
}

pub struct InlineValuesCapabilities;

impl RegisterCapabilities for InlineValuesCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.inline_value_provider = Some(OneOf::Left(true));
    }
}
