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
    let parse_elapsed = parse_start.elapsed();
    if tree.has_syntax_errors() {
        return source.text.to_string();
    }

    let ctx = FormatContext::new(config);
    let chunk = tree.get_chunk_node();
    if profile_enabled {
        let (ir, format_profile) = format_chunk_with_profile(&ctx, &chunk);
        let print_start = Instant::now();
        let (output, printer_profile) = Printer::new(config).print_with_profile(&ir);
        let print_elapsed = print_start.elapsed();

        eprintln!(
            "formatter-profile parse_ms={:.3} spacing_ms={:.3} layout_ms={:.3} render_ms={:.3} print_ms={:.3} fits_calls={} fits_nodes={} has_hard_line_calls={} has_hard_line_nodes={} print_fill_calls={} group_count={} align_group_count={} line_suffix_clones={} doc_count={}",
            parse_elapsed.as_secs_f64() * 1000.0,
            format_profile.spacing.as_secs_f64() * 1000.0,
            format_profile.layout.as_secs_f64() * 1000.0,
            format_profile.render.as_secs_f64() * 1000.0,
            print_elapsed.as_secs_f64() * 1000.0,
            printer_profile.fits_calls,
            printer_profile.fits_nodes_visited,
            printer_profile.has_hard_line_calls,
            printer_profile.has_hard_line_nodes_visited,
            printer_profile.print_fill_calls,
            printer_profile.group_count,
            printer_profile.align_group_count,
            printer_profile.line_suffix_clones,
            ir.len(),
        );
        eprintln!("{}", format_profile.sequence.summary());
        eprintln!("{}", format_profile.render_hotspots.summary());

        output
    } else {
        let ir = formatter::format_chunk(&ctx, &chunk);
        Printer::new(config).print(&ir)
    }
}

pub fn reformat_chunk(chunk: &LuaChunk, config: &LuaFormatConfig) -> String {
    let ctx = FormatContext::new(config);
    let ir = formatter::format_chunk(&ctx, chunk);

    Printer::new(config).print(&ir)
}
