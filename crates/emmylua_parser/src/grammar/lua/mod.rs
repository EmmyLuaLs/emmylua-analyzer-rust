use stat::parse_stats;

use crate::{
    kind::{LuaSyntaxKind, LuaTokenKind},
    parser::{LuaParser, MarkerEventContainer},
    parser_error::LuaParseError,
};

use super::ParseResult;

mod expr;
mod stat;
mod test;

pub fn parse_chunk(p: &mut LuaParser) {
    let m = p.mark(LuaSyntaxKind::Block);

    p.init();
    while p.current_token() != LuaTokenKind::TkEof {
        let consume_count = p.current_token_index();
        parse_stats(p);

        if p.current_token_index() == consume_count {
            let m = p.mark(LuaSyntaxKind::UnknownStat);
            p.bump();
            p.push_error(LuaParseError::from_source_range(
                &t!("unexpected token"),
                p.current_token_range(),
            ));

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

fn expect_token(p: &mut LuaParser, token: LuaTokenKind) -> Result<(), LuaParseError> {
    if p.current_token() == token {
        p.bump();
        Ok(())
    } else {
        Err(LuaParseError::from_source_range(
            &t!(
                "expected %{token}, but get %{current}",
                token = token,
                current = p.current_token()
            ),
            p.current_token_range(),
        ))
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
