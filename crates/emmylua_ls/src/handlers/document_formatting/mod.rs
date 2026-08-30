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

use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, snapshot_query};
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
    cancel_token: CancellationToken,
) -> RequestOutcome<Vec<TextEdit>> {
    let uri = params.text_document.uri;

    // Extract only the needed data while holding the lock, then immediately release
    // the workspace lock and analysis snapshot to avoid blocking other requests/writes
    // during external_tool_format(...).await.
    let client_id = {
        let workspace_manager = context.workspace_manager().lock().await;
        workspace_manager.client_config.client_id
    };
    let extracted = match snapshot_query(
        context.analysis(),
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        cancel_token,
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            let document = analysis.salsa.document(file_id)?;
            let file_path = document.path.clone();
            let normalized_path = file_path
                .as_deref()
                .map(|p| p.to_string_lossy().to_string().replace("\\", "/"))
                .unwrap_or_default();
            let emmyrc = analysis.get_emmyrc();
            let formatting_options = FormattingOptions {
                indent_size: params.options.tab_size,
                use_tabs: !params.options.insert_spaces,
                insert_final_newline: params.options.insert_final_newline.unwrap_or(true),
                non_standard_symbol: !emmyrc.runtime.nonstandard_symbol.is_empty(),
            };
            Some((
                document,
                emmyrc,
                file_path,
                normalized_path,
                formatting_options,
            ))
        },
    )
    .await
    {
        RequestOutcome::Ready(data) => data,
        RequestOutcome::Missing => {
            return RequestOutcome::Missing;
        }
        RequestOutcome::Cancelled(source) => return RequestOutcome::Cancelled(source),
    };

    let (document, emmyrc, file_path, normalized_path, formatting_options) = extracted;

    let text = document.get_text().to_string();
    let mut formatted_text = if let Some(external_config) = &emmyrc.format.external_tool {
        match external_tool_format(
            external_config,
            &text,
            &normalized_path,
            None,
            formatting_options,
        )
        .await
        {
            Some(formatted) => formatted,
            None => return RequestOutcome::Missing,
        }
    } else {
        format_with_workspace_formatter(
            &text,
            file_path.as_deref(),
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
        format_diff(&text, &formatted_text, &document, replace_all_limit)
    } else {
        let document_range = document.get_document_lsp_range();
        vec![TextEdit {
            range: document_range,
            new_text: formatted_text,
        }]
    };

    RequestOutcome::Ready(text_edits)
}

pub(crate) fn format_with_workspace_formatter(
    text: &str,
    source_path: Option<&Path>,
    emmyrc: &Emmyrc,
    tab_size: usize,
    insert_spaces: bool,
    insert_final_newline: bool,
) -> String {
    let config = build_workspace_formatter_config(
        source_path,
        tab_size,
        insert_spaces,
        insert_final_newline,
    );

    let mut node_cache = NodeCache::default();
    let tree = LuaParser::parse(text, emmyrc.get_parse_config(&mut node_cache));
    if tree.has_syntax_errors() {
        return text.to_string();
    }

    reformat_chunk(&tree.get_chunk_node(), &config)
}

pub(crate) fn build_workspace_formatter_config(
    source_path: Option<&Path>,
    tab_size: usize,
    insert_spaces: bool,
    insert_final_newline: bool,
) -> LuaFormatConfig {
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
    config
}

pub struct DocumentFormattingCapabilities;

impl RegisterCapabilities for DocumentFormattingCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.document_formatting_provider = Some(OneOf::Left(true));
    }
}
