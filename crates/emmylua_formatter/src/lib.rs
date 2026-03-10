pub mod cmd_args;
pub mod config;
mod formatter;
pub mod ir;
mod printer;
mod test;

use emmylua_parser::{LuaParser, ParserConfig};
use formatter::FormatContext;
use printer::Printer;

pub use config::LuaFormatConfig;

pub fn reformat_lua_code(code: &str, config: &LuaFormatConfig) -> String {
    let tree = LuaParser::parse(code, ParserConfig::default());

    let ctx = FormatContext::new(config);
    let chunk = tree.get_chunk_node();
    let ir = formatter::format_chunk(&ctx, &chunk);

    Printer::new(config).print(&ir)
}
