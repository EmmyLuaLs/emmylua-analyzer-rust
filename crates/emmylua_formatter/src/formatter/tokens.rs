use emmylua_parser::LuaTokenKind;

use crate::ir::{self, DocIR};

pub fn tok(kind: LuaTokenKind) -> DocIR {
    ir::syntax_token(kind)
}

pub fn comma_space_sep() -> Vec<DocIR> {
    vec![tok(LuaTokenKind::TkComma), ir::space()]
}

pub fn comma_soft_line_sep() -> Vec<DocIR> {
    vec![tok(LuaTokenKind::TkComma), ir::soft_line()]
}
