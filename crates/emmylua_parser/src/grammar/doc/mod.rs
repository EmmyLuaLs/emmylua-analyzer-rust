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
                p.set_lexer_state(LuaDocLexerState::Tag);
                p.bump();
                parse_tag(p);
            }
            LuaTokenKind::TkDocLongStart => {
                p.set_lexer_state(LuaDocLexerState::Tag);
                p.bump();
                parse_long_tag(p);
            }
            LuaTokenKind::TkNormalStart => {
                p.set_lexer_state(LuaDocLexerState::NormalDescription);
                let mut m = p.mark(LuaSyntaxKind::DocDescription);

                p.bump();

                if_token_bump(p, LuaTokenKind::TkWhitespace);

                if matches!(
                    p.current_token(),
                    LuaTokenKind::TkDocRegion | LuaTokenKind::TkDocEndRegion
                ) {
                    m.undo(p);
                    p.bump();
                    m = p.mark(LuaSyntaxKind::DocDescription);
                }

                while let LuaTokenKind::TkDocDetail
                | LuaTokenKind::TkEndOfLine
                | LuaTokenKind::TkWhitespace
                | LuaTokenKind::TkDocContinue
                | LuaTokenKind::TkNormalStart = p.current_token()
                {
                    p.bump();
                }

                m.complete(p);
            }
            LuaTokenKind::TkLongCommentStart => {
                p.set_lexer_state(LuaDocLexerState::LongDescription);
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

        // A tag that stops at its own line end (`---@field`) leaves the next line's
        // `---` description start current; it is parsed on the next loop, not skipped.
        if let Some(reader) = &p.lexer.reader
            && !reader.is_eof()
            && !matches!(
                p.current_token(),
                LuaTokenKind::TkDocStart
                    | LuaTokenKind::TkDocLongStart
                    | LuaTokenKind::TkNormalStart
            )
        {
            p.bump_to_end();
            continue;
        }

        p.set_lexer_state(LuaDocLexerState::Init);
    }
}

fn parse_description(p: &mut LuaDocParser) {
    let m = p.mark(LuaSyntaxKind::DocDescription);

    while let LuaTokenKind::TkDocDetail
    | LuaTokenKind::TkEndOfLine
    | LuaTokenKind::TkWhitespace
    | LuaTokenKind::TkDocContinue
    | LuaTokenKind::TkNormalStart = p.current_token()
    {
        p.bump();
    }

    m.complete(p);
}

/// Same-line description only: plain `---` lines that follow belong to the next tag
/// (`---@field` docs are written above the field, as in lua-language-server).
fn parse_line_description(p: &mut LuaDocParser) {
    let m = p.mark(LuaSyntaxKind::DocDescription);

    // `TkNormalStart` here is a same-line `-- text`; a new line ends at `TkEndOfLine` first.
    while let LuaTokenKind::TkDocDetail
    | LuaTokenKind::TkWhitespace
    | LuaTokenKind::TkDocContinue
    | LuaTokenKind::TkNormalStart = p.current_token()
    {
        p.bump();
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
