mod tag;
mod test;
mod types;

use tag::{parse_long_tag, parse_tag};

use crate::{
    kind::{LuaSyntaxKind, LuaTokenKind},
    lexer::LuaDocLexerState,
    parser::{LuaDocParser, MarkerEventContainer},
    parser_error::LuaParseError,
};

pub fn parse_comment(p: &mut LuaDocParser) {
    let m = p.mark(LuaSyntaxKind::Comment);

    parse_docs(p);

    m.complete(p);
}

fn parse_docs(p: &mut LuaDocParser) {
    while p.current_token() != LuaTokenKind::TkEof {
        match p.current_token() {
            LuaTokenKind::TkDocStart => {
                p.set_state(LuaDocLexerState::Tag);
                p.bump();
                parse_tag(p);
            }
            LuaTokenKind::TkDocLongStart => {
                p.set_state(LuaDocLexerState::Tag);
                p.bump();
                parse_long_tag(p);
            }
            LuaTokenKind::TkNormalStart => {
                p.set_state(LuaDocLexerState::NormalDescription);
                p.bump();

                if matches!(
                    p.current_token(),
                    LuaTokenKind::TkDocRegion | LuaTokenKind::TkDocEndRegion
                ) {
                    p.bump();
                }

                parse_normal_description(p);
            }
            LuaTokenKind::TkLongCommentStart => {
                p.set_state(LuaDocLexerState::LongDescription);
                p.bump();

                parse_description(p);
            }
            LuaTokenKind::TKDocTriviaStart => {
                p.bump();
            }
            _ => {
                p.bump();
            }
        }

        if let Some(reader) = &p.lexer.reader {
            if !reader.is_eof()
                && !matches!(
                    p.current_token(),
                    LuaTokenKind::TkDocStart | LuaTokenKind::TkDocLongStart
                )
            {
                p.bump_to_end();
                continue;
            }
        }

        p.set_state(LuaDocLexerState::Init);
    }
}

fn parse_description(p: &mut LuaDocParser) {
    let m = p.mark(LuaSyntaxKind::DocDescription);

    loop {
        match p.current_token() {
            LuaTokenKind::TkDocDetail
            | LuaTokenKind::TkEndOfLine
            | LuaTokenKind::TkWhitespace
            | LuaTokenKind::TkDocContinue
            | LuaTokenKind::TkNormalStart => {
                p.bump();
            }
            _ => {
                break;
            }
        }
    }

    m.complete(p);
}

fn expect_token(p: &mut LuaDocParser, token: LuaTokenKind) -> Result<(), LuaParseError> {
    if p.current_token() == token {
        p.bump();
        Ok(())
    } else {
        Err(LuaParseError::syntax_error_from(
            &t!(
                "expected %{token}, but get %{current}",
                token = token,
                current = p.current_token()
            ),
            p.current_token_range(),
        ))
    }
}

fn if_token_bump(p: &mut LuaDocParser, token: LuaTokenKind) -> bool {
    if p.current_token() == token {
        p.bump();
        true
    } else {
        false
    }
}

fn parse_normal_description(p: &mut LuaDocParser) {
    let m = p.mark(LuaSyntaxKind::DocDescription);

    loop {
        match p.current_token() {
            LuaTokenKind::TkDocDetail
            | LuaTokenKind::TkEndOfLine
            | LuaTokenKind::TkWhitespace
            | LuaTokenKind::TkDocContinue
            | LuaTokenKind::TkNormalStart => {
                p.bump();
            }
            _ => {
                break;
            }
        }
    }

    m.complete(p);
}
