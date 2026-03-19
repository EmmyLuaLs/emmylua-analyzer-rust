pub mod cmd_args;
pub mod config;
mod formatter;
pub mod ir;
mod printer;
mod test;
mod workspace;

use emmylua_parser::{LuaChunk, LuaLanguageLevel, LuaParser, ParserConfig};
use formatter::FormatContext;
use printer::Printer;

pub use config::{
    AlignConfig, CommentConfig, EmmyDocConfig, EndOfLine, ExpandStrategy, IndentConfig, IndentKind,
    LayoutConfig, LuaFormatConfig, OutputConfig, SpacingConfig, TrailingComma,
};
pub use workspace::{
    FileCollectorOptions, FormatOutput, FormatPathResult, FormatterError, ResolvedConfig,
    collect_lua_files, default_config_toml, discover_config_path, format_file, format_text,
    format_text_for_path, load_format_config, parse_format_config, resolve_config_for_path,
};

pub struct SourceText<'a> {
    pub text: &'a str,
    pub level: LuaLanguageLevel,
}

pub fn reformat_lua_code(source: &SourceText, config: &LuaFormatConfig) -> String {
    let tree = LuaParser::parse(source.text, ParserConfig::with_level(source.level));

    let ctx = FormatContext::new(config);
    let chunk = tree.get_chunk_node();
    let ir = formatter::format_chunk(&ctx, &chunk);

    Printer::new(config).print(&ir)
}

pub fn reformat_chunk(chunk: &LuaChunk, config: &LuaFormatConfig) -> String {
    let ctx = FormatContext::new(config);
    let ir = formatter::format_chunk(&ctx, chunk);

    Printer::new(config).print(&ir)
}
