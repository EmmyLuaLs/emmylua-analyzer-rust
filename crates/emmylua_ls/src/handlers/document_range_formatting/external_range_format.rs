use crate::handlers::{
    document_formatting::{FormattingOptions, FormattingRange, external_tool_format},
    document_range_formatting::RangeFormatResult,
};
use emmylua_code_analysis::{EmmyrcExternalTool, LuaDocument};

pub async fn external_tool_range_format(
    emmyrc_external_tool: &EmmyrcExternalTool,
    document: &LuaDocument<'_>,
    range: &lsp_types::Range,
    file_path: &str,
    options: FormattingOptions,
) -> Option<RangeFormatResult> {
    let start_offset =
        document.get_offset(range.start.line as usize, range.start.character as usize)?;
    let end_offset = document.get_offset(range.end.line as usize, range.end.character as usize)?;

    let formatting_range = FormattingRange {
        start_offset,
        end_offset,
        start_line: range.start.line,
        end_line: range.end.line,
    };

    let text = document.get_text();
    let document_range = document.get_document_lsp_range();
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
