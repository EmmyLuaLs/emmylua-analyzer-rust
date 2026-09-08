use crate::handlers::{
    document_formatting::{FormattingOptions, FormattingRange, external_tool_format},
    document_range_formatting::RangeFormatResult,
};
use emmylua_code_analysis::EmmyrcExternalTool;
use rowan::TextSize;

pub async fn external_tool_range_format(
    emmyrc_external_tool: &EmmyrcExternalTool,
    text: &str,
    start_offset: TextSize,
    end_offset: TextSize,
    line_count: usize,
    file_path: &str,
    options: FormattingOptions,
) -> Option<RangeFormatResult> {
    let formatting_range = FormattingRange {
        start_offset,
        end_offset,
        start_line: 0,
        end_line: line_count as u32,
    };

    let document_range = lsp_types::Range {
        start: lsp_types::Position {
            line: 0,
            character: 0,
        },
        end: lsp_types::Position {
            line: line_count as u32,
            character: 0,
        },
    };
    let formatted_text = external_tool_format(
        emmyrc_external_tool,
        text,
        file_path,
        Some(formatting_range),
        options,
    )
    .await?;

    Some(RangeFormatResult {
        text: formatted_text,
        start_line: document_range.start.line as i32,
        start_col: document_range.start.character as i32,
        end_line: document_range.end.line as i32,
        end_col: document_range.end.character as i32,
    })
}
