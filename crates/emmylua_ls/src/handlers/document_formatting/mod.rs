use emmylua_code_analysis::reformat_code;
use lsp_types::{
    ClientCapabilities, DocumentFormattingParams, OneOf, ServerCapabilities, TextEdit,
};
use tokio_util::sync::CancellationToken;

use crate::context::ServerContextSnapshot;

pub async fn on_formatting_handler(
    context: ServerContextSnapshot,
    params: DocumentFormattingParams,
    _: CancellationToken,
) -> Option<Vec<TextEdit>> {
    let uri = params.text_document.uri;
    let analysis = context.analysis.read().await;
    let config_manager = context.workspace_manager.read().await;
    let client_id = config_manager.client_config.client_id;

    let file_id = analysis.get_file_id(&uri)?;
    let semantic_model = analysis.compilation.get_semantic_model(file_id)?;
    let document = semantic_model.get_document();
    let text = document.get_text();
    let file_path = document.get_file_path();
    let normalized_path = file_path.to_string_lossy().to_string().replace("\\", "/");
    let mut formatted_text = reformat_code(text, &normalized_path);
    if client_id.is_intellij() || client_id.is_other() {
        formatted_text = formatted_text.replace("\r\n", "\n");
    }

    let document_range = document.get_document_lsp_range();
    let text_edit = TextEdit {
        range: document_range,
        new_text: formatted_text,
    };

    Some(vec![text_edit])
}

pub fn register_capabilities(
    server_capabilities: &mut ServerCapabilities,
    _: &ClientCapabilities,
) -> Option<()> {
    server_capabilities.document_formatting_provider = Some(OneOf::Left(true));

    Some(())
}
