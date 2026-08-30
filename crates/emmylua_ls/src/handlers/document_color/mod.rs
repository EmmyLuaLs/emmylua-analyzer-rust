mod build_color;

use build_color::{build_colors, convert_color_to_hex};
use emmylua_parser::LuaAstNode;
use lsp_types::{
    ClientCapabilities, ColorInformation, ColorPresentation, ColorPresentationParams,
    ColorProviderCapability, DocumentColorParams, ServerCapabilities, TextEdit,
};
use tokio_util::sync::CancellationToken;

use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, snapshot_query};

use super::RegisterCapabilities;

pub async fn on_document_color(
    context: ServerContextSnapshot,
    params: DocumentColorParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<Vec<ColorInformation>> {
    let uri = params.text_document.uri;
    match snapshot_query(
        context.analysis(),
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        cancel_token,
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            let model = analysis.semantic_model(file_id)?;
            if !analysis.get_emmyrc().document_color.enable {
                return None;
            }
            let document = analysis.salsa.document(file_id)?;
            let root = model.chunk()?;
            Some(build_colors(root.syntax().clone(), &document))
        },
    )
    .await
    {
        RequestOutcome::Ready(colors) => RequestOutcome::Ready(colors),
        RequestOutcome::Cancelled(source) => RequestOutcome::Cancelled(source),
        RequestOutcome::Missing => RequestOutcome::Ready(Vec::new()),
    }
}

pub async fn on_document_color_presentation(
    context: ServerContextSnapshot,
    params: ColorPresentationParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<Vec<ColorPresentation>> {
    let uri = params.text_document.uri;
    match snapshot_query(
        context.analysis(),
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        cancel_token,
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            let document = analysis.salsa.document(file_id)?;
            let range = document.to_rowan_range(params.range)?;
            let color = params.color;
            let text = document.get_text_slice(range);
            let color_text = convert_color_to_hex(color, text.len());
            let color_presentations = vec![ColorPresentation {
                label: text.to_string(),
                text_edit: Some(TextEdit {
                    range: params.range,
                    new_text: color_text,
                }),
                additional_text_edits: None,
            }];
            Some(color_presentations)
        },
    )
    .await
    {
        RequestOutcome::Ready(presentations) => RequestOutcome::Ready(presentations),
        RequestOutcome::Cancelled(source) => RequestOutcome::Cancelled(source),
        RequestOutcome::Missing => RequestOutcome::Ready(Vec::new()),
    }
}

pub struct DocumentColorCapabilities;

impl RegisterCapabilities for DocumentColorCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.color_provider = Some(ColorProviderCapability::Simple(true));
    }
}
