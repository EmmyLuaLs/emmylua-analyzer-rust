use emmylua_parser::{LexerConfig, LexerState, LuaLexer, LuaTokenData, LuaTokenKind, Reader};

use crate::{CodeBlockHighlightKind, DescItemKind, util::ResultContainer};

pub fn process_lua_code_block<'a, C: ResultContainer>(
    c: &mut C,
    line_reader: Reader<'a>,
    state: LexerState,
) -> LexerState {
    let mut lexer = LuaLexer::new_with_state(line_reader, state, LexerConfig::default(), None);
    let tokens = lexer.tokenize();
    for (i, token) in tokens.iter().enumerate() {
        if !matches!(
            token.kind,
            LuaTokenKind::TkEof | LuaTokenKind::TkEndOfLine | LuaTokenKind::TkWhitespace
        ) {
            let highlight_kind = to_highlight_kind(token, i, &tokens);
            if highlight_kind != CodeBlockHighlightKind::None {
                c.emit_range(token.range, DescItemKind::CodeBlockHl(highlight_kind));
            }
        }
    }

    lexer.get_state()
}

fn to_highlight_kind(
    token: &LuaTokenData,
    i: usize,
    tokens: &[LuaTokenData],
) -> CodeBlockHighlightKind {
    match token.kind {
        LuaTokenKind::TkLongString | LuaTokenKind::TkString => CodeBlockHighlightKind::String,
        LuaTokenKind::TkAnd
        | LuaTokenKind::TkBreak
        | LuaTokenKind::TkDo
        | LuaTokenKind::TkElse
        | LuaTokenKind::TkElseIf
        | LuaTokenKind::TkEnd
        | LuaTokenKind::TkFor
        | LuaTokenKind::TkFunction
        | LuaTokenKind::TkGoto
        | LuaTokenKind::TkIf
        | LuaTokenKind::TkIn
        | LuaTokenKind::TkNot
        | LuaTokenKind::TkOr
        | LuaTokenKind::TkRepeat
        | LuaTokenKind::TkReturn
        | LuaTokenKind::TkThen
        | LuaTokenKind::TkUntil
        | LuaTokenKind::TkWhile
        | LuaTokenKind::TkGlobal
        | LuaTokenKind::TkLocal
        | LuaTokenKind::TkConst
        | LuaTokenKind::TkContinue => CodeBlockHighlightKind::Keyword,
        LuaTokenKind::TkPlus
        | LuaTokenKind::TkMinus
        | LuaTokenKind::TkMul
        | LuaTokenKind::TkDiv
        | LuaTokenKind::TkIDiv
        | LuaTokenKind::TkDot
        | LuaTokenKind::TkConcat
        | LuaTokenKind::TkEq
        | LuaTokenKind::TkGe
        | LuaTokenKind::TkLe
        | LuaTokenKind::TkNe
        | LuaTokenKind::TkShl
        | LuaTokenKind::TkShr
        | LuaTokenKind::TkLt
        | LuaTokenKind::TkGt
        | LuaTokenKind::TkMod
        | LuaTokenKind::TkPow
        | LuaTokenKind::TkLen
        | LuaTokenKind::TkBitAnd
        | LuaTokenKind::TkBitOr
        | LuaTokenKind::TkBitXor
        | LuaTokenKind::TkLeftBrace
        | LuaTokenKind::TkRightBrace
        | LuaTokenKind::TkLeftBracket
        | LuaTokenKind::TkRightBracket
        | LuaTokenKind::TkLeftParen
        | LuaTokenKind::TkRightParen
        | LuaTokenKind::TkComma
        | LuaTokenKind::TkSemicolon
        | LuaTokenKind::TkLogicalAnd
        | LuaTokenKind::TkLogicalOr
        | LuaTokenKind::TkSafeNavigation
        | LuaTokenKind::TkTernary
        | LuaTokenKind::TkArrow
        | LuaTokenKind::TkAssign => CodeBlockHighlightKind::Operators,
        LuaTokenKind::TkComplex | LuaTokenKind::TkInt | LuaTokenKind::TkFloat => {
            CodeBlockHighlightKind::Number
        }
        LuaTokenKind::TkShortComment | LuaTokenKind::TkLongComment => {
            CodeBlockHighlightKind::Comment
        }
        LuaTokenKind::TkName => {
            if let Some(next_token) = tokens.get(i + 1) {
                match next_token.kind {
                    LuaTokenKind::TkLeftBrace
                    | LuaTokenKind::TkLeftParen
                    | LuaTokenKind::TkString
                    | LuaTokenKind::TkLongString => return CodeBlockHighlightKind::Function,
                    _ => {}
                }
            }

            if let Some(prev_token) = tokens.get(i.wrapping_sub(1))
                && matches!(
                    prev_token.kind,
                    LuaTokenKind::TkDot | LuaTokenKind::TkDbColon
                )
            {
                return CodeBlockHighlightKind::Property;
            }

            CodeBlockHighlightKind::Variable
        }

        _ => CodeBlockHighlightKind::None,
    }
}
