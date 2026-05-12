use crate::{LuaFormatConfig, SourceText, reformat_range, reformat_range_in_chunk};
use emmylua_parser::{LuaLanguageLevel, LuaParser, ParserConfig};
use rowan::{TextRange, TextSize};

fn apply_range_edit(source: &str, range: TextRange, replacement: &str) -> String {
    let mut text = source.to_string();
    text.replace_range(
        usize::from(range.start())..usize::from(range.end()),
        replacement,
    );
    text
}

#[test]
fn test_reformat_range_expands_to_enclosing_statement_lines() {
    let source = "local function func1()\n    local a = { { a = 1, aa = 2, aaa = 3, aaaa = 4, aaaaa = 5, aaaaaa = 6, aaaaaaa = 7, aaaaaaaaa = 8, aaaaaaaaaa = 9, aaaaaaaaaaa = 10 } }\n    local b\nend\n";
    let selection_start = source.find("aa = 2").expect("selection start should exist");
    let selection_end = selection_start + "aa = 2".len();
    let selection = TextRange::new(
        TextSize::new(selection_start as u32),
        TextSize::new(selection_end as u32),
    );

    let output = reformat_range(
        &SourceText {
            text: source,
            level: LuaLanguageLevel::Lua54,
        },
        selection,
        &LuaFormatConfig::default(),
    )
    .expect("range format should succeed");

    let expected_fragment = "{ a = 1, aa = 2, aaa = 3, aaaa = 4, aaaaa = 5, aaaaaa = 6, aaaaaaa = 7, aaaaaaaaa = 8, aaaaaaaaaa = 9, aaaaaaaaaaa = 10 }";
    let expected_start = source
        .find(expected_fragment)
        .expect("table expr should start") as u32;
    let expected_end = expected_start + expected_fragment.len() as u32;
    assert_eq!(output.replace_range.start(), TextSize::new(expected_start));
    assert_eq!(output.replace_range.end(), TextSize::new(expected_end));
    assert!(!output.text.contains("local function func1"));
    let replaced = apply_range_edit(source, output.replace_range, &output.text);
    assert!(replaced.contains("aaaaaaaaaaa = 10"));
    assert!(replaced.contains("    local b\n"));
}

#[test]
fn test_reformat_range_can_replace_top_level_statement_selection() {
    let source = "local function func1()\n    local a = { { a = 1, aa = 2, aaa = 3, aaaa = 4, aaaaa = 5, aaaaaa = 6, aaaaaaa = 7, aaaaaaaaa = 8, aaaaaaaaaa = 9, aaaaaaaaaaa = 10 } }\n    local b\nend\n";
    let selection = TextRange::new(TextSize::new(0), TextSize::new(source.len() as u32));

    let output = reformat_range(
        &SourceText {
            text: source,
            level: LuaLanguageLevel::Lua54,
        },
        selection,
        &LuaFormatConfig::default(),
    )
    .expect("range format should succeed");

    let replaced = apply_range_edit(source, output.replace_range, &output.text);
    assert_eq!(output.replace_range, selection);
    assert!(replaced.contains("            aaaaaaaaaaa = 10"));
    assert!(replaced.ends_with("end\n"));
}

#[test]
fn test_reformat_range_in_chunk_matches_text_entrypoint() {
    let source = "local function func1()\n    local a = { { a = 1, aa = 2, aaa = 3, aaaa = 4, aaaaa = 5, aaaaaa = 6, aaaaaaa = 7, aaaaaaaaa = 8, aaaaaaaaaa = 9, aaaaaaaaaaa = 10 } }\n    local b\nend\n";
    let selection_start = source.find("aa = 2").expect("selection start should exist");
    let selection_end = selection_start + "aa = 2".len();
    let selection = TextRange::new(
        TextSize::new(selection_start as u32),
        TextSize::new(selection_end as u32),
    );

    let from_text = reformat_range(
        &SourceText {
            text: source,
            level: LuaLanguageLevel::Lua54,
        },
        selection,
        &LuaFormatConfig::default(),
    )
    .expect("text entrypoint should succeed");

    let tree = LuaParser::parse(source, ParserConfig::with_level(LuaLanguageLevel::Lua54));
    let from_chunk = reformat_range_in_chunk(
        source,
        &tree.get_chunk_node(),
        selection,
        &LuaFormatConfig::default(),
        LuaLanguageLevel::Lua54,
    )
    .expect("chunk entrypoint should succeed");

    assert_eq!(from_chunk, from_text);
}

#[test]
fn test_reformat_range_selection_inside_call_args_expands_to_statement() {
    let source = "local result = vim.deprecate(\n    'vim.lsp.codelens.refresh({ bufnr = bufnr})',\n    'vim.lsp.codelens.enable(true, { bufnr = bufnr })',\n    '0.13.0'\n)\n";
    let selection_start = source
        .find("enable(true")
        .expect("selection start should exist");
    let selection_end = selection_start + "enable(true".len();
    let selection = TextRange::new(
        TextSize::new(selection_start as u32),
        TextSize::new(selection_end as u32),
    );

    let output = reformat_range(
        &SourceText {
            text: source,
            level: LuaLanguageLevel::Lua54,
        },
        selection,
        &LuaFormatConfig::default(),
    )
    .expect("range format should succeed");

    let expected_start = source.find('(').expect("call arg list should start") as u32;
    let expected_end = source.rfind(')').expect("call arg list should end") as u32 + 1;
    assert_eq!(output.replace_range.start(), TextSize::new(expected_start));
    assert_eq!(output.replace_range.end(), TextSize::new(expected_end));
    assert!(!output.text.contains("local result = "));
    let replaced = apply_range_edit(source, output.replace_range, &output.text);
    assert!(replaced.contains("vim.deprecate("));
    assert!(replaced.contains("'0.13.0'"));
}

#[test]
fn test_reformat_range_selection_inside_param_list_expands_to_function() {
    let source = "local function shellify(cmd, opts, env)\n    return cmd\nend\n";
    let selection_start = source.find("opts").expect("selection start should exist");
    let selection_end = selection_start + "opts".len();
    let selection = TextRange::new(
        TextSize::new(selection_start as u32),
        TextSize::new(selection_end as u32),
    );

    let output = reformat_range(
        &SourceText {
            text: source,
            level: LuaLanguageLevel::Lua54,
        },
        selection,
        &LuaFormatConfig::default(),
    )
    .expect("range format should succeed");

    let expected_start = source.find("(cmd").expect("param list should start") as u32;
    let expected_end = expected_start + "(cmd, opts, env)".len() as u32;
    assert_eq!(output.replace_range.start(), TextSize::new(expected_start));
    assert_eq!(output.replace_range.end(), TextSize::new(expected_end));
    assert!(!output.text.contains("local function shellify"));
    let replaced = apply_range_edit(source, output.replace_range, &output.text);
    assert!(replaced.starts_with("local function shellify("));
    assert!(replaced.ends_with("end\n"));
}

#[test]
fn test_reformat_range_selection_inside_table_field_replaces_only_table_expr() {
    let source = "local value = { a = 1, aaaaaaaaa = 2, aaaaaaaaaa = 3, aaaaaaaaaaa = 4, aaaaaaaaaaaa = 5 }\nlocal after = 1\n";
    let selection_start = source
        .find("aaaaaaaaaa = 3")
        .expect("selection start should exist");
    let selection_end = selection_start + "aaaaaaaaaa = 3".len();
    let selection = TextRange::new(
        TextSize::new(selection_start as u32),
        TextSize::new(selection_end as u32),
    );

    let output = reformat_range(
        &SourceText {
            text: source,
            level: LuaLanguageLevel::Lua54,
        },
        selection,
        &LuaFormatConfig::default(),
    )
    .expect("range format should succeed");

    let expected_start = source.find('{').expect("table expr should start") as u32;
    let expected_end = source.find('}').expect("table expr should end") as u32 + 1;
    assert_eq!(output.replace_range.start(), TextSize::new(expected_start));
    assert_eq!(output.replace_range.end(), TextSize::new(expected_end));
    assert!(!output.text.contains("local value = "));

    let replaced = apply_range_edit(source, output.replace_range, &output.text);
    assert!(replaced.starts_with("local value = {"));
    assert!(replaced.contains("local after = 1\n"));
}

#[test]
fn test_reformat_range_recomputes_statement_indent_from_context() {
    let source = "local function func1()\n  local value = { a = 1, aaaaaaaaa = 2, aaaaaaaaaa = 3, aaaaaaaaaaa = 4, aaaaaaaaaaaa = 5 }\nend\n";
    let selection_start = source
        .find("local value")
        .expect("selection start should exist");
    let selection_end = selection_start + "local value".len();
    let selection = TextRange::new(
        TextSize::new(selection_start as u32),
        TextSize::new(selection_end as u32),
    );

    let mut config = LuaFormatConfig::default();
    config.layout.max_line_width = 40;

    let output = reformat_range(
        &SourceText {
            text: source,
            level: LuaLanguageLevel::Lua54,
        },
        selection,
        &config,
    )
    .expect("range format should succeed");

    let replaced = apply_range_edit(source, output.replace_range, &output.text);
    assert!(replaced.contains("\n    local value = {\n"));
    assert!(!replaced.contains("\n  local value = {\n"));
}
