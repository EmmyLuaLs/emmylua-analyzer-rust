#![cfg(feature = "cli")]
pub mod cmd_args;
pub mod config;
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
use emmylua_parser::{LuaChunk, LuaLanguageLevel, LuaParser, ParserConfig};
use formatter::{FormatContext, format_chunk_with_profile};
use printer::Printer;
use std::time::Instant;
pub use workspace::{
    ChangedLineRange, FileCollectorOptions, FormatCheckPathResult, FormatCheckResult, FormatOutput,
    FormatPathResult, FormatterError, ResolvedConfig, check_file, check_text, check_text_for_path,
    collect_lua_files, default_config_toml, discover_config_path, format_file, format_text,
    format_text_for_path, load_format_config, parse_format_config, resolve_config_for_path,
};

pub struct SourceText<'a> {
    pub text: &'a str,
    pub level: LuaLanguageLevel,
}

pub fn reformat_lua_code(source: &SourceText, config: &LuaFormatConfig) -> String {
    let profile_enabled = std::env::var_os("EMMYLUA_FORMATTER_PROFILE").is_some();
    let parse_start = Instant::now();
    let tree = LuaParser::parse(source.text, ParserConfig::with_level(source.level));
    let parse = parse_start.elapsed();
    if tree.has_syntax_errors() {
        return source.text.to_string();
    }

    let ctx = FormatContext::new(config);
    let chunk = tree.get_chunk_node();
    let (ir, phase_profile, render_hotspots) = format_chunk_with_profile(&ctx, &chunk);
    let mut p = Printer::new(config);
    let capacity = (source.text.len() as f64 * 1.2).ceil() as usize;
    p = p.with_capacity(capacity);
    let print_start = Instant::now();
    let result = p.print(&ir);
    let print = print_start.elapsed();

    if profile_enabled {
        eprintln!(
            "formatter-profile parse_ms={:.3} print_ms={:.3}",
            parse.as_secs_f64() * 1000.0,
            print.as_secs_f64() * 1000.0,
        );
        eprintln!("{}", phase_profile.summary());
        eprintln!("{}", render_hotspots.summary());
    }

    result
}

pub fn reformat_chunk(chunk: &LuaChunk, config: &LuaFormatConfig) -> String {
    let ctx = FormatContext::new(config);
    let ir = formatter::format_chunk(&ctx, chunk);

    Printer::new(config).print(&ir)
}
