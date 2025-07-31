use super::{
    lua_doc_parser::LuaDocParser,
    marker::{MarkEvent, MarkerEventContainer},
    parser_config::ParserConfig,
};
use crate::text::Reader;
use crate::{
    LuaSyntaxTree, LuaTreeBuilder,
    grammar::parse_chunk,
    kind::LuaTokenKind,
    lexer::{LuaLexer, LuaTokenData},
    parser_error::LuaParseError,
    text::SourceRange,
};

#[allow(unused)]
pub struct LuaParser<'a> {
    text: &'a str,
    events: Vec<MarkEvent>,
    tokens: Vec<LuaTokenData>,
    token_index: usize,
    current_token: LuaTokenKind,
    mark_level: usize,
    pub parse_config: ParserConfig<'a>,
    pub(crate) errors: &'a mut Vec<LuaParseError>,
}

impl MarkerEventContainer for LuaParser<'_> {
    fn get_mark_level(&self) -> usize {
        self.mark_level
    }

    fn incr_mark_level(&mut self) {
        self.mark_level += 1;
    }

    fn decr_mark_level(&mut self) {
        self.mark_level -= 1;
    }

    fn get_events(&mut self) -> &mut Vec<MarkEvent> {
        &mut self.events
    }
}

impl<'a> LuaParser<'a> {
    #[allow(unused)]
    pub fn parse(text: &'a str, config: ParserConfig) -> LuaSyntaxTree {
        let mut errors: Vec<LuaParseError> = Vec::new();
        let tokens = {
            let mut lexer =
                LuaLexer::new(Reader::new(text), config.lexer_config(), Some(&mut errors));
            lexer.tokenize()
        };

        let mut parser = LuaParser {
            text,
            events: Vec::new(),
            tokens,
            token_index: 0,
            current_token: LuaTokenKind::None,
            parse_config: config,
            mark_level: 0,
            errors: &mut errors,
        };

        parse_chunk(&mut parser);
        let errors = parser.get_errors();
        let root = {
            let mut builder = LuaTreeBuilder::new(
                parser.origin_text(),
                parser.events,
                parser.parse_config.node_cache(),
            );
            builder.build();
            builder.finish()
        };
        LuaSyntaxTree::new(root, errors)
    }

    pub fn init(&mut self) {
        if self.tokens.is_empty() {
            self.current_token = LuaTokenKind::TkEof;
        } else {
            self.current_token = self.tokens[0].kind;
        }

        if is_trivia_kind(self.current_token) {
            self.bump();
        }
    }

    pub fn origin_text(&self) -> &'a str {
        self.text
    }

    pub fn current_token(&self) -> LuaTokenKind {
        self.current_token
    }

    pub fn current_token_index(&self) -> usize {
        self.token_index
    }

    pub fn current_token_range(&self) -> SourceRange {
        if self.token_index >= self.tokens.len() {
            if self.tokens.is_empty() {
                return SourceRange::EMPTY;
            } else {
                return self.tokens[self.tokens.len() - 1].range;
            }
        }

        self.tokens[self.token_index].range
    }

    pub fn current_token_text(&self) -> &str {
        let range = &self.tokens[self.token_index].range;
        &self.text[range.start_offset..range.end_offset()]
    }

    pub fn set_current_token_kind(&mut self, kind: LuaTokenKind) {
        if self.token_index < self.tokens.len() {
            self.tokens[self.token_index].kind = kind;
            self.current_token = kind;
        }
    }

    pub fn bump(&mut self) {
        if !is_invalid_kind(self.current_token) && self.token_index < self.tokens.len() {
            let token = &self.tokens[self.token_index];
            self.events.push(MarkEvent::EatToken {
                kind: token.kind,
                range: token.range,
            });
        }

        let mut next_index = self.token_index + 1;
        self.skip_trivia(&mut next_index);
        self.parse_trivia_tokens(next_index);
        self.token_index = next_index;

        if self.token_index >= self.tokens.len() {
            self.current_token = LuaTokenKind::TkEof;
            return;
        }

        self.current_token = self.tokens[self.token_index].kind;
    }

    pub fn peek_next_token(&self) -> LuaTokenKind {
        let mut next_index = self.token_index + 1;
        self.skip_trivia(&mut next_index);

        if next_index >= self.tokens.len() {
            LuaTokenKind::None
        } else {
            self.tokens[next_index].kind
        }
    }

    fn skip_trivia(&self, index: &mut usize) {
        if index >= &mut self.tokens.len() {
            return;
        }

        let mut kind = self.tokens[*index].kind;
        while is_trivia_kind(kind) {
            *index += 1;
            if *index >= self.tokens.len() {
                break;
            }
            kind = self.tokens[*index].kind;
        }
    }

    // Analyze consecutive whitespace/comments
    // At this point, comments may be in the wrong parent node, adjustments will be made in the subsequent treeBuilder
    fn parse_trivia_tokens(&mut self, next_index: usize) {
        let mut line_count = 0;
        let start = self.token_index;
        let mut doc_tokens: Vec<LuaTokenData> = Vec::new();
        for i in start..next_index {
            let token = &self.tokens[i];
            match token.kind {
                LuaTokenKind::TkShortComment | LuaTokenKind::TkLongComment => {
                    line_count = 0;
                    doc_tokens.push(*token);
                }
                LuaTokenKind::TkEndOfLine => {
                    line_count += 1;

                    if doc_tokens.is_empty() {
                        self.events.push(MarkEvent::EatToken {
                            kind: token.kind,
                            range: token.range,
                        });
                    } else {
                        doc_tokens.push(*token);
                    }

                    // If there are two EOFs after the comment, the previous comment is considered a group of comments
                    if line_count > 1 && !doc_tokens.is_empty() {
                        self.parse_comments(&doc_tokens);
                        doc_tokens.clear();
                    }
                    // check if the comment is an inline comment
                    // first is comment, second is endofline
                    else if doc_tokens.len() == 2 && i >= 2 {
                        let mut temp_index = i as isize - 2;
                        let mut inline_comment = false;
                        while temp_index >= 0 {
                            let kind = self.tokens[temp_index as usize].kind;
                            match kind {
                                LuaTokenKind::TkEndOfLine => {
                                    break;
                                }
                                LuaTokenKind::TkWhitespace => {
                                    temp_index -= 1;
                                    continue;
                                }
                                _ => {
                                    inline_comment = true;
                                    break;
                                }
                            }
                        }

                        if inline_comment {
                            self.parse_comments(&doc_tokens);
                            doc_tokens.clear();
                        }
                    }
                }
                LuaTokenKind::TkShebang | LuaTokenKind::TkWhitespace => {
                    if doc_tokens.is_empty() {
                        self.events.push(MarkEvent::EatToken {
                            kind: token.kind,
                            range: token.range,
                        });
                    } else {
                        doc_tokens.push(*token);
                    }
                }
                _ => {
                    if !doc_tokens.is_empty() {
                        self.parse_comments(&doc_tokens);
                        doc_tokens.clear();
                    }
                }
            }
        }

        if !doc_tokens.is_empty() {
            self.parse_comments(&doc_tokens);
        }
    }

    fn parse_comments(&mut self, comment_tokens: &Vec<LuaTokenData>) {
        let mut trivia_token_start = comment_tokens.len();
        // Reverse iterate over comment_tokens, removing whitespace and end-of-line tokens
        for i in (0..comment_tokens.len()).rev() {
            if matches!(
                comment_tokens[i].kind,
                LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine
            ) {
                trivia_token_start = i;
            } else {
                break;
            }
        }

        let tokens = &comment_tokens[..trivia_token_start];
        LuaDocParser::parse(self, tokens);

        for i in trivia_token_start..comment_tokens.len() {
            let token = &comment_tokens[i];
            self.events.push(MarkEvent::EatToken {
                kind: token.kind,
                range: token.range,
            });
        }
    }

    pub fn push_error(&mut self, err: LuaParseError) {
        self.errors.push(err);
    }

    pub fn has_error(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn get_errors(&self) -> Vec<LuaParseError> {
        self.errors.clone()
    }
}

fn is_trivia_kind(kind: LuaTokenKind) -> bool {
    matches!(
        kind,
        LuaTokenKind::TkShortComment
            | LuaTokenKind::TkLongComment
            | LuaTokenKind::TkEndOfLine
            | LuaTokenKind::TkWhitespace
            | LuaTokenKind::TkShebang
    )
}

fn is_invalid_kind(kind: LuaTokenKind) -> bool {
    matches!(
        kind,
        LuaTokenKind::None
            | LuaTokenKind::TkEof
            | LuaTokenKind::TkWhitespace
            | LuaTokenKind::TkShebang
            | LuaTokenKind::TkEndOfLine
            | LuaTokenKind::TkShortComment
            | LuaTokenKind::TkLongComment
    )
}

#[cfg(test)]
mod tests {
    use crate::text::Reader;
    use crate::{
        LuaParser, kind::LuaTokenKind, lexer::LuaLexer, parser::ParserConfig,
        parser_error::LuaParseError,
    };

    #[allow(unused)]
    fn new_parser<'a>(
        text: &'a str,
        config: ParserConfig<'a>,
        errors: &'a mut Vec<LuaParseError>,
        show_tokens: bool,
    ) -> LuaParser<'a> {
        let tokens = {
            let mut lexer = LuaLexer::new(Reader::new(text), config.lexer_config(), Some(errors));
            lexer.tokenize()
        };

        if show_tokens {
            println!("tokens: ");
            for t in &tokens {
                println!("{:?}", t);
            }
        }

        let mut parser = LuaParser {
            text,
            events: Vec::new(),
            tokens,
            token_index: 0,
            current_token: LuaTokenKind::None,
            parse_config: config,
            mark_level: 0,
            errors,
        };
        parser.init();

        parser
    }

    #[test]
    fn test_parse_and_ast() {
        let lua_code = r#"
            function foo(a, b)
                return a + b
            end
        "#;

        let tree = LuaParser::parse(lua_code, ParserConfig::default());
        println!("{:#?}", tree.get_red_root());
    }

    #[test]
    fn test_parse_and_ast_with_error() {
        let lua_code = r#"
            function foo(a, b)
                return a + b
        "#;

        let tree = LuaParser::parse(lua_code, ParserConfig::default());
        println!("{:#?}", tree.get_red_root());
    }

    #[test]
    fn test_parse_comment() {
        let lua_code = r#"
            -- comment
            local t
            -- inline comment
        "#;

        let tree = LuaParser::parse(lua_code, ParserConfig::default());
        println!("{:#?}", tree.get_red_root());
    }

    #[test]
    fn test_parse_empty_file() {
        let lua_code = r#""#;

        let tree = LuaParser::parse(lua_code, ParserConfig::default());
        println!("{:#?}", tree.get_red_root());
    }
}
