use emmylua_parser::{LexerState, Reader, SourceRange};

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SqlTokenKind {
    TkEof,
    TkEndOfLine,
    TkWhitespace,
    TkLineComment,
    TkBlockComment,
    TkSingleQuotedString,
    TkDoubleQuotedString,
    TkBacktickString,
    TkInteger,
    TkFloat,
    TkHexNumber,
    TkKeyword,
    TkDataType,
    TkFunction,
    TkOperator,
    TkIdentifier,
    TkParameter,
    TkSemicolon,
    TkComma,
    TkDot,
    TkLeftParen,
    TkRightParen,
    TkLeftBracket,
    TkRightBracket,
    TkUnknown,
}

#[derive(Debug)]
pub struct SqlTokenData {
    pub kind: SqlTokenKind,
    pub range: SourceRange,
}

impl SqlTokenData {
    pub fn new(kind: SqlTokenKind, range: SourceRange) -> Self {
        Self { kind, range }
    }
}

#[derive(Debug)]
pub struct SqlLexer<'a> {
    reader: Reader<'a>,
    state: LexerState,
}

impl<'a> SqlLexer<'a> {
    #[allow(unused)]
    pub fn new(text: &'a str) -> Self {
        let mut reader = Reader::new(text);
        reader.reset_buff();
        Self {
            reader,
            state: LexerState::Normal,
        }
    }

    pub fn new_with_state(reader: Reader<'a>, state: LexerState) -> Self {
        Self { reader, state }
    }

    pub fn get_state(&self) -> LexerState {
        self.state
    }

    pub fn tokenize(&mut self) -> Vec<SqlTokenData> {
        let mut tokens = vec![];

        loop {
            let token = self.next_token();
            if matches!(token.kind, SqlTokenKind::TkEof) {
                break;
            }
            tokens.push(token);
        }

        tokens
    }

    pub fn next_token(&mut self) -> SqlTokenData {
        // If in a multi-line comment state, continue scanning the comment.
        if self.state == LexerState::LongComment(0) {
            return self.scan_block_comment_continue();
        }

        if self.reader.is_eof() {
            self.reader.reset_buff();
            return SqlTokenData::new(SqlTokenKind::TkEof, self.reader.current_range());
        }

        self.reader.reset_buff();
        let ch = self.reader.current_char();

        match ch {
            ' ' | '\t' => self.scan_whitespace(),
            '\n' | '\r' => self.scan_newline(),
            '-' if self.reader.next_char() == '-' => self.scan_line_comment(),
            '/' if self.reader.next_char() == '*' => self.scan_block_comment(),
            '\'' => self.scan_single_quoted_string(),
            '"' => self.scan_double_quoted_string(),
            '`' => self.scan_backtick_string(),
            '0'..='9' => self.scan_number(),
            'a'..='z' | 'A'..='Z' | '_' => self.scan_identifier_or_keyword(),
            ';' => {
                self.reader.bump();
                SqlTokenData::new(SqlTokenKind::TkSemicolon, self.reader.current_range())
            }
            ',' => {
                self.reader.bump();
                SqlTokenData::new(SqlTokenKind::TkComma, self.reader.current_range())
            }
            '.' => {
                self.reader.bump();
                SqlTokenData::new(SqlTokenKind::TkDot, self.reader.current_range())
            }
            '(' => {
                self.reader.bump();
                SqlTokenData::new(SqlTokenKind::TkLeftParen, self.reader.current_range())
            }
            ')' => {
                self.reader.bump();
                SqlTokenData::new(SqlTokenKind::TkRightParen, self.reader.current_range())
            }
            '[' => {
                self.reader.bump();
                SqlTokenData::new(SqlTokenKind::TkLeftBracket, self.reader.current_range())
            }
            ']' => {
                self.reader.bump();
                SqlTokenData::new(SqlTokenKind::TkRightBracket, self.reader.current_range())
            }
            '=' | '<' | '>' | '!' | '+' | '-' | '*' | '/' | '%' | '&' | '|' | '^' | '~' => {
                self.scan_operator()
            }
            '@' | ':' => self.scan_parameter(),
            _ => {
                self.reader.bump();
                SqlTokenData::new(SqlTokenKind::TkUnknown, self.reader.current_range())
            }
        }
    }

    #[allow(unused)]
    fn skip_trivia(&mut self) {
        while !self.reader.is_eof() {
            match self.reader.current_char() {
                ' ' | '\t' => {
                    self.reader.bump();
                }
                _ => break,
            }
        }
    }

    fn scan_whitespace(&mut self) -> SqlTokenData {
        while !self.reader.is_eof() {
            match self.reader.current_char() {
                ' ' | '\t' => {
                    self.reader.bump();
                }
                _ => break,
            }
        }

        SqlTokenData::new(SqlTokenKind::TkWhitespace, self.reader.current_range())
    }

    fn scan_newline(&mut self) -> SqlTokenData {
        if self.reader.current_char() == '\r' {
            self.reader.bump();
        }
        if !self.reader.is_eof() && self.reader.current_char() == '\n' {
            self.reader.bump();
        }

        SqlTokenData::new(SqlTokenKind::TkEndOfLine, self.reader.current_range())
    }

    fn scan_line_comment(&mut self) -> SqlTokenData {
        // Skip '--'
        self.reader.bump();
        self.reader.bump();

        while !self.reader.is_eof() {
            let ch = self.reader.current_char();
            if ch == '\n' || ch == '\r' {
                break;
            }
            self.reader.bump();
        }

        SqlTokenData::new(SqlTokenKind::TkLineComment, self.reader.current_range())
    }

    fn scan_block_comment(&mut self) -> SqlTokenData {
        // Skip '/*'
        self.reader.bump();
        self.reader.bump();

        while !self.reader.is_eof() {
            if self.reader.current_char() == '*' && self.reader.next_char() == '/' {
                self.reader.bump();
                self.reader.bump();
                self.state = LexerState::Normal;
                return SqlTokenData::new(
                    SqlTokenKind::TkBlockComment,
                    self.reader.current_range(),
                );
            }
            self.reader.bump();
        }

        // If the end of file is reached without a closing marker, set the state to a multi-line comment.
        self.state = LexerState::LongComment(0);
        SqlTokenData::new(SqlTokenKind::TkBlockComment, self.reader.current_range())
    }

    fn scan_block_comment_continue(&mut self) -> SqlTokenData {
        self.reader.reset_buff();

        while !self.reader.is_eof() {
            if self.reader.current_char() == '*' && self.reader.next_char() == '/' {
                self.reader.bump();
                self.reader.bump();
                self.state = LexerState::Normal;
                return SqlTokenData::new(
                    SqlTokenKind::TkBlockComment,
                    self.reader.current_range(),
                );
            }
            self.reader.bump();
        }

        // Still inside a multi-line comment.
        SqlTokenData::new(SqlTokenKind::TkBlockComment, self.reader.current_range())
    }

    fn scan_single_quoted_string(&mut self) -> SqlTokenData {
        // Skip opening quote
        self.reader.bump();

        while !self.reader.is_eof() {
            let ch = self.reader.current_char();
            self.reader.bump();

            if ch == '\'' {
                // Check for escaped quote
                if !self.reader.is_eof() && self.reader.current_char() == '\'' {
                    self.reader.bump();
                } else {
                    break;
                }
            }
        }

        SqlTokenData::new(
            SqlTokenKind::TkSingleQuotedString,
            self.reader.current_range(),
        )
    }

    fn scan_double_quoted_string(&mut self) -> SqlTokenData {
        // Skip opening quote
        self.reader.bump();

        while !self.reader.is_eof() {
            let ch = self.reader.current_char();
            self.reader.bump();

            if ch == '"' {
                // Check for escaped quote
                if !self.reader.is_eof() && self.reader.current_char() == '"' {
                    self.reader.bump();
                } else {
                    break;
                }
            }
        }

        SqlTokenData::new(
            SqlTokenKind::TkDoubleQuotedString,
            self.reader.current_range(),
        )
    }

    fn scan_backtick_string(&mut self) -> SqlTokenData {
        // Skip opening backtick
        self.reader.bump();

        while !self.reader.is_eof() {
            let ch = self.reader.current_char();
            self.reader.bump();

            if ch == '`' {
                break;
            }
        }

        SqlTokenData::new(SqlTokenKind::TkBacktickString, self.reader.current_range())
    }

    fn scan_number(&mut self) -> SqlTokenData {
        let mut is_float = false;
        let mut is_hex = false;

        // Check for hex number
        if self.reader.current_char() == '0' && matches!(self.reader.next_char(), 'x' | 'X') {
            is_hex = true;
            self.reader.bump();
            self.reader.bump();

            while !self.reader.is_eof() {
                match self.reader.current_char() {
                    '0'..='9' | 'a'..='f' | 'A'..='F' => {
                        self.reader.bump();
                    }
                    _ => break,
                }
            }
        } else {
            // Regular number
            while !self.reader.is_eof() {
                match self.reader.current_char() {
                    '0'..='9' => {
                        self.reader.bump();
                    }
                    '.' if !is_float => {
                        is_float = true;
                        self.reader.bump();
                    }
                    'e' | 'E' if !is_hex => {
                        self.reader.bump();
                        if matches!(self.reader.current_char(), '+' | '-') {
                            self.reader.bump();
                        }
                        is_float = true;
                    }
                    _ => break,
                }
            }
        }

        let token_kind = if is_hex {
            SqlTokenKind::TkHexNumber
        } else if is_float {
            SqlTokenKind::TkFloat
        } else {
            SqlTokenKind::TkInteger
        };

        SqlTokenData::new(token_kind, self.reader.current_range())
    }

    fn scan_identifier_or_keyword(&mut self) -> SqlTokenData {
        while !self.reader.is_eof() {
            match self.reader.current_char() {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '_' => {
                    self.reader.bump();
                }
                _ => break,
            }
        }

        let text = self.reader.current_text();
        let token_kind = match text.to_uppercase().as_str() {
            // SQL Keywords
            "SELECT" | "FROM" | "WHERE" | "INSERT" | "UPDATE" | "DELETE" | "CREATE" | "DROP"
            | "ALTER" | "TABLE" | "INDEX" | "VIEW" | "DATABASE" | "SCHEMA" | "CONSTRAINT"
            | "PRIMARY" | "FOREIGN" | "KEY" | "REFERENCES" | "UNIQUE" | "NOT" | "NULL"
            | "DEFAULT" | "CHECK" | "AUTO_INCREMENT" | "IDENTITY" | "SEQUENCE" | "INNER"
            | "LEFT" | "RIGHT" | "FULL" | "OUTER" | "JOIN" | "ON" | "UNION" | "INTERSECT"
            | "EXCEPT" | "ORDER" | "BY" | "GROUP" | "HAVING" | "LIMIT" | "OFFSET" | "TOP"
            | "DISTINCT" | "ALL" | "AS" | "CASE" | "WHEN" | "THEN" | "ELSE" | "END" | "IF"
            | "EXISTS" | "IN" | "BETWEEN" | "LIKE" | "IS" | "AND" | "OR" | "XOR" | "SOME"
            | "ANY" | "TRUE" | "FALSE" | "UNKNOWN" | "BEGIN" | "COMMIT" | "ROLLBACK"
            | "TRANSACTION" | "SAVEPOINT" | "GRANT" | "REVOKE" | "PRIVILEGE" | "ROLE" | "USER"
            | "PASSWORD" | "EXECUTE" | "PROCEDURE" | "FUNCTION" | "TRIGGER" | "CURSOR"
            | "DECLARE" | "SET" | "GET" | "CALL" | "RETURN" | "WHILE" | "FOR" | "LOOP"
            | "BREAK" | "CONTINUE" | "GOTO" | "LABEL" => SqlTokenKind::TkKeyword,

            // Data Types
            "INT" | "INTEGER" | "BIGINT" | "SMALLINT" | "TINYINT" | "DECIMAL" | "NUMERIC"
            | "FLOAT" | "DOUBLE" | "REAL" | "MONEY" | "SMALLMONEY" | "CHAR" | "VARCHAR"
            | "NCHAR" | "NVARCHAR" | "TEXT" | "NTEXT" | "BINARY" | "VARBINARY" | "IMAGE"
            | "BLOB" | "CLOB" | "DATE" | "TIME" | "DATETIME" | "DATETIME2" | "SMALLDATETIME"
            | "TIMESTAMP" | "YEAR" | "INTERVAL" | "BOOLEAN" | "BIT" | "UUID" | "GUID" | "XML"
            | "JSON" | "GEOMETRY" | "GEOGRAPHY" => SqlTokenKind::TkDataType,

            // Common Functions
            "COUNT" | "SUM" | "AVG" | "MIN" | "MAX" | "FIRST" | "LAST" | "UPPER" | "LOWER"
            | "TRIM" | "LTRIM" | "RTRIM" | "SUBSTRING" | "REPLACE" | "CONCAT" | "LENGTH"
            | "LEN" | "CHARINDEX" | "PATINDEX" | "ROUND" | "CEILING" | "FLOOR" | "ABS"
            | "POWER" | "SQRT" | "RAND" | "GETDATE" | "NOW" | "CURRENT_DATE" | "CURRENT_TIME"
            | "CURRENT_TIMESTAMP" | "DATEADD" | "DATEDIFF" | "DATEPART" | "MONTH" | "DAY"
            | "CAST" | "CONVERT" | "ISNULL" | "COALESCE" | "NULLIF" => SqlTokenKind::TkFunction,

            _ => SqlTokenKind::TkIdentifier,
        };

        SqlTokenData::new(token_kind, self.reader.current_range())
    }

    fn scan_operator(&mut self) -> SqlTokenData {
        let first_char = self.reader.current_char();
        self.reader.bump();

        // Check for two-character operators
        if !self.reader.is_eof() {
            let second_char = self.reader.current_char();
            match (first_char, second_char) {
                ('=', '=')
                | ('!', '=')
                | ('<', '=')
                | ('>', '=')
                | ('<', '>')
                | ('|', '|')
                | ('&', '&')
                | ('+', '=')
                | ('-', '=')
                | ('*', '=')
                | ('/', '=')
                | ('%', '=')
                | ('&', '=')
                | ('|', '=')
                | ('^', '=') => {
                    self.reader.bump();
                }
                _ => {}
            }
        }

        SqlTokenData::new(SqlTokenKind::TkOperator, self.reader.current_range())
    }

    fn scan_parameter(&mut self) -> SqlTokenData {
        // Skip the parameter marker
        self.reader.bump();

        // Scan parameter name/number
        while !self.reader.is_eof() {
            match self.reader.current_char() {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '_' => {
                    self.reader.bump();
                }
                _ => break,
            }
        }

        SqlTokenData::new(SqlTokenKind::TkParameter, self.reader.current_range())
    }

    #[allow(unused)]
    pub fn reset(&mut self, text: &'a str) {
        self.reader = Reader::new(text);
        self.reader.reset_buff();
        self.state = LexerState::Normal;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn next_non_whitespace_token(lexer: &mut SqlLexer) -> SqlTokenData {
        loop {
            let token = lexer.next_token();
            if !matches!(token.kind, SqlTokenKind::TkWhitespace) {
                return token;
            }
        }
    }

    #[test]
    fn test_keywords() {
        let mut lexer = SqlLexer::new("SELECT FROM WHERE");

        let token1 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token1.kind, SqlTokenKind::TkKeyword);

        let token2 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token2.kind, SqlTokenKind::TkKeyword);

        let token3 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token3.kind, SqlTokenKind::TkKeyword);
    }

    #[test]
    fn test_identifiers() {
        let mut lexer = SqlLexer::new("table_name column_name");

        let token1 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token1.kind, SqlTokenKind::TkIdentifier);

        let token2 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token2.kind, SqlTokenKind::TkIdentifier);
    }

    #[test]
    fn test_numbers() {
        let mut lexer = SqlLexer::new("123 45.67 0xFF 1.23e-4");

        let token1 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token1.kind, SqlTokenKind::TkInteger);

        let token2 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token2.kind, SqlTokenKind::TkFloat);

        let token3 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token3.kind, SqlTokenKind::TkHexNumber);

        let token4 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token4.kind, SqlTokenKind::TkFloat);
    }

    #[test]
    fn test_strings() {
        let mut lexer = SqlLexer::new("'single' \"double\" `backtick`");

        let token1 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token1.kind, SqlTokenKind::TkSingleQuotedString);

        let token2 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token2.kind, SqlTokenKind::TkDoubleQuotedString);

        let token3 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token3.kind, SqlTokenKind::TkBacktickString);
    }

    #[test]
    fn test_comments() {
        let mut lexer = SqlLexer::new("-- line comment\n/* block comment */");

        let token1 = lexer.next_token();
        assert_eq!(token1.kind, SqlTokenKind::TkLineComment);

        let token2 = lexer.next_token();
        assert_eq!(token2.kind, SqlTokenKind::TkEndOfLine);

        let token3 = lexer.next_token();
        assert_eq!(token3.kind, SqlTokenKind::TkBlockComment);
    }

    #[test]
    fn test_operators() {
        let mut lexer = SqlLexer::new("= != <= >= <> || &&");

        let token1 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token1.kind, SqlTokenKind::TkOperator);

        let token2 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token2.kind, SqlTokenKind::TkOperator);

        let token3 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token3.kind, SqlTokenKind::TkOperator);
    }

    #[test]
    fn test_parameters() {
        let mut lexer = SqlLexer::new("@param1 :param2");

        let token1 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token1.kind, SqlTokenKind::TkParameter);

        let token2 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token2.kind, SqlTokenKind::TkParameter);
    }

    #[test]
    fn test_punctuation() {
        let mut lexer = SqlLexer::new("(); , . []");

        assert_eq!(
            next_non_whitespace_token(&mut lexer).kind,
            SqlTokenKind::TkLeftParen
        );
        assert_eq!(
            next_non_whitespace_token(&mut lexer).kind,
            SqlTokenKind::TkRightParen
        );
        assert_eq!(
            next_non_whitespace_token(&mut lexer).kind,
            SqlTokenKind::TkSemicolon
        );
        assert_eq!(
            next_non_whitespace_token(&mut lexer).kind,
            SqlTokenKind::TkComma
        );
        assert_eq!(
            next_non_whitespace_token(&mut lexer).kind,
            SqlTokenKind::TkDot
        );
        assert_eq!(
            next_non_whitespace_token(&mut lexer).kind,
            SqlTokenKind::TkLeftBracket
        );
        assert_eq!(
            next_non_whitespace_token(&mut lexer).kind,
            SqlTokenKind::TkRightBracket
        );
    }

    #[test]
    fn test_data_types() {
        let mut lexer = SqlLexer::new("INT VARCHAR DATETIME");

        let token1 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token1.kind, SqlTokenKind::TkDataType);

        let token2 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token2.kind, SqlTokenKind::TkDataType);

        let token3 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token3.kind, SqlTokenKind::TkDataType);
    }

    #[test]
    fn test_functions() {
        let mut lexer = SqlLexer::new("COUNT SUM AVG");

        let token1 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token1.kind, SqlTokenKind::TkFunction);

        let token2 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token2.kind, SqlTokenKind::TkFunction);

        let token3 = next_non_whitespace_token(&mut lexer);
        assert_eq!(token3.kind, SqlTokenKind::TkFunction);
    }

    #[test]
    fn test_complex_sql() {
        let sql = "SELECT id, name FROM users WHERE age > 18;";
        let mut lexer = SqlLexer::new(sql);

        let mut tokens = Vec::new();
        loop {
            let token = lexer.next_token();
            if matches!(token.kind, SqlTokenKind::TkEof) {
                break;
            }
            if !matches!(token.kind, SqlTokenKind::TkWhitespace) {
                tokens.push(token);
            }
        }

        // Verify we have the expected tokens
        assert!(tokens.len() > 10);
        assert_eq!(tokens[0].kind, SqlTokenKind::TkKeyword); // SELECT
        assert_eq!(tokens[1].kind, SqlTokenKind::TkIdentifier); // id
        assert_eq!(tokens[2].kind, SqlTokenKind::TkComma); // ,
        assert_eq!(tokens[3].kind, SqlTokenKind::TkIdentifier); // name
        assert_eq!(tokens[4].kind, SqlTokenKind::TkKeyword); // FROM
    }
}
