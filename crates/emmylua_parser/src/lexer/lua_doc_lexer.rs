use crate::{
    kind::LuaTokenKind,
    text::{Reader, SourceRange},
};

use super::{is_name_continue, is_name_start};

#[derive(Debug, Clone)]
pub struct LuaDocLexer<'a> {
    origin_text: &'a str,
    origin_token_kind: LuaTokenKind,
    pub state: LuaDocLexerState,
    pub reader: Option<Reader<'a>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LuaDocLexerState {
    Init,
    Tag,
    Normal,
    FieldStart,
    Description,
    LongDescription,
    Trivia,
    See,
    Version,
    Source,
    NormalDescription,
    CastExpr,
}

impl LuaDocLexer<'_> {
    pub fn new(origin_text: &str) -> LuaDocLexer<'_> {
        LuaDocLexer {
            origin_text,
            reader: None,
            origin_token_kind: LuaTokenKind::None,
            state: LuaDocLexerState::Init,
        }
    }

    pub fn is_invalid(&self) -> bool {
        match self.reader {
            Some(ref reader) => reader.is_eof(),
            None => true,
        }
    }

    pub fn reset(&mut self, kind: LuaTokenKind, range: SourceRange) {
        self.reader = Some(Reader::new_with_range(self.origin_text, range));
        self.origin_token_kind = kind;
    }

    pub fn lex(&mut self) -> LuaTokenKind {
        let reader = self.reader.as_mut().unwrap();
        reader.reset_buff();

        if reader.is_eof() {
            return LuaTokenKind::TkEof;
        }

        match self.state {
            LuaDocLexerState::Init => self.lex_init(),
            LuaDocLexerState::Tag => self.lex_tag(),
            LuaDocLexerState::Normal => self.lex_normal(),
            LuaDocLexerState::FieldStart => self.lex_field_start(),
            LuaDocLexerState::Description => self.lex_description(),
            LuaDocLexerState::LongDescription => self.lex_long_description(),
            LuaDocLexerState::Trivia => self.lex_trivia(),
            LuaDocLexerState::See => self.lex_see(),
            LuaDocLexerState::Version => self.lex_version(),
            LuaDocLexerState::Source => self.lex_source(),
            LuaDocLexerState::NormalDescription => self.lex_normal_description(),
            LuaDocLexerState::CastExpr => self.lex_cast_expr(),
        }
    }

    pub fn current_token_range(&self) -> SourceRange {
        self.reader.as_ref().unwrap().saved_range()
    }

    fn lex_init(&mut self) -> LuaTokenKind {
        let reader = self.reader.as_mut().unwrap();
        match reader.current_char() {
            '-' if reader.is_start_of_line() => {
                let count = reader.consume_char_n_times('-', 3);
                match count {
                    2 => {
                        if self.origin_token_kind == LuaTokenKind::TkLongComment {
                            reader.bump();
                            reader.eat_when('=');
                            reader.bump();

                            match reader.current_char() {
                                '@' => {
                                    reader.bump();
                                    LuaTokenKind::TkDocLongStart
                                }
                                _ => LuaTokenKind::TkLongCommentStart,
                            }
                        } else {
                            LuaTokenKind::TkNormalStart
                        }
                    }
                    3 => {
                        reader.eat_while(is_doc_whitespace);
                        match reader.current_char() {
                            '@' => {
                                reader.bump();
                                LuaTokenKind::TkDocStart
                            }
                            _ => LuaTokenKind::TkNormalStart,
                        }
                    }
                    _ => {
                        reader.eat_while(|_| true);
                        LuaTokenKind::TKDocTriviaStart
                    }
                }
            }
            _ => {
                reader.eat_while(|_| true);
                LuaTokenKind::TkDocTrivia
            }
        }
    }

    fn lex_tag(&mut self) -> LuaTokenKind {
        let reader = self.reader.as_mut().unwrap();
        match reader.current_char() {
            ch if is_doc_whitespace(ch) => {
                reader.eat_while(is_doc_whitespace);
                LuaTokenKind::TkWhitespace
            }
            ch if is_name_start(ch) => {
                reader.bump();
                reader.eat_while(is_name_continue);
                let text = reader.current_saved_text();
                to_tag(text)
            }
            _ => {
                reader.eat_while(|_| true);
                LuaTokenKind::TkDocTrivia
            }
        }
    }

    fn lex_normal(&mut self) -> LuaTokenKind {
        let reader = self.reader.as_mut().unwrap();
        match reader.current_char() {
            ch if is_doc_whitespace(ch) => {
                reader.eat_while(is_doc_whitespace);
                LuaTokenKind::TkWhitespace
            }
            ':' => {
                reader.bump();
                LuaTokenKind::TkColon
            }
            '.' => {
                reader.bump();
                if reader.current_char() == '.' && reader.next_char() == '.' {
                    reader.bump();
                    reader.bump();
                    LuaTokenKind::TkDots
                } else {
                    LuaTokenKind::TkDot
                }
            }
            ',' => {
                reader.bump();
                LuaTokenKind::TkComma
            }
            ';' => {
                reader.bump();
                LuaTokenKind::TkSemicolon
            }
            '(' => {
                reader.bump();
                LuaTokenKind::TkLeftParen
            }
            ')' => {
                reader.bump();
                LuaTokenKind::TkRightParen
            }
            '[' => {
                reader.bump();
                LuaTokenKind::TkLeftBracket
            }
            ']' => {
                reader.bump();
                if self.origin_token_kind == LuaTokenKind::TkLongComment {
                    match reader.current_char() {
                        '=' => {
                            reader.eat_when('=');
                            reader.bump();
                            return LuaTokenKind::TkLongCommentEnd;
                        }
                        ']' => {
                            reader.bump();
                            return LuaTokenKind::TkLongCommentEnd;
                        }
                        _ => (),
                    }
                }

                LuaTokenKind::TkRightBracket
            }
            '{' => {
                reader.bump();
                LuaTokenKind::TkLeftBrace
            }
            '}' => {
                reader.bump();
                LuaTokenKind::TkRightBrace
            }
            '<' => {
                reader.bump();
                LuaTokenKind::TkLt
            }
            '>' => {
                reader.bump();
                LuaTokenKind::TkGt
            }
            '|' => {
                reader.bump();
                LuaTokenKind::TkDocOr
            }
            '&' => {
                reader.bump();
                LuaTokenKind::TkDocAnd
            }
            '?' => {
                reader.bump();
                LuaTokenKind::TkDocQuestion
            }
            '+' => {
                reader.bump();
                LuaTokenKind::TkPlus
            }
            '-' => {
                let count = reader.eat_when('-');
                match count {
                    1 => LuaTokenKind::TkMinus,
                    3 => {
                        reader.eat_while(is_doc_whitespace);
                        match reader.current_char() {
                            '@' => {
                                reader.bump();
                                LuaTokenKind::TkDocStart
                            }
                            '|' => {
                                reader.bump();
                                // compact luals
                                if matches!(reader.current_char(), '+' | '>') {
                                    reader.bump();
                                }
                                LuaTokenKind::TkDocContinueOr
                            }
                            _ => LuaTokenKind::TkDocContinue,
                        }
                    }
                    _ => LuaTokenKind::TkDocTrivia,
                }
            }
            '#' | '@' => {
                reader.eat_while(|_| true);
                LuaTokenKind::TkDocDetail
            }
            ch if ch.is_ascii_digit() => {
                reader.eat_while(|ch| ch.is_ascii_digit());
                LuaTokenKind::TkInt
            }
            ch if ch == '"' || ch == '\'' => {
                reader.bump();
                reader.eat_while(|c| c != ch);
                if reader.current_char() == ch {
                    reader.bump();
                }

                LuaTokenKind::TkString
            }
            ch if is_name_start(ch) || ch == '`' => {
                let (text, str_tpl) = read_doc_name(reader);
                if str_tpl {
                    return LuaTokenKind::TkStringTemplateType;
                }
                to_token_or_name(text)
            }
            _ => {
                reader.eat_while(|_| true);
                LuaTokenKind::TkDocTrivia
            }
        }
    }

    fn lex_field_start(&mut self) -> LuaTokenKind {
        let reader = self.reader.as_mut().unwrap();
        match reader.current_char() {
            ch if is_name_start(ch) => {
                let (text, _) = read_doc_name(reader);
                to_modification_or_name(text)
            }
            _ => self.lex_normal(),
        }
    }

    fn lex_description(&mut self) -> LuaTokenKind {
        let reader = self.reader.as_mut().unwrap();
        match reader.current_char() {
            ch if is_doc_whitespace(ch) => {
                reader.eat_while(is_doc_whitespace);
                LuaTokenKind::TkWhitespace
            }
            '-' if reader.is_start_of_line() => {
                let count = reader.consume_char_n_times('-', 3);
                match count {
                    2 => {
                        if self.origin_token_kind == LuaTokenKind::TkLongComment {
                            reader.bump();
                            reader.eat_when('=');
                            reader.bump();

                            match reader.current_char() {
                                '@' => {
                                    reader.bump();
                                    LuaTokenKind::TkDocLongStart
                                }
                                _ => LuaTokenKind::TkLongCommentStart,
                            }
                        } else {
                            LuaTokenKind::TkNormalStart
                        }
                    }
                    3 => {
                        reader.eat_while(is_doc_whitespace);
                        match reader.current_char() {
                            '@' => {
                                reader.bump();
                                LuaTokenKind::TkDocStart
                            }
                            '|' => {
                                reader.bump();
                                // compact luals
                                if matches!(reader.current_char(), '+' | '>') {
                                    reader.bump();
                                }

                                LuaTokenKind::TkDocContinueOr
                            }
                            _ => LuaTokenKind::TkNormalStart,
                        }
                    }
                    _ => {
                        reader.eat_while(|_| true);
                        LuaTokenKind::TKDocTriviaStart
                    }
                }
            }
            _ => {
                reader.eat_while(|_| true);
                LuaTokenKind::TkDocDetail
            }
        }
    }

    fn lex_long_description(&mut self) -> LuaTokenKind {
        let reader = self.reader.as_mut().unwrap();
        let text = reader.get_source_text();
        let mut chars = text.chars().rev().peekable();
        let mut trivia_count = 0;
        while let Some(&ch) = chars.peek() {
            if ch != ']' && ch != '=' {
                break;
            }
            chars.next();
            trivia_count += 1;
        }
        let end_pos = text.len() - trivia_count;

        if reader.get_current_end_pos() < end_pos {
            while reader.get_current_end_pos() < end_pos {
                reader.bump();
            }
            LuaTokenKind::TkDocDetail
        } else {
            reader.eat_while(|_| true);
            LuaTokenKind::TkDocTrivia
        }
    }

    fn lex_trivia(&mut self) -> LuaTokenKind {
        let reader = self.reader.as_mut().unwrap();
        reader.eat_while(|_| true);
        LuaTokenKind::TkDocTrivia
    }

    fn lex_see(&mut self) -> LuaTokenKind {
        let reader = self.reader.as_mut().unwrap();
        match reader.current_char() {
            ' ' | '\t' => {
                reader.eat_while(is_doc_whitespace);
                LuaTokenKind::TkWhitespace
            }
            _ => {
                reader.eat_while(|_| true);
                LuaTokenKind::TkDocSeeContent
            }
        }
    }

    fn lex_version(&mut self) -> LuaTokenKind {
        let reader = self.reader.as_mut().unwrap();
        match reader.current_char() {
            ',' => {
                reader.bump();
                LuaTokenKind::TkComma
            }
            '>' => {
                reader.bump();
                if reader.current_char() == '=' {
                    reader.bump();
                    LuaTokenKind::TkGe
                } else {
                    LuaTokenKind::TkGt
                }
            }
            '<' => {
                reader.bump();
                if reader.current_char() == '=' {
                    reader.bump();
                    LuaTokenKind::TkLe
                } else {
                    LuaTokenKind::TkLt
                }
            }
            ch if is_doc_whitespace(ch) => {
                reader.eat_while(is_doc_whitespace);
                LuaTokenKind::TkWhitespace
            }
            ch if ch.is_ascii_digit() => {
                reader.eat_while(|ch| ch.is_ascii_digit() || ch == '.');
                LuaTokenKind::TkDocVersionNumber
            }
            ch if is_name_start(ch) => {
                let (text, _) = read_doc_name(reader);
                match text {
                    "JIT" => LuaTokenKind::TkDocVersionNumber,
                    _ => LuaTokenKind::TkName,
                }
            }
            _ => self.lex_normal(),
        }
    }

    fn lex_source(&mut self) -> LuaTokenKind {
        let reader = self.reader.as_mut().unwrap();
        match reader.current_char() {
            ch if is_doc_whitespace(ch) => {
                reader.eat_while(is_doc_whitespace);
                LuaTokenKind::TkWhitespace
            }
            ch if is_name_start(ch) => {
                reader.bump();
                reader.eat_while(is_source_continue);
                LuaTokenKind::TKDocPath
            }
            ch if ch == '"' || ch == '\'' => {
                reader.bump();
                reader.eat_while(|c| c != '\'' && c != '"');
                if reader.current_char() == '\'' || reader.current_char() == '"' {
                    reader.bump();
                }

                LuaTokenKind::TKDocPath
            }
            _ => self.lex_normal(),
        }
    }

    fn lex_normal_description(&mut self) -> LuaTokenKind {
        let reader = self.reader.as_mut().unwrap();
        match reader.current_char() {
            ch if is_doc_whitespace(ch) => {
                reader.eat_while(is_doc_whitespace);
                LuaTokenKind::TkWhitespace
            }
            ch if ch.is_ascii_alphabetic() => {
                reader.eat_while(|c| c.is_ascii_alphabetic());
                let text = reader.current_saved_text();
                match text {
                    "region" => LuaTokenKind::TkDocRegion,
                    "endregion" => LuaTokenKind::TkDocEndRegion,
                    _ => {
                        reader.eat_while(|_| true);
                        LuaTokenKind::TkDocDetail
                    }
                }
            }
            '-' if reader.is_start_of_line() => {
                let count = reader.consume_char_n_times('-', 3);
                match count {
                    2 => {
                        if self.origin_token_kind == LuaTokenKind::TkLongComment {
                            reader.bump();
                            reader.eat_when('=');
                            reader.bump();

                            match reader.current_char() {
                                '@' => {
                                    reader.bump();
                                    LuaTokenKind::TkDocLongStart
                                }
                                _ => LuaTokenKind::TkLongCommentStart,
                            }
                        } else {
                            LuaTokenKind::TkNormalStart
                        }
                    }
                    3 => {
                        reader.eat_while(is_doc_whitespace);
                        match reader.current_char() {
                            '@' => {
                                reader.bump();
                                LuaTokenKind::TkDocStart
                            }
                            _ => LuaTokenKind::TkNormalStart,
                        }
                    }
                    _ => {
                        reader.eat_while(|_| true);
                        LuaTokenKind::TKDocTriviaStart
                    }
                }
            }
            _ => {
                reader.eat_while(|_| true);
                LuaTokenKind::TkDocDetail
            }
        }
    }

    fn lex_cast_expr(&mut self) -> LuaTokenKind {
        let reader = self.reader.as_mut().unwrap();
        match reader.current_char() {
            ch if is_doc_whitespace(ch) => {
                reader.eat_while(is_doc_whitespace);
                LuaTokenKind::TkWhitespace
            }
            '.' => {
                reader.bump();
                LuaTokenKind::TkDot
            }
            ch if is_name_start(ch) => {
                reader.bump();
                reader.eat_while(is_name_continue);
                LuaTokenKind::TkName
            }
            _ => self.lex_normal(),
        }
    }
}

fn to_tag(text: &str) -> LuaTokenKind {
    match text {
        "class" => LuaTokenKind::TkTagClass,
        "enum" => LuaTokenKind::TkTagEnum,
        "interface" => LuaTokenKind::TkTagInterface,
        "alias" => LuaTokenKind::TkTagAlias,
        "module" => LuaTokenKind::TkTagModule,
        "field" => LuaTokenKind::TkTagField,
        "type" => LuaTokenKind::TkTagType,
        "param" => LuaTokenKind::TkTagParam,
        "return" => LuaTokenKind::TkTagReturn,
        "return_cast" => LuaTokenKind::TkTagReturnCast,
        "generic" => LuaTokenKind::TkTagGeneric,
        "see" => LuaTokenKind::TkTagSee,
        "overload" => LuaTokenKind::TkTagOverload,
        "async" => LuaTokenKind::TkTagAsync,
        "cast" => LuaTokenKind::TkTagCast,
        "deprecated" => LuaTokenKind::TkTagDeprecated,
        "private" | "protected" | "public" | "package" | "internal" => {
            LuaTokenKind::TkTagVisibility
        }
        "readonly" => LuaTokenKind::TkTagReadonly,
        "diagnostic" => LuaTokenKind::TkTagDiagnostic,
        "meta" => LuaTokenKind::TkTagMeta,
        "version" => LuaTokenKind::TkTagVersion,
        "as" => LuaTokenKind::TkTagAs,
        "nodiscard" => LuaTokenKind::TkTagNodiscard,
        "operator" => LuaTokenKind::TkTagOperator,
        "mapping" => LuaTokenKind::TkTagMapping,
        "namespace" => LuaTokenKind::TkTagNamespace,
        "using" => LuaTokenKind::TkTagUsing,
        "source" => LuaTokenKind::TkTagSource,
        _ => LuaTokenKind::TkTagOther,
    }
}

fn to_modification_or_name(text: &str) -> LuaTokenKind {
    match text {
        "private" | "protected" | "public" | "package" => LuaTokenKind::TkDocVisibility,
        "readonly" => LuaTokenKind::TkDocReadonly,
        _ => LuaTokenKind::TkName,
    }
}

fn to_token_or_name(text: &str) -> LuaTokenKind {
    match text {
        "true" => LuaTokenKind::TkTrue,
        "false" => LuaTokenKind::TkFalse,
        "keyof" => LuaTokenKind::TkDocKeyOf,
        "extends" => LuaTokenKind::TkDocExtends,
        "as" => LuaTokenKind::TkDocAs,
        "and" => LuaTokenKind::TkAnd,
        "or" => LuaTokenKind::TkOr,
        _ => LuaTokenKind::TkName,
    }
}

fn is_doc_whitespace(ch: char) -> bool {
    ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n'
}

fn read_doc_name<'a>(reader: &'a mut Reader) -> (&'a str, bool /* str tpl */) {
    reader.bump();
    let mut str_tpl = false;
    while !reader.is_eof() {
        match reader.current_char() {
            ch if is_name_continue(ch) => {
                reader.bump();
            }
            // donot continue if next char is '.' or '-' or '*' or '`'
            '.' | '-' | '*' => {
                let next = reader.next_char();
                if next == '.' || next == '-' || next == '*' {
                    break;
                }

                reader.bump();
            }
            '`' => {
                str_tpl = true;
                reader.bump();
            }
            _ => break,
        }
    }

    (reader.current_saved_text(), str_tpl)
}

fn is_source_continue(ch: char) -> bool {
    is_name_continue(ch)
        || ch == '.'
        || ch == '-'
        || ch == '/'
        || ch == ' '
        || ch == ':'
        || ch == '#'
        || ch == '\\'
}

#[cfg(test)]
mod tests {
    use crate::kind::LuaTokenKind;
    use crate::lexer::LuaDocLexer;
    use crate::text::SourceRange;

    #[test]
    fn test_lex() {
        let text = r#"-- comment"#;
        let mut lexer = LuaDocLexer::new(text);
        lexer.reset(LuaTokenKind::TkShortComment, SourceRange::new(0, 10));
        let k1 = lexer.lex();
        assert_eq!(k1, LuaTokenKind::TkNormalStart);
        let k2 = lexer.lex();
        let range = lexer.current_token_range();
        let text = lexer.origin_text[range.start_offset..range.end_offset()].to_string();
        assert_eq!(text, " comment");
        assert_eq!(k2, LuaTokenKind::TkDocTrivia);
    }
}
