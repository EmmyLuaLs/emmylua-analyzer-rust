mod external_range_format;

use emmylua_formatter::reformat_range_in_chunk;
use lsp_types::{
    ClientCapabilities, DocumentRangeFormattingParams, OneOf, Position, Range, ServerCapabilities,
    TextEdit,
};
use tokio_util::sync::CancellationToken;

use crate::{
    context::ServerContextSnapshot,
    handlers::{
        document_formatting::{FormattingOptions, build_workspace_formatter_config},
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
        let syntax_tree = analysis
            .compilation
            .get_db()
            .get_vfs()
            .get_syntax_tree(&file_id)?;
        let chunk = syntax_tree.get_chunk_node();
        let config = build_workspace_formatter_config(
            Some(file_path.as_path()),
            params.options.tab_size as usize,
            params.options.insert_spaces,
            params.options.insert_final_newline.unwrap_or(true),
        );
        let selection = document.to_rowan_range(request_range)?;
        let output = reformat_range_in_chunk(
            document.get_text(),
            &chunk,
            selection,
            &config,
            emmyrc.get_language_level(),
        )?;
        let mut new_text = output.text;
        if client_id.is_intellij() || client_id.is_other() {
            new_text = new_text.replace("\r\n", "\n");
        }

        return Some(vec![TextEdit {
            range: document.to_lsp_range(output.replace_range)?,
            new_text,
        }]);
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

pub struct DocumentRangeFormattingCapabilities;

impl RegisterCapabilities for DocumentRangeFormattingCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.document_range_formatting_provider = Some(OneOf::Left(true));
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Reverse;

    use emmylua_code_analysis::{Emmyrc, Vfs, VirtualUrlGenerator};
    use emmylua_formatter::reformat_range_in_chunk;
    use emmylua_parser::{LuaLanguageLevel, LuaParser, ParserConfig};
    use lsp_types::{Position, Range, TextEdit};

    use crate::handlers::document_formatting::build_workspace_formatter_config;

    fn create_document<'a>(vfs: &'a mut Vfs, text: &str) -> emmylua_code_analysis::LuaDocument<'a> {
        vfs.update_config(Emmyrc::default().into());
        let vg = VirtualUrlGenerator::new();
        let uri = vg.new_uri("range.lua");
        let id = vfs.set_file_content(&uri, Some(text.to_string()));
        vfs.get_document(&id).unwrap()
    }

    fn build_range_edit(
        document: &emmylua_code_analysis::LuaDocument<'_>,
        request_range: Range,
    ) -> Option<TextEdit> {
        let config = build_workspace_formatter_config(
            Some(document.get_file_path().as_path()),
            4,
            true,
            true,
        );
        let tree = LuaParser::parse(
            document.get_text(),
            ParserConfig::with_level(LuaLanguageLevel::Lua55),
        );
        let output = reformat_range_in_chunk(
            document.get_text(),
            &tree.get_chunk_node(),
            document.to_rowan_range(request_range)?,
            &config,
            LuaLanguageLevel::Lua55,
        )?;

        Some(TextEdit {
            range: document.to_lsp_range(output.replace_range)?,
            new_text: output.text,
        })
    }

    #[test]
    fn range_edit_expands_only_to_selected_statement_lines() {
        let source = "local a=1\nlocal b=2\n";
        let mut vfs = Vfs::new();
        let document = create_document(&mut vfs, source);
        let first_line_len = source
            .lines()
            .next()
            .expect("first line should exist")
            .len() as u32;

        let edit = build_range_edit(
            &document,
            Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: first_line_len,
                },
            },
        )
        .expect("range format should succeed");

        assert_eq!(edit.range.start.line, 0);
        assert_eq!(edit.range.end.line, 1);
        assert_eq!(edit.new_text, "local a = 1\n");
    }

    fn apply_text_edits(
        document: &emmylua_code_analysis::LuaDocument<'_>,
        source: &str,
        edits: &[TextEdit],
    ) -> String {
        let mut applied = source.to_string();
        let mut ranges: Vec<_> = edits
            .iter()
            .map(|edit| {
                let start = document
                    .get_offset(
                        edit.range.start.line as usize,
                        edit.range.start.character as usize,
                    )
                    .expect("edit start offset should exist");
                let end = document
                    .get_offset(
                        edit.range.end.line as usize,
                        edit.range.end.character as usize,
                    )
                    .expect("edit end offset should exist");
                (start, end, edit.new_text.as_str())
            })
            .collect();
        ranges.sort_by_key(|right| Reverse(right.0));

        for (start, end, new_text) in ranges {
            applied.replace_range(usize::from(start)..usize::from(end), new_text);
        }

        applied
    }

    #[test]
    fn range_edit_keeps_full_multiline_expansion_when_selecting_block() {
        let source = "local function func1()\n    local a = { { a = 1, aa = 2, aaa = 3, aaaa = 4, aaaaa = 5, aaaaaa = 6, aaaaaaa = 7, aaaaaaaaa = 8, aaaaaaaaaa = 9, aaaaaaaaaaa = 10 } }\n    local b\nend\n";
        let mut vfs = Vfs::new();
        let document = create_document(&mut vfs, source);
        let range = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 3,
                character: 3,
            },
        };

        let edit = build_range_edit(&document, range).expect("range format should succeed");
        let config = build_workspace_formatter_config(
            Some(document.get_file_path().as_path()),
            4,
            true,
            true,
        );
        let tree = LuaParser::parse(source, ParserConfig::with_level(LuaLanguageLevel::Lua55));
        let formatted = reformat_range_in_chunk(
            source,
            &tree.get_chunk_node(),
            document
                .to_rowan_range(range)
                .expect("full document range should convert"),
            &config,
            LuaLanguageLevel::Lua55,
        )
        .expect("formatter range output should exist")
        .text;
        let edits = vec![edit];
        let applied = apply_text_edits(&document, source, &edits);

        assert_eq!(applied, formatted);
        assert!(applied.contains("            aaaaaaaaaaa = 10"));
        assert!(applied.contains("    local b"));
    }
}
