use crate::meta_text::meta_keyword;
use emmylua_parser::{LuaSyntaxToken, LuaTokenKind};

pub fn is_keyword(token: LuaSyntaxToken) -> bool {
    match token.kind().into() {
        LuaTokenKind::TkLocal => true,
        LuaTokenKind::TkFunction => true,
        LuaTokenKind::TkEnd => true,
        LuaTokenKind::TkIf => true,
        LuaTokenKind::TkThen => true,
        LuaTokenKind::TkElse => true,
        LuaTokenKind::TkElseIf => true,
        LuaTokenKind::TkWhile => true,
        LuaTokenKind::TkDo => true,
        LuaTokenKind::TkFor => true,
        LuaTokenKind::TkIn => true,
        LuaTokenKind::TkRepeat => true,
        LuaTokenKind::TkUntil => true,
        LuaTokenKind::TkReturn => true,
        LuaTokenKind::TkBreak => true,
        LuaTokenKind::TkGoto => true,
        _ => false,
    }
}

// todo add usage
pub fn hover_keyword(token: LuaSyntaxToken) -> String {
    match token.kind().into() {
        LuaTokenKind::TkLocal => meta_keyword("local"),
        LuaTokenKind::TkFunction => meta_keyword("function"),
        LuaTokenKind::TkEnd => meta_keyword("end"),
        LuaTokenKind::TkIf => meta_keyword("if"),
        LuaTokenKind::TkThen => meta_keyword("then"),
        LuaTokenKind::TkElse => meta_keyword("else"),
        LuaTokenKind::TkElseIf => meta_keyword("elseif"),
        LuaTokenKind::TkWhile => meta_keyword("while"),
        LuaTokenKind::TkDo => meta_keyword("do"),
        LuaTokenKind::TkFor => meta_keyword("for"),
        LuaTokenKind::TkIn => meta_keyword("in"),
        LuaTokenKind::TkRepeat => meta_keyword("repeat"),
        LuaTokenKind::TkUntil => meta_keyword("until"),
        LuaTokenKind::TkReturn => meta_keyword("return"),
        LuaTokenKind::TkBreak => meta_keyword("break"),
        LuaTokenKind::TkGoto => meta_keyword("goto"),
        _ => "".to_string(),
    }
}
