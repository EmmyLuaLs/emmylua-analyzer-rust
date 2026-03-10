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
    // Preserve shebang line (e.g. #!/usr/bin/lua)
    let (shebang, lua_code) = if code.starts_with("#!") {
        match code.find('\n') {
            Some(pos) => (&code[..=pos], &code[pos + 1..]),
            None => (code, ""),
        }
    } else {
        ("", code)
    };

    let tree = LuaParser::parse(lua_code, ParserConfig::default());

    let ctx = FormatContext::new(config);
    let chunk = tree.get_chunk_node();
    let ir = formatter::format_chunk(&ctx, &chunk);

    let output = Printer::new(config).print(&ir);

    if shebang.is_empty() {
        output
    } else {
        format!("{}{}", shebang, output)
    }
}
