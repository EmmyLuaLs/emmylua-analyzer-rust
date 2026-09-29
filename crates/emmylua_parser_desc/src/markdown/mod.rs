mod test;

use crate::lang::{CodeBlockLang, process_code};
use crate::markdown_rst::{eat_rst_flag_body, process_inline_code};
use crate::util::{
    BacktrackPoint, ResultContainer, desc_to_lines, is_blank, is_code_directive, is_punct, is_ws,
};
use crate::{CodeBlockHighlightKind, DescItem, DescItemKind, LuaDescParser};
use emmylua_parser::{LexerState, Reader, SourceRange};
use emmylua_parser::{LuaAstNode, LuaDocDescription};

/// Error types for Markdown parsing
#[allow(unused)]
#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    /// Nesting depth exceeded the maximum allowed limit
    NestingTooDeep,
    /// Invalid Markdown syntax encountered
    InvalidSyntax,
    /// Unexpected end of file
    UnexpectedEof,
    /// Invalid fence configuration
    InvalidFence,
}

type ParseResult<T> = Result<T, ParseError>;

pub struct MarkdownParser {
    states: Vec<State>,
    inline_state: Vec<InlineState>,
    #[allow(unused)]
    primary_domain: Option<String>,
    enable_myst: bool,
    results: Vec<DescItem>,
    cursor_position: Option<usize>,
    state: LexerState,
    max_nesting_depth: usize,
}

#[derive(Copy, Clone)]
enum State {
    Quote {
        scope_start: usize,
    },
    Indented {
        indent: usize,
        scope_start: usize,
    },
    Code {
        scope_start: usize,
    },
    FencedCode {
        n_fences: usize,
        fence: char,
        lang: CodeBlockLang,
        scope_start: usize,
    },
    FencedDirectiveParams {
        n_fences: usize,
        fence: char,
        lang: Option<CodeBlockLang>,
        scope_start: usize,
    },
    FencedDirectiveParamsLong {
        n_fences: usize,
        fence: char,
        lang: Option<CodeBlockLang>,
        scope_start: usize,
    },
    FencedDirectiveBody {
        n_fences: usize,
        fence: char,
        scope_start: usize,
    },
    Math {
        scope_start: usize,
    },
}

enum InlineState {
    Em(char, SourceRange, usize),
    Strong(char, SourceRange, usize),
    Both(char, SourceRange, usize),
}

impl LuaDescParser for MarkdownParser {
    fn parse(&mut self, text: &str, desc: LuaDocDescription) -> Vec<DescItem> {
        assert!(self.results.is_empty());

        self.states.clear();
        self.inline_state.clear();

        let desc_end = desc.get_range().end().into();

        for range in desc_to_lines(text, desc, self.cursor_position) {
            // Process line.
            let line = &text[range.start_offset..range.end_offset()];
            self.process_line(&mut Reader::new_with_range(line, range));
        }

        self.flush_state(
            0,
            &mut Reader::new_with_range("", SourceRange::new(desc_end, 0)),
        );

        std::mem::take(&mut self.results)
    }
}

impl ResultContainer for MarkdownParser {
    fn results(&self) -> &Vec<DescItem> {
        &self.results
    }

    fn results_mut(&mut self) -> &mut Vec<DescItem> {
        &mut self.results
    }

    fn cursor_position(&self) -> Option<usize> {
        self.cursor_position
    }
}

impl MarkdownParser {
    pub fn new(cursor_position: Option<usize>) -> Self {
        Self {
            states: Vec::new(),
            inline_state: Vec::new(),
            primary_domain: None,
            enable_myst: false,
            results: Vec::new(),
            cursor_position,
            state: LexerState::Normal,
            max_nesting_depth: 64, // Reasonable limit to prevent stack overflow
        }
    }

    pub fn new_myst(primary_domain: Option<String>, cursor_position: Option<usize>) -> Self {
        Self {
            states: Vec::new(),
            inline_state: Vec::new(),
            primary_domain,
            enable_myst: true,
            results: Vec::new(),
            cursor_position,
            state: LexerState::Normal,
            max_nesting_depth: 64,
        }
    }

    fn process_line(&mut self, reader: &mut Reader) {
        // Check nesting depth to prevent stack overflow
        if self.states.len() >= self.max_nesting_depth {
            // Skip processing if nesting is too deep
            reader.eat_till_end();
            self.emit(reader, DescItemKind::CodeBlock);
            return;
        }

        // First, find out which blocks are still present and which finished.
        let mut last_state = 0;
        let states_copy = self.states.clone();

        for (i, &state) in states_copy.iter().enumerate() {
            match state {
                State::Quote { .. } => {
                    if self.try_process_quote_continuation(reader).is_ok() {
                        // Continue with nested states.
                    } else {
                        break;
                    }
                }
                State::Indented { indent, .. } => {
                    if self.try_process_indented(reader, indent).is_ok() {
                        // Continue with nested states.
                    } else {
                        break;
                    }
                }
                State::Code { .. } => {
                    if self.try_process_code(reader).is_ok() {
                        return;
                    } else {
                        break;
                    }
                }
                State::FencedCode {
                    n_fences,
                    fence,
                    lang,
                    ..
                } => {
                    if self.try_process_fence_end(reader, n_fences, fence).is_ok() {
                        self.flush_state(i, reader);
                        return;
                    } else {
                        self.process_code_line(reader, lang);
                        return;
                    }
                }
                State::FencedDirectiveParams {
                    n_fences,
                    fence,
                    lang,
                    scope_start,
                } => {
                    if self.try_process_fence_end(reader, n_fences, fence).is_ok() {
                        self.flush_state(i, reader);
                        return;
                    } else if self.try_process_fence_long_params_marker(reader).is_ok() {
                        self.flush_state(i + 1, reader);
                        self.states.pop();
                        self.states.push(State::FencedDirectiveParamsLong {
                            n_fences,
                            fence,
                            lang,
                            scope_start,
                        });
                        return;
                    } else if self.try_process_fence_short_param(reader).is_ok() {
                        return;
                    } else if lang.is_some() {
                        self.flush_state(i + 1, reader);
                        self.states.pop();
                        let lang = lang.unwrap_or(CodeBlockLang::None);
                        self.states.push(State::FencedCode {
                            n_fences,
                            fence,
                            lang,
                            scope_start,
                        });
                        self.process_code_line(reader, lang);
                        return;
                    } else {
                        self.flush_state(i + 1, reader);
                        self.states.pop();
                        self.states.push(State::FencedDirectiveBody {
                            n_fences,
                            fence,
                            scope_start,
                        });
                        last_state = i + 1;
                        break;
                    }
                }
                State::FencedDirectiveParamsLong {
                    n_fences,
                    fence,
                    lang,
                    scope_start,
                } => {
                    if self.try_process_fence_end(reader, n_fences, fence).is_ok() {
                        self.flush_state(i, reader);
                        return;
                    } else if self.try_process_fence_long_params_marker(reader).is_ok() {
                        self.flush_state(i + 1, reader);
                        self.states.pop();
                        if lang.is_some() {
                            self.states.push(State::FencedCode {
                                n_fences,
                                fence,
                                lang: lang.unwrap_or(CodeBlockLang::None),
                                scope_start,
                            });
                        } else {
                            self.states.push(State::FencedDirectiveBody {
                                n_fences,
                                fence,
                                scope_start,
                            });
                        }
                        return;
                    } else {
                        self.process_code_line(reader, lang.unwrap_or(CodeBlockLang::None));
                        return;
                    }
                }
                State::FencedDirectiveBody {
                    n_fences, fence, ..
                } => {
                    if self.try_process_fence_end(reader, n_fences, fence).is_ok() {
                        self.flush_state(i, reader);
                        return;
                    } else {
                        // Continue with nested states.
                    }
                }
                State::Math { .. } => {
                    if self.try_process_math_end(reader).is_ok() {
                        self.flush_state(i, reader);
                        return;
                    } else {
                        reader.eat_till_end();
                        self.emit(reader, DescItemKind::CodeBlock);
                        return;
                    }
                }
            }

            last_state = i + 1;
        }

        self.flush_state(last_state, reader);

        // Second, handle the rest of the line. Each iteration will add a new block
        // onto the state stack. The final iteration will handle inline content.
        loop {
            if !self.try_start_new_block(reader) {
                // No more blocks to start.
                break;
            }
        }
    }

    #[must_use]
    fn try_start_new_block(&mut self, reader: &mut Reader) -> bool {
        const HAS_MORE_CONTENT: bool = true;
        const NO_MORE_CONTENT: bool = false;

        if is_blank(reader.tail_text()) {
            // Just an empty line, nothing to do here.
            reader.eat_till_end();
            reader.reset_buff();
            return NO_MORE_CONTENT;
        }

        // All markdown blocks can start with at most 3 whitespaces.
        // 4 whitespaces start a code block.
        let mut indent = reader.consume_n_times(is_ws, 3);

        match reader.current_char() {
            // Thematic break or list start.
            '-' | '_' | '*' | '+' => {
                if self.try_process_thematic_break(reader).is_ok() {
                    return NO_MORE_CONTENT;
                } else if let Ok((indent_more, scope_start)) = self.try_process_list(reader) {
                    indent += indent_more;
                    self.states.push(State::Indented {
                        indent,
                        scope_start,
                    });
                    return HAS_MORE_CONTENT;
                } else {
                    // This is a normal text, continue to inline parsing.
                }
            }
            // Heading.
            '#' => {
                let scope_start = reader.current_range().start_offset;

                reader.reset_buff();
                reader.eat_when('#');
                self.emit(reader, DescItemKind::Markup);
                self.process_inline_content(reader);

                let scope_end = reader.current_range().end_offset();
                self.emit_range(
                    SourceRange::from_start_end(scope_start, scope_end),
                    DescItemKind::Scope,
                );

                return NO_MORE_CONTENT;
            }
            // Fenced code.
            '`' | '~' | ':' => {
                // Try improved fence processing first, fallback to original on error
                match self.try_process_fence_start_improved(reader) {
                    Ok((n_fences, fence, scope_start)) => {
                        if let Ok((dir_name, dir_args)) =
                            self.try_process_fence_directive_name(reader)
                        {
                            // This is a directive.
                            let is_code = is_code_directive(dir_name);
                            let lang = if is_code {
                                CodeBlockLang::try_parse(dir_args.trim())
                            } else {
                                None
                            };
                            self.states.push(State::FencedDirectiveParams {
                                n_fences,
                                fence,
                                lang,
                                scope_start,
                            });
                        } else {
                            // This is a code block.
                            reader.eat_till_end();
                            let lang = CodeBlockLang::try_parse(reader.current_text().trim());
                            self.emit(reader, DescItemKind::CodeBlock);
                            self.states.push(State::FencedCode {
                                n_fences,
                                fence,
                                lang: lang.unwrap_or(CodeBlockLang::None),
                                scope_start,
                            });
                        }
                        return NO_MORE_CONTENT;
                    }
                    Err(_) => {
                        // Fallback to original method for compatibility
                        if let Ok((n_fences, fence, scope_start)) =
                            self.try_process_fence_start(reader)
                        {
                            if let Ok((dir_name, dir_args)) =
                                self.try_process_fence_directive_name(reader)
                            {
                                // This is a directive.
                                let is_code = is_code_directive(dir_name);
                                let lang = if is_code {
                                    CodeBlockLang::try_parse(dir_args.trim())
                                } else {
                                    None
                                };
                                self.states.push(State::FencedDirectiveParams {
                                    n_fences,
                                    fence,
                                    lang,
                                    scope_start,
                                });
                            } else {
                                // This is a code block.
                                reader.eat_till_end();
                                let lang = CodeBlockLang::try_parse(reader.current_text().trim());
                                self.emit(reader, DescItemKind::CodeBlock);
                                self.states.push(State::FencedCode {
                                    n_fences,
                                    fence,
                                    lang: lang.unwrap_or(CodeBlockLang::None),
                                    scope_start,
                                });
                            }
                            return NO_MORE_CONTENT;
                        } else {
                            // This is a normal text, continue to inline parsing.
                        }
                    }
                }
            }
            // Indented code.
            ' ' | '\t' => {
                let scope_start = reader.current_range().start_offset;
                reader.bump();
                reader.reset_buff();
                reader.eat_till_end();
                self.emit(reader, DescItemKind::CodeBlock);
                self.states.push(State::Code { scope_start });
                return NO_MORE_CONTENT;
            }
            // Numbered list.
            '0'..='9' => {
                if let Ok((indent_more, scope_start)) = self.try_process_list(reader) {
                    indent += indent_more;
                    self.states.push(State::Indented {
                        indent,
                        scope_start,
                    });
                    return HAS_MORE_CONTENT;
                } else {
                    // This is a normal text, continue to inline parsing.
                }
            }
            // Quote.
            '>' => {
                if let Ok(scope_start) = self.try_process_quote(reader) {
                    self.states.push(State::Quote { scope_start });
                    return HAS_MORE_CONTENT;
                } else {
                    // This is a normal text, continue to inline parsing.
                }
            }
            // Math block.
            '$' if self.enable_myst => {
                if let Ok(scope_start) = self.try_process_math(reader) {
                    self.states.push(State::Math { scope_start });
                    return NO_MORE_CONTENT;
                } else {
                    // This is a normal text, continue to inline parsing.
                }
            }
            // Maybe a link anchor.
            '[' => {
                let bt = BacktrackPoint::new(self, reader);

                let scope_start = reader.current_range().start_offset;

                if Self::eat_link_title(reader)
                    && reader.current_char() == ':'
                    && is_ws(reader.next_char())
                {
                    self.emit(reader, DescItemKind::Link);
                    reader.bump();
                    self.emit(reader, DescItemKind::Markup);
                    reader.eat_while(is_ws);
                    reader.reset_buff();
                    reader.eat_till_end();
                    self.emit(reader, DescItemKind::Link);

                    let scope_end = reader.current_range().end_offset();
                    self.emit_range(
                        SourceRange::from_start_end(scope_start, scope_end),
                        DescItemKind::Scope,
                    );

                    bt.commit(self, reader);
                    return NO_MORE_CONTENT;
                } else {
                    bt.rollback(self, reader);
                }
            }
            // Normal text.
            _ => {
                // Continue to inline parsing.
            }
        }

        // Didn't detect start of any nested block. Parse the rest of the line
        // as an inline context.
        reader.reset_buff();
        self.process_inline_content(reader);
        NO_MORE_CONTENT
    }

    fn try_process_thematic_break<'a>(&mut self, reader: &mut Reader<'a>) -> Result<(), ()> {
        // Line that consists of three or more of the same symbol (`-`, `*`, or `_`),
        // possibly separated by spaces. I.e.: `" - - - "`.

        let bt = BacktrackPoint::new(self, reader);

        let scope_start = reader.current_range().start_offset;

        reader.eat_while(is_ws);
        reader.reset_buff();

        let first_char = reader.current_char();
        if !matches!(first_char, '-' | '*' | '_') {
            bt.rollback(self, reader);
            return Err(());
        } else {
            reader.bump();
            self.emit(reader, DescItemKind::Markup);
        }

        let mut n_marks = 1;
        loop {
            reader.eat_while(is_ws);
            reader.reset_buff();
            if reader.is_eof() {
                break;
            } else if reader.current_char() == first_char {
                reader.bump();
                self.emit(reader, DescItemKind::Markup);
                n_marks += 1;
            } else {
                bt.rollback(self, reader);
                return Err(());
            }
        }

        if n_marks >= 3 {
            reader.eat_till_end();
            reader.reset_buff();

            let scope_end = reader.current_range().end_offset();
            self.emit_range(
                SourceRange::from_start_end(scope_start, scope_end),
                DescItemKind::Scope,
            );

            bt.commit(self, reader);
            Ok(())
        } else {
            bt.rollback(self, reader);
            Err(())
        }
    }

    fn try_process_quote<'a>(&mut self, reader: &mut Reader<'a>) -> Result<usize, ()> {
        // Quote start, i.e. `"   > text..."`.

        let bt = BacktrackPoint::new(self, reader);
        let scope_start = reader.current_range().start_offset;

        match self.try_process_quote_continuation(reader) {
            Ok(()) => {
                bt.commit(self, reader);
                Ok(scope_start)
            }
            Err(()) => {
                bt.rollback(self, reader);
                Err(())
            }
        }
    }

    fn try_process_quote_continuation<'a>(&mut self, reader: &mut Reader<'a>) -> Result<(), ()> {
        // Quote start, i.e. `"   > text..."`.

        let bt = BacktrackPoint::new(self, reader);

        reader.consume_n_times(is_ws, 3);

        if reader.current_char() == '>' {
            reader.reset_buff();
            reader.bump();
            self.emit(reader, DescItemKind::Markup);
            reader.consume_n_times(is_ws, 1);
            reader.reset_buff();

            bt.commit(self, reader);
            Ok(())
        } else {
            bt.rollback(self, reader);
            Err(())
        }
    }

    fn try_process_indented<'a>(
        &mut self,
        reader: &mut Reader<'a>,
        indent: usize,
    ) -> Result<(), ()> {
        // Block indented by at least `indent` spaces. This continues a list,
        // i.e.:
        //
        //     - list
        //       list continuation, indented by at least 2 spaces.

        let bt = BacktrackPoint::new(self, reader);

        let found_indent = reader.consume_n_times(is_ws, indent);
        if reader.is_eof() || found_indent == indent {
            reader.reset_buff();
            bt.commit(self, reader);
            Ok(())
        } else {
            bt.rollback(self, reader);
            Err(())
        }
    }

    fn try_process_code<'a>(&mut self, reader: &mut Reader<'a>) -> Result<(), ()> {
        // Block indented by at least 4 spaces, i.e. `"    code"`.
        let bt = BacktrackPoint::new(self, reader);

        let found_indent = reader.consume_n_times(is_ws, 4);
        if found_indent == 4 || reader.is_eof() {
            reader.reset_buff();
            self.process_code_line(reader, CodeBlockLang::None);
            bt.commit(self, reader);
            Ok(())
        } else {
            bt.rollback(self, reader);
            Err(())
        }
    }

    fn try_process_list<'a>(&mut self, reader: &mut Reader<'a>) -> Result<(usize, usize), ()> {
        // Either numbered or non-numbered list start.
        let bt = BacktrackPoint::new(self, reader);
        let scope_start = reader.current_range().start_offset;

        let mut indent = reader.consume_n_times(is_ws, 3);
        match reader.current_char() {
            '-' | '*' | '+' => {
                indent += 2;
                reader.reset_buff();
                reader.bump();
                self.emit(reader, DescItemKind::Markup);
                if reader.is_eof() {
                    bt.commit(self, reader);
                    return Ok((indent, scope_start));
                } else if !is_ws(reader.current_char()) {
                    bt.rollback(self, reader);
                    return Err(());
                }
                reader.bump();
            }
            '0'..='9' => {
                reader.reset_buff();
                indent += reader.eat_while(|c| c.is_ascii_digit()) + 2;
                if !matches!(reader.current_char(), '.' | ')' | ':') {
                    bt.rollback(self, reader);
                    return Err(());
                }
                reader.bump();
                self.emit(reader, DescItemKind::Markup);
                if reader.is_eof() {
                    bt.commit(self, reader);
                    return Ok((indent, scope_start));
                } else if !is_ws(reader.current_char()) {
                    bt.rollback(self, reader);
                    return Err(());
                }
                reader.bump();
            }
            _ => {
                bt.rollback(self, reader);
                return Err(());
            }
        }

        let text = reader.tail_text();
        if text.len() >= 4 && text.is_char_boundary(4) && is_blank(&text[..4]) {
            // List marker followed by a space, then 4 more spaces
            // is parsed as a list marker followed by a space,
            // then code block.
            reader.reset_buff();
            bt.commit(self, reader);
            Ok((indent, scope_start))
        } else {
            // List marker followed by a space, then up to 3 more spaces
            // is parsed as a list marker
            indent += reader.eat_while(is_ws);
            reader.reset_buff();
            bt.commit(self, reader);
            Ok((indent, scope_start))
        }
    }

    fn try_process_fence_start<'a>(
        &mut self,
        reader: &mut Reader<'a>,
    ) -> Result<(usize, char, usize), ()> {
        // Start of a fenced block. MySt allows fenced blocks
        // using colons, i.e.:
        //
        //     :::syntax
        //     code
        //     :::

        let bt = BacktrackPoint::new(self, reader);
        let scope_start = reader.current_range().start_offset;

        reader.consume_n_times(is_ws, 3);
        match reader.current_char() {
            '`' => {
                reader.reset_buff();
                let n_fences = reader.eat_when('`');
                if n_fences < 3 {
                    bt.rollback(self, reader);
                    return Err(());
                }
                if reader.tail_text().contains('`') {
                    bt.rollback(self, reader);
                    return Err(());
                }
                self.emit(reader, DescItemKind::Markup);

                bt.commit(self, reader);
                Ok((n_fences, '`', scope_start))
            }
            '~' => {
                reader.reset_buff();
                let n_fences = reader.eat_when('~');
                if n_fences < 3 {
                    bt.rollback(self, reader);
                    return Err(());
                }
                self.emit(reader, DescItemKind::Markup);

                bt.commit(self, reader);
                Ok((n_fences, '~', scope_start))
            }
            ':' if self.enable_myst => {
                reader.reset_buff();
                let n_fences = reader.eat_when(':');
                if n_fences < 3 {
                    bt.rollback(self, reader);
                    return Err(());
                }
                self.emit(reader, DescItemKind::Markup);

                bt.commit(self, reader);
                Ok((n_fences, ':', scope_start))
            }
            _ => {
                bt.rollback(self, reader);
                Err(())
            }
        }
    }

    /// Improved fence start processing with better error handling
    fn try_process_fence_start_improved(
        &mut self,
        reader: &mut Reader,
    ) -> ParseResult<(usize, char, usize)> {
        let bt = BacktrackPoint::new(self, reader);
        let scope_start = reader.current_range().start_offset;

        reader.consume_n_times(is_ws, 3);
        match reader.current_char() {
            '`' => {
                reader.reset_buff();
                let n_fences = reader.eat_when('`');
                if n_fences < 3 {
                    bt.rollback(self, reader);
                    return Err(ParseError::InvalidFence);
                }
                if reader.tail_text().contains('`') {
                    bt.rollback(self, reader);
                    return Err(ParseError::InvalidSyntax);
                }
                self.emit(reader, DescItemKind::Markup);

                bt.commit(self, reader);
                Ok((n_fences, '`', scope_start))
            }
            '~' => {
                reader.reset_buff();
                let n_fences = reader.eat_when('~');
                if n_fences < 3 {
                    bt.rollback(self, reader);
                    return Err(ParseError::InvalidFence);
                }
                self.emit(reader, DescItemKind::Markup);

                bt.commit(self, reader);
                Ok((n_fences, '~', scope_start))
            }
            ':' if self.enable_myst => {
                reader.reset_buff();
                let n_fences = reader.eat_when(':');
                if n_fences < 3 {
                    bt.rollback(self, reader);
                    return Err(ParseError::InvalidFence);
                }
                self.emit(reader, DescItemKind::Markup);

                bt.commit(self, reader);
                Ok((n_fences, ':', scope_start))
            }
            _ => {
                bt.rollback(self, reader);
                Err(ParseError::InvalidSyntax)
            }
        }
    }

    fn try_process_fence_directive_name<'a>(
        &mut self,
        reader: &mut Reader<'a>,
    ) -> Result<(&'a str, &'a str), ()> {
        // MySt extension for embedding RST directives
        // into markdown code blocks:
        //
        //     ```{dir_name} dir_args
        //     :dir_short_param: dir_short_param_value
        //     dir_body
        //     ```

        if !self.enable_myst {
            return Err(());
        }

        let bt = BacktrackPoint::new(self, reader);

        if reader.current_char() != '{' {
            bt.rollback(self, reader);
            return Err(());
        }
        reader.bump();
        self.emit(reader, DescItemKind::Markup);
        reader.eat_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '+' | '_' | '-'));
        if reader.current_char() != '}' {
            bt.rollback(self, reader);
            return Err(());
        }
        let dir_name = reader.current_text();
        self.emit(reader, DescItemKind::Arg);
        reader.bump();
        self.emit(reader, DescItemKind::Markup);
        reader.eat_while(is_ws);
        reader.reset_buff();
        reader.eat_till_end();
        let dir_args = reader.current_text();
        self.emit(reader, DescItemKind::CodeBlock);
        bt.commit(self, reader);
        Ok((dir_name, dir_args))
    }

    fn try_process_fence_short_param<'a>(&mut self, reader: &mut Reader<'a>) -> Result<(), ()> {
        let bt = BacktrackPoint::new(self, reader);

        reader.eat_while(is_ws);
        if reader.current_char() != ':' {
            bt.rollback(self, reader);
            return Err(());
        }
        reader.reset_buff();
        reader.bump();
        self.emit(reader, DescItemKind::Markup);
        eat_rst_flag_body(reader);
        self.emit(reader, DescItemKind::Arg);
        if reader.current_char() != ':' {
            bt.rollback(self, reader);
            return Err(());
        }
        reader.bump();
        self.emit(reader, DescItemKind::Markup);
        reader.eat_while(is_ws);
        reader.reset_buff();
        reader.eat_till_end();
        self.emit(reader, DescItemKind::CodeBlock);
        bt.commit(self, reader);
        Ok(())
    }

    fn try_process_fence_long_params_marker<'a>(
        &mut self,
        reader: &mut Reader<'a>,
    ) -> Result<(), ()> {
        let bt = BacktrackPoint::new(self, reader);

        reader.eat_while(is_ws);
        if !reader.tail_text().starts_with("---") {
            bt.rollback(self, reader);
            return Err(());
        }
        if !is_blank(&reader.tail_text()[3..]) {
            bt.rollback(self, reader);
            return Err(());
        }
        reader.reset_buff();
        reader.bump();
        reader.bump();
        reader.bump();
        self.emit(reader, DescItemKind::Markup);
        reader.eat_till_end();
        reader.reset_buff();
        bt.commit(self, reader);
        Ok(())
    }

    fn try_process_fence_end<'a>(
        &mut self,
        reader: &mut Reader<'a>,
        n_fences: usize,
        fence: char,
    ) -> Result<(), ()> {
        let bt = BacktrackPoint::new(self, reader);

        reader.consume_n_times(is_ws, 3);
        reader.reset_buff();
        if reader.eat_when(fence) != n_fences {
            bt.rollback(self, reader);
            return Err(());
        }
        if !is_blank(reader.tail_text()) {
            bt.rollback(self, reader);
            return Err(());
        }
        self.emit(reader, DescItemKind::Markup);
        reader.eat_till_end();
        reader.reset_buff();

        bt.commit(self, reader);
        Ok(())
    }

    fn try_process_math<'a>(&mut self, reader: &mut Reader<'a>) -> Result<usize, ()> {
        // MySt extension for LaTaX-like math markup:
        //
        //     $$
        //     \frac{1}{2}
        //     $$ (anchor)

        if !self.enable_myst {
            return Err(());
        }

        let bt = BacktrackPoint::new(self, reader);
        let scope_start = reader.current_range().start_offset;

        reader.consume_n_times(is_ws, 3);
        if reader.current_char() == '$' && reader.next_char() == '$' {
            reader.reset_buff();
            reader.bump();
            reader.bump();
            if !is_blank(reader.tail_text()) {
                bt.rollback(self, reader);
                return Err(());
            }
            self.emit(reader, DescItemKind::Markup);
            reader.eat_till_end();
            reader.reset_buff();

            bt.commit(self, reader);
            Ok(scope_start)
        } else {
            bt.rollback(self, reader);
            Err(())
        }
    }

    fn try_process_math_end<'a>(&mut self, reader: &mut Reader<'a>) -> Result<(), ()> {
        // MySt extension for LaTaX-like math markup:
        //
        //     $$
        //     \frac{1}{2}
        //     $$ (anchor)

        if !self.enable_myst {
            return Err(());
        }

        let bt = BacktrackPoint::new(self, reader);

        reader.consume_n_times(is_ws, 3);
        if reader.current_char() == '$' && reader.next_char() == '$' {
            reader.reset_buff();
            reader.bump();
            reader.bump();
            self.emit(reader, DescItemKind::Markup);
            reader.eat_while(is_ws);
            reader.reset_buff();
            if reader.current_char() == '(' {
                reader.bump();
                reader.eat_while(|c| {
                    c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '+' | '_' | '-')
                });
                if reader.current_char() != ')' {
                    bt.rollback(self, reader);
                    return Err(());
                }
                reader.bump();
                self.emit(reader, DescItemKind::Arg);
            }
            reader.eat_till_end();
            reader.reset_buff();

            bt.commit(self, reader);
            Ok(())
        } else {
            bt.rollback(self, reader);
            Err(())
        }
    }

    fn process_code_line(&mut self, reader: &mut Reader, lang: CodeBlockLang) {
        if self.cursor_position.is_some() {
            // No point in calculating this when all we care
            // is what's under the user's cursor.
            return;
        }

        reader.eat_till_end();
        if lang != CodeBlockLang::None && self.cursor_position.is_none() {
            let line_range = reader.current_range();
            let prev_reader = reader.reset_buff_into_sub_reader();
            self.state = process_code(self, line_range, prev_reader, self.state, lang);
        } else {
            self.emit(reader, DescItemKind::CodeBlock);
        }
    }

    fn process_inline_content(&mut self, reader: &mut Reader) {
        assert!(self.inline_state.is_empty());

        if self
            .cursor_position
            .is_some_and(|offset| !reader.tail_range().contains_inclusive(offset))
        {
            // No point in calculating this when all we care
            // is what's under the user's cursor.
            return;
        }

        while !reader.is_eof() {
            match reader.current_char() {
                '\\' => {
                    reader.bump();
                    reader.bump();
                }
                '`' => {
                    let bt = BacktrackPoint::new(self, reader);

                    let prev = reader.reset_buff_into_sub_reader();
                    let after_prev = reader.current_char();

                    if !Self::eat_inline_code(reader, None) {
                        bt.rollback(self, reader);
                        reader.bump();
                        // guard.backtrack(reader);
                        continue;
                    }

                    self.process_inline_content_style(prev, after_prev);
                    process_inline_code(
                        self,
                        reader.reset_buff_into_sub_reader(),
                        DescItemKind::Code,
                    );

                    bt.commit(self, reader);
                }
                '$' if self.enable_myst => {
                    let bt = BacktrackPoint::new(self, reader);

                    let prev = reader.reset_buff_into_sub_reader();
                    let after_prev = reader.current_char();

                    if !Self::eat_inline_math(reader) {
                        bt.rollback(self, reader);
                        reader.bump();
                        // guard.backtrack(reader);
                        continue;
                    }

                    self.process_inline_content_style(prev, after_prev);

                    self.process_inline_math(reader.reset_buff_into_sub_reader());

                    bt.commit(self, reader);
                }
                '[' => {
                    let bt = BacktrackPoint::new(self, reader);

                    let prev = reader.reset_buff_into_sub_reader();
                    let after_prev = reader.current_char();

                    if !Self::eat_link_title(reader) {
                        bt.rollback(self, reader);
                        reader.bump();
                        // guard.backtrack(reader);
                        continue;
                    }

                    let title_range = reader.current_range();
                    reader.reset_buff();

                    if reader.current_char() == '(' && !Self::eat_link_url(reader) {
                        bt.rollback(self, reader);
                        reader.bump();
                        // guard.backtrack(reader);
                        continue;
                    }

                    let url_range = reader.current_range();
                    reader.reset_buff();

                    self.process_inline_content_style(prev, after_prev);

                    self.emit_range(title_range, DescItemKind::Link);
                    self.emit_range(url_range, DescItemKind::Link);

                    bt.commit(self, reader);
                }
                '{' => {
                    let prev = reader.reset_buff_into_sub_reader();
                    let after_prev = reader.current_char();

                    // Try MyST inline role first (if MyST is enabled)
                    if self.enable_myst {
                        let bt = BacktrackPoint::new(self, reader);
                        if Self::eat_myst_inline_role(reader) {
                            self.process_inline_content_style(prev, after_prev);
                            self.process_myst_inline_role(reader.reset_buff_into_sub_reader());
                            bt.commit(self, reader);
                            continue;
                        }
                        bt.rollback(self, reader);
                    }

                    // Try Javadoc link
                    let bt = BacktrackPoint::new(self, reader);
                    if Self::eat_javadoc_link(reader) {
                        self.process_inline_content_style(prev, after_prev);
                        self.process_javadoc_link(reader.reset_buff_into_sub_reader());
                        bt.commit(self, reader);
                        continue;
                    }

                    bt.rollback(self, reader);

                    reader.bump();
                }
                _ => {
                    reader.bump();
                }
            }
        }

        if !reader.current_range().is_empty() {
            self.process_inline_content_style(reader.reset_buff_into_sub_reader(), ' ');
        }
        self.inline_state.clear();
    }

    #[must_use]
    fn eat_inline_code(reader: &mut Reader, cursor_position: Option<usize>) -> bool {
        let n_backticks = reader.eat_when('`');
        if n_backticks == 0 {
            return false;
        }
        while !reader.is_eof() {
            if reader.current_char() == '`' {
                let found_n_backticks = reader.eat_when('`');
                if found_n_backticks == n_backticks {
                    return true;
                }
            } else {
                reader.bump();
            }
        }

        if let Some(cursor_position) = cursor_position {
            reader.current_range().contains_inclusive(cursor_position)
        } else {
            false
        }
    }

    #[must_use]
    fn eat_inline_math(reader: &mut Reader) -> bool {
        let n_marks = reader.eat_when('$');
        if n_marks == 0 || n_marks > 2 {
            return false;
        }
        while !reader.is_eof() {
            if reader.current_char() == '$' {
                let found_n_marks = reader.eat_when('$');
                if found_n_marks == n_marks {
                    return true;
                }
            } else {
                reader.bump();
            }
        }

        false
    }

    #[must_use]
    fn eat_link_title(reader: &mut Reader) -> bool {
        if reader.current_char() != '[' {
            return false;
        }
        reader.bump();

        let mut depth = 1;

        while !reader.is_eof() {
            match reader.current_char() {
                '[' => {
                    depth += 1;
                    reader.bump();
                }
                ']' => {
                    depth -= 1;
                    reader.bump();
                    if depth == 0 {
                        return true;
                    }
                }
                '\\' => {
                    reader.bump();
                    reader.bump();
                }
                '`' => {
                    let prev_reader = reader.clone();
                    if !Self::eat_inline_code(reader, None) {
                        *reader = prev_reader;
                        reader.bump();
                    }
                }
                '$' => {
                    let prev_reader = reader.clone();
                    if !Self::eat_inline_math(reader) {
                        *reader = prev_reader;
                        reader.bump();
                    }
                }
                _ => reader.bump(),
            }
        }

        false
    }

    #[must_use]
    fn eat_link_url(reader: &mut Reader) -> bool {
        if reader.current_char() != '(' {
            return false;
        }
        reader.bump();

        if reader.current_char() == '<' {
            while !reader.is_eof() {
                if reader.current_char() == '>' && reader.next_char() == ')' {
                    reader.bump();
                    reader.bump();
                    return true;
                } else if reader.current_char() == '\\' {
                    reader.bump();
                    reader.bump();
                } else {
                    reader.bump();
                }
            }
        } else {
            let mut depth = 1;

            while !reader.is_eof() {
                match reader.current_char() {
                    '(' => {
                        depth += 1;
                        reader.bump();
                    }
                    ')' => {
                        depth -= 1;
                        reader.bump();
                        if depth == 0 {
                            return true;
                        }
                    }
                    '\\' => {
                        reader.bump();
                        reader.bump();
                    }
                    ' ' | '\t' => {
                        return false;
                    }
                    _ => reader.bump(),
                }
            }
        }

        false
    }

    #[must_use]
    fn eat_javadoc_link(reader: &mut Reader) -> bool {
        // Parse {@link class#method} or {@link class.method} format
        if reader.current_char() != '{' {
            return false;
        }
        reader.bump();

        // Expect '@link'
        if !reader.tail_text().starts_with("@link") {
            return false;
        }

        // Consume '@link'
        for _ in 0..5 {
            reader.bump();
        }

        // Consume whitespace after @link
        if !reader.current_char().is_whitespace() {
            return false;
        }
        reader.eat_while(|c| c.is_whitespace());

        // Parse the reference (class#method or class.method)
        while !reader.is_eof() {
            match reader.current_char() {
                '}' => {
                    reader.bump();
                    return true;
                }
                '\\' => {
                    reader.bump();
                    reader.bump();
                }
                _ => reader.bump(),
            }
        }

        false
    }

    #[must_use]
    fn eat_myst_inline_role(reader: &mut Reader) -> bool {
        // Parse {role}`content` format
        if reader.current_char() != '{' {
            return false;
        }
        reader.bump();

        // Eat role name (can contain letters, numbers, colon, dash, underscore)
        let role_start = reader.current_range().start_offset;
        reader.eat_while(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_' | '.'));
        let role_end = reader.current_range().end_offset();

        if role_start == role_end {
            return false; // No role name found
        }

        if reader.current_char() != '}' {
            return false;
        }
        reader.bump();

        if reader.current_char() != '`' {
            return false;
        }

        // Find the matching closing backtick
        reader.bump();
        while !reader.is_eof() {
            if reader.current_char() == '`' {
                reader.bump();
                return true;
            } else if reader.current_char() == '\\' {
                reader.bump();
                if !reader.is_eof() {
                    reader.bump();
                }
            } else {
                reader.bump();
            }
        }

        false
    }

    fn process_inline_math(&mut self, mut reader: Reader) {
        let n_backticks = reader.eat_when('$');
        self.emit(&mut reader, DescItemKind::Markup);
        while reader.tail_range().length > n_backticks {
            reader.bump();
        }
        self.emit(&mut reader, DescItemKind::Code);
        reader.eat_till_end();
        self.emit(&mut reader, DescItemKind::Markup);
    }

    fn process_myst_inline_role(&mut self, mut reader: Reader) {
        // Process {role}`content` format
        // Emit opening brace as markup
        reader.bump(); // consume '{'
        self.emit(&mut reader, DescItemKind::Markup);

        // Emit role name as Arg
        reader.eat_while(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_' | '.'));
        let role_text = reader.current_text();

        // Check if this is a lua role
        let is_lua_ref = role_text.starts_with("lua:")
            || (self.primary_domain.as_deref() == Some("lua")
                && !role_text.contains(":")
                && crate::util::is_lua_role(role_text));

        self.emit(&mut reader, DescItemKind::Arg);

        // Emit closing brace and opening backtick as markup
        reader.bump(); // consume '}'
        reader.bump(); // consume '`'
        self.emit(&mut reader, DescItemKind::Markup);

        // Process content until closing backtick
        reader.reset_buff();

        if is_lua_ref {
            self.process_myst_lua_content(&mut reader);
        } else {
            // For non-lua roles, treat as code
            while !reader.is_eof() && reader.current_char() != '`' {
                if reader.current_char() == '\\' {
                    reader.bump();
                    if !reader.is_eof() {
                        reader.bump();
                    }
                } else {
                    reader.bump();
                }
            }
            self.emit(&mut reader, DescItemKind::Code);
        }

        // Emit closing backtick as markup
        if reader.current_char() == '`' {
            reader.bump();
            self.emit(&mut reader, DescItemKind::Markup);
        }
    }

    fn process_myst_lua_content(&mut self, reader: &mut Reader) {
        // Handle special lua reference patterns
        if reader.is_eof() || reader.current_char() == '`' {
            // Empty content
            self.emit(reader, DescItemKind::Ref);
            return;
        }

        if reader.current_char() == '~' {
            // Short form: ~ref
            reader.bump();
            self.emit(reader, DescItemKind::Code);
            reader.reset_buff();

            // Rest is reference
            while !reader.is_eof() && reader.current_char() != '`' {
                reader.bump();
            }
            self.emit(reader, DescItemKind::Ref);
        } else if reader.current_char() == '<' {
            // Angle bracket format: <ref> or <~ref>
            reader.bump();
            self.emit(reader, DescItemKind::Code);
            reader.reset_buff();

            if reader.current_char() == '~' {
                reader.bump();
                self.emit(reader, DescItemKind::Code);
                reader.reset_buff();
            }

            // Reference part
            while !reader.is_eof() && reader.current_char() != '>' && reader.current_char() != '`' {
                reader.bump();
            }
            self.emit(reader, DescItemKind::Ref);

            // Closing >
            reader.reset_buff();
            if reader.current_char() == '>' {
                reader.bump();
                self.emit(reader, DescItemKind::Code);
            }
        } else {
            // Check for title format by looking ahead
            let mut title_present = false;
            let mut temp_pos = 0;
            let mut temp_reader = reader.clone();

            // Look for " <" pattern
            while !temp_reader.is_eof() && temp_reader.current_char() != '`' {
                if temp_reader.current_char() == ' ' && temp_reader.next_char() == '<' {
                    title_present = true;
                    break;
                }
                temp_reader.bump();
                temp_pos += 1;
            }

            if title_present {
                // Title format: "title <ref>" or "title <~ref>"
                for _ in 0..temp_pos {
                    reader.bump();
                }
                reader.bump(); // space
                reader.bump(); // <
                self.emit(reader, DescItemKind::Code);
                reader.reset_buff();

                if reader.current_char() == '~' {
                    reader.bump();
                    self.emit(reader, DescItemKind::Code);
                    reader.reset_buff();
                }

                // Reference
                while !reader.is_eof()
                    && reader.current_char() != '>'
                    && reader.current_char() != '`'
                {
                    reader.bump();
                }
                self.emit(reader, DescItemKind::Ref);

                // Closing >
                reader.reset_buff();
                if reader.current_char() == '>' {
                    reader.bump();
                    self.emit(reader, DescItemKind::Code);
                }
            } else {
                // Plain reference
                while !reader.is_eof() && reader.current_char() != '`' {
                    reader.bump();
                }
                self.emit(reader, DescItemKind::Ref);
            }
        }
    }

    fn process_javadoc_link(&mut self, mut reader: Reader) {
        // Process {@link class#method} or {@link class.method} format
        // The reader should already be positioned at the start of the javadoc link

        // Emit opening brace as markup
        reader.bump(); // consume '{'
        self.emit(
            &mut reader,
            DescItemKind::CodeBlockHl(CodeBlockHighlightKind::Operators),
        );

        // Emit '@link' as markup
        reader.bump(); // consume '@'
        reader.eat_while(|c| c.is_ascii_alphabetic()); // consume 'link'
        self.emit(
            &mut reader,
            DescItemKind::CodeBlockHl(CodeBlockHighlightKind::Decorator),
        );

        // Skip whitespace and reset buffer for content
        reader.eat_while(|c| c.is_whitespace());
        reader.reset_buff();

        // Process the reference content until closing brace
        while !reader.is_eof() && reader.current_char() != '}' {
            if reader.current_char() == '\\' {
                reader.bump();
                if !reader.is_eof() {
                    reader.bump();
                }
            } else {
                reader.bump();
            }
        }

        // Emit the link reference content
        self.emit(&mut reader, DescItemKind::JavadocLink);

        // Emit closing brace as markup
        if reader.current_char() == '}' {
            reader.bump();
            self.emit(
                &mut reader,
                DescItemKind::CodeBlockHl(CodeBlockHighlightKind::Operators),
            );
        }
    }

    fn process_inline_content_style(&mut self, mut reader: Reader, char_after: char) {
        if self.cursor_position.is_some() {
            // No point in calculating this when all we care
            // is what's under the user's cursor.
            return;
        }

        let char_after = if char_after == '\0' { ' ' } else { char_after };
        while !reader.is_eof() {
            match reader.current_char() {
                '\\' => {
                    reader.reset_buff();
                    reader.bump();
                    reader.bump();
                    self.emit(&mut reader, DescItemKind::Markup);
                }
                ch @ '*' | ch @ '_' => {
                    reader.reset_buff();

                    let mut left_char = reader.prev_char();
                    let n_chars = reader.eat_when(ch);
                    let mut right_char = reader.current_char();

                    if left_char == '\0' {
                        left_char = ' ';
                    }
                    if right_char == '\0' {
                        right_char = char_after;
                    }

                    let left_is_punct = is_punct(left_char);
                    let left_is_ws = left_char.is_whitespace();
                    let right_is_punct = is_punct(left_char);
                    let right_is_ws = right_char.is_whitespace();

                    let is_left_flanking =
                        !right_is_ws && (!right_is_punct || (left_is_ws || left_is_punct));
                    let is_right_flanking =
                        !left_is_ws && (!left_is_punct || (right_is_ws || right_is_punct));

                    let (can_start_highlight, can_end_highlight) = if ch == '*' {
                        (is_left_flanking, is_right_flanking)
                    } else {
                        (
                            is_left_flanking && (!is_right_flanking || left_is_punct),
                            is_right_flanking && (!is_left_flanking || right_is_punct),
                        )
                    };

                    if can_start_highlight && can_end_highlight {
                        if self.has_highlight(ch, n_chars) {
                            self.end_highlight(ch, n_chars, &mut reader);
                        } else {
                            self.start_highlight(ch, n_chars, &mut reader);
                        }
                    } else if can_start_highlight {
                        self.start_highlight(ch, n_chars, &mut reader);
                    } else if can_end_highlight {
                        self.end_highlight(ch, n_chars, &mut reader);
                    }
                }
                _ => {
                    reader.bump();
                }
            }
        }

        reader.reset_buff();
    }

    fn flush_state(&mut self, end: usize, reader: &mut Reader) {
        let drained_states: Vec<State> = self.states.drain(end..).rev().collect();

        for state in drained_states {
            let scope_start = match state {
                State::Quote { scope_start, .. } => scope_start,
                State::Indented { scope_start, .. } => scope_start,
                State::Code { scope_start, .. } => scope_start,
                State::FencedCode { scope_start, .. } => scope_start,
                State::FencedDirectiveParams { scope_start, .. } => scope_start,
                State::FencedDirectiveParamsLong { scope_start, .. } => scope_start,
                State::FencedDirectiveBody { scope_start, .. } => scope_start,
                State::Math { scope_start, .. } => scope_start,
            };

            self.state = LexerState::Normal;
            let scope_end = reader.current_range().end_offset();
            self.emit_range(
                SourceRange::from_start_end(scope_start, scope_end),
                DescItemKind::Scope,
            );
        }
    }

    fn has_highlight(&mut self, r_ch: char, r_n_chars: usize) -> bool {
        match self.inline_state.last() {
            Some(&InlineState::Em(ch, ..)) => ch == r_ch && r_n_chars == 1,
            Some(&InlineState::Strong(ch, ..)) => ch == r_ch && r_n_chars == 2,
            Some(&InlineState::Both(ch, ..)) => ch == r_ch,
            _ => false,
        }
    }

    fn start_highlight(&mut self, r_ch: char, r_n_chars: usize, reader: &mut Reader) {
        match r_n_chars {
            0 => {}
            1 => self.inline_state.push(InlineState::Em(
                r_ch,
                reader.current_range(),
                reader.current_range().start_offset,
            )),
            2 => self.inline_state.push(InlineState::Strong(
                r_ch,
                reader.current_range(),
                reader.current_range().start_offset,
            )),
            _ => self.inline_state.push(InlineState::Both(
                r_ch,
                reader.current_range(),
                reader.current_range().start_offset,
            )),
        }
        reader.reset_buff();
    }

    fn end_highlight(&mut self, r_ch: char, mut r_n_chars: usize, reader: &mut Reader) {
        while r_n_chars > 0 {
            match self.inline_state.last() {
                Some(InlineState::Em(ch, ..)) => {
                    if ch == &r_ch && (r_n_chars == 1 || r_n_chars >= 3) {
                        let Some(InlineState::Em(_, start_markup_range, scope_start)) =
                            self.inline_state.pop()
                        else {
                            unreachable!();
                        };

                        let scope_end = reader.current_range().end_offset();
                        self.emit_range(
                            SourceRange::from_start_end(scope_start, scope_end),
                            DescItemKind::Em,
                        );
                        self.emit_range(start_markup_range, DescItemKind::Markup);
                        self.emit(reader, DescItemKind::Markup);

                        r_n_chars -= 1;
                    } else {
                        break;
                    }
                }
                Some(InlineState::Strong(ch, ..)) => {
                    if ch == &r_ch && r_n_chars >= 2 {
                        let Some(InlineState::Strong(_, start_markup_range, scope_start)) =
                            self.inline_state.pop()
                        else {
                            unreachable!();
                        };

                        let scope_end = reader.current_range().end_offset();
                        self.emit_range(
                            SourceRange::from_start_end(scope_start, scope_end),
                            DescItemKind::Strong,
                        );
                        self.emit_range(start_markup_range, DescItemKind::Markup);
                        self.emit(reader, DescItemKind::Markup);

                        r_n_chars -= 2;
                    } else {
                        break;
                    }
                }
                Some(InlineState::Both(ch, ..)) => {
                    if ch == &r_ch {
                        let Some(InlineState::Both(_, start_markup_range, scope_start)) =
                            self.inline_state.pop()
                        else {
                            unreachable!();
                        };

                        let scope_end = reader.current_range().end_offset();
                        self.emit_range(
                            SourceRange::from_start_end(scope_start, scope_end),
                            DescItemKind::Em,
                        );
                        self.emit_range(
                            SourceRange::from_start_end(scope_start, scope_end),
                            DescItemKind::Strong,
                        );
                        self.emit_range(start_markup_range, DescItemKind::Markup);
                        self.emit(reader, DescItemKind::Markup);

                        if r_n_chars == 1 {
                            self.start_highlight(r_ch, 2, reader);
                            r_n_chars = 0;
                        } else if r_n_chars == 2 {
                            self.start_highlight(r_ch, 1, reader);
                            r_n_chars = 0;
                        } else {
                            r_n_chars -= 3;
                        }
                    } else {
                        break;
                    }
                }
                _ => {
                    break;
                }
            }
        }
        reader.reset_buff();
    }
}
