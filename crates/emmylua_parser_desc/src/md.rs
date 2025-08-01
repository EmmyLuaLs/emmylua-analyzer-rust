use crate::rst::{eat_rst_flag_body, process_inline_code, process_lua_ref};
use crate::util::{
    BacktrackPoint, ResultContainer, desc_to_lines, is_blank, is_code_directive, is_lua_role,
    is_punct, is_ws, process_lua_code,
};
use crate::{DescItem, DescItemKind, LuaDescParser};
use emmylua_parser::{LexerConfig, LuaLexer, LuaLexerState, Reader, SourceRange};
use emmylua_parser::{LuaAstNode, LuaDocDescription};
use std::cell::RefCell;
use std::rc::Rc;

pub struct MdParser {
    states: Rc<RefCell<Vec<State>>>,
    inline_state: Vec<InlineState>,
    primary_domain: Option<String>,
    enable_myst: bool,
    results: Vec<DescItem>,
    cursor_position: Option<usize>,
    lua_lexer_state: LuaLexerState,
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
        is_lua: bool,
        scope_start: usize,
    },
    FencedDirectiveParams {
        n_fences: usize,
        fence: char,
        is_code: bool,
        is_lua: bool,
        scope_start: usize,
    },
    FencedDirectiveParamsLong {
        n_fences: usize,
        fence: char,
        is_code: bool,
        is_lua: bool,
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

impl LuaDescParser for MdParser {
    fn parse(&mut self, text: &str, desc: LuaDocDescription) -> Vec<DescItem> {
        assert!(self.results.is_empty());

        self.states.borrow_mut().clear();
        self.inline_state.clear();

        let desc_end = desc.get_range().end().into();

        for range in desc_to_lines(text, desc, self.cursor_position) {
            // Process line.
            let line = &text[range.start_offset..range.end_offset()];
            self.process_line(&mut Reader::new_with_range(line, range));
        }

        self.flush_state(
            &mut self.states.clone().borrow_mut(),
            0,
            &mut Reader::new_with_range("", SourceRange::new(desc_end, 0)),
        );

        std::mem::take(&mut self.results)
    }
}

impl ResultContainer for MdParser {
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

impl MdParser {
    pub fn new(cursor_position: Option<usize>) -> Self {
        Self {
            states: Default::default(),
            inline_state: Default::default(),
            primary_domain: None,
            enable_myst: false,
            results: Vec::new(),
            cursor_position,
            lua_lexer_state: LuaLexerState::Normal,
        }
    }

    pub fn new_myst(primary_domain: Option<String>, cursor_position: Option<usize>) -> Self {
        Self {
            states: Default::default(),
            inline_state: Default::default(),
            primary_domain,
            enable_myst: true,
            results: Vec::new(),
            cursor_position,
            lua_lexer_state: LuaLexerState::Normal,
        }
    }

    fn process_line(&mut self, reader: &mut Reader) {
        // First, find out which blocks are still present
        // and which finished.
        let mut last_state = 0;
        let states = self.states.clone();
        let mut states = states.borrow_mut();
        for (i, &state) in states.iter().enumerate() {
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
                    is_lua,
                    ..
                } => {
                    if self.try_process_fence_end(reader, n_fences, fence).is_ok() {
                        self.flush_state(&mut states, i, reader);
                        return;
                    } else {
                        self.process_code_line(reader, is_lua);
                        return;
                    }
                }
                State::FencedDirectiveParams {
                    n_fences,
                    fence,
                    is_code,
                    is_lua,
                    scope_start,
                } => {
                    if self.try_process_fence_end(reader, n_fences, fence).is_ok() {
                        self.flush_state(&mut states, i, reader);
                        return;
                    } else if self.try_process_fence_long_params_marker(reader).is_ok() {
                        self.flush_state(&mut states, i + 1, reader);
                        states.pop();
                        states.push(State::FencedDirectiveParamsLong {
                            n_fences,
                            fence,
                            is_code,
                            is_lua,
                            scope_start,
                        });
                        return;
                    }
                    if self.try_process_fence_short_param(reader).is_ok() {
                        return;
                    } else if is_code {
                        self.flush_state(&mut states, i + 1, reader);
                        states.pop();
                        states.push(State::FencedCode {
                            n_fences,
                            fence,
                            is_lua,
                            scope_start,
                        });
                        self.process_code_line(reader, is_lua);
                        return;
                    } else {
                        self.flush_state(&mut states, i + 1, reader);
                        states.pop();
                        states.push(State::FencedDirectiveBody {
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
                    is_code,
                    is_lua,
                    scope_start,
                } => {
                    if self.try_process_fence_end(reader, n_fences, fence).is_ok() {
                        self.flush_state(&mut states, i, reader);
                        return;
                    } else if self.try_process_fence_long_params_marker(reader).is_ok() {
                        self.flush_state(&mut states, i + 1, reader);
                        states.pop();
                        if is_code {
                            states.push(State::FencedCode {
                                n_fences,
                                fence,
                                is_lua,
                                scope_start,
                            });
                        } else {
                            states.push(State::FencedDirectiveBody {
                                n_fences,
                                fence,
                                scope_start,
                            });
                        }
                        return;
                    } else {
                        self.process_code_line(reader, is_lua);
                        return;
                    }
                }
                State::FencedDirectiveBody {
                    n_fences, fence, ..
                } => {
                    if self.try_process_fence_end(reader, n_fences, fence).is_ok() {
                        self.flush_state(&mut states, i, reader);
                        return;
                    } else {
                        // Continue with nested states.
                    }
                }
                State::Math { .. } => {
                    if self.try_process_math_end(reader).is_ok() {
                        self.flush_state(&mut states, i, reader);
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

        drop(states);

        self.flush_state(&mut self.states.clone().borrow_mut(), last_state, reader);

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
                    self.states.borrow_mut().push(State::Indented {
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
                if let Ok((n_fences, fence, scope_start)) = self.try_process_fence_start(reader) {
                    if let Ok((dir_name, dir_args)) = self.try_process_fence_directive_name(reader)
                    {
                        // This is a directive.
                        let is_code = is_code_directive(dir_name);
                        let is_lua = is_code && dir_args.trim() == "lua";
                        self.states.borrow_mut().push(State::FencedDirectiveParams {
                            n_fences,
                            fence,
                            is_code,
                            is_lua,
                            scope_start,
                        });
                    } else {
                        // This is a code block.
                        reader.eat_till_end();
                        let is_lua = reader.current_text().trim() == "lua";
                        self.emit(reader, DescItemKind::CodeBlock);
                        self.states.borrow_mut().push(State::FencedCode {
                            n_fences,
                            fence,
                            is_lua,
                            scope_start,
                        });
                    }
                    return NO_MORE_CONTENT;
                } else {
                    // This is a normal text, continue to inline parsing.
                }
            }
            // Indented code.
            ' ' | '\t' => {
                let scope_start = reader.current_range().start_offset;
                reader.bump();
                reader.reset_buff();
                reader.eat_till_end();
                self.emit(reader, DescItemKind::CodeBlock);
                self.states.borrow_mut().push(State::Code { scope_start });
                return NO_MORE_CONTENT;
            }
            // Numbered list.
            '0'..='9' => {
                if let Ok((indent_more, scope_start)) = self.try_process_list(reader) {
                    indent += indent_more;
                    self.states.borrow_mut().push(State::Indented {
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
                    self.states.borrow_mut().push(State::Quote { scope_start });
                    return HAS_MORE_CONTENT;
                } else {
                    // This is a normal text, continue to inline parsing.
                }
            }
            // Math block.
            '$' if self.enable_myst => {
                if let Ok(scope_start) = self.try_process_math(reader) {
                    self.states.borrow_mut().push(State::Math { scope_start });
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
            self.process_code_line(reader, false);
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
        if text.len() >= 4 && is_blank(&text[..4]) {
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
        if !is_blank(&reader.tail_text()) {
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
            if !is_blank(&reader.tail_text()) {
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

    fn process_code_line(&mut self, reader: &mut Reader, is_lua: bool) {
        if self.cursor_position.is_some() {
            // No point in calculating this when all we care
            // is what's under the user's cursor.
            return;
        }

        reader.eat_till_end();
        if is_lua && self.cursor_position.is_none() {
            let line_range = reader.current_range();
            let mut lexer = LuaLexer::new_with_state(
                reader.reset_buff_into_sub_reader(),
                self.lua_lexer_state,
                LexerConfig::default(),
                None,
            );
            let lua_tokens = lexer.tokenize();
            self.lua_lexer_state = lexer.get_state();

            process_lua_code(self, line_range, lua_tokens);
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
                '{' if self.enable_myst => {
                    let bt = BacktrackPoint::new(self, reader);

                    let prev = reader.reset_buff_into_sub_reader();
                    let after_prev = reader.current_char();

                    let Ok(role_text) = self.process_role_name(reader) else {
                        bt.rollback(self, reader);
                        reader.bump();
                        // guard.backtrack(reader);
                        continue;
                    };

                    if !Self::eat_inline_code(reader, self.cursor_position) {
                        bt.rollback(self, reader);
                        reader.bump();
                        // guard.backtrack(reader);
                        continue;
                    }

                    let code = reader.reset_buff_into_sub_reader();

                    self.process_inline_content_style(prev, after_prev);

                    let is_lua_ref = role_text.starts_with("lua:")
                        || (self.primary_domain.as_deref() == Some("lua")
                            && !role_text.contains(":")
                            && is_lua_role(role_text));

                    if is_lua_ref {
                        process_lua_ref(self, code);
                    } else {
                        process_inline_code(self, code, DescItemKind::Code);
                    }

                    bt.commit(self, reader);
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

    fn process_role_name<'a>(&mut self, reader: &mut Reader<'a>) -> Result<&'a str, ()> {
        if reader.current_char() != '{' {
            return Err(());
        }
        reader.bump();
        self.emit(reader, DescItemKind::Markup);
        reader.eat_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '+' | '_' | '-'));
        if reader.current_char() == '}' {
            let role_text = reader.current_text();
            self.emit(reader, DescItemKind::Arg);
            reader.bump();
            self.emit(reader, DescItemKind::Markup);
            Ok(role_text)
        } else {
            Err(())
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

                    let can_start_highlight;
                    let can_end_highlight;
                    if ch == '*' {
                        can_start_highlight = is_left_flanking;
                        can_end_highlight = is_right_flanking;
                    } else {
                        can_start_highlight =
                            is_left_flanking && (!is_right_flanking || left_is_punct);
                        can_end_highlight =
                            is_right_flanking && (!is_left_flanking || right_is_punct);
                    }

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

    fn flush_state(&mut self, states: &mut Vec<State>, end: usize, reader: &mut Reader) {
        for state in states.drain(end..).rev() {
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

            self.lua_lexer_state = LuaLexerState::Normal;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testlib::test;

    #[test]
    fn test_md() {
        let code = r#"
--- # Inline code
---
--- `code`
--- `` code ` with ` backticks ``
--- `code``with``backticks`
--- `broken code
--- [link]
--- [link `with backticks]`]
--- [link [with brackets] ]
--- [link](explicit_href)
--- [link](explicit()href)
--- [link](<explicit)href>)
--- Paragraph with `code`!
--- Paragraph with [link]!
--- \` escaped backtick
--- *em* em*in*text
--- _em_ em_in_text
--- **strong** strong**in**text
--- __strong__ strong__in__text
--- broken *em
--- broken em*
--- broken **strong
--- broken strong**
--- ***both*** both***in***text
--- ***both end* separately**
--- ***both end** separately*
--- *both **start separately***
--- **both *start separately***
--- *`foo`*
---
--- # Blocks
---
--- ## Thematic breaks
---
--- - - -
--- _ _ _
--- * * *
--- - _ -
---
--- ## Lists
---
--- - List
--- * List 2
--- + List 3
--- -Broken list
---
--- -    List with indented text
---
--- -     List with code
---       Continuation
---
---       Continuation 2
---
--- -
---   List that starts with empty string
---
---  Not list
---
---   -  not code
---
---         still not code
---
---     code
---
--- ## Numbered lists
---
--- 1. List
--- 2: List
--- 3) List
---   Not list
---
--- ## Code
---
---     This is code
---      This is also code
---       function foo() end
---
--- ## Fenced code
---
--- ```syntax
--- code
--- ```
--- not code
--- ~~~syntax
--- code
--- ```
--- still code
--- ~~~
---
--- ````code with 4 fences
--- ```
--- ````
---
--- ```inline code```
--- not code
---
--- ```lua
--- function foo()
---     local long_string = [[
---         content
---     ]]
--- end
--- ```
---
--- ## Quotes
---
--- > Quote
--- > Continues
---
--- > Quote 2
---
--- ## Disabled MySt extensions
---
--- $$
--- math
--- $$
---
--- ```{directive}
--- ```
---
--- ## Link anchor
---
--- [link]: https://example.com
"#;
        let expected = r#"
--- <Scope><Markup>#</Markup> Inline code</Scope>
---
--- <Markup>`</Markup><Code>code</Code><Markup>`</Markup>
--- <Markup>``</Markup><Code> code ` with ` backticks </Code><Markup>``</Markup>
--- <Markup>`</Markup><Code>code``with``backticks</Code><Markup>`</Markup>
--- `broken code
--- <Link>[link]</Link>
--- <Link>[link `with backticks]`]</Link>
--- <Link>[link [with brackets] ]</Link>
--- <Link>[link](explicit_href)</Link>
--- <Link>[link](explicit()href)</Link>
--- <Link>[link](<explicit)href>)</Link>
--- Paragraph with <Markup>`</Markup><Code>code</Code><Markup>`</Markup>!
--- Paragraph with <Link>[link]</Link>!
--- <Markup>\`</Markup> escaped backtick
--- <Em><Markup>*</Markup>em<Markup>*</Markup></Em> em<Em><Markup>*</Markup>in<Markup>*</Markup></Em>text
--- <Em><Markup>_</Markup>em<Markup>_</Markup></Em> em_in_text
--- <Strong><Markup>**</Markup>strong<Markup>**</Markup></Strong> strong<Strong><Markup>**</Markup>in<Markup>**</Markup></Strong>text
--- <Strong><Markup>__</Markup>strong<Markup>__</Markup></Strong> strong__in__text
--- broken *em
--- broken em*
--- broken **strong
--- broken strong**
--- <Em><Strong><Markup>***</Markup>both<Markup>***</Markup></Strong></Em> both<Em><Strong><Markup>***</Markup>in<Markup>***</Markup></Strong></Em>text
--- <Em><Strong><Markup>***</Markup>both end<Markup>*</Markup></Strong></Em><Strong> separately<Markup>**</Markup></Strong>
--- <Em><Strong><Markup>***</Markup>both end<Markup>**</Markup></Strong></Em><Em> separately<Markup>*</Markup></Em>
--- <Em><Markup>*</Markup>both <Strong><Markup>**</Markup>start separately<Markup>***</Markup></Strong></Em>
--- <Strong><Markup>**</Markup>both <Em><Markup>*</Markup>start separately<Markup>***</Markup></Em></Strong>
--- <Em><Markup>*</Markup><Markup>`</Markup><Code>foo</Code><Markup>`</Markup><Markup>*</Markup></Em>
---
--- <Scope><Markup>#</Markup> Blocks</Scope>
---
--- <Scope><Markup>##</Markup> Thematic breaks</Scope>
---
--- <Scope><Markup>-</Markup> <Markup>-</Markup> <Markup>-</Markup></Scope>
--- <Scope><Markup>_</Markup> <Markup>_</Markup> <Markup>_</Markup></Scope>
--- <Scope><Markup>*</Markup> <Markup>*</Markup> <Markup>*</Markup></Scope>
--- <Scope><Markup>-</Markup> _ -
---
--- </Scope><Scope><Markup>##</Markup> Lists</Scope>
---
--- <Scope><Markup>-</Markup> List
--- </Scope><Scope><Markup>*</Markup> List 2
--- </Scope><Scope><Markup>+</Markup> List 3
--- </Scope>-Broken list
---
--- <Scope><Markup>-</Markup>    List with indented text
---
--- </Scope><Scope><Markup>-</Markup> <Scope>    <CodeBlock>List with code</CodeBlock>
---       <CodeBlock>Continuation</CodeBlock>
---
---       <CodeBlock>Continuation 2</CodeBlock>
---
--- </Scope></Scope><Scope><Markup>-</Markup>
---   List that starts with empty string
---
--- </Scope> Not list
---
--- <Scope>  <Markup>-</Markup>  not code
---
---         still not code
---
--- </Scope><Scope>    <CodeBlock>code</CodeBlock>
---
--- </Scope><Scope><Markup>##</Markup> Numbered lists</Scope>
---
--- <Scope><Markup>1.</Markup> List
--- </Scope><Scope><Markup>2:</Markup> List
--- </Scope><Scope><Markup>3)</Markup> List
--- </Scope>  Not list
---
--- <Scope><Markup>##</Markup> Code</Scope>
---
--- <Scope>    <CodeBlock>This is code</CodeBlock>
---     <CodeBlock> This is also code</CodeBlock>
---     <CodeBlock>  function foo() end</CodeBlock>
---
--- </Scope><Scope><Markup>##</Markup> Fenced code</Scope>
---
--- <Scope><Markup>```</Markup><CodeBlock>syntax</CodeBlock>
--- <CodeBlock>code</CodeBlock>
--- <Markup>```</Markup></Scope>
--- not code
--- <Scope><Markup>~~~</Markup><CodeBlock>syntax</CodeBlock>
--- <CodeBlock>code</CodeBlock>
--- <CodeBlock>```</CodeBlock>
--- <CodeBlock>still code</CodeBlock>
--- <Markup>~~~</Markup></Scope>
---
--- <Scope><Markup>````</Markup><CodeBlock>code with 4 fences</CodeBlock>
--- <CodeBlock>```</CodeBlock>
--- <Markup>````</Markup></Scope>
---
--- <Markup>```</Markup><Code>inline code</Code><Markup>```</Markup>
--- not code
---
--- <Scope><Markup>```</Markup><CodeBlock>lua</CodeBlock>
--- <CodeBlockHl(TkFunction)>function</CodeBlockHl(TkFunction)><CodeBlock> </CodeBlock><CodeBlockHl(TkName)>foo</CodeBlockHl(TkName)><CodeBlockHl(TkLeftParen)>(</CodeBlockHl(TkLeftParen)><CodeBlockHl(TkRightParen)>)</CodeBlockHl(TkRightParen)>
--- <CodeBlock>    </CodeBlock><CodeBlockHl(TkLocal)>local</CodeBlockHl(TkLocal)><CodeBlock> </CodeBlock><CodeBlockHl(TkName)>long_string</CodeBlockHl(TkName)><CodeBlock> </CodeBlock><CodeBlockHl(TkAssign)>=</CodeBlockHl(TkAssign)><CodeBlock> </CodeBlock><CodeBlockHl(TkLongString)>[[</CodeBlockHl(TkLongString)>
--- <CodeBlockHl(TkLongString)>        content</CodeBlockHl(TkLongString)>
--- <CodeBlockHl(TkLongString)>    ]]</CodeBlockHl(TkLongString)>
--- <CodeBlockHl(TkEnd)>end</CodeBlockHl(TkEnd)>
--- <Markup>```</Markup></Scope>
---
--- <Scope><Markup>##</Markup> Quotes</Scope>
---
--- <Scope><Markup>></Markup> Quote
--- <Markup>></Markup> Continues
---</Scope>
--- <Scope><Markup>></Markup> Quote 2
---</Scope>
--- <Scope><Markup>##</Markup> Disabled MySt extensions</Scope>
---
--- $$
--- math
--- $$
---
--- <Scope><Markup>```</Markup><CodeBlock>{directive}</CodeBlock>
--- <Markup>```</Markup></Scope>
---
--- <Scope><Markup>##</Markup> Link anchor</Scope>
---
--- <Scope><Link>[link]</Link><Markup>:</Markup> <Link>https://example.com</Link></Scope>
"#;

        test(&code, Box::new(MdParser::new(None)), &expected);
    }

    #[test]
    fn test_myst() {
        let code = r#"
--- # Inline
---
--- {lua:obj}`a.b.c`, {lua:obj}`~a.b.c`,
--- {lua:obj}`<a.b.c>`, {lua:obj}`<~a.b.c>`, {lua:obj}`title <~a.b.c>`.
--- $inline math$, text, $$more inline math$$, a simple $dollar,
--- $$even more inline math$$.
---
--- # Directives
---
--- ```{directive}
--- ```
--- ```{directive}
--- Body
--- ```
--- ```{directive}
--- :param: value
--- Body
--- ```
--- ```{directive}
--- ---
--- param
--- ---
--- Body
--- ```
--- ````{directive1}
--- Body
--- ```{directive2}
--- Body
--- ```
--- Body
--- ````
--- ```{code-block} lua
--- function foo() end
--- ```
---
--- # Math
---
--- $$
--- \frac{1}{2}
--- $$
---
--- Text
---
--- $$
--- \frac{1}{2}
--- $$ (anchor)
"#;

        let expected = r#"
--- <Scope><Markup>#</Markup> Inline</Scope>
---
--- <Markup>{</Markup><Arg>lua:obj</Arg><Markup>}`</Markup><Ref>a.b.c</Ref><Markup>`</Markup>, <Markup>{</Markup><Arg>lua:obj</Arg><Markup>}`</Markup><Code>~</Code><Ref>a.b.c</Ref><Markup>`</Markup>,
--- <Markup>{</Markup><Arg>lua:obj</Arg><Markup>}`</Markup><Code><</Code><Ref>a.b.c</Ref><Code>></Code><Markup>`</Markup>, <Markup>{</Markup><Arg>lua:obj</Arg><Markup>}`</Markup><Code><~</Code><Ref>a.b.c</Ref><Code>></Code><Markup>`</Markup>, <Markup>{</Markup><Arg>lua:obj</Arg><Markup>}`</Markup><Code>title <~</Code><Ref>a.b.c</Ref><Code>></Code><Markup>`</Markup>.
--- <Markup>$</Markup><Code>inline math</Code><Markup>$</Markup>, text, <Markup>$$</Markup><Code>more inline math</Code><Markup>$$</Markup>, a simple $dollar,
--- <Markup>$$</Markup><Code>even more inline math</Code><Markup>$$</Markup>.
---
--- <Scope><Markup>#</Markup> Directives</Scope>
---
--- <Scope><Markup>```{</Markup><Arg>directive</Arg><Markup>}</Markup>
--- <Markup>```</Markup></Scope>
--- <Scope><Markup>```{</Markup><Arg>directive</Arg><Markup>}</Markup>
--- Body
--- <Markup>```</Markup></Scope>
--- <Scope><Markup>```{</Markup><Arg>directive</Arg><Markup>}</Markup>
--- <Markup>:</Markup><Arg>param</Arg><Markup>:</Markup> <CodeBlock>value</CodeBlock>
--- Body
--- <Markup>```</Markup></Scope>
--- <Scope><Markup>```{</Markup><Arg>directive</Arg><Markup>}</Markup>
--- <Markup>---</Markup>
--- <CodeBlock>param</CodeBlock>
--- <Markup>---</Markup>
--- Body
--- <Markup>```</Markup></Scope>
--- <Scope><Markup>````{</Markup><Arg>directive1</Arg><Markup>}</Markup>
--- Body
--- <Scope><Markup>```{</Markup><Arg>directive2</Arg><Markup>}</Markup>
--- Body
--- <Markup>```</Markup></Scope>
--- Body
--- <Markup>````</Markup></Scope>
--- <Scope><Markup>```{</Markup><Arg>code-block</Arg><Markup>}</Markup> <CodeBlock>lua</CodeBlock>
--- <CodeBlockHl(TkFunction)>function</CodeBlockHl(TkFunction)><CodeBlock> </CodeBlock><CodeBlockHl(TkName)>foo</CodeBlockHl(TkName)><CodeBlockHl(TkLeftParen)>(</CodeBlockHl(TkLeftParen)><CodeBlockHl(TkRightParen)>)</CodeBlockHl(TkRightParen)><CodeBlock> </CodeBlock><CodeBlockHl(TkEnd)>end</CodeBlockHl(TkEnd)>
--- <Markup>```</Markup></Scope>
---
--- <Scope><Markup>#</Markup> Math</Scope>
---
--- <Scope><Markup>$$</Markup>
--- <CodeBlock>\frac{1}{2}</CodeBlock>
--- <Markup>$$</Markup></Scope>
---
--- Text
---
--- <Scope><Markup>$$</Markup>
--- <CodeBlock>\frac{1}{2}</CodeBlock>
--- <Markup>$$</Markup> <Arg>(anchor)</Arg></Scope>
"#;

        test(&code, Box::new(MdParser::new_myst(None, None)), &expected);
    }

    #[test]
    fn test_myst_primary_domain() {
        let code = r#"--- See {obj}`ref`"#;

        let expected = r#"
            --- See <Markup>{</Markup><Arg>obj</Arg><Markup>}`</Markup><Ref>ref</Ref><Markup>`</Markup>
        "#;

        test(
            &code,
            Box::new(MdParser::new_myst(Some("lua".to_string()), None)),
            &expected,
        );
    }

    #[test]
    fn test_myst_search_at_offset() {
        let code = r#"--- See {lua:obj}`x` {lua:obj}`ref`"#;
        let expected = r#"--- See {lua:obj}`x` {lua:obj}`<Ref>ref</Ref>`"#;
        test(
            &code,
            Box::new(MdParser::new_myst(None, Some(31))),
            &expected,
        );
        test(
            &code,
            Box::new(MdParser::new_myst(None, Some(32))),
            &expected,
        );
        test(
            &code,
            Box::new(MdParser::new_myst(None, Some(34))),
            &expected,
        );

        let code = r#"--- See {lua:obj}`x` {lua:obj}`"#;
        let expected = r#"--- See {lua:obj}`x` {lua:obj}`<Ref></Ref>"#;
        test(
            &code,
            Box::new(MdParser::new_myst(None, Some(31))),
            &expected,
        );

        let code = r#"--- See {lua:obj}`x` {lua:obj}``..."#;
        let expected = r#"--- See {lua:obj}`x` {lua:obj}`<Ref>`...</Ref>"#;
        test(
            &code,
            Box::new(MdParser::new_myst(None, Some(31))),
            &expected,
        );
    }

    #[test]
    fn test_md_no_indent() {
        let code = r#"
---```lua
---
--- local t = 213
---```
---
--- .. code-block:: lua
---
---    local t = 123
---    yes = 1123
local t = 123
"#;

        let expected = r#"
---<Scope><Markup>```</Markup><CodeBlock>lua</CodeBlock>
---
---<CodeBlock> </CodeBlock><CodeBlockHl(TkLocal)>local</CodeBlockHl(TkLocal)><CodeBlock> </CodeBlock><CodeBlockHl(TkName)>t</CodeBlockHl(TkName)><CodeBlock> </CodeBlock><CodeBlockHl(TkAssign)>=</CodeBlockHl(TkAssign)><CodeBlock> </CodeBlock><CodeBlockHl(TkInt)>213</CodeBlockHl(TkInt)>
---<Markup>```</Markup></Scope>
---
--- .. code-block:: lua
---
---<Scope>    <CodeBlock>local t = 123</CodeBlock>
---    <CodeBlock>yes = 1123</CodeBlock></Scope>
local t = 123
"#;

        test(&code, Box::new(MdParser::new(None)), &expected);
    }
}
