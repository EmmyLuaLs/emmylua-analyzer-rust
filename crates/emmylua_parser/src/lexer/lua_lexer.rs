use crate::{
    LexerState, LuaNonStdSymbol, kind::LuaTokenKind, parser_error::LuaParseError, text::Reader,
};

use super::{is_name_continue, is_name_start, lexer_config::LexerConfig, token_data::LuaTokenData};

pub struct LuaLexer<'a> {
    reader: Reader<'a>,
    lexer_config: LexerConfig,
    errors: Option<&'a mut Vec<LuaParseError>>,
    state: LexerState,
}

impl<'a> LuaLexer<'a> {
    pub fn new(
        reader: Reader<'a>,
        lexer_config: LexerConfig,
        errors: Option<&'a mut Vec<LuaParseError>>,
    ) -> Self {
        Self::new_with_state(reader, LexerState::Normal, lexer_config, errors)
    }

    pub fn new_with_state(
        reader: Reader<'a>,
        state: LexerState,
        lexer_config: LexerConfig,
        errors: Option<&'a mut Vec<LuaParseError>>,
    ) -> Self {
        LuaLexer {
            reader,
            lexer_config,
            errors,
            state,
        }
    }

    pub fn tokenize(&mut self) -> Vec<LuaTokenData> {
        let mut tokens = vec![];

        if self.state == LexerState::Normal && self.reader.current_char() == '#' {
            self.reader.eat_while(|ch| ch != '\n' && ch != '\r');
            tokens.push(LuaTokenData::new(
                LuaTokenKind::TkShebang,
                self.reader.current_range(),
            ));
        }

        while !self.reader.is_eof() {
            let kind = match self.state {
                LexerState::Normal => self.lex(),
                LexerState::String(quote) => self.lex_string(quote),
                LexerState::LongString(sep) => self.lex_long_string(sep),
                LexerState::LongComment(sep) => {
                    self.lex_long_string(sep);
                    LuaTokenKind::TkLongComment
                }
            };
            if kind == LuaTokenKind::TkEof {
                break;
            }

            tokens.push(LuaTokenData::new(kind, self.reader.current_range()));
        }

        tokens
    }

    pub fn get_state(&self) -> LexerState {
        self.state
    }

    pub fn continue_with_new_reader(&mut self, reader: Reader<'a>) -> Vec<LuaTokenData> {
        assert!(self.reader.is_eof(), "previous reader wasn't exhausted");
        self.reader = reader;
        self.tokenize()
    }

    fn support_non_std_symbol(&self, symbol: LuaNonStdSymbol) -> bool {
        self.lexer_config.non_std_symbols.support(symbol)
    }

    fn name_to_kind(&self, name: &str) -> LuaTokenKind {
        match name {
            "and" => LuaTokenKind::TkAnd,
            "break" => LuaTokenKind::TkBreak,
            "do" => LuaTokenKind::TkDo,
            "else" => LuaTokenKind::TkElse,
            "elseif" => LuaTokenKind::TkElseIf,
            "end" => LuaTokenKind::TkEnd,
            "false" => LuaTokenKind::TkFalse,
            "for" => LuaTokenKind::TkFor,
            "function" => LuaTokenKind::TkFunction,
            "goto" => {
                if self.lexer_config.support_goto() {
                    LuaTokenKind::TkGoto
                } else {
                    LuaTokenKind::TkName
                }
            }
            "if" => LuaTokenKind::TkIf,
            "in" => LuaTokenKind::TkIn,
            "local" => LuaTokenKind::TkLocal,
            "nil" => LuaTokenKind::TkNil,
            "not" => LuaTokenKind::TkNot,
            "or" => LuaTokenKind::TkOr,
            "repeat" => LuaTokenKind::TkRepeat,
            "return" => LuaTokenKind::TkReturn,
            "then" => LuaTokenKind::TkThen,
            "true" => LuaTokenKind::TkTrue,
            "until" => LuaTokenKind::TkUntil,
            "while" => LuaTokenKind::TkWhile,
            "continue" => {
                if self.support_non_std_symbol(LuaNonStdSymbol::Continue) {
                    LuaTokenKind::TkBreak
                } else {
                    LuaTokenKind::TkName
                }
            }
            _ => LuaTokenKind::TkName,
        }
    }

    fn lex(&mut self) -> LuaTokenKind {
        self.reader.reset_buff();

        match self.reader.current_char() {
            '\n' | '\r' => self.lex_new_line(),
            ' ' | '\t' => self.lex_white_space(),
            '-' => {
                self.reader.bump();
                if self.reader.current_char() == '='
                    && self.support_non_std_symbol(LuaNonStdSymbol::MinusAssign)
                {
                    self.reader.bump();
                    return LuaTokenKind::TkMinusAssign;
                }
                if self.reader.current_char() != '-' {
                    return LuaTokenKind::TkMinus;
                }

                self.reader.bump();
                if self.reader.current_char() == '[' {
                    self.reader.bump();
                    let sep = self.skip_sep();
                    if self.reader.current_char() == '[' {
                        self.reader.bump();
                        self.state = LexerState::LongComment(sep);
                        self.lex_long_string(sep);
                        return LuaTokenKind::TkLongComment;
                    }
                }

                self.reader.eat_while(|ch| ch != '\n' && ch != '\r');
                LuaTokenKind::TkShortComment
            }
            '[' => {
                self.reader.bump();
                let sep = self.skip_sep();
                if sep == 0 && self.reader.current_char() != '[' {
                    return LuaTokenKind::TkLeftBracket;
                }
                if self.reader.current_char() != '[' {
                    self.error(|| t!("invalid long string delimiter"));
                    return LuaTokenKind::TkLongString;
                }

                self.reader.bump();
                self.state = LexerState::LongString(sep);
                self.lex_long_string(sep)
            }
            '=' => {
                self.reader.bump();
                if self.reader.current_char() != '=' {
                    return LuaTokenKind::TkAssign;
                }
                self.reader.bump();
                LuaTokenKind::TkEq
            }
            '<' => {
                self.reader.bump();
                match self.reader.current_char() {
                    '=' => {
                        self.reader.bump();
                        LuaTokenKind::TkLe
                    }
                    '<' => {
                        if !self.lexer_config.support_integer_operation() {
                            self.error(|| t!("bitwise operation is not supported"));
                        }

                        self.reader.bump();
                        if self.reader.current_char() == '='
                            && self.support_non_std_symbol(LuaNonStdSymbol::ShiftLeftAssign)
                        {
                            self.reader.bump();
                            return LuaTokenKind::TkShiftLeftAssign;
                        }
                        LuaTokenKind::TkShl
                    }
                    _ => LuaTokenKind::TkLt,
                }
            }
            '>' => {
                self.reader.bump();
                match self.reader.current_char() {
                    '=' => {
                        self.reader.bump();
                        LuaTokenKind::TkGe
                    }
                    '>' => {
                        if !self.lexer_config.support_integer_operation() {
                            self.error(|| t!("bitwise operation is not supported"));
                        }

                        self.reader.bump();
                        if self.reader.current_char() == '='
                            && self.support_non_std_symbol(LuaNonStdSymbol::ShiftRightAssign)
                        {
                            self.reader.bump();
                            return LuaTokenKind::TkShiftRightAssign;
                        }
                        LuaTokenKind::TkShr
                    }
                    _ => LuaTokenKind::TkGt,
                }
            }
            '~' => {
                self.reader.bump();
                if self.reader.current_char() != '=' {
                    if !self.lexer_config.support_integer_operation() {
                        self.error(|| t!("bitwise operation is not supported"));
                    }
                    return LuaTokenKind::TkBitXor;
                }
                self.reader.bump();
                LuaTokenKind::TkNe
            }
            ':' => {
                self.reader.bump();
                if self.reader.current_char() != ':' {
                    return LuaTokenKind::TkColon;
                }
                self.reader.bump();
                LuaTokenKind::TkDbColon
            }
            '"' | '\'' | '`' => {
                let quote = self.reader.current_char();
                if quote == '`' && !self.support_non_std_symbol(LuaNonStdSymbol::Backtick) {
                    self.reader.bump();
                    return LuaTokenKind::TkUnknown;
                }

                self.reader.bump();
                self.state = LexerState::String(quote);
                self.lex_string(quote)
            }
            '.' => {
                if self.reader.next_char().is_ascii_digit() {
                    return self.lex_number();
                }

                self.reader.bump();
                if self.reader.current_char() != '.' {
                    return LuaTokenKind::TkDot;
                }
                self.reader.bump();
                if self.reader.current_char() != '.' {
                    return LuaTokenKind::TkConcat;
                }
                self.reader.bump();
                LuaTokenKind::TkDots
            }
            '0'..='9' => self.lex_number(),
            '/' => {
                self.reader.bump();
                let current_char = self.reader.current_char();
                match current_char {
                    '*' if self.support_non_std_symbol(LuaNonStdSymbol::SlashStar) => {
                        // "/*" is a long comment
                        self.reader.bump();
                        loop {
                            let ch = self.reader.current_char();
                            match ch {
                                '*' => {
                                    self.reader.bump();
                                    if self.reader.current_char() == '/' {
                                        self.reader.bump();
                                        return LuaTokenKind::TkLongComment;
                                    }
                                }
                                _ if self.reader.is_eof() => {
                                    self.error(|| t!("unfinished long comment"));
                                    return LuaTokenKind::TkLongComment;
                                }
                                _ => {
                                    self.reader.bump();
                                }
                            }
                        }
                    }
                    '=' if self.support_non_std_symbol(LuaNonStdSymbol::SlashAssign) => {
                        self.reader.bump();
                        LuaTokenKind::TkSlashAssign
                    }
                    _ if current_char != '/' => LuaTokenKind::TkDiv,
                    _ if self.support_non_std_symbol(LuaNonStdSymbol::DoubleSlash) => {
                        // "//" is a short comment
                        self.reader.bump();
                        self.reader.eat_while(|ch| ch != '\n' && ch != '\r');
                        LuaTokenKind::TkShortComment
                    }
                    _ => {
                        if !self.lexer_config.support_integer_operation() {
                            self.error(|| t!("integer division is not supported"));
                        }

                        self.reader.bump();
                        if self.reader.current_char() == '='
                            && self.support_non_std_symbol(LuaNonStdSymbol::DoubleSlashAssign)
                        {
                            self.reader.bump();
                            return LuaTokenKind::TkDoubleSlashAssign;
                        }
                        LuaTokenKind::TkIDiv
                    }
                }
            }
            '*' => {
                self.reader.bump();
                if self.reader.current_char() == '='
                    && self.support_non_std_symbol(LuaNonStdSymbol::StarAssign)
                {
                    self.reader.bump();
                    return LuaTokenKind::TkStarAssign;
                }
                LuaTokenKind::TkMul
            }
            '+' => {
                self.reader.bump();
                if self.reader.current_char() == '='
                    && self.support_non_std_symbol(LuaNonStdSymbol::PlusAssign)
                {
                    self.reader.bump();
                    return LuaTokenKind::TkPlusAssign;
                }
                LuaTokenKind::TkPlus
            }
            '%' => {
                self.reader.bump();
                if self.reader.current_char() == '='
                    && self.support_non_std_symbol(LuaNonStdSymbol::PercentAssign)
                {
                    self.reader.bump();
                    return LuaTokenKind::TkPercentAssign;
                }
                LuaTokenKind::TkMod
            }
            '^' => {
                self.reader.bump();
                if self.reader.current_char() == '='
                    && self.support_non_std_symbol(LuaNonStdSymbol::CaretAssign)
                {
                    self.reader.bump();
                    return LuaTokenKind::TkCaretAssign;
                }
                LuaTokenKind::TkPow
            }
            '#' => {
                self.reader.bump();
                LuaTokenKind::TkLen
            }
            '!' => {
                if !self.support_non_std_symbol(LuaNonStdSymbol::Exclamation) {
                    self.reader.bump();
                    return LuaTokenKind::TkUnknown;
                }

                self.reader.bump();
                if self.reader.current_char() == '='
                    && self.support_non_std_symbol(LuaNonStdSymbol::NotEqual)
                {
                    self.reader.bump();
                    return LuaTokenKind::TkNe;
                }
                LuaTokenKind::TkNot
            }
            '&' => {
                self.reader.bump();
                if self.reader.current_char() == '&'
                    && self.support_non_std_symbol(LuaNonStdSymbol::DoubleAmp)
                {
                    self.reader.bump();
                    return LuaTokenKind::TkAnd;
                }
                if self.reader.current_char() == '='
                    && self.support_non_std_symbol(LuaNonStdSymbol::AmpAssign)
                {
                    self.reader.bump();
                    return LuaTokenKind::TkAmpAssign;
                }

                if !self.lexer_config.support_integer_operation() {
                    self.error(|| t!("bitwise operation is not supported"));
                }
                LuaTokenKind::TkBitAnd
            }
            '|' => {
                self.reader.bump();
                if self.reader.current_char() == '|'
                    && self.support_non_std_symbol(LuaNonStdSymbol::DoublePipe)
                {
                    self.reader.bump();
                    return LuaTokenKind::TkOr;
                }

                if self.reader.current_char() == '='
                    && self.support_non_std_symbol(LuaNonStdSymbol::PipeAssign)
                {
                    self.reader.bump();
                    return LuaTokenKind::TkPipeAssign;
                }

                if !self.lexer_config.support_integer_operation() {
                    self.error(|| t!("bitwise operation is not supported"));
                }
                LuaTokenKind::TkBitOr
            }
            '(' => {
                self.reader.bump();
                LuaTokenKind::TkLeftParen
            }
            ')' => {
                self.reader.bump();
                LuaTokenKind::TkRightParen
            }
            '{' => {
                self.reader.bump();
                LuaTokenKind::TkLeftBrace
            }
            '}' => {
                self.reader.bump();
                LuaTokenKind::TkRightBrace
            }
            ']' => {
                self.reader.bump();
                LuaTokenKind::TkRightBracket
            }
            ';' => {
                self.reader.bump();
                LuaTokenKind::TkSemicolon
            }
            ',' => {
                self.reader.bump();
                LuaTokenKind::TkComma
            }
            '@' => {
                self.reader.bump();
                LuaTokenKind::TkAt
            }
            _ if self.reader.is_eof() => LuaTokenKind::TkEof,
            ch if is_name_start(ch) => {
                self.reader.bump();
                self.reader.eat_while(is_name_continue);
                let name = self.reader.current_text();
                self.name_to_kind(name)
            }
            _ => {
                self.reader.bump();
                LuaTokenKind::TkUnknown
            }
        }
    }

    fn lex_new_line(&mut self) -> LuaTokenKind {
        match self.reader.current_char() {
            // support \n or \n\r
            '\n' => {
                self.reader.bump();
                if self.reader.current_char() == '\r' {
                    self.reader.bump();
                }
            }
            // support \r or \r\n
            '\r' => {
                self.reader.bump();
                if self.reader.current_char() == '\n' {
                    self.reader.bump();
                }
            }
            _ => {}
        }

        LuaTokenKind::TkEndOfLine
    }

    fn lex_white_space(&mut self) -> LuaTokenKind {
        self.reader.eat_while(|ch| ch == ' ' || ch == '\t');
        LuaTokenKind::TkWhitespace
    }

    fn skip_sep(&mut self) -> usize {
        self.reader.eat_when('=')
    }

    fn lex_string(&mut self, quote: char) -> LuaTokenKind {
        while !self.reader.is_eof() {
            let ch = self.reader.current_char();
            if ch == quote || ch == '\n' || ch == '\r' {
                break;
            }

            if ch != '\\' {
                self.reader.bump();
                continue;
            }

            self.reader.bump();
            match self.reader.current_char() {
                'z' => {
                    self.reader.bump();
                    self.reader
                        .eat_while(|c| c == ' ' || c == '\t' || c == '\r' || c == '\n');
                }
                '\r' | '\n' => {
                    self.lex_new_line();
                }
                _ => {
                    self.reader.bump();
                }
            }
        }

        if self.reader.current_char() == quote || !self.reader.is_eof() {
            self.state = LexerState::Normal;
        }

        if self.reader.current_char() != quote {
            self.error(|| t!("unfinished string"));
            return LuaTokenKind::TkString;
        }

        self.reader.bump();
        LuaTokenKind::TkString
    }

    fn lex_long_string(&mut self, sep: usize) -> LuaTokenKind {
        let mut end = false;
        while !self.reader.is_eof() {
            match self.reader.current_char() {
                ']' => {
                    self.reader.bump();
                    let count = self.reader.eat_when('=');
                    if count == sep && self.reader.current_char() == ']' {
                        self.reader.bump();
                        end = true;
                        break;
                    }
                }
                _ => {
                    self.reader.bump();
                }
            }
        }

        if end || !self.reader.is_eof() {
            self.state = LexerState::Normal;
        }

        if !end {
            self.error(|| t!("unfinished long string or comment"));
        }

        LuaTokenKind::TkLongString
    }

    fn lex_number(&mut self) -> LuaTokenKind {
        enum NumberState {
            Int,
            Float,
            Hex,
            HexFloat,
            WithExpo,
            Bin,
        }

        let mut state = NumberState::Int;
        let first = self.reader.current_char();
        self.reader.bump();
        match first {
            '0' if matches!(self.reader.current_char(), 'X' | 'x') => {
                self.reader.bump();
                state = NumberState::Hex;
            }
            '0' if matches!(self.reader.current_char(), 'B' | 'b')
                && self.lexer_config.support_binary_integer() =>
            {
                self.reader.bump();
                state = NumberState::Bin;
            }
            '.' => {
                state = NumberState::Float;
            }
            _ => {}
        }

        while !self.reader.is_eof() {
            let ch = self.reader.current_char();
            let continue_ = match state {
                NumberState::Int => match ch {
                    '0'..='9' => true,
                    '.' => {
                        state = NumberState::Float;
                        true
                    }
                    _ if matches!(self.reader.current_char(), 'e' | 'E') => {
                        if matches!(self.reader.next_char(), '+' | '-') {
                            self.reader.bump();
                        }
                        state = NumberState::WithExpo;
                        true
                    }
                    _ => false,
                },
                NumberState::Float => match ch {
                    '0'..='9' => true,
                    _ if matches!(self.reader.current_char(), 'e' | 'E') => {
                        if matches!(self.reader.next_char(), '+' | '-') {
                            self.reader.bump();
                        }
                        state = NumberState::WithExpo;
                        true
                    }
                    _ => false,
                },
                NumberState::Hex => match ch {
                    '0'..='9' | 'a'..='f' | 'A'..='F' => true,
                    '.' => {
                        state = NumberState::HexFloat;
                        true
                    }
                    _ if matches!(self.reader.current_char(), 'P' | 'p') => {
                        if matches!(self.reader.next_char(), '+' | '-') {
                            self.reader.bump();
                        }
                        state = NumberState::WithExpo;
                        true
                    }
                    _ => false,
                },
                NumberState::HexFloat => match ch {
                    '0'..='9' | 'a'..='f' | 'A'..='F' => true,
                    _ if matches!(self.reader.current_char(), 'P' | 'p') => {
                        if matches!(self.reader.next_char(), '+' | '-') {
                            self.reader.bump();
                        }
                        state = NumberState::WithExpo;
                        true
                    }
                    _ => false,
                },
                NumberState::WithExpo => ch.is_ascii_digit(),
                NumberState::Bin => matches!(ch, '0' | '1'),
            };

            if continue_ {
                self.reader.bump();
            } else {
                break;
            }
        }

        if self.lexer_config.support_complex_number()
            && matches!(self.reader.current_char(), 'i' | 'I')
        {
            self.reader.bump();
            return LuaTokenKind::TkComplex;
        }

        if self.lexer_config.support_ll_integer()
            && matches!(
                state,
                NumberState::Int | NumberState::Hex | NumberState::Bin
            )
        {
            self.reader
                .eat_while(|ch| matches!(ch, 'u' | 'U' | 'l' | 'L'));
            return LuaTokenKind::TkInt;
        }

        if self.reader.current_char().is_alphabetic() {
            let ch = self.reader.current_char();
            self.error(|| t!("unexpected character '%{ch}' after number literal", ch = ch));
        }

        match state {
            NumberState::Int | NumberState::Hex => LuaTokenKind::TkInt,
            _ => LuaTokenKind::TkFloat,
        }
    }

    fn error<F, R>(&mut self, msg: F)
    where
        F: FnOnce() -> R,
        R: AsRef<str>,
    {
        if let Some(errors) = &mut self.errors {
            errors.push(LuaParseError::syntax_error_from(
                msg().as_ref(),
                self.reader.current_range(),
            ))
        }
    }
}
