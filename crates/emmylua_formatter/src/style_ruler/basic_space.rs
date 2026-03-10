use emmylua_parser::{LuaAstNode, LuaSyntaxId, LuaSyntaxKind, LuaSyntaxToken, LuaTokenKind};
use rowan::NodeOrToken;

use crate::{
    format::{LuaFormatter, TokenExpected},
    styles::LuaCodeStyle,
};

use super::StyleRuler;

pub struct BasicSpaceRuler;

impl StyleRuler for BasicSpaceRuler {
    fn apply_style(f: &mut LuaFormatter, _: &LuaCodeStyle) {
        let root = f.get_root();
        for node_or_token in root.syntax().descendants_with_tokens() {
            if let NodeOrToken::Token(token) = node_or_token {
                let syntax_id = LuaSyntaxId::from_token(&token);
                match token.kind().to_token() {
                    LuaTokenKind::TkLeftParen | LuaTokenKind::TkLeftBracket => {
                        if let Some(prev_token) = get_prev_sibling_token_without_space(&token) {
                            match prev_token.kind().to_token() {
                                LuaTokenKind::TkName
                                | LuaTokenKind::TkRightParen
                                | LuaTokenKind::TkRightBracket => {
                                    f.add_token_left_expected(syntax_id, TokenExpected::Space(0));
                                }
                                LuaTokenKind::TkString
                                | LuaTokenKind::TkRightBrace
                                | LuaTokenKind::TkLongString => {
                                    f.add_token_left_expected(syntax_id, TokenExpected::Space(1));
                                }
                                _ => {}
                            }
                        }

                        f.add_token_right_expected(syntax_id, TokenExpected::Space(0));
                    }
                    LuaTokenKind::TkRightBracket | LuaTokenKind::TkRightParen => {
                        f.add_token_left_expected(syntax_id, TokenExpected::Space(0));
                    }
                    LuaTokenKind::TkLeftBrace => {
                        f.add_token_right_expected(syntax_id, TokenExpected::Space(1));
                    }
                    LuaTokenKind::TkRightBrace => {
                        f.add_token_left_expected(syntax_id, TokenExpected::Space(1));
                    }
                    LuaTokenKind::TkComma => {
                        f.add_token_left_expected(syntax_id, TokenExpected::Space(0));
                        f.add_token_right_expected(syntax_id, TokenExpected::Space(1));
                    }
                    LuaTokenKind::TkPlus | LuaTokenKind::TkMinus => {
                        if is_parent_syntax(&token, LuaSyntaxKind::UnaryExpr) {
                            f.add_token_right_expected(syntax_id, TokenExpected::Space(0));
                            continue;
                        }

                        f.add_token_left_expected(syntax_id, TokenExpected::Space(1));
                        f.add_token_right_expected(syntax_id, TokenExpected::Space(1));
                    }
                    LuaTokenKind::TkLt => {
                        if is_parent_syntax(&token, LuaSyntaxKind::Attribute) {
                            f.add_token_left_expected(syntax_id, TokenExpected::Space(1));
                            f.add_token_right_expected(syntax_id, TokenExpected::Space(0));
                            continue;
                        }

                        f.add_token_left_expected(syntax_id, TokenExpected::Space(1));
                        f.add_token_right_expected(syntax_id, TokenExpected::Space(1));
                    }
                    LuaTokenKind::TkGt => {
                        if is_parent_syntax(&token, LuaSyntaxKind::Attribute) {
                            f.add_token_left_expected(syntax_id, TokenExpected::Space(0));
                            f.add_token_right_expected(syntax_id, TokenExpected::Space(1));
                            continue;
                        }

                        f.add_token_left_expected(syntax_id, TokenExpected::Space(1));
                        f.add_token_right_expected(syntax_id, TokenExpected::Space(1));
                    }
                    LuaTokenKind::TkMul
                    | LuaTokenKind::TkDiv
                    | LuaTokenKind::TkIDiv
                    | LuaTokenKind::TkMod
                    | LuaTokenKind::TkPow
                    | LuaTokenKind::TkConcat
                    | LuaTokenKind::TkAssign
                    | LuaTokenKind::TkBitAnd
                    | LuaTokenKind::TkBitOr
                    | LuaTokenKind::TkBitXor
                    | LuaTokenKind::TkEq
                    | LuaTokenKind::TkGe
                    | LuaTokenKind::TkLe
                    | LuaTokenKind::TkNe
                    | LuaTokenKind::TkAnd
                    | LuaTokenKind::TkOr
                    | LuaTokenKind::TkShl
                    | LuaTokenKind::TkShr => {
                        f.add_token_left_expected(syntax_id, TokenExpected::Space(1));
                        f.add_token_right_expected(syntax_id, TokenExpected::Space(1));
                    }
                    LuaTokenKind::TkColon => {
                        if is_parent_syntax(&token, LuaSyntaxKind::IndexExpr) {
                            f.add_token_left_expected(syntax_id, TokenExpected::Space(0));
                            f.add_token_right_expected(syntax_id, TokenExpected::Space(0));
                            continue;
                        }
                        f.add_token_left_expected(syntax_id, TokenExpected::MaxSpace(1));
                        f.add_token_right_expected(syntax_id, TokenExpected::MaxSpace(1));
                    }
                    LuaTokenKind::TkDot => {
                        f.add_token_left_expected(syntax_id, TokenExpected::Space(0));
                        f.add_token_right_expected(syntax_id, TokenExpected::Space(0));
                    }
                    LuaTokenKind::TkLocal
                    | LuaTokenKind::TkFunction
                    | LuaTokenKind::TkIf
                    | LuaTokenKind::TkWhile
                    | LuaTokenKind::TkFor
                    | LuaTokenKind::TkRepeat
                    | LuaTokenKind::TkReturn
                    | LuaTokenKind::TkDo
                    | LuaTokenKind::TkElseIf
                    | LuaTokenKind::TkElse
                    | LuaTokenKind::TkThen
                    | LuaTokenKind::TkUntil
                    | LuaTokenKind::TkIn
                    | LuaTokenKind::TkNot => {
                        f.add_token_left_expected(syntax_id, TokenExpected::Space(1));
                        f.add_token_right_expected(syntax_id, TokenExpected::Space(1));
                    }
                    _ => {}
                }
            }
        }
    }
}

fn is_parent_syntax(token: &LuaSyntaxToken, kind: LuaSyntaxKind) -> bool {
    if let Some(parent) = token.parent() {
        return parent.kind().to_syntax() == kind;
    }
    false
}

fn get_prev_sibling_token_without_space(token: &LuaSyntaxToken) -> Option<LuaSyntaxToken> {
    let mut current = token.clone();
    while let Some(prev) = current.prev_token() {
        if prev.kind().to_token() != LuaTokenKind::TkWhitespace {
            return Some(prev);
        }
        current = prev;
    }

    None
}
