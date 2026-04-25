mod external_range_format;

use emmylua_code_analysis::LuaDocument;
use lsp_types::{
    ClientCapabilities, DocumentRangeFormattingParams, OneOf, Position, Range, ServerCapabilities,
    TextEdit,
};
use tokio_util::sync::CancellationToken;

use crate::{
    context::ServerContextSnapshot,
    handlers::{
        document_formatting::{FormattingOptions, format_diff, format_with_workspace_formatter},
        document_range_formatting::external_range_format::external_tool_range_format,
    },
};

use super::RegisterCapabilities;

pub struct RangeFormatResult {
    pub text: String,
    pub start_line: i32,
    pub start_col: i32,
    pub end_line: i32,
    pub end_col: i32,
}

pub async fn on_range_formatting_handler(
    context: ServerContextSnapshot,
    params: DocumentRangeFormattingParams,
    _: CancellationToken,
) -> Option<Vec<TextEdit>> {
    let uri = params.text_document.uri;
    let request_range = params.range;
    let analysis = context.analysis().read().await;
    let workspace_manager = context.workspace_manager().read().await;
    let client_id = workspace_manager.client_config.client_id;
    let file_id = analysis.get_file_id(&uri)?;
    let emmyrc = analysis.get_emmyrc();
    let document = analysis
        .compilation
        .get_db()
        .get_vfs()
        .get_document(&file_id)?;
    let file_path = document.get_file_path();
    let normalized_path = file_path.to_string_lossy().to_string().replace("\\", "/");
    let formatting_options = FormattingOptions {
        indent_size: params.options.tab_size,
        use_tabs: !params.options.insert_spaces,
        insert_final_newline: params.options.insert_final_newline.unwrap_or(true),
        non_standard_symbol: !emmyrc.runtime.nonstandard_symbol.is_empty(),
    };
    let formatted_result = if let Some(external_tool) = &emmyrc.format.external_tool_range_format {
        external_tool_range_format(
            external_tool,
            &document,
            &request_range,
            &normalized_path,
            formatting_options,
        )
        .await?
    } else {
        let formatted_text = format_with_workspace_formatter(
            document.get_text(),
            Some(file_path.as_path()),
            &emmyrc,
            params.options.tab_size as usize,
            params.options.insert_spaces,
            params.options.insert_final_newline.unwrap_or(true),
        );

        return Some(build_range_edits(
            &document,
            &request_range,
            document.get_text(),
            &formatted_text,
            client_id.is_intellij() || client_id.is_other(),
        ));
    };

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
                line: formatted_result.end_line as u32,
                character: formatted_result.end_col as u32,
            },
        },
        new_text: formatted_text,
    };

    Some(vec![text_edit])
}

fn build_range_edits(
    document: &LuaDocument<'_>,
    request_range: &Range,
    source_text: &str,
    formatted_text: &str,
    normalize_newlines: bool,
) -> Vec<TextEdit> {
    let full_edits = format_diff(source_text, formatted_text, document, usize::MAX / 2);

    let filtered = full_edits
        .into_iter()
        .filter(|edit| edit_intersects_requested_lines(&edit.range, request_range))
        .map(|mut edit| {
            if normalize_newlines {
                edit.new_text = edit.new_text.replace("\r\n", "\n");
            }
            edit
        })
        .collect();

    merge_replace_edits(filtered)
}

fn edit_intersects_requested_lines(edit_range: &Range, request_range: &Range) -> bool {
    let request_start = request_range.start.line;
    let request_end = request_range.end.line;

    if edit_range.start == edit_range.end {
        let line = edit_range.start.line;
        return line >= request_start && line <= request_end;
    }

    let edit_start = edit_range.start.line;
    let edit_end_exclusive = if edit_range.end.character == 0 {
        edit_range.end.line
    } else {
        edit_range.end.line.saturating_add(1)
    };

    let request_end_exclusive = if request_range.end.character == 0 {
        request_end
    } else {
        request_end.saturating_add(1)
    };

    edit_start < request_end_exclusive && request_start < edit_end_exclusive
}

fn merge_replace_edits(edits: Vec<TextEdit>) -> Vec<TextEdit> {
    let mut merged = Vec::with_capacity(edits.len());
    let mut index = 0;

    while index < edits.len() {
        if index + 1 < edits.len() {
            let current = &edits[index];
            let next = &edits[index + 1];

            if current.new_text.is_empty()
                && next.range.start == next.range.end
                && current.range.start == next.range.start
            {
                merged.push(TextEdit {
                    range: current.range,
                    new_text: next.new_text.clone(),
                });
                index += 2;
                continue;
            }
        }

        merged.push(edits[index].clone());
        index += 1;
    }

    merged
}

pub struct DocumentRangeFormattingCapabilities;

impl RegisterCapabilities for DocumentRangeFormattingCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.document_range_formatting_provider = Some(OneOf::Left(true));
    }
}

#[cfg(test)]
mod tests {
    use super::build_range_edits;
    use emmylua_code_analysis::{Emmyrc, Vfs, VirtualUrlGenerator};
    use lsp_types::{Position, Range};

    fn create_document<'a>(vfs: &'a mut Vfs, text: &str) -> emmylua_code_analysis::LuaDocument<'a> {
        vfs.update_config(Emmyrc::default().into());
        let vg = VirtualUrlGenerator::new();
        let uri = vg.new_uri("range.lua");
        let id = vfs.set_file_content(&uri, Some(text.to_string()));
        vfs.get_document(&id).unwrap()
    }

    #[test]
    fn range_edits_only_keep_overlapping_lines() {
        let source = "local a=1\nlocal b=2\n";
        let formatted = "local a = 1\nlocal b = 2\n";
        let mut vfs = Vfs::new();
        let document = create_document(&mut vfs, source);

        let edits = build_range_edits(
            &document,
            &Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 999,
                },
            },
            source,
            formatted,
            false,
        );

        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range.start.line, 0);
        assert_eq!(edits[0].new_text, "local a = 1\n");
    }
}
