#![cfg(feature = "cli")]
pub mod cmd_args;
pub mod config;
pub mod diff;
mod formatter;
pub mod ir;
mod printer;
mod test;
mod workspace;

pub use config::{
    AlignConfig, CommentConfig, EmmyDocConfig, EndOfLine, ExpandStrategy, IndentConfig, IndentKind,
    LayoutConfig, LuaFormatConfig, LuaSyntaxLevel, OutputConfig, QuoteStyle, SingleArgCallParens,
    SpacingConfig, SyntaxConfig, TrailingComma, TrailingTableSeparator,
};
use emmylua_parser::{
    LuaAstNode, LuaChunk, LuaLanguageLevel, LuaParseError, LuaParseErrorKind, LuaParser,
    ParserConfig,
};
use formatter::FormatContext;
use printer::Printer;
pub use rowan::TextRange;
pub use workspace::{
    ChangedLineRange, FileCollectorOptions, FormatCheckPathResult, FormatCheckResult, FormatOutput,
    FormatPathResult, FormatterError, ResolvedConfig, check_file, check_text, check_text_for_path,
    collect_lua_files, default_config_toml, discover_config_path, format_file, format_text,
    format_text_for_path, load_format_config, parse_format_config, resolve_config_for_path,
};

pub use formatter::range_format::RangeFormatOutput;

pub struct SourceText<'a> {
    pub text: &'a str,
    pub level: LuaLanguageLevel,
}

/// A syntax error with a 1-based line/column position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaSyntaxErrorInfo {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

/// The result of reformatting, including any syntax error that prevented
/// formatting from running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReformatResult {
    pub formatted: String,
    /// Set when the source could not be parsed and was returned unchanged.
    pub syntax_error: Option<LuaSyntaxErrorInfo>,
}

pub fn reformat_lua_code(source: &SourceText, config: &LuaFormatConfig) -> String {
    reformat_lua_code_with_info(source, config).formatted
}

pub fn reformat_lua_code_with_info(
    source: &SourceText,
    config: &LuaFormatConfig,
) -> ReformatResult {
    let tree = LuaParser::parse(source.text, ParserConfig::with_level(source.level));
    let syntax_error = tree
        .get_errors()
        .iter()
        .find(|err| err.kind == LuaParseErrorKind::SyntaxError)
        .map(|err| syntax_error_info(source.text, err));

    if tree.has_syntax_errors() {
        return ReformatResult {
            formatted: source.text.to_string(),
            syntax_error,
        };
    }

    let ctx = FormatContext::new(config);
    let chunk = tree.get_chunk_node();
    let ir = formatter::format_chunk(&ctx, &chunk);
    let mut p = Printer::new(config);
    p = p.with_source_line_ending(source.text);
    let capacity = (source.text.len() as f64 * 1.2).ceil() as usize;
    p = p.with_capacity(capacity);
    ReformatResult {
        formatted: p.print(&ir),
        syntax_error,
    }
}

fn syntax_error_info(source: &str, error: &LuaParseError) -> LuaSyntaxErrorInfo {
    let start = (u32::from(error.range.start()) as usize).min(source.len());
    let prefix = &source[..start];
    let line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
    let after_newline = prefix
        .rfind('\n')
        .map(|index| &prefix[index + 1..])
        .unwrap_or(prefix);
    let column = after_newline.chars().count() + 1;
    LuaSyntaxErrorInfo {
        message: error.message.clone(),
        line,
        column,
    }
}

pub fn reformat_chunk(chunk: &LuaChunk, config: &LuaFormatConfig) -> String {
    let ctx = FormatContext::new(config);
    let ir = formatter::format_chunk(&ctx, chunk);
    let source = chunk.syntax().text().to_string();
    Printer::new(config)
        .with_source_line_ending(&source)
        .print(&ir)
}

pub fn reformat_range(
    source: &SourceText,
    selection: TextRange,
    config: &LuaFormatConfig,
) -> Option<RangeFormatOutput> {
    formatter::range_format::reformat_range(source, selection, config)
}

pub fn reformat_range_in_chunk(
    source_text: &str,
    chunk: &LuaChunk,
    selection: TextRange,
    config: &LuaFormatConfig,
    level: LuaLanguageLevel,
) -> Option<RangeFormatOutput> {
    formatter::range_format::reformat_range_in_chunk(source_text, chunk, selection, config, level)
}
