mod expr;
mod stat;
mod test;

use stat::parse_stats;

use crate::{
    grammar::ParseFailReason,
    kind::{LuaSyntaxKind, LuaTokenKind},
    parser::{LuaParser, MarkerEventContainer},
    parser_error::LuaParseError,
};

use super::ParseResult;

pub fn parse_chunk(p: &mut LuaParser) {
    let m = p.mark(LuaSyntaxKind::Block);

    p.init();
    while p.current_token() != LuaTokenKind::TkEof {
        let consume_count = p.current_token_index();
        parse_stats(p);

        // Check if no token was consumed to prevent infinite loop
        if p.current_token_index() == consume_count {
            let error_range = p.current_token_range();
            let m = p.mark(LuaSyntaxKind::UnknownStat);

            // Provide more detailed error information
            let error_msg = match p.current_token() {
                LuaTokenKind::TkRightBrace => {
                    t!("unexpected '}' - missing opening '{{' or extra closing brace")
                }
                LuaTokenKind::TkRightParen => {
                    t!("unexpected ')' - missing opening '(' or extra closing parenthesis")
                }
                LuaTokenKind::TkRightBracket => {
                    t!("unexpected ']' - missing opening '[' or extra closing bracket")
                }
                LuaTokenKind::TkElse => {
                    t!("unexpected 'else' - missing corresponding 'if' statement")
                }
                LuaTokenKind::TkElseIf => {
                    t!("unexpected 'elseif' - missing corresponding 'if' statement")
                }
                LuaTokenKind::TkEnd => {
                    t!("unexpected 'end' - missing corresponding block statement")
                }
                LuaTokenKind::TkUntil => {
                    t!("unexpected 'until' - missing corresponding 'repeat' statement")
                }
                LuaTokenKind::TkThen => {
                    t!("unexpected 'then' - missing corresponding 'if' statement")
                }
                LuaTokenKind::TkDo => {
                    t!("unexpected 'do' - missing corresponding loop statement")
                }
                _ => {
                    t!(
                        "unexpected token '%{token}' - expected statement",
                        token = p.current_token()
                    )
                }
            };

            p.push_error(LuaParseError::syntax_error_from(&error_msg, error_range));

            p.bump(); // Consume current token to avoid infinite loop
            m.complete(p);
        }
    }

    m.complete(p);
}

fn parse_block(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::Block);

    parse_stats(p);

    Ok(m.complete(p))
}

fn expect_token(p: &mut LuaParser, token: LuaTokenKind) -> Result<(), ParseFailReason> {
    if p.current_token() == token {
        p.bump();
        Ok(())
    } else {
        if p.current_token() == LuaTokenKind::TkEof {
            return Err(ParseFailReason::Eof);
        }

        Err(ParseFailReason::UnexpectedToken)
    }
}

fn if_token_bump(p: &mut LuaParser, token: LuaTokenKind) -> bool {
    if p.current_token() == token {
        p.bump();
        true
    } else {
        false
    }
}

/// Check if a token is a statement start token
fn is_statement_start_token(token: LuaTokenKind) -> bool {
    matches!(
        token,
        LuaTokenKind::TkLocal
            | LuaTokenKind::TkFunction
            | LuaTokenKind::TkIf
            | LuaTokenKind::TkFor
            | LuaTokenKind::TkWhile
            | LuaTokenKind::TkDo
            | LuaTokenKind::TkName
            | LuaTokenKind::TkReturn
            | LuaTokenKind::TkBreak
            | LuaTokenKind::TkContinue
    )
}
