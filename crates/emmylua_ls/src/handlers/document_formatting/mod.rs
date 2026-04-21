mod external_format;
mod format_diff;

use std::path::Path;

use emmylua_code_analysis::Emmyrc;
use emmylua_formatter::{IndentKind, LuaFormatConfig, reformat_chunk, resolve_config_for_path};
use emmylua_parser::LuaParser;
use lsp_types::{
    ClientCapabilities, DocumentFormattingParams, OneOf, ServerCapabilities, TextEdit,
};
use rowan::NodeCache;
use tokio_util::sync::CancellationToken;

use crate::context::ServerContextSnapshot;
pub use external_format::{FormattingRange, external_tool_format};
pub(crate) use format_diff::format_diff;

use super::RegisterCapabilities;

pub struct FormattingOptions {
    pub indent_size: u32,
    pub use_tabs: bool,
    pub insert_final_newline: bool,
    pub non_standard_symbol: bool,
}

pub async fn on_formatting_handler(
    context: ServerContextSnapshot,
    params: DocumentFormattingParams,
    _: CancellationToken,
) -> Option<Vec<TextEdit>> {
    let uri = params.text_document.uri;
    let analysis = context.analysis().read().await;
    let workspace_manager = context.workspace_manager().read().await;
    let client_id = workspace_manager.client_config.client_id;
    let emmyrc = analysis.get_emmyrc();

    let file_id = analysis.get_file_id(&uri)?;
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

    let mut formatted_text = if let Some(external_config) = &emmyrc.format.external_tool {
        external_tool_format(
            external_config,
            text,
            &normalized_path,
            None,
            formatting_options,
        )
        .await?
    } else {
        format_with_workspace_formatter(
            text,
            Some(file_path.as_path()),
            &emmyrc,
            params.options.tab_size as usize,
            params.options.insert_spaces,
            params.options.insert_final_newline.unwrap_or(true),
        )
    };

    if client_id.is_intellij() || client_id.is_other() {
        formatted_text = formatted_text.replace("\r\n", "\n");
    }

    let replace_all_limit = 50;
    let text_edits = if emmyrc.format.use_diff {
        format_diff(text, &formatted_text, &document, replace_all_limit)
    } else {
        let document_range = document.get_document_lsp_range();
        vec![TextEdit {
            range: document_range,
            new_text: formatted_text,
        }]
    };

    Some(text_edits)
}

pub(crate) fn format_with_workspace_formatter(
    text: &str,
    source_path: Option<&Path>,
    emmyrc: &Emmyrc,
    tab_size: usize,
    insert_spaces: bool,
    insert_final_newline: bool,
) -> String {
    let mut config = resolve_config_for_path(source_path, None)
        .map(|resolved| resolved.config)
        .unwrap_or_else(|_| LuaFormatConfig::default());
    config.indent.kind = if insert_spaces {
        IndentKind::Space
    } else {
        IndentKind::Tab
    };
    config.indent.width = tab_size.max(1);
    config.output.insert_final_newline = insert_final_newline;

    let mut node_cache = NodeCache::default();
    let tree = LuaParser::parse(text, emmyrc.get_parse_config(&mut node_cache));
    if tree.has_syntax_errors() {
        return text.to_string();
    }

    reformat_chunk(&tree.get_chunk_node(), &config)
}

pub struct DocumentFormattingCapabilities;

impl RegisterCapabilities for DocumentFormattingCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.document_formatting_provider = Some(OneOf::Left(true));
    }
}

#[cfg(test)]
mod tests {
    use super::format_with_workspace_formatter;
    use emmylua_code_analysis::Emmyrc;

    #[test]
    fn formats_constructor_source_doc_block() {
        let input = r#"-- ============================================================================
-- @source - Source file annotation
-- ============================================================================
---@[constructor("1123")]
---@source https://example.com/api/docs
---@param id number
---@return table
function externalAPI(id)
    return {}
end
"#;

        let formatted =
            format_with_workspace_formatter(input, None, &Emmyrc::default(), 4, true, true);

        assert_eq!(formatted, input);
    }
}
