use emmylua_parser::{LexerState, Reader, SourceRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonTokenKind {
    TkEof,
    TkWhitespace,
    TkString,
    TkNumber,
    TkKeyword, // true, false, null
    TkColon,
    TkComma,
    TkLeftBrace,
    TkRightBrace,
    TkLeftBracket,
    TkRightBracket,
    TkUnknown,
}

#[derive(Debug)]
pub struct JsonTokenData {
    pub kind: JsonTokenKind,
    pub range: SourceRange,
}

impl JsonTokenData {
    pub fn new(kind: JsonTokenKind, range: SourceRange) -> Self {
        Self { kind, range }
    }
}

#[derive(Debug)]
pub struct JsonLexer<'a> {
    reader: Reader<'a>,
    state: LexerState,
}

impl<'a> JsonLexer<'a> {
    pub fn new_with_state(reader: Reader<'a>, state: LexerState) -> Self {
        JsonLexer { reader, state }
    }

    pub fn tokenize(&mut self) -> Vec<JsonTokenData> {
        let mut tokens = vec![];

        while !self.reader.is_eof() {
            let kind = match self.state {
                LexerState::Normal => self.lex(),
                LexerState::String(quote) => self.lex_string(quote),
                _ => JsonTokenKind::TkUnknown,
            };

            if kind == JsonTokenKind::TkEof {
                break;
            }

            tokens.push(JsonTokenData::new(kind, self.reader.current_range()));
        }

        tokens
    }

    pub fn get_state(&self) -> LexerState {
        self.state
    }

    fn lex(&mut self) -> JsonTokenKind {
        self.reader.reset_buff();

        match self.reader.current_char() {
            ' ' | '\t' | '\n' | '\r' => self.lex_whitespace(),
            '"' => {
                let quote = self.reader.current_char();
                self.reader.bump();
                self.state = LexerState::String(quote);
                self.lex_string(quote)
            }
            '0'..='9' | '-' => self.lex_number(),
            'a'..='z' | 'A'..='Z' => self.lex_keyword(),
            ':' => {
                self.reader.bump();
                JsonTokenKind::TkColon
            }
            ',' => {
                self.reader.bump();
                JsonTokenKind::TkComma
            }
            '{' => {
                self.reader.bump();
                JsonTokenKind::TkLeftBrace
            }
            '}' => {
                self.reader.bump();
                JsonTokenKind::TkRightBrace
            }
            '[' => {
                self.reader.bump();
                JsonTokenKind::TkLeftBracket
            }
            ']' => {
                self.reader.bump();
                JsonTokenKind::TkRightBracket
            }
            _ if self.reader.is_eof() => JsonTokenKind::TkEof,
            _ => {
                self.reader.bump();
                JsonTokenKind::TkUnknown
            }
        }
    }

    fn lex_whitespace(&mut self) -> JsonTokenKind {
        self.reader
            .eat_while(|c| c == ' ' || c == '\t' || c == '\n' || c == '\r');
        JsonTokenKind::TkWhitespace
    }

    fn lex_string(&mut self, quote: char) -> JsonTokenKind {
        while !self.reader.is_eof() {
            let ch = self.reader.current_char();
            if ch == quote {
                break;
            }

            if ch == '\\' {
                self.reader.bump();
                if !self.reader.is_eof() {
                    self.reader.bump();
                }
            } else {
                self.reader.bump();
            }
        }

        if self.reader.current_char() == quote {
            self.reader.bump();
            self.state = LexerState::Normal;
        }

        JsonTokenKind::TkString
    }

    fn lex_number(&mut self) -> JsonTokenKind {
        // Handle negative numbers
        if self.reader.current_char() == '-' {
            self.reader.bump();
        }

        // Integer part
        if self.reader.current_char() == '0' {
            self.reader.bump();
        } else if self.reader.current_char().is_ascii_digit() {
            self.reader.eat_while(|c| c.is_ascii_digit());
        } else {
            // Invalid number (just a minus sign or something else)
            return JsonTokenKind::TkUnknown;
        }

        // Decimal part
        if self.reader.current_char() == '.' {
            self.reader.bump();
            if self.reader.current_char().is_ascii_digit() {
                self.reader.eat_while(|c| c.is_ascii_digit());
            } else {
                // Invalid number (decimal point without digits)
                return JsonTokenKind::TkUnknown;
            }
        }

        // Exponent part
        if self.reader.current_char() == 'e' || self.reader.current_char() == 'E' {
            self.reader.bump();
            if self.reader.current_char() == '+' || self.reader.current_char() == '-' {
                self.reader.bump();
            }
            if self.reader.current_char().is_ascii_digit() {
                self.reader.eat_while(|c| c.is_ascii_digit());
            } else {
                // Invalid number (exponent without digits)
                return JsonTokenKind::TkUnknown;
            }
        }

        JsonTokenKind::TkNumber
    }

    fn lex_keyword(&mut self) -> JsonTokenKind {
        self.reader.eat_while(|c| c.is_alphabetic());
        let keyword = self.reader.current_text();

        match keyword {
            "true" | "false" | "null" => JsonTokenKind::TkKeyword,
            _ => JsonTokenKind::TkUnknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emmylua_parser::Reader;
    use googletest::prelude::*;

    #[gtest]
    fn test_json_lexer_basic() {
        let json = r#"
{
    "name": "John Doe",
    "age": 30,
    "married": true,
    "address": null,
    "scores": [85, 90, 78],
    "pi": 3.14159,
    "negative": -42
}
"#;

        let reader = Reader::new(json);
        let mut lexer = JsonLexer::new_with_state(reader, LexerState::Normal);
        let tokens = lexer.tokenize();

        // Count different token types
        let mut string_count = 0;
        let mut number_count = 0;
        let mut keyword_count = 0;
        let mut brace_count = 0;
        let mut bracket_count = 0;

        for token in &tokens {
            match token.kind {
                JsonTokenKind::TkString => string_count += 1,
                JsonTokenKind::TkNumber => number_count += 1,
                JsonTokenKind::TkKeyword => keyword_count += 1,
                JsonTokenKind::TkLeftBrace | JsonTokenKind::TkRightBrace => brace_count += 1,
                JsonTokenKind::TkLeftBracket | JsonTokenKind::TkRightBracket => bracket_count += 1,
                _ => {}
            }
        }

        expect_gt!(string_count, 0, "Should find strings");
        expect_gt!(number_count, 0, "Should find numbers");
        expect_gt!(keyword_count, 0, "Should find keywords");
        expect_gt!(brace_count, 0, "Should find braces");
        expect_gt!(bracket_count, 0, "Should find brackets");

        println!(
            "Found {} strings, {} numbers, {} keywords, {} braces, {} brackets",
            string_count, number_count, keyword_count, brace_count, bracket_count
        );
    }

    #[gtest]
    fn test_json_lexer_keywords() {
        let json = "true false null";

        let reader = Reader::new(json);
        let mut lexer = JsonLexer::new_with_state(reader, LexerState::Normal);
        let tokens = lexer.tokenize();

        let keywords: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == JsonTokenKind::TkKeyword)
            .collect();

        expect_eq!(keywords.len(), 3, "Should find exactly 3 keywords");
    }

    #[gtest]
    fn test_json_lexer_numbers() {
        let json = "42 -17 3.14 -2.5 1e10 1E-5 -1.23e+4";

        let reader = Reader::new(json);
        let mut lexer = JsonLexer::new_with_state(reader, LexerState::Normal);
        let tokens = lexer.tokenize();

        let numbers: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == JsonTokenKind::TkNumber)
            .collect();

        expect_eq!(numbers.len(), 7, "Should find exactly 7 numbers");
    }

    #[gtest]
    fn test_json_lexer_strings() {
        let json = r#""hello" "world with spaces" "escaped\"quote" "unicode\u0041""#;

        let reader = Reader::new(json);
        let mut lexer = JsonLexer::new_with_state(reader, LexerState::Normal);
        let tokens = lexer.tokenize();

        let strings: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == JsonTokenKind::TkString)
            .collect();

        expect_eq!(strings.len(), 4, "Should find exactly 4 strings");
    }

    #[gtest]
    fn test_json_lexer_structure() {
        let json = r#"{"key": ["value1", "value2"]}"#;

        let reader = Reader::new(json);
        let mut lexer = JsonLexer::new_with_state(reader, LexerState::Normal);
        let tokens = lexer.tokenize();

        // Should have proper structure tokens
        let has_left_brace = tokens.iter().any(|t| t.kind == JsonTokenKind::TkLeftBrace);
        let has_right_brace = tokens.iter().any(|t| t.kind == JsonTokenKind::TkRightBrace);
        let has_left_bracket = tokens
            .iter()
            .any(|t| t.kind == JsonTokenKind::TkLeftBracket);
        let has_right_bracket = tokens
            .iter()
            .any(|t| t.kind == JsonTokenKind::TkRightBracket);
        let has_colon = tokens.iter().any(|t| t.kind == JsonTokenKind::TkColon);
        let has_comma = tokens.iter().any(|t| t.kind == JsonTokenKind::TkComma);

        expect_true!(has_left_brace, "Should have left brace");
        expect_true!(has_right_brace, "Should have right brace");
        expect_true!(has_left_bracket, "Should have left bracket");
        expect_true!(has_right_bracket, "Should have right bracket");
        expect_true!(has_colon, "Should have colon");
        expect_true!(has_comma, "Should have comma");
    }
}
