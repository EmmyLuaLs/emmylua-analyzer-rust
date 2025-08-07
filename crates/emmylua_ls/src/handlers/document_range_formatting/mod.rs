use emmylua_code_analysis::{FormattingOptions, range_format_code};
use lsp_types::{
    ClientCapabilities, DocumentRangeFormattingParams, OneOf, Position, Range, ServerCapabilities,
    TextEdit,
};
use tokio_util::sync::CancellationToken;

use crate::context::ServerContextSnapshot;

use super::RegisterCapabilities;

pub async fn on_range_formatting_handler(
    context: ServerContextSnapshot,
    params: DocumentRangeFormattingParams,
    _: CancellationToken,
) -> Option<Vec<TextEdit>> {
    let uri = params.text_document.uri;
    let request_range = params.range;
    let analysis = context.analysis.read().await;
    let config_manager = context.workspace_manager.read().await;
    let client_id = config_manager.client_config.client_id;
    let file_id = analysis.get_file_id(&uri)?;
    let syntax_tree = analysis
        .compilation
        .get_db()
        .get_vfs()
        .get_syntax_tree(&file_id)?;

    if syntax_tree.has_syntax_errors() {
        return None;
    }
    let emmyrc = analysis.get_emmyrc();
    let document = analysis
        .compilation
        .get_db()
        .get_vfs()
        .get_document(&file_id)?;
    let text = document.get_text();
    let file_path = document.get_file_path();
    let normalized_path = file_path.to_string_lossy().to_string().replace("\\", "/");
    let formatting_options = FormattingOptions {
        indent_size: params.options.tab_size,
        use_tabs: !params.options.insert_spaces,
        insert_final_newline: params.options.insert_final_newline.unwrap_or(true),
        non_standard_symbol: !emmyrc.runtime.nonstandard_symbol.is_empty(),
    };
    let formatted_result = range_format_code(
        text,
        &normalized_path,
        request_range.start.line as i32,
        0,
        request_range.end.line as i32 + 1,
        0,
        formatting_options,
    )?;

    let mut formatted_text = formatted_result.text;

    if client_id.is_intellij() || client_id.is_other() {
        formatted_text = formatted_text.replace("\r\n", "\n");
    }

    let text_edit = TextEdit {
        range: Range {
            start: Position {
                line: formatted_result.start_line as u32,
                character: formatted_result.start_col as u32,
            },
            end: Position {
                line: formatted_result.end_line as u32 + 1,
                character: 0,
            },
        },
        new_text: formatted_text,
    };

    Some(vec![text_edit])
}

pub struct DocumentRangeFormatting;

impl RegisterCapabilities for DocumentRangeFormatting {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.document_range_formatting_provider = Some(OneOf::Left(true));
    }
}
