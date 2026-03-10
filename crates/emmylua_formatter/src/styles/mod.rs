mod lua_indent;

pub use lua_indent::LuaIndent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LuaCodeStyle {
    /// The indentation style to use
    pub indent: LuaIndent,
    /// The maximum width of a line before wrapping
    pub max_line_width: usize,
}
