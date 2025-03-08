mod build_color;

use build_color::{build_colors, convert_color_to_hex};
use emmylua_parser::LuaAstNode;
use lsp_types::{
    ClientCapabilities, ColorInformation, ColorPresentation, ColorPresentationParams,
    ColorProviderCapability, DocumentColorParams, ServerCapabilities, TextEdit,
};
use tokio_util::sync::CancellationToken;

use crate::context::ServerContextSnapshot;

pub async fn on_document_color(
    context: ServerContextSnapshot,
    params: DocumentColorParams,
    _: CancellationToken,
) -> Vec<ColorInformation> {
    let uri = params.text_document.uri;
    let analysis = context.analysis.read().await;
    let file_id = if let Some(file_id) = analysis.get_file_id(&uri) {
        file_id
    } else {
        return vec![];
    };

    let semantic_model =
        if let Some(semantic_model) = analysis.compilation.get_semantic_model(file_id) {
            semantic_model
        } else {
            return vec![];
        };

    if !semantic_model.get_emmyrc().document_color.enable {
        return vec![];
    }

    let document = semantic_model.get_document();
    let root = semantic_model.get_root();
    build_colors(root.syntax().clone(), &document)
}

pub async fn on_document_color_presentation(
    context: ServerContextSnapshot,
    params: ColorPresentationParams,
    _: CancellationToken,
) -> Vec<ColorPresentation> {
    let uri = params.text_document.uri;
    let analysis = context.analysis.read().await;
    let file_id = if let Some(file_id) = analysis.get_file_id(&uri) {
        file_id
    } else {
        return vec![];
    };

    let semantic_model =
        if let Some(semantic_model) = analysis.compilation.get_semantic_model(file_id) {
            semantic_model
        } else {
            return vec![];
        };
    let document = semantic_model.get_document();

    let range = if let Some(range) = document.to_rowan_range(params.range.clone()) {
        range
    } else {
        return vec![];
    };
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

    color_presentations
}

pub fn register_capabilities(
    server_capabilities: &mut ServerCapabilities,
    _: &ClientCapabilities,
) -> Option<()> {
    server_capabilities.color_provider = Some(ColorProviderCapability::Simple(true));
    Some(())
}
