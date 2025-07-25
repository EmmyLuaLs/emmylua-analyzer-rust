use crate::lexer::is_doc_whitespace;
use crate::parser::desc_parser::rst::{eat_rst_flag_body, process_inline_code, process_lua_ref};
use crate::parser::desc_parser::util::{
    check_marks_are_consistent, directive_is_code, find_common_indent, is_blank, is_punct,
    BacktrackGuard,
};
use crate::parser::desc_parser::LuaDescParser;
use crate::parser::{MarkEvent, Marker, MarkerEventContainer};
use crate::text::{Reader, ReaderWithMarks, SourceRange};
use crate::{LuaSyntaxKind, LuaTokenKind};
use std::ops::DerefMut;

pub struct MdParser {
    state: Vec<State>,
    inline_state: Vec<InlineState>,
    primary_domain: Option<String>,
    enable_myst: bool,
}

enum State {
    Quote {
        marker: Marker,
    },
    Indented {
        indent: usize,
        marker: Marker,
    },
    Code {
        marker: Marker,
    },
    FencedCode {
        n_fences: usize,
        fence: char,
        marker: Marker,
    },
    FencedDirectiveParams {
        n_fences: usize,
        fence: char,
        is_code: bool,
        marker: Marker,
    },
    FencedDirectiveParamsLong {
        n_fences: usize,
        fence: char,
        is_code: bool,
        marker: Marker,
    },
    FencedDirectiveBody {
        n_fences: usize,
        fence: char,
        marker: Marker,
    },
    Math {
        marker: Marker,
    },
}

enum InlineState {
    Em(char, Marker),
    Strong(char, Marker),
    Both(char, Marker, Marker),
}

impl LuaDescParser for MdParser {
    fn parse(&mut self, text: &str, events: &[MarkEvent]) -> Vec<MarkEvent> {
        self.state.clear();
        self.inline_state.clear();

        let common_indent = find_common_indent(
            events.iter().filter_map(|event| match *event {
                MarkEvent::EatToken {
                    kind: LuaTokenKind::TkDocDetail,
                    range,
                } => Some(&text[range.start_offset..range.end_offset()]),
                _ => None,
            }),
            0,
        );

        let mut result = Vec::new();

        let mut seen_line = false;
        for event in events {
            match event {
                &MarkEvent::EatToken {
                    kind: LuaTokenKind::TkDocDetail,
                    mut range,
                } => {
                    // Strip leading indent.
                    if range.length >= common_indent {
                        result.push(MarkEvent::EatToken {
                            kind: LuaTokenKind::TkWhitespace,
                            range: SourceRange::new(range.start_offset, common_indent),
                        });
                        range.start_offset += common_indent;
                        range.length -= common_indent;
                    }

                    // Process line.
                    let line = &text[range.start_offset..range.end_offset()];
                    let reader = Reader::new_with_range(line, range);
                    let mut reader_with_marks = ReaderWithMarks::new(reader, &mut result);
                    self.process_line(&mut reader_with_marks);
                    seen_line = true;
                }
                MarkEvent::EatToken {
                    kind: LuaTokenKind::TkEndOfLine,
                    ..
                } => {
                    if !seen_line {
                        // Process an empty line.
                        self.process_line(&mut ReaderWithMarks::new(
                            Default::default(),
                            &mut result,
                        ));
                    }
                    seen_line = false;
                    result.push(*event)
                }
                event => result.push(*event),
            }
        }

        self.flush_state(0, &mut ReaderWithMarks::new(Reader::default(), &mut result));

        check_marks_are_consistent(&result);

        result
    }
}

impl MdParser {
    pub fn new() -> Self {
        Self {
            state: Vec::new(),
            inline_state: Vec::new(),
            primary_domain: None,
            enable_myst: false,
        }
    }

    pub fn new_myst(primary_domain: Option<String>) -> Self {
        Self {
            state: Vec::new(),
            inline_state: Vec::new(),
            primary_domain,
            enable_myst: true,
        }
    }

    fn process_line(&mut self, reader: &mut ReaderWithMarks) {
        // First, find out which blocks are still present
        // and which finished.
        let mut last_state = 0;
        for (i, state) in self.state.iter().enumerate() {
            match *state {
                State::Quote { .. } => {
                    if self.try_read_quote_continuation(reader).is_ok() {
                        // Continue with nested states.
                    } else {
                        break;
                    }
                }
                State::Indented { indent, .. } => {
                    if self.try_read_indented(reader, indent).is_ok() {
                        // Continue with nested states.
                    } else {
                        break;
                    }
                }
                State::Code { .. } => {
                    if self.try_read_code(reader).is_ok() {
                        return;
                    } else {
                        break;
                    }
                }
                State::FencedCode {
                    n_fences, fence, ..
                } => {
                    if self.try_read_fence_end(reader, n_fences, fence).is_ok() {
                        self.flush_state(i, reader);
                        return;
                    } else {
                        reader.eat_while(|_| true);
                        reader.emit(LuaTokenKind::TkDocDetailCode);
                        return;
                    }
                }
                State::FencedDirectiveParams {
                    n_fences,
                    fence,
                    is_code,
                    marker,
                } => {
                    if self.try_read_fence_end(reader, n_fences, fence).is_ok() {
                        self.flush_state(i, reader);
                        return;
                    } else if self.try_read_fence_long_params_marker(reader).is_ok() {
                        self.flush_state(i + 1, reader);
                        self.state.pop();
                        self.state.push(State::FencedDirectiveParamsLong {
                            n_fences,
                            fence,
                            is_code,
                            marker,
                        });
                        return;
                    }
                    if self.try_read_fence_short_param(reader).is_ok() {
                        return;
                    } else if is_code {
                        self.flush_state(i + 1, reader);
                        self.state.pop();
                        self.state.push(State::FencedCode {
                            n_fences,
                            fence,
                            marker,
                        });
                        reader.eat_while(|_| true);
                        reader.emit(LuaTokenKind::TkDocDetailCode);
                        return;
                    } else {
                        self.flush_state(i + 1, reader);
                        self.state.pop();
                        self.state.push(State::FencedDirectiveBody {
                            n_fences,
                            fence,
                            marker,
                        });
                        last_state = i + 1;
                        break;
                    }
                }
                State::FencedDirectiveParamsLong {
                    n_fences,
                    fence,
                    is_code,
                    marker,
                } => {
                    if self.try_read_fence_end(reader, n_fences, fence).is_ok() {
                        self.flush_state(i, reader);
                        return;
                    } else if self.try_read_fence_long_params_marker(reader).is_ok() {
                        self.flush_state(i + 1, reader);
                        self.state.pop();
                        if is_code {
                            self.state.push(State::FencedCode {
                                n_fences,
                                fence,
                                marker,
                            });
                        } else {
                            self.state.push(State::FencedDirectiveBody {
                                n_fences,
                                fence,
                                marker,
                            });
                        }
                        return;
                    } else {
                        reader.eat_while(|_| true);
                        reader.emit(LuaTokenKind::TkDocDetailCode);
                        return;
                    }
                }
                State::FencedDirectiveBody {
                    n_fences, fence, ..
                } => {
                    if self.try_read_fence_end(reader, n_fences, fence).is_ok() {
                        self.flush_state(i, reader);
                        return;
                    } else {
                        // Continue with nested states.
                    }
                }
                State::Math { .. } => {
                    if self.try_read_math_end(reader).is_ok() {
                        self.flush_state(i, reader);
                        return;
                    } else {
                        reader.eat_while(|_| true);
                        reader.emit(LuaTokenKind::TkDocDetailCode);
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
    fn try_start_new_block(&mut self, reader: &mut ReaderWithMarks) -> bool {
        const HAS_MORE_CONTENT: bool = true;
        const NO_MORE_CONTENT: bool = false;

        if is_blank(reader.tail_text()) {
            // Just an empty line, nothing to do here.
            reader.eat_while(|_| true);
            reader.emit(LuaTokenKind::TkDocDetail);
            return NO_MORE_CONTENT;
        }

        // All markdown blocks can start with at most 3 whitespaces.
        // 4 whitespaces start a code block.
        let mut indent = reader.consume_n_times(is_doc_whitespace, 3);

        match reader.current_char() {
            // Thematic break or list start.
            '-' | '_' | '*' | '+' => {
                if self.try_read_thematic_break(reader).is_ok() {
                    return NO_MORE_CONTENT;
                } else if let Ok((indent_more, marker)) = self.try_read_list(reader) {
                    indent += indent_more;
                    self.state.push(State::Indented { indent, marker });
                    return HAS_MORE_CONTENT;
                } else {
                    // This is a normal text, continue to inline parsing.
                }
            }
            // Heading.
            '#' => {
                let marker = reader.mark(LuaSyntaxKind::DocScope);
                reader.emit(LuaTokenKind::TkDocDetail);
                reader.eat_when('#');
                reader.emit(LuaTokenKind::TkDocDetailMarkup);
                self.process_inline_content(reader);
                marker.complete(reader);
                return NO_MORE_CONTENT;
            }
            // Fenced code.
            '`' | '~' | ':' => {
                if let Ok((n_fences, fence, marker)) = self.try_read_fence_start(reader) {
                    if let Ok(directive_name) = self.try_read_fence_directive_name(reader) {
                        // This is a directive.
                        let is_code = directive_is_code(directive_name);
                        self.state.push(State::FencedDirectiveParams {
                            n_fences,
                            fence,
                            is_code,
                            marker,
                        });
                    } else {
                        // This is a code block.
                        reader.eat_while(|_| true);
                        reader.emit(LuaTokenKind::TkDocDetailCode);
                        self.state.push(State::FencedCode {
                            n_fences,
                            fence,
                            marker,
                        });
                    }
                    return NO_MORE_CONTENT;
                } else {
                    // This is a normal text, continue to inline parsing.
                }
            }
            // Indented code.
            ' ' | '\t' => {
                let marker = reader.mark(LuaSyntaxKind::DocScope);
                reader.bump();
                reader.emit(LuaTokenKind::TkDocDetail);
                reader.eat_while(|_| true);
                reader.emit(LuaTokenKind::TkDocDetailCode);
                self.state.push(State::Code { marker });
                return NO_MORE_CONTENT;
            }
            // Numbered list.
            '0'..='9' => {
                if let Ok((indent_more, marker)) = self.try_read_list(reader) {
                    indent += indent_more;
                    self.state.push(State::Indented { indent, marker });
                    return HAS_MORE_CONTENT;
                } else {
                    // This is a normal text, continue to inline parsing.
                }
            }
            // Quote.
            '>' => {
                if let Ok(marker) = self.try_read_quote(reader) {
                    self.state.push(State::Quote { marker });
                    return HAS_MORE_CONTENT;
                } else {
                    // This is a normal text, continue to inline parsing.
                }
            }
            // Math block.
            '$' if self.enable_myst => {
                if let Ok(marker) = self.try_read_math(reader) {
                    self.state.push(State::Math { marker });
                    return NO_MORE_CONTENT;
                } else {
                    // This is a normal text, continue to inline parsing.
                }
            }
            // Maybe a link anchor.
            '[' => {
                let mut reader = reader.backtrack_point();
                let marker = reader.mark(LuaSyntaxKind::DocScope);
                if Self::eat_link_title(reader.deref_mut())
                    && reader.current_char() == ':'
                    && is_doc_whitespace(reader.next_char())
                {
                    reader.emit(LuaTokenKind::TkDocDetailInlineLink);
                    reader.bump();
                    reader.emit(LuaTokenKind::TkDocDetailInlineMarkup);
                    reader.eat_while(is_doc_whitespace);
                    reader.emit(LuaTokenKind::TkDocDetail);
                    reader.eat_while(|_| true);
                    reader.emit(LuaTokenKind::TkDocDetailInlineLink);
                    marker.complete(reader.deref_mut());
                    reader.commit();
                    return NO_MORE_CONTENT;
                }
            }
            // Normal text.
            _ => {
                // Continue to inline parsing.
            }
        }

        // Didn't detect start of any nested block. Parse the rest of the line
        // as an inline context.
        reader.emit(LuaTokenKind::TkDocDetail);
        self.process_inline_content(reader);
        NO_MORE_CONTENT
    }

    fn try_read_thematic_break<'a>(&self, reader: &mut ReaderWithMarks<'a>) -> Result<(), ()> {
        // Line that consists of three or more of the same symbol (`-`, `*`, or `_`),
        // possibly separated by spaces. I.e.: `" - - - "`.

        let mut reader = reader.backtrack_point();

        let marker = reader.mark(LuaSyntaxKind::DocScope);

        reader.eat_while(is_doc_whitespace);
        reader.emit(LuaTokenKind::TkDocDetail);

        let first_char = reader.current_char();
        if !matches!(first_char, '-' | '*' | '_') {
            return Err(());
        } else {
            reader.bump();
            reader.emit(LuaTokenKind::TkDocDetailMarkup);
        }

        let mut n_marks = 1;
        loop {
            reader.eat_while(is_doc_whitespace);
            reader.emit(LuaTokenKind::TkDocDetail);
            if reader.is_eof() {
                break;
            } else if reader.current_char() == first_char {
                reader.bump();
                reader.emit(LuaTokenKind::TkDocDetailMarkup);
                n_marks += 1;
            } else {
                return Err(());
            }
        }

        if n_marks >= 3 {
            reader.eat_while(|_| true);
            reader.emit(LuaTokenKind::TkDocDetail);
            marker.complete(reader.deref_mut());
            reader.commit();
            Ok(())
        } else {
            Err(())
        }
    }

    fn try_read_quote<'a>(&self, reader: &mut ReaderWithMarks<'a>) -> Result<Marker, ()> {
        // Quote start, i.e. `"   > text..."`.

        let mut reader = reader.backtrack_point();
        let marker = reader.mark(LuaSyntaxKind::DocScope);

        match self.try_read_quote_continuation(reader.deref_mut()) {
            Ok(()) => {
                reader.commit();
                Ok(marker)
            }
            Err(()) => Err(()),
        }
    }

    fn try_read_quote_continuation<'a>(&self, reader: &mut ReaderWithMarks<'a>) -> Result<(), ()> {
        // Quote start, i.e. `"   > text..."`.

        let mut reader = reader.backtrack_point();

        reader.consume_n_times(is_doc_whitespace, 3);

        if reader.current_char() == '>' {
            reader.emit(LuaTokenKind::TkDocDetail);
            reader.bump();
            reader.emit(LuaTokenKind::TkDocDetailMarkup);
            reader.consume_n_times(is_doc_whitespace, 1);
            reader.emit(LuaTokenKind::TkDocDetail);

            reader.commit();
            Ok(())
        } else {
            Err(())
        }
    }

    fn try_read_indented<'a>(
        &self,
        reader: &mut ReaderWithMarks<'a>,
        indent: usize,
    ) -> Result<(), ()> {
        // Block indented by at least `indent` spaces. This continues a list,
        // i.e.:
        //
        //     - list
        //       list continuation, indented by at least 2 spaces.

        let mut reader = reader.backtrack_point();

        let found_indent = reader.consume_n_times(is_doc_whitespace, indent);
        if reader.is_eof() || found_indent == indent {
            reader.emit(LuaTokenKind::TkDocDetail);
            reader.commit();
            Ok(())
        } else {
            Err(())
        }
    }

    fn try_read_code<'a>(&self, reader: &mut ReaderWithMarks<'a>) -> Result<(), ()> {
        // Block indented by at least 4 spaces, i.e. `"    code"`.
        let mut reader = reader.backtrack_point();

        let found_indent = reader.consume_n_times(is_doc_whitespace, 4);
        if found_indent == 4 || reader.is_eof() {
            reader.emit(LuaTokenKind::TkDocDetail);
            reader.eat_while(|_| true);
            reader.emit(LuaTokenKind::TkDocDetailCode);

            reader.commit();
            Ok(())
        } else {
            Err(())
        }
    }

    fn try_read_list<'a>(&self, reader: &mut ReaderWithMarks<'a>) -> Result<(usize, Marker), ()> {
        // Either numbered or non-numbered list start.
        let mut reader = reader.backtrack_point();
        let marker = reader.mark(LuaSyntaxKind::DocScope);

        let mut indent = reader.consume_n_times(is_doc_whitespace, 3);
        match reader.current_char() {
            '-' | '*' | '+' => {
                indent += 2;
                reader.emit(LuaTokenKind::TkDocDetail);
                reader.bump();
                reader.emit(LuaTokenKind::TkDocDetailMarkup);
                if reader.is_eof() {
                    reader.commit();
                    return Ok((indent, marker));
                } else if !is_doc_whitespace(reader.current_char()) {
                    return Err(());
                }
                reader.bump();
            }
            '0'..='9' => {
                reader.emit(LuaTokenKind::TkDocDetail);
                indent += reader.eat_while(|c| c.is_ascii_digit()) + 2;
                if !matches!(reader.current_char(), '.' | ')' | ':') {
                    return Err(());
                }
                reader.bump();
                reader.emit(LuaTokenKind::TkDocDetailMarkup);
                if reader.is_eof() {
                    reader.commit();
                    return Ok((indent, marker));
                } else if !is_doc_whitespace(reader.current_char()) {
                    return Err(());
                }
                reader.bump();
            }
            _ => return Err(()),
        }

        let text = reader.tail_text();
        if text.len() >= 4 && is_blank(&text[..4]) {
            // List marker followed by a space, then 4 more spaces
            // is parsed as a list marker followed by a space,
            // then code block.
            reader.emit(LuaTokenKind::TkDocDetail);
            reader.commit();
            Ok((indent, marker))
        } else {
            // List marker followed by a space, then up to 3 more spaces
            // is parsed as a list marker
            indent += reader.eat_while(is_doc_whitespace);
            reader.emit(LuaTokenKind::TkDocDetail);
            reader.commit();
            Ok((indent, marker))
        }
    }

    fn try_read_fence_start<'a>(
        &self,
        reader: &mut ReaderWithMarks<'a>,
    ) -> Result<(usize, char, Marker), ()> {
        // Start of a fenced block. MySt allows fenced blocks
        // using colons, i.e.:
        //
        //     :::syntax
        //     code
        //     :::

        let mut reader = reader.backtrack_point();
        let marker = reader.mark(LuaSyntaxKind::DocScope);

        reader.consume_n_times(is_doc_whitespace, 3);
        match reader.current_char() {
            '`' => {
                reader.emit(LuaTokenKind::TkDocDetail);
                let n_fences = reader.eat_when('`');
                if n_fences < 3 {
                    return Err(());
                }
                if reader.tail_text().contains('`') {
                    return Err(());
                }
                reader.emit(LuaTokenKind::TkDocDetailMarkup);

                reader.commit();
                Ok((n_fences, '`', marker))
            }
            '~' => {
                reader.emit(LuaTokenKind::TkDocDetail);
                let n_fences = reader.eat_when('~');
                if n_fences < 3 {
                    return Err(());
                }
                reader.emit(LuaTokenKind::TkDocDetailMarkup);

                reader.commit();
                Ok((n_fences, '~', marker))
            }
            ':' if self.enable_myst => {
                reader.emit(LuaTokenKind::TkDocDetail);
                let n_fences = reader.eat_when(':');
                if n_fences < 3 {
                    return Err(());
                }
                reader.emit(LuaTokenKind::TkDocDetailMarkup);

                reader.commit();
                Ok((n_fences, ':', marker))
            }
            _ => Err(()),
        }
    }

    fn try_read_fence_directive_name<'a>(
        &self,
        reader: &mut ReaderWithMarks<'a>,
    ) -> Result<&'a str, ()> {
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

        let mut reader = reader.backtrack_point();

        if reader.current_char() != '{' {
            return Err(());
        }
        reader.bump();
        reader.emit(LuaTokenKind::TkDocDetailArgMarkup);
        reader.eat_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '+' | '_' | '-'));
        if reader.current_char() != '}' {
            return Err(());
        }
        let dir_name = reader.current_text();
        reader.emit(LuaTokenKind::TkDocDetailArg);
        reader.bump();
        reader.emit(LuaTokenKind::TkDocDetailArgMarkup);
        reader.eat_while(is_doc_whitespace);
        reader.emit(LuaTokenKind::TkDocDetail);
        reader.eat_while(|_| true);
        reader.emit(LuaTokenKind::TkDocDetailCode);
        reader.commit();
        Ok(dir_name)
    }

    fn try_read_fence_short_param<'a>(&self, reader: &mut ReaderWithMarks<'a>) -> Result<(), ()> {
        let mut reader = reader.backtrack_point();

        reader.eat_while(is_doc_whitespace);
        if reader.current_char() != ':' {
            return Err(());
        }
        reader.emit(LuaTokenKind::TkDocDetail);
        reader.bump();
        reader.emit(LuaTokenKind::TkDocDetailArgMarkup);
        eat_rst_flag_body(reader.deref_mut());
        reader.emit(LuaTokenKind::TkDocDetailArg);
        if reader.current_char() != ':' {
            return Err(());
        }
        reader.bump();
        reader.emit(LuaTokenKind::TkDocDetailArgMarkup);
        reader.eat_while(is_doc_whitespace);
        reader.emit(LuaTokenKind::TkDocDetail);
        reader.eat_while(|_| true);
        reader.emit(LuaTokenKind::TkDocDetailCode);
        reader.commit();
        Ok(())
    }

    fn try_read_fence_long_params_marker<'a>(
        &self,
        reader: &mut ReaderWithMarks<'a>,
    ) -> Result<(), ()> {
        let mut reader = reader.backtrack_point();

        reader.eat_while(is_doc_whitespace);
        if !reader.tail_text().starts_with("---") {
            return Err(());
        }
        if !is_blank(&reader.tail_text()[3..]) {
            return Err(());
        }
        reader.emit(LuaTokenKind::TkDocDetail);
        reader.bump();
        reader.bump();
        reader.bump();
        reader.emit(LuaTokenKind::TkDocDetailMarkup);
        reader.eat_while(|_| true);
        reader.emit(LuaTokenKind::TkDocDetail);
        reader.commit();
        Ok(())
    }

    fn try_read_fence_end<'a>(
        &self,
        reader: &mut ReaderWithMarks<'a>,
        n_fences: usize,
        fence: char,
    ) -> Result<(), ()> {
        let mut reader = reader.backtrack_point();

        reader.consume_n_times(is_doc_whitespace, 3);
        reader.emit(LuaTokenKind::TkDocDetail);
        if reader.eat_when(fence) != n_fences {
            return Err(());
        }
        if !is_blank(&reader.tail_text()) {
            return Err(());
        }
        reader.emit(LuaTokenKind::TkDocDetailMarkup);
        reader.eat_while(|_| true);
        reader.emit(LuaTokenKind::TkDocDetail);

        reader.commit();
        Ok(())
    }

    fn try_read_math<'a>(&self, reader: &mut ReaderWithMarks<'a>) -> Result<Marker, ()> {
        // MySt extension for LaTaX-like math markup:
        //
        //     $$
        //     \frac{1}{2}
        //     $$ (anchor)

        if !self.enable_myst {
            return Err(());
        }

        let mut reader = reader.backtrack_point();
        let marker = reader.mark(LuaSyntaxKind::DocScope);

        reader.consume_n_times(is_doc_whitespace, 3);
        if reader.current_char() == '$' && reader.next_char() == '$' {
            reader.emit(LuaTokenKind::TkDocDetail);
            reader.bump();
            reader.bump();
            if !is_blank(&reader.tail_text()) {
                return Err(());
            }
            reader.emit(LuaTokenKind::TkDocDetailMarkup);
            reader.eat_while(|_| true);
            reader.emit(LuaTokenKind::TkDocDetail);

            reader.commit();
            Ok(marker)
        } else {
            Err(())
        }
    }

    fn try_read_math_end<'a>(&self, reader: &mut ReaderWithMarks<'a>) -> Result<(), ()> {
        // MySt extension for LaTaX-like math markup:
        //
        //     $$
        //     \frac{1}{2}
        //     $$ (anchor)

        if !self.enable_myst {
            return Err(());
        }

        let mut reader = reader.backtrack_point();

        reader.consume_n_times(is_doc_whitespace, 3);
        if reader.current_char() == '$' && reader.next_char() == '$' {
            reader.emit(LuaTokenKind::TkDocDetail);
            reader.bump();
            reader.bump();
            reader.emit(LuaTokenKind::TkDocDetailMarkup);
            reader.eat_while(is_doc_whitespace);
            reader.emit(LuaTokenKind::TkDocDetail);
            if reader.current_char() == '(' {
                reader.bump();
                reader.eat_while(|c| {
                    c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '+' | '_' | '-')
                });
                if reader.current_char() != ')' {
                    return Err(());
                }
                reader.bump();
                reader.emit(LuaTokenKind::TkDocDetailArg);
            }
            reader.eat_while(|_| true);
            reader.emit(LuaTokenKind::TkDocDetail);

            reader.commit();
            Ok(())
        } else {
            Err(())
        }
    }

    fn process_inline_content(&mut self, reader: &mut ReaderWithMarks) {
        assert!(self.inline_state.is_empty());

        let mut guard = BacktrackGuard::new(500);

        while !reader.is_eof() {
            match reader.current_char() {
                '\\' => {
                    reader.bump();
                    reader.bump();
                }
                '`' => {
                    let mut bt = reader.backtrack_point();

                    let prev = bt.reset_buff_into_sub_reader();
                    let after_prev = bt.current_char();

                    if !Self::eat_inline_code(&mut bt) {
                        bt.rollback();
                        reader.bump();
                        guard.backtrack(reader);
                        continue;
                    }

                    let code = bt.reset_buff_into_sub_reader();

                    self.process_inline_content_style(
                        &mut ReaderWithMarks::new(prev, bt.get_events()),
                        after_prev,
                    );

                    process_inline_code(
                        &mut ReaderWithMarks::new(code, bt.get_events()),
                        LuaTokenKind::TkDocDetailInlineCode,
                    );

                    bt.commit();
                }
                '$' if self.enable_myst => {
                    let mut bt = reader.backtrack_point();

                    let prev = bt.reset_buff_into_sub_reader();
                    let after_prev = bt.current_char();

                    if !Self::eat_inline_math(&mut bt) {
                        bt.rollback();
                        reader.bump();
                        guard.backtrack(reader);
                        continue;
                    }

                    let code = bt.reset_buff_into_sub_reader();

                    self.process_inline_content_style(
                        &mut ReaderWithMarks::new(prev, bt.get_events()),
                        after_prev,
                    );

                    self.process_inline_math(&mut ReaderWithMarks::new(code, bt.get_events()));

                    bt.commit();
                }
                '[' => {
                    let mut bt = reader.backtrack_point();

                    let prev = bt.reset_buff_into_sub_reader();
                    let after_prev = bt.current_char();

                    if !Self::eat_link_title(&mut bt) {
                        bt.rollback();
                        reader.bump();
                        guard.backtrack(reader);
                        continue;
                    }

                    let title_range = bt.current_range();
                    bt.reset_buff();

                    if bt.current_char() == '(' && !Self::eat_link_url(&mut bt) {
                        bt.rollback();
                        reader.bump();
                        guard.backtrack(reader);
                        continue;
                    }

                    let url_range = bt.current_range();
                    bt.reset_buff();

                    self.process_inline_content_style(
                        &mut ReaderWithMarks::new(prev, bt.get_events()),
                        after_prev,
                    );

                    if title_range.length > 0 {
                        bt.get_events().push(MarkEvent::EatToken {
                            kind: LuaTokenKind::TkDocDetailInlineLink,
                            range: title_range,
                        });
                    }

                    if url_range.length > 0 {
                        bt.get_events().push(MarkEvent::EatToken {
                            kind: LuaTokenKind::TkDocDetailInlineLink,
                            range: url_range,
                        });
                    }

                    bt.commit();
                }
                '{' if self.enable_myst => {
                    let mut bt = reader.backtrack_point();

                    let prev = bt.reset_buff_into_sub_reader();
                    let after_prev = bt.current_char();

                    if !Self::eat_role_name(&mut bt) {
                        bt.rollback();
                        reader.bump();
                        guard.backtrack(reader);
                        continue;
                    }

                    let role_text = bt.current_text();
                    let role = bt.reset_buff_into_sub_reader();

                    if !Self::eat_inline_code(&mut bt) {
                        bt.rollback();
                        reader.bump();
                        guard.backtrack(reader);
                        continue;
                    }

                    let code = bt.reset_buff_into_sub_reader();

                    self.process_inline_content_style(
                        &mut ReaderWithMarks::new(prev, bt.get_events()),
                        after_prev,
                    );

                    let marker = bt.mark(LuaSyntaxKind::DocRef);

                    self.process_role_name(&mut ReaderWithMarks::new(role, bt.get_events()));

                    let is_lua_ref = role_text.starts_with("{lua:")
                        || (self.primary_domain.as_deref() == Some("lua")
                            && !role_text.contains(":"));

                    if is_lua_ref {
                        process_lua_ref(&mut ReaderWithMarks::new(code, bt.get_events()));
                    } else {
                        process_inline_code(
                            &mut ReaderWithMarks::new(code, bt.get_events()),
                            LuaTokenKind::TkDocDetailInlineCode,
                        );
                    }

                    marker.complete(bt.deref_mut());

                    bt.commit();
                }
                _ => {
                    reader.bump();
                }
            }
        }

        if !reader.current_range().is_empty() {
            self.process_inline_content_style(
                &mut ReaderWithMarks::new(reader.reset_buff_into_sub_reader(), reader.get_events()),
                ' ',
            );
        }
        self.flush_inline_state(reader);
    }

    #[must_use]
    fn eat_inline_code(reader: &mut ReaderWithMarks) -> bool {
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

        false
    }

    #[must_use]
    fn eat_inline_math(reader: &mut ReaderWithMarks) -> bool {
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
    fn eat_link_title(reader: &mut ReaderWithMarks) -> bool {
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
                    let mut bt = reader.backtrack_point();
                    if Self::eat_inline_code(bt.deref_mut()) {
                        bt.commit()
                    } else {
                        bt.rollback();
                        reader.bump();
                    }
                }
                '$' => {
                    let mut bt = reader.backtrack_point();
                    if !Self::eat_inline_math(&mut bt) {
                        bt.commit();
                    } else {
                        bt.rollback();
                        reader.bump();
                    }
                }
                _ => reader.bump(),
            }
        }

        false
    }

    #[must_use]
    fn eat_link_url(reader: &mut ReaderWithMarks) -> bool {
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

    fn flush_state(&mut self, end: usize, reader: &mut ReaderWithMarks) {
        for state in self.state.drain(end..).rev() {
            let marker = match state {
                State::Quote { marker, .. } => marker,
                State::Indented { marker, .. } => marker,
                State::Code { marker, .. } => marker,
                State::FencedCode { marker, .. } => marker,
                State::FencedDirectiveParams { marker, .. } => marker,
                State::FencedDirectiveParamsLong { marker, .. } => marker,
                State::FencedDirectiveBody { marker, .. } => marker,
                State::Math { marker, .. } => marker,
            };
            marker.complete(reader);
        }
    }

    fn process_inline_math(&mut self, reader: &mut ReaderWithMarks) {
        let n_backticks = reader.eat_when('$');
        reader.emit(LuaTokenKind::TkDocDetailInlineMarkup);
        while reader.tail_range().length > n_backticks {
            reader.bump();
        }
        reader.emit(LuaTokenKind::TkDocDetailInlineCode);
        reader.eat_while(|_| true);
        reader.emit(LuaTokenKind::TkDocDetailInlineMarkup);
    }

    #[must_use]
    fn eat_role_name(reader: &mut ReaderWithMarks) -> bool {
        if reader.current_char() != '{' {
            return false;
        }
        reader.bump();
        reader.eat_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '+' | '_' | '-'));
        if reader.current_char() == '}' {
            reader.bump();
            true
        } else {
            false
        }
    }

    fn process_role_name(&mut self, reader: &mut ReaderWithMarks) {
        reader.eat_when('{');
        reader.emit(LuaTokenKind::TkDocDetailInlineArgMarkup);
        reader.eat_while(|c| c != '}');
        reader.emit(LuaTokenKind::TkDocDetailInlineArg);
        reader.eat_when('}');
        reader.emit(LuaTokenKind::TkDocDetailInlineArgMarkup);
        reader.eat_while(|_| true);
        reader.emit(LuaTokenKind::TkDocDetail);
    }

    fn process_inline_content_style(&mut self, reader: &mut ReaderWithMarks, char_after: char) {
        let char_after = if char_after == '\0' { ' ' } else { char_after };
        while !reader.is_eof() {
            match reader.current_char() {
                '\\' => {
                    reader.emit(LuaTokenKind::TkDocDetail);
                    reader.bump();
                    reader.bump();
                    reader.emit(LuaTokenKind::TkDocDetailInlineMarkup);
                }
                ch @ '*' | ch @ '_' => {
                    reader.emit(LuaTokenKind::TkDocDetail);

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
                            reader.emit(LuaTokenKind::TkDocDetailInlineMarkup);
                            self.end_highlight(ch, n_chars, reader);
                        } else {
                            self.start_highlight(ch, n_chars, reader);
                            reader.emit(LuaTokenKind::TkDocDetailInlineMarkup);
                        }
                    } else if can_start_highlight {
                        self.start_highlight(ch, n_chars, reader);
                        reader.emit(LuaTokenKind::TkDocDetailInlineMarkup);
                    } else if can_end_highlight {
                        reader.emit(LuaTokenKind::TkDocDetailInlineMarkup);
                        self.end_highlight(ch, n_chars, reader);
                    }
                }
                _ => {
                    reader.bump();
                }
            }
        }

        reader.emit(LuaTokenKind::TkDocDetail);
    }

    fn flush_inline_state(&mut self, reader: &mut ReaderWithMarks) {
        for state in self.inline_state.drain(..) {
            match state {
                InlineState::Em(_, marker) => {
                    marker.undo(reader);
                }
                InlineState::Strong(_, marker) => {
                    marker.undo(reader);
                }
                InlineState::Both(_, em, strong) => {
                    em.undo(reader);
                    strong.undo(reader);
                }
            };
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

    fn start_highlight(&mut self, r_ch: char, r_n_chars: usize, reader: &mut ReaderWithMarks) {
        match r_n_chars {
            0 => {}
            1 => self
                .inline_state
                .push(InlineState::Em(r_ch, reader.mark(LuaSyntaxKind::DocEm))),
            2 => self.inline_state.push(InlineState::Strong(
                r_ch,
                reader.mark(LuaSyntaxKind::DocStrong),
            )),
            _ => self.inline_state.push(InlineState::Both(
                r_ch,
                reader.mark(LuaSyntaxKind::DocEm),
                reader.mark(LuaSyntaxKind::DocStrong),
            )),
        }
    }

    fn end_highlight(&mut self, r_ch: char, mut r_n_chars: usize, reader: &mut ReaderWithMarks) {
        while r_n_chars > 0 {
            match self.inline_state.last() {
                Some(InlineState::Em(ch, _)) => {
                    if ch == &r_ch && (r_n_chars == 1 || r_n_chars >= 3) {
                        let Some(InlineState::Em(_, marker)) = self.inline_state.pop() else {
                            unreachable!();
                        };
                        marker.complete(reader);
                        r_n_chars -= 1;
                    } else {
                        break;
                    }
                }
                Some(InlineState::Strong(ch, ..)) => {
                    if ch == &r_ch && r_n_chars >= 2 {
                        let Some(InlineState::Strong(_, marker)) = self.inline_state.pop() else {
                            unreachable!();
                        };
                        marker.complete(reader);
                        r_n_chars -= 2;
                    } else {
                        break;
                    }
                }
                Some(InlineState::Both(ch, ..)) => {
                    if ch == &r_ch {
                        let Some(InlineState::Both(_, em, strong)) = self.inline_state.pop() else {
                            unreachable!();
                        };

                        strong.complete(reader);
                        em.complete(reader);

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
    }
}

#[cfg(test)]
mod tests {
    use crate::{DescParserType, LuaParser, ParserConfig};

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
        let expected = r###"
Syntax(Chunk)@0..1681
  Syntax(Block)@0..1681
    Token(TkEndOfLine)@0..1 "\n"
    Syntax(Comment)@1..681
      Token(TkNormalStart)@1..4 "---"
      Token(TkWhitespace)@4..5 " "
      Syntax(DocDescription)@5..681
        Syntax(DocScope)@5..18
          Token(TkDocDetailMarkup)@5..6 "#"
          Token(TkDocDetail)@6..18 " Inline code"
        Token(TkEndOfLine)@18..19 "\n"
        Token(TkNormalStart)@19..22 "---"
        Token(TkEndOfLine)@22..23 "\n"
        Token(TkNormalStart)@23..26 "---"
        Token(TkWhitespace)@26..27 " "
        Token(TkDocDetailInlineMarkup)@27..28 "`"
        Token(TkDocDetailInlineCode)@28..32 "code"
        Token(TkDocDetailInlineMarkup)@32..33 "`"
        Token(TkEndOfLine)@33..34 "\n"
        Token(TkNormalStart)@34..37 "---"
        Token(TkWhitespace)@37..38 " "
        Token(TkDocDetailInlineMarkup)@38..40 "``"
        Token(TkDocDetailInlineCode)@40..65 " code ` with ` backti ..."
        Token(TkDocDetailInlineMarkup)@65..67 "``"
        Token(TkEndOfLine)@67..68 "\n"
        Token(TkNormalStart)@68..71 "---"
        Token(TkWhitespace)@71..72 " "
        Token(TkDocDetailInlineMarkup)@72..73 "`"
        Token(TkDocDetailInlineCode)@73..94 "code``with``backticks"
        Token(TkDocDetailInlineMarkup)@94..95 "`"
        Token(TkEndOfLine)@95..96 "\n"
        Token(TkNormalStart)@96..99 "---"
        Token(TkWhitespace)@99..100 " "
        Token(TkDocDetail)@100..112 "`broken code"
        Token(TkEndOfLine)@112..113 "\n"
        Token(TkNormalStart)@113..116 "---"
        Token(TkWhitespace)@116..117 " "
        Token(TkDocDetailInlineLink)@117..123 "[link]"
        Token(TkEndOfLine)@123..124 "\n"
        Token(TkNormalStart)@124..127 "---"
        Token(TkWhitespace)@127..128 " "
        Token(TkDocDetailInlineLink)@128..152 "[link `with backticks]`]"
        Token(TkEndOfLine)@152..153 "\n"
        Token(TkNormalStart)@153..156 "---"
        Token(TkWhitespace)@156..157 " "
        Token(TkDocDetailInlineLink)@157..180 "[link [with brackets] ]"
        Token(TkEndOfLine)@180..181 "\n"
        Token(TkNormalStart)@181..184 "---"
        Token(TkWhitespace)@184..185 " "
        Token(TkDocDetailInlineLink)@185..191 "[link]"
        Token(TkDocDetailInlineLink)@191..206 "(explicit_href)"
        Token(TkEndOfLine)@206..207 "\n"
        Token(TkNormalStart)@207..210 "---"
        Token(TkWhitespace)@210..211 " "
        Token(TkDocDetailInlineLink)@211..217 "[link]"
        Token(TkDocDetailInlineLink)@217..233 "(explicit()href)"
        Token(TkEndOfLine)@233..234 "\n"
        Token(TkNormalStart)@234..237 "---"
        Token(TkWhitespace)@237..238 " "
        Token(TkDocDetailInlineLink)@238..244 "[link]"
        Token(TkDocDetailInlineLink)@244..261 "(<explicit)href>)"
        Token(TkEndOfLine)@261..262 "\n"
        Token(TkNormalStart)@262..265 "---"
        Token(TkWhitespace)@265..266 " "
        Token(TkDocDetail)@266..281 "Paragraph with "
        Token(TkDocDetailInlineMarkup)@281..282 "`"
        Token(TkDocDetailInlineCode)@282..286 "code"
        Token(TkDocDetailInlineMarkup)@286..287 "`"
        Token(TkDocDetail)@287..288 "!"
        Token(TkEndOfLine)@288..289 "\n"
        Token(TkNormalStart)@289..292 "---"
        Token(TkWhitespace)@292..293 " "
        Token(TkDocDetail)@293..308 "Paragraph with "
        Token(TkDocDetailInlineLink)@308..314 "[link]"
        Token(TkDocDetail)@314..315 "!"
        Token(TkEndOfLine)@315..316 "\n"
        Token(TkNormalStart)@316..319 "---"
        Token(TkWhitespace)@319..320 " "
        Token(TkDocDetailInlineMarkup)@320..322 "\\`"
        Token(TkDocDetail)@322..339 " escaped backtick"
        Token(TkEndOfLine)@339..340 "\n"
        Token(TkNormalStart)@340..343 "---"
        Token(TkWhitespace)@343..344 " "
        Syntax(DocEm)@344..348
          Token(TkDocDetailInlineMarkup)@344..345 "*"
          Token(TkDocDetail)@345..347 "em"
          Token(TkDocDetailInlineMarkup)@347..348 "*"
        Token(TkDocDetail)@348..351 " em"
        Syntax(DocEm)@351..355
          Token(TkDocDetailInlineMarkup)@351..352 "*"
          Token(TkDocDetail)@352..354 "in"
          Token(TkDocDetailInlineMarkup)@354..355 "*"
        Token(TkDocDetail)@355..359 "text"
        Token(TkEndOfLine)@359..360 "\n"
        Token(TkNormalStart)@360..363 "---"
        Token(TkWhitespace)@363..364 " "
        Syntax(DocEm)@364..368
          Token(TkDocDetailInlineMarkup)@364..365 "_"
          Token(TkDocDetail)@365..367 "em"
          Token(TkDocDetailInlineMarkup)@367..368 "_"
        Token(TkDocDetail)@368..371 " em"
        Token(TkDocDetail)@371..374 "_in"
        Token(TkDocDetail)@374..379 "_text"
        Token(TkEndOfLine)@379..380 "\n"
        Token(TkNormalStart)@380..383 "---"
        Token(TkWhitespace)@383..384 " "
        Syntax(DocStrong)@384..394
          Token(TkDocDetailInlineMarkup)@384..386 "**"
          Token(TkDocDetail)@386..392 "strong"
          Token(TkDocDetailInlineMarkup)@392..394 "**"
        Token(TkDocDetail)@394..401 " strong"
        Syntax(DocStrong)@401..407
          Token(TkDocDetailInlineMarkup)@401..403 "**"
          Token(TkDocDetail)@403..405 "in"
          Token(TkDocDetailInlineMarkup)@405..407 "**"
        Token(TkDocDetail)@407..411 "text"
        Token(TkEndOfLine)@411..412 "\n"
        Token(TkNormalStart)@412..415 "---"
        Token(TkWhitespace)@415..416 " "
        Syntax(DocStrong)@416..426
          Token(TkDocDetailInlineMarkup)@416..418 "__"
          Token(TkDocDetail)@418..424 "strong"
          Token(TkDocDetailInlineMarkup)@424..426 "__"
        Token(TkDocDetail)@426..433 " strong"
        Token(TkDocDetail)@433..437 "__in"
        Token(TkDocDetail)@437..443 "__text"
        Token(TkEndOfLine)@443..444 "\n"
        Token(TkNormalStart)@444..447 "---"
        Token(TkWhitespace)@447..448 " "
        Token(TkDocDetail)@448..455 "broken "
        Token(TkDocDetailInlineMarkup)@455..456 "*"
        Token(TkDocDetail)@456..458 "em"
        Token(TkEndOfLine)@458..459 "\n"
        Token(TkNormalStart)@459..462 "---"
        Token(TkWhitespace)@462..463 " "
        Token(TkDocDetail)@463..472 "broken em"
        Token(TkDocDetailInlineMarkup)@472..473 "*"
        Token(TkEndOfLine)@473..474 "\n"
        Token(TkNormalStart)@474..477 "---"
        Token(TkWhitespace)@477..478 " "
        Token(TkDocDetail)@478..485 "broken "
        Token(TkDocDetailInlineMarkup)@485..487 "**"
        Token(TkDocDetail)@487..493 "strong"
        Token(TkEndOfLine)@493..494 "\n"
        Token(TkNormalStart)@494..497 "---"
        Token(TkWhitespace)@497..498 " "
        Token(TkDocDetail)@498..511 "broken strong"
        Token(TkDocDetailInlineMarkup)@511..513 "**"
        Token(TkEndOfLine)@513..514 "\n"
        Token(TkNormalStart)@514..517 "---"
        Token(TkWhitespace)@517..518 " "
        Syntax(DocEm)@518..528
          Syntax(DocStrong)@518..528
            Token(TkDocDetailInlineMarkup)@518..521 "***"
            Token(TkDocDetail)@521..525 "both"
            Token(TkDocDetailInlineMarkup)@525..528 "***"
        Token(TkDocDetail)@528..533 " both"
        Syntax(DocEm)@533..541
          Syntax(DocStrong)@533..541
            Token(TkDocDetailInlineMarkup)@533..536 "***"
            Token(TkDocDetail)@536..538 "in"
            Token(TkDocDetailInlineMarkup)@538..541 "***"
        Token(TkDocDetail)@541..545 "text"
        Token(TkEndOfLine)@545..546 "\n"
        Token(TkNormalStart)@546..549 "---"
        Token(TkWhitespace)@549..550 " "
        Syntax(DocEm)@550..562
          Syntax(DocStrong)@550..562
            Token(TkDocDetailInlineMarkup)@550..553 "***"
            Token(TkDocDetail)@553..561 "both end"
            Token(TkDocDetailInlineMarkup)@561..562 "*"
        Syntax(DocStrong)@562..575
          Token(TkDocDetail)@562..573 " separately"
          Token(TkDocDetailInlineMarkup)@573..575 "**"
        Token(TkEndOfLine)@575..576 "\n"
        Token(TkNormalStart)@576..579 "---"
        Token(TkWhitespace)@579..580 " "
        Syntax(DocEm)@580..593
          Syntax(DocStrong)@580..593
            Token(TkDocDetailInlineMarkup)@580..583 "***"
            Token(TkDocDetail)@583..591 "both end"
            Token(TkDocDetailInlineMarkup)@591..593 "**"
        Syntax(DocEm)@593..605
          Token(TkDocDetail)@593..604 " separately"
          Token(TkDocDetailInlineMarkup)@604..605 "*"
        Token(TkEndOfLine)@605..606 "\n"
        Token(TkNormalStart)@606..609 "---"
        Token(TkWhitespace)@609..610 " "
        Syntax(DocEm)@610..637
          Token(TkDocDetailInlineMarkup)@610..611 "*"
          Token(TkDocDetail)@611..616 "both "
          Syntax(DocStrong)@616..637
            Token(TkDocDetailInlineMarkup)@616..618 "**"
            Token(TkDocDetail)@618..634 "start separately"
            Token(TkDocDetailInlineMarkup)@634..637 "***"
        Token(TkEndOfLine)@637..638 "\n"
        Token(TkNormalStart)@638..641 "---"
        Token(TkWhitespace)@641..642 " "
        Syntax(DocStrong)@642..669
          Token(TkDocDetailInlineMarkup)@642..644 "**"
          Token(TkDocDetail)@644..649 "both "
          Syntax(DocEm)@649..669
            Token(TkDocDetailInlineMarkup)@649..650 "*"
            Token(TkDocDetail)@650..666 "start separately"
            Token(TkDocDetailInlineMarkup)@666..669 "***"
        Token(TkEndOfLine)@669..670 "\n"
        Token(TkNormalStart)@670..673 "---"
        Token(TkWhitespace)@673..674 " "
        Syntax(DocEm)@674..681
          Token(TkDocDetailInlineMarkup)@674..675 "*"
          Token(TkDocDetailInlineMarkup)@675..676 "`"
          Token(TkDocDetailInlineCode)@676..679 "foo"
          Token(TkDocDetailInlineMarkup)@679..680 "`"
          Token(TkDocDetailInlineMarkup)@680..681 "*"
    Token(TkEndOfLine)@681..682 "\n"
    Token(TkEndOfLine)@682..683 "\n"
    Syntax(Comment)@683..1680
      Token(TkNormalStart)@683..686 "---"
      Token(TkWhitespace)@686..687 " "
      Syntax(DocDescription)@687..1680
        Syntax(DocScope)@687..695
          Token(TkDocDetailMarkup)@687..688 "#"
          Token(TkDocDetail)@688..695 " Blocks"
        Token(TkEndOfLine)@695..696 "\n"
        Token(TkNormalStart)@696..699 "---"
        Token(TkEndOfLine)@699..700 "\n"
        Token(TkNormalStart)@700..703 "---"
        Token(TkWhitespace)@703..704 " "
        Syntax(DocScope)@704..722
          Token(TkDocDetailMarkup)@704..706 "##"
          Token(TkDocDetail)@706..722 " Thematic breaks"
        Token(TkEndOfLine)@722..723 "\n"
        Token(TkNormalStart)@723..726 "---"
        Token(TkEndOfLine)@726..727 "\n"
        Token(TkNormalStart)@727..730 "---"
        Token(TkWhitespace)@730..731 " "
        Syntax(DocScope)@731..736
          Token(TkDocDetailMarkup)@731..732 "-"
          Token(TkDocDetail)@732..733 " "
          Token(TkDocDetailMarkup)@733..734 "-"
          Token(TkDocDetail)@734..735 " "
          Token(TkDocDetailMarkup)@735..736 "-"
        Token(TkEndOfLine)@736..737 "\n"
        Token(TkNormalStart)@737..740 "---"
        Token(TkWhitespace)@740..741 " "
        Syntax(DocScope)@741..746
          Token(TkDocDetailMarkup)@741..742 "_"
          Token(TkDocDetail)@742..743 " "
          Token(TkDocDetailMarkup)@743..744 "_"
          Token(TkDocDetail)@744..745 " "
          Token(TkDocDetailMarkup)@745..746 "_"
        Token(TkEndOfLine)@746..747 "\n"
        Token(TkNormalStart)@747..750 "---"
        Token(TkWhitespace)@750..751 " "
        Syntax(DocScope)@751..756
          Token(TkDocDetailMarkup)@751..752 "*"
          Token(TkDocDetail)@752..753 " "
          Token(TkDocDetailMarkup)@753..754 "*"
          Token(TkDocDetail)@754..755 " "
          Token(TkDocDetailMarkup)@755..756 "*"
        Token(TkEndOfLine)@756..757 "\n"
        Token(TkNormalStart)@757..760 "---"
        Token(TkWhitespace)@760..761 " "
        Syntax(DocScope)@761..774
          Token(TkDocDetailMarkup)@761..762 "-"
          Token(TkDocDetail)@762..763 " "
          Token(TkDocDetail)@763..766 "_ -"
          Token(TkEndOfLine)@766..767 "\n"
          Token(TkNormalStart)@767..770 "---"
          Token(TkEndOfLine)@770..771 "\n"
          Token(TkNormalStart)@771..774 "---"
        Token(TkWhitespace)@774..775 " "
        Syntax(DocScope)@775..783
          Token(TkDocDetailMarkup)@775..777 "##"
          Token(TkDocDetail)@777..783 " Lists"
        Token(TkEndOfLine)@783..784 "\n"
        Token(TkNormalStart)@784..787 "---"
        Token(TkEndOfLine)@787..788 "\n"
        Token(TkNormalStart)@788..791 "---"
        Token(TkWhitespace)@791..792 " "
        Syntax(DocScope)@792..802
          Token(TkDocDetailMarkup)@792..793 "-"
          Token(TkDocDetail)@793..794 " "
          Token(TkDocDetail)@794..798 "List"
          Token(TkEndOfLine)@798..799 "\n"
          Token(TkNormalStart)@799..802 "---"
        Token(TkWhitespace)@802..803 " "
        Syntax(DocScope)@803..815
          Token(TkDocDetailMarkup)@803..804 "*"
          Token(TkDocDetail)@804..805 " "
          Token(TkDocDetail)@805..811 "List 2"
          Token(TkEndOfLine)@811..812 "\n"
          Token(TkNormalStart)@812..815 "---"
        Token(TkWhitespace)@815..816 " "
        Syntax(DocScope)@816..828
          Token(TkDocDetailMarkup)@816..817 "+"
          Token(TkDocDetail)@817..818 " "
          Token(TkDocDetail)@818..824 "List 3"
          Token(TkEndOfLine)@824..825 "\n"
          Token(TkNormalStart)@825..828 "---"
        Token(TkWhitespace)@828..829 " "
        Token(TkDocDetail)@829..841 "-Broken list"
        Token(TkEndOfLine)@841..842 "\n"
        Token(TkNormalStart)@842..845 "---"
        Token(TkEndOfLine)@845..846 "\n"
        Token(TkNormalStart)@846..849 "---"
        Token(TkWhitespace)@849..850 " "
        Syntax(DocScope)@850..886
          Token(TkDocDetailMarkup)@850..851 "-"
          Token(TkDocDetail)@851..855 "    "
          Token(TkDocDetail)@855..878 "List with indented text"
          Token(TkEndOfLine)@878..879 "\n"
          Token(TkNormalStart)@879..882 "---"
          Token(TkEndOfLine)@882..883 "\n"
          Token(TkNormalStart)@883..886 "---"
        Token(TkWhitespace)@886..887 " "
        Syntax(DocScope)@887..967
          Token(TkDocDetailMarkup)@887..888 "-"
          Token(TkDocDetail)@888..889 " "
          Syntax(DocScope)@889..967
            Token(TkDocDetail)@889..893 "    "
            Token(TkDocDetailCode)@893..907 "List with code"
            Token(TkEndOfLine)@907..908 "\n"
            Token(TkNormalStart)@908..911 "---"
            Token(TkWhitespace)@911..912 " "
            Token(TkDocDetail)@912..914 "  "
            Token(TkDocDetail)@914..918 "    "
            Token(TkDocDetailCode)@918..930 "Continuation"
            Token(TkEndOfLine)@930..931 "\n"
            Token(TkNormalStart)@931..934 "---"
            Token(TkEndOfLine)@934..935 "\n"
            Token(TkNormalStart)@935..938 "---"
            Token(TkWhitespace)@938..939 " "
            Token(TkDocDetail)@939..941 "  "
            Token(TkDocDetail)@941..945 "    "
            Token(TkDocDetailCode)@945..959 "Continuation 2"
            Token(TkEndOfLine)@959..960 "\n"
            Token(TkNormalStart)@960..963 "---"
            Token(TkEndOfLine)@963..964 "\n"
            Token(TkNormalStart)@964..967 "---"
        Token(TkWhitespace)@967..968 " "
        Syntax(DocScope)@968..1018
          Token(TkDocDetailMarkup)@968..969 "-"
          Token(TkEndOfLine)@969..970 "\n"
          Token(TkNormalStart)@970..973 "---"
          Token(TkWhitespace)@973..974 " "
          Token(TkDocDetail)@974..976 "  "
          Token(TkDocDetail)@976..1010 "List that starts with ..."
          Token(TkEndOfLine)@1010..1011 "\n"
          Token(TkNormalStart)@1011..1014 "---"
          Token(TkEndOfLine)@1014..1015 "\n"
          Token(TkNormalStart)@1015..1018 "---"
        Token(TkWhitespace)@1018..1019 " "
        Token(TkDocDetail)@1019..1020 " "
        Token(TkDocDetail)@1020..1028 "Not list"
        Token(TkEndOfLine)@1028..1029 "\n"
        Token(TkNormalStart)@1029..1032 "---"
        Token(TkEndOfLine)@1032..1033 "\n"
        Token(TkNormalStart)@1033..1036 "---"
        Token(TkWhitespace)@1036..1037 " "
        Syntax(DocScope)@1037..1089
          Token(TkDocDetail)@1037..1039 "  "
          Token(TkDocDetailMarkup)@1039..1040 "-"
          Token(TkDocDetail)@1040..1042 "  "
          Token(TkDocDetail)@1042..1050 "not code"
          Token(TkEndOfLine)@1050..1051 "\n"
          Token(TkNormalStart)@1051..1054 "---"
          Token(TkEndOfLine)@1054..1055 "\n"
          Token(TkNormalStart)@1055..1058 "---"
          Token(TkWhitespace)@1058..1059 " "
          Token(TkDocDetail)@1059..1064 "     "
          Token(TkDocDetail)@1064..1067 "   "
          Token(TkDocDetail)@1067..1081 "still not code"
          Token(TkEndOfLine)@1081..1082 "\n"
          Token(TkNormalStart)@1082..1085 "---"
          Token(TkEndOfLine)@1085..1086 "\n"
          Token(TkNormalStart)@1086..1089 "---"
        Token(TkWhitespace)@1089..1090 " "
        Syntax(DocScope)@1090..1106
          Token(TkDocDetail)@1090..1094 "    "
          Token(TkDocDetailCode)@1094..1098 "code"
          Token(TkEndOfLine)@1098..1099 "\n"
          Token(TkNormalStart)@1099..1102 "---"
          Token(TkEndOfLine)@1102..1103 "\n"
          Token(TkNormalStart)@1103..1106 "---"
        Token(TkWhitespace)@1106..1107 " "
        Syntax(DocScope)@1107..1124
          Token(TkDocDetailMarkup)@1107..1109 "##"
          Token(TkDocDetail)@1109..1124 " Numbered lists"
        Token(TkEndOfLine)@1124..1125 "\n"
        Token(TkNormalStart)@1125..1128 "---"
        Token(TkEndOfLine)@1128..1129 "\n"
        Token(TkNormalStart)@1129..1132 "---"
        Token(TkWhitespace)@1132..1133 " "
        Syntax(DocScope)@1133..1144
          Token(TkDocDetailMarkup)@1133..1135 "1."
          Token(TkDocDetail)@1135..1136 " "
          Token(TkDocDetail)@1136..1140 "List"
          Token(TkEndOfLine)@1140..1141 "\n"
          Token(TkNormalStart)@1141..1144 "---"
        Token(TkWhitespace)@1144..1145 " "
        Syntax(DocScope)@1145..1156
          Token(TkDocDetailMarkup)@1145..1147 "2:"
          Token(TkDocDetail)@1147..1148 " "
          Token(TkDocDetail)@1148..1152 "List"
          Token(TkEndOfLine)@1152..1153 "\n"
          Token(TkNormalStart)@1153..1156 "---"
        Token(TkWhitespace)@1156..1157 " "
        Syntax(DocScope)@1157..1168
          Token(TkDocDetailMarkup)@1157..1159 "3)"
          Token(TkDocDetail)@1159..1160 " "
          Token(TkDocDetail)@1160..1164 "List"
          Token(TkEndOfLine)@1164..1165 "\n"
          Token(TkNormalStart)@1165..1168 "---"
        Token(TkWhitespace)@1168..1169 " "
        Token(TkDocDetail)@1169..1171 "  "
        Token(TkDocDetail)@1171..1179 "Not list"
        Token(TkEndOfLine)@1179..1180 "\n"
        Token(TkNormalStart)@1180..1183 "---"
        Token(TkEndOfLine)@1183..1184 "\n"
        Token(TkNormalStart)@1184..1187 "---"
        Token(TkWhitespace)@1187..1188 " "
        Syntax(DocScope)@1188..1195
          Token(TkDocDetailMarkup)@1188..1190 "##"
          Token(TkDocDetail)@1190..1195 " Code"
        Token(TkEndOfLine)@1195..1196 "\n"
        Token(TkNormalStart)@1196..1199 "---"
        Token(TkEndOfLine)@1199..1200 "\n"
        Token(TkNormalStart)@1200..1203 "---"
        Token(TkWhitespace)@1203..1204 " "
        Syntax(DocScope)@1204..1255
          Token(TkDocDetail)@1204..1208 "    "
          Token(TkDocDetailCode)@1208..1220 "This is code"
          Token(TkEndOfLine)@1220..1221 "\n"
          Token(TkNormalStart)@1221..1224 "---"
          Token(TkWhitespace)@1224..1225 " "
          Token(TkDocDetail)@1225..1229 "    "
          Token(TkDocDetailCode)@1229..1247 " This is also code"
          Token(TkEndOfLine)@1247..1248 "\n"
          Token(TkNormalStart)@1248..1251 "---"
          Token(TkEndOfLine)@1251..1252 "\n"
          Token(TkNormalStart)@1252..1255 "---"
        Token(TkWhitespace)@1255..1256 " "
        Syntax(DocScope)@1256..1270
          Token(TkDocDetailMarkup)@1256..1258 "##"
          Token(TkDocDetail)@1258..1270 " Fenced code"
        Token(TkEndOfLine)@1270..1271 "\n"
        Token(TkNormalStart)@1271..1274 "---"
        Token(TkEndOfLine)@1274..1275 "\n"
        Token(TkNormalStart)@1275..1278 "---"
        Token(TkWhitespace)@1278..1279 " "
        Syntax(DocScope)@1279..1305
          Token(TkDocDetailMarkup)@1279..1282 "```"
          Token(TkDocDetailCode)@1282..1288 "syntax"
          Token(TkEndOfLine)@1288..1289 "\n"
          Token(TkNormalStart)@1289..1292 "---"
          Token(TkWhitespace)@1292..1293 " "
          Token(TkDocDetailCode)@1293..1297 "code"
          Token(TkEndOfLine)@1297..1298 "\n"
          Token(TkNormalStart)@1298..1301 "---"
          Token(TkWhitespace)@1301..1302 " "
          Token(TkDocDetailMarkup)@1302..1305 "```"
        Token(TkEndOfLine)@1305..1306 "\n"
        Token(TkNormalStart)@1306..1309 "---"
        Token(TkWhitespace)@1309..1310 " "
        Token(TkDocDetail)@1310..1318 "not code"
        Token(TkEndOfLine)@1318..1319 "\n"
        Token(TkNormalStart)@1319..1322 "---"
        Token(TkWhitespace)@1322..1323 " "
        Syntax(DocScope)@1323..1372
          Token(TkDocDetailMarkup)@1323..1326 "~~~"
          Token(TkDocDetailCode)@1326..1332 "syntax"
          Token(TkEndOfLine)@1332..1333 "\n"
          Token(TkNormalStart)@1333..1336 "---"
          Token(TkWhitespace)@1336..1337 " "
          Token(TkDocDetailCode)@1337..1341 "code"
          Token(TkEndOfLine)@1341..1342 "\n"
          Token(TkNormalStart)@1342..1345 "---"
          Token(TkWhitespace)@1345..1346 " "
          Token(TkDocDetailCode)@1346..1349 "```"
          Token(TkEndOfLine)@1349..1350 "\n"
          Token(TkNormalStart)@1350..1353 "---"
          Token(TkWhitespace)@1353..1354 " "
          Token(TkDocDetailCode)@1354..1364 "still code"
          Token(TkEndOfLine)@1364..1365 "\n"
          Token(TkNormalStart)@1365..1368 "---"
          Token(TkWhitespace)@1368..1369 " "
          Token(TkDocDetailMarkup)@1369..1372 "~~~"
        Token(TkEndOfLine)@1372..1373 "\n"
        Token(TkNormalStart)@1373..1376 "---"
        Token(TkEndOfLine)@1376..1377 "\n"
        Token(TkNormalStart)@1377..1380 "---"
        Token(TkWhitespace)@1380..1381 " "
        Syntax(DocScope)@1381..1420
          Token(TkDocDetailMarkup)@1381..1385 "````"
          Token(TkDocDetailCode)@1385..1403 "code with 4 fences"
          Token(TkEndOfLine)@1403..1404 "\n"
          Token(TkNormalStart)@1404..1407 "---"
          Token(TkWhitespace)@1407..1408 " "
          Token(TkDocDetailCode)@1408..1411 "```"
          Token(TkEndOfLine)@1411..1412 "\n"
          Token(TkNormalStart)@1412..1415 "---"
          Token(TkWhitespace)@1415..1416 " "
          Token(TkDocDetailMarkup)@1416..1420 "````"
        Token(TkEndOfLine)@1420..1421 "\n"
        Token(TkNormalStart)@1421..1424 "---"
        Token(TkEndOfLine)@1424..1425 "\n"
        Token(TkNormalStart)@1425..1428 "---"
        Token(TkWhitespace)@1428..1429 " "
        Token(TkDocDetailInlineMarkup)@1429..1432 "```"
        Token(TkDocDetailInlineCode)@1432..1443 "inline code"
        Token(TkDocDetailInlineMarkup)@1443..1446 "```"
        Token(TkEndOfLine)@1446..1447 "\n"
        Token(TkNormalStart)@1447..1450 "---"
        Token(TkWhitespace)@1450..1451 " "
        Token(TkDocDetail)@1451..1459 "not code"
        Token(TkEndOfLine)@1459..1460 "\n"
        Token(TkNormalStart)@1460..1463 "---"
        Token(TkEndOfLine)@1463..1464 "\n"
        Token(TkNormalStart)@1464..1467 "---"
        Token(TkWhitespace)@1467..1468 " "
        Syntax(DocScope)@1468..1477
          Token(TkDocDetailMarkup)@1468..1470 "##"
          Token(TkDocDetail)@1470..1477 " Quotes"
        Token(TkEndOfLine)@1477..1478 "\n"
        Token(TkNormalStart)@1478..1481 "---"
        Token(TkEndOfLine)@1481..1482 "\n"
        Token(TkNormalStart)@1482..1485 "---"
        Token(TkWhitespace)@1485..1486 " "
        Syntax(DocScope)@1486..1513
          Token(TkDocDetailMarkup)@1486..1487 ">"
          Token(TkDocDetail)@1487..1488 " "
          Token(TkDocDetail)@1488..1493 "Quote"
          Token(TkEndOfLine)@1493..1494 "\n"
          Token(TkNormalStart)@1494..1497 "---"
          Token(TkWhitespace)@1497..1498 " "
          Token(TkDocDetailMarkup)@1498..1499 ">"
          Token(TkDocDetail)@1499..1500 " "
          Token(TkDocDetail)@1500..1509 "Continues"
          Token(TkEndOfLine)@1509..1510 "\n"
          Token(TkNormalStart)@1510..1513 "---"
        Token(TkEndOfLine)@1513..1514 "\n"
        Token(TkNormalStart)@1514..1517 "---"
        Token(TkWhitespace)@1517..1518 " "
        Syntax(DocScope)@1518..1531
          Token(TkDocDetailMarkup)@1518..1519 ">"
          Token(TkDocDetail)@1519..1520 " "
          Token(TkDocDetail)@1520..1527 "Quote 2"
          Token(TkEndOfLine)@1527..1528 "\n"
          Token(TkNormalStart)@1528..1531 "---"
        Token(TkEndOfLine)@1531..1532 "\n"
        Token(TkNormalStart)@1532..1535 "---"
        Token(TkWhitespace)@1535..1536 " "
        Syntax(DocScope)@1536..1563
          Token(TkDocDetailMarkup)@1536..1538 "##"
          Token(TkDocDetail)@1538..1563 " Disabled MySt extens ..."
        Token(TkEndOfLine)@1563..1564 "\n"
        Token(TkNormalStart)@1564..1567 "---"
        Token(TkEndOfLine)@1567..1568 "\n"
        Token(TkNormalStart)@1568..1571 "---"
        Token(TkWhitespace)@1571..1572 " "
        Token(TkDocDetail)@1572..1574 "$$"
        Token(TkEndOfLine)@1574..1575 "\n"
        Token(TkNormalStart)@1575..1578 "---"
        Token(TkWhitespace)@1578..1579 " "
        Token(TkDocDetail)@1579..1583 "math"
        Token(TkEndOfLine)@1583..1584 "\n"
        Token(TkNormalStart)@1584..1587 "---"
        Token(TkWhitespace)@1587..1588 " "
        Token(TkDocDetail)@1588..1590 "$$"
        Token(TkEndOfLine)@1590..1591 "\n"
        Token(TkNormalStart)@1591..1594 "---"
        Token(TkEndOfLine)@1594..1595 "\n"
        Token(TkNormalStart)@1595..1598 "---"
        Token(TkWhitespace)@1598..1599 " "
        Syntax(DocScope)@1599..1621
          Token(TkDocDetailMarkup)@1599..1602 "```"
          Token(TkDocDetailCode)@1602..1613 "{directive}"
          Token(TkEndOfLine)@1613..1614 "\n"
          Token(TkNormalStart)@1614..1617 "---"
          Token(TkWhitespace)@1617..1618 " "
          Token(TkDocDetailMarkup)@1618..1621 "```"
        Token(TkEndOfLine)@1621..1622 "\n"
        Token(TkNormalStart)@1622..1625 "---"
        Token(TkEndOfLine)@1625..1626 "\n"
        Token(TkNormalStart)@1626..1629 "---"
        Token(TkWhitespace)@1629..1630 " "
        Syntax(DocScope)@1630..1644
          Token(TkDocDetailMarkup)@1630..1632 "##"
          Token(TkDocDetail)@1632..1644 " Link anchor"
        Token(TkEndOfLine)@1644..1645 "\n"
        Token(TkNormalStart)@1645..1648 "---"
        Token(TkEndOfLine)@1648..1649 "\n"
        Token(TkNormalStart)@1649..1652 "---"
        Token(TkWhitespace)@1652..1653 " "
        Syntax(DocScope)@1653..1680
          Token(TkDocDetailInlineLink)@1653..1659 "[link]"
          Token(TkDocDetailInlineMarkup)@1659..1660 ":"
          Token(TkDocDetail)@1660..1661 " "
          Token(TkDocDetailInlineLink)@1661..1680 "https://example.com"
    Token(TkEndOfLine)@1680..1681 "\n"
"###;

        let tree = LuaParser::parse(
            code,
            ParserConfig::with_desc_parser_type(DescParserType::Md),
        );
        let result = format!("{:#?}", tree.get_red_root()).trim().to_string();
        let expected = expected.trim().to_string();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_myst() {
        let code = r#"
--- # Inline
---
--- {lua:obj}`a.b.c`, {lua:obj}`~a.b.c`,
--- {lua:obj}`<a.b.c>`, {lua:obj}`<~a.b.c>`, {lua:obj}`title <~a.b.c>`.
--- $inline math$, text, $$more inline math$$,
--- $$even more inline math$$.

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

        let expected = r###"
Syntax(Chunk)@0..655
  Syntax(Block)@0..655
    Token(TkEndOfLine)@0..1 "\n"
    Syntax(Comment)@1..208
      Token(TkNormalStart)@1..4 "---"
      Token(TkWhitespace)@4..5 " "
      Syntax(DocDescription)@5..208
        Syntax(DocScope)@5..13
          Token(TkDocDetailMarkup)@5..6 "#"
          Token(TkDocDetail)@6..13 " Inline"
        Token(TkEndOfLine)@13..14 "\n"
        Token(TkNormalStart)@14..17 "---"
        Token(TkEndOfLine)@17..18 "\n"
        Token(TkNormalStart)@18..21 "---"
        Token(TkWhitespace)@21..22 " "
        Syntax(DocRef)@22..38
          Token(TkDocDetailInlineArgMarkup)@22..23 "{"
          Token(TkDocDetailInlineArg)@23..30 "lua:obj"
          Token(TkDocDetailInlineArgMarkup)@30..31 "}"
          Token(TkDocDetailInlineMarkup)@31..32 "`"
          Token(TkDocDetailRef)@32..37 "a.b.c"
          Token(TkDocDetailInlineMarkup)@37..38 "`"
        Token(TkDocDetail)@38..40 ", "
        Syntax(DocRef)@40..57
          Token(TkDocDetailInlineArgMarkup)@40..41 "{"
          Token(TkDocDetailInlineArg)@41..48 "lua:obj"
          Token(TkDocDetailInlineArgMarkup)@48..49 "}"
          Token(TkDocDetailInlineMarkup)@49..50 "`"
          Token(TkDocDetailInlineCode)@50..51 "~"
          Token(TkDocDetailRef)@51..56 "a.b.c"
          Token(TkDocDetailInlineMarkup)@56..57 "`"
        Token(TkDocDetail)@57..58 ","
        Token(TkEndOfLine)@58..59 "\n"
        Token(TkNormalStart)@59..62 "---"
        Token(TkWhitespace)@62..63 " "
        Syntax(DocRef)@63..81
          Token(TkDocDetailInlineArgMarkup)@63..64 "{"
          Token(TkDocDetailInlineArg)@64..71 "lua:obj"
          Token(TkDocDetailInlineArgMarkup)@71..72 "}"
          Token(TkDocDetailInlineMarkup)@72..73 "`"
          Token(TkDocDetailInlineCode)@73..74 "<"
          Token(TkDocDetailRef)@74..79 "a.b.c"
          Token(TkDocDetailInlineCode)@79..80 ">"
          Token(TkDocDetailInlineMarkup)@80..81 "`"
        Token(TkDocDetail)@81..83 ", "
        Syntax(DocRef)@83..102
          Token(TkDocDetailInlineArgMarkup)@83..84 "{"
          Token(TkDocDetailInlineArg)@84..91 "lua:obj"
          Token(TkDocDetailInlineArgMarkup)@91..92 "}"
          Token(TkDocDetailInlineMarkup)@92..93 "`"
          Token(TkDocDetailInlineCode)@93..95 "<~"
          Token(TkDocDetailRef)@95..100 "a.b.c"
          Token(TkDocDetailInlineCode)@100..101 ">"
          Token(TkDocDetailInlineMarkup)@101..102 "`"
        Token(TkDocDetail)@102..104 ", "
        Syntax(DocRef)@104..129
          Token(TkDocDetailInlineArgMarkup)@104..105 "{"
          Token(TkDocDetailInlineArg)@105..112 "lua:obj"
          Token(TkDocDetailInlineArgMarkup)@112..113 "}"
          Token(TkDocDetailInlineMarkup)@113..114 "`"
          Token(TkDocDetailInlineCode)@114..122 "title <~"
          Token(TkDocDetailRef)@122..127 "a.b.c"
          Token(TkDocDetailInlineCode)@127..128 ">"
          Token(TkDocDetailInlineMarkup)@128..129 "`"
        Token(TkDocDetail)@129..130 "."
        Token(TkEndOfLine)@130..131 "\n"
        Token(TkNormalStart)@131..134 "---"
        Token(TkWhitespace)@134..135 " "
        Token(TkDocDetailInlineMarkup)@135..136 "$"
        Token(TkDocDetailInlineCode)@136..147 "inline math"
        Token(TkDocDetailInlineMarkup)@147..148 "$"
        Token(TkDocDetail)@148..156 ", text, "
        Token(TkDocDetailInlineMarkup)@156..158 "$$"
        Token(TkDocDetailInlineCode)@158..174 "more inline math"
        Token(TkDocDetailInlineMarkup)@174..176 "$$"
        Token(TkDocDetail)@176..177 ","
        Token(TkEndOfLine)@177..178 "\n"
        Token(TkNormalStart)@178..181 "---"
        Token(TkWhitespace)@181..182 " "
        Token(TkDocDetailInlineMarkup)@182..184 "$$"
        Token(TkDocDetailInlineCode)@184..205 "even more inline math"
        Token(TkDocDetailInlineMarkup)@205..207 "$$"
        Token(TkDocDetail)@207..208 "."
    Token(TkEndOfLine)@208..209 "\n"
    Token(TkEndOfLine)@209..210 "\n"
    Syntax(Comment)@210..654
      Token(TkNormalStart)@210..213 "---"
      Token(TkWhitespace)@213..214 " "
      Syntax(DocDescription)@214..654
        Syntax(DocScope)@214..226
          Token(TkDocDetailMarkup)@214..215 "#"
          Token(TkDocDetail)@215..226 " Directives"
        Token(TkEndOfLine)@226..227 "\n"
        Token(TkNormalStart)@227..230 "---"
        Token(TkEndOfLine)@230..231 "\n"
        Token(TkNormalStart)@231..234 "---"
        Token(TkWhitespace)@234..235 " "
        Syntax(DocScope)@235..257
          Token(TkDocDetailMarkup)@235..238 "```"
          Token(TkDocDetailArgMarkup)@238..239 "{"
          Token(TkDocDetailArg)@239..248 "directive"
          Token(TkDocDetailArgMarkup)@248..249 "}"
          Token(TkEndOfLine)@249..250 "\n"
          Token(TkNormalStart)@250..253 "---"
          Token(TkWhitespace)@253..254 " "
          Token(TkDocDetailMarkup)@254..257 "```"
        Token(TkEndOfLine)@257..258 "\n"
        Token(TkNormalStart)@258..261 "---"
        Token(TkWhitespace)@261..262 " "
        Syntax(DocScope)@262..293
          Token(TkDocDetailMarkup)@262..265 "```"
          Token(TkDocDetailArgMarkup)@265..266 "{"
          Token(TkDocDetailArg)@266..275 "directive"
          Token(TkDocDetailArgMarkup)@275..276 "}"
          Token(TkEndOfLine)@276..277 "\n"
          Token(TkNormalStart)@277..280 "---"
          Token(TkWhitespace)@280..281 " "
          Token(TkDocDetail)@281..285 "Body"
          Token(TkEndOfLine)@285..286 "\n"
          Token(TkNormalStart)@286..289 "---"
          Token(TkWhitespace)@289..290 " "
          Token(TkDocDetailMarkup)@290..293 "```"
        Token(TkEndOfLine)@293..294 "\n"
        Token(TkNormalStart)@294..297 "---"
        Token(TkWhitespace)@297..298 " "
        Syntax(DocScope)@298..347
          Token(TkDocDetailMarkup)@298..301 "```"
          Token(TkDocDetailArgMarkup)@301..302 "{"
          Token(TkDocDetailArg)@302..311 "directive"
          Token(TkDocDetailArgMarkup)@311..312 "}"
          Token(TkEndOfLine)@312..313 "\n"
          Token(TkNormalStart)@313..316 "---"
          Token(TkWhitespace)@316..317 " "
          Token(TkDocDetailArgMarkup)@317..318 ":"
          Token(TkDocDetailArg)@318..323 "param"
          Token(TkDocDetailArgMarkup)@323..324 ":"
          Token(TkDocDetail)@324..325 " "
          Token(TkDocDetailCode)@325..330 "value"
          Token(TkEndOfLine)@330..331 "\n"
          Token(TkNormalStart)@331..334 "---"
          Token(TkWhitespace)@334..335 " "
          Token(TkDocDetail)@335..339 "Body"
          Token(TkEndOfLine)@339..340 "\n"
          Token(TkNormalStart)@340..343 "---"
          Token(TkWhitespace)@343..344 " "
          Token(TkDocDetailMarkup)@344..347 "```"
        Token(TkEndOfLine)@347..348 "\n"
        Token(TkNormalStart)@348..351 "---"
        Token(TkWhitespace)@351..352 " "
        Syntax(DocScope)@352..409
          Token(TkDocDetailMarkup)@352..355 "```"
          Token(TkDocDetailArgMarkup)@355..356 "{"
          Token(TkDocDetailArg)@356..365 "directive"
          Token(TkDocDetailArgMarkup)@365..366 "}"
          Token(TkEndOfLine)@366..367 "\n"
          Token(TkNormalStart)@367..370 "---"
          Token(TkWhitespace)@370..371 " "
          Token(TkDocDetailMarkup)@371..374 "---"
          Token(TkEndOfLine)@374..375 "\n"
          Token(TkNormalStart)@375..378 "---"
          Token(TkWhitespace)@378..379 " "
          Token(TkDocDetailCode)@379..384 "param"
          Token(TkEndOfLine)@384..385 "\n"
          Token(TkNormalStart)@385..388 "---"
          Token(TkWhitespace)@388..389 " "
          Token(TkDocDetailMarkup)@389..392 "---"
          Token(TkEndOfLine)@392..393 "\n"
          Token(TkNormalStart)@393..396 "---"
          Token(TkWhitespace)@396..397 " "
          Token(TkDocDetail)@397..401 "Body"
          Token(TkEndOfLine)@401..402 "\n"
          Token(TkNormalStart)@402..405 "---"
          Token(TkWhitespace)@405..406 " "
          Token(TkDocDetailMarkup)@406..409 "```"
        Token(TkEndOfLine)@409..410 "\n"
        Token(TkNormalStart)@410..413 "---"
        Token(TkWhitespace)@413..414 " "
        Syntax(DocScope)@414..494
          Token(TkDocDetailMarkup)@414..418 "````"
          Token(TkDocDetailArgMarkup)@418..419 "{"
          Token(TkDocDetailArg)@419..429 "directive1"
          Token(TkDocDetailArgMarkup)@429..430 "}"
          Token(TkEndOfLine)@430..431 "\n"
          Token(TkNormalStart)@431..434 "---"
          Token(TkWhitespace)@434..435 " "
          Token(TkDocDetail)@435..439 "Body"
          Token(TkEndOfLine)@439..440 "\n"
          Token(TkNormalStart)@440..443 "---"
          Token(TkWhitespace)@443..444 " "
          Syntax(DocScope)@444..476
            Token(TkDocDetailMarkup)@444..447 "```"
            Token(TkDocDetailArgMarkup)@447..448 "{"
            Token(TkDocDetailArg)@448..458 "directive2"
            Token(TkDocDetailArgMarkup)@458..459 "}"
            Token(TkEndOfLine)@459..460 "\n"
            Token(TkNormalStart)@460..463 "---"
            Token(TkWhitespace)@463..464 " "
            Token(TkDocDetail)@464..468 "Body"
            Token(TkEndOfLine)@468..469 "\n"
            Token(TkNormalStart)@469..472 "---"
            Token(TkWhitespace)@472..473 " "
            Token(TkDocDetailMarkup)@473..476 "```"
          Token(TkEndOfLine)@476..477 "\n"
          Token(TkNormalStart)@477..480 "---"
          Token(TkWhitespace)@480..481 " "
          Token(TkDocDetail)@481..485 "Body"
          Token(TkEndOfLine)@485..486 "\n"
          Token(TkNormalStart)@486..489 "---"
          Token(TkWhitespace)@489..490 " "
          Token(TkDocDetailMarkup)@490..494 "````"
        Token(TkEndOfLine)@494..495 "\n"
        Token(TkNormalStart)@495..498 "---"
        Token(TkWhitespace)@498..499 " "
        Syntax(DocScope)@499..549
          Token(TkDocDetailMarkup)@499..502 "```"
          Token(TkDocDetailArgMarkup)@502..503 "{"
          Token(TkDocDetailArg)@503..513 "code-block"
          Token(TkDocDetailArgMarkup)@513..514 "}"
          Token(TkDocDetail)@514..515 " "
          Token(TkDocDetailCode)@515..518 "lua"
          Token(TkEndOfLine)@518..519 "\n"
          Token(TkNormalStart)@519..522 "---"
          Token(TkWhitespace)@522..523 " "
          Token(TkDocDetailCode)@523..541 "function foo() end"
          Token(TkEndOfLine)@541..542 "\n"
          Token(TkNormalStart)@542..545 "---"
          Token(TkWhitespace)@545..546 " "
          Token(TkDocDetailMarkup)@546..549 "```"
        Token(TkEndOfLine)@549..550 "\n"
        Token(TkNormalStart)@550..553 "---"
        Token(TkEndOfLine)@553..554 "\n"
        Token(TkNormalStart)@554..557 "---"
        Token(TkWhitespace)@557..558 " "
        Syntax(DocScope)@558..564
          Token(TkDocDetailMarkup)@558..559 "#"
          Token(TkDocDetail)@559..564 " Math"
        Token(TkEndOfLine)@564..565 "\n"
        Token(TkNormalStart)@565..568 "---"
        Token(TkEndOfLine)@568..569 "\n"
        Token(TkNormalStart)@569..572 "---"
        Token(TkWhitespace)@572..573 " "
        Syntax(DocScope)@573..598
          Token(TkDocDetailMarkup)@573..575 "$$"
          Token(TkEndOfLine)@575..576 "\n"
          Token(TkNormalStart)@576..579 "---"
          Token(TkWhitespace)@579..580 " "
          Token(TkDocDetailCode)@580..591 "\\frac{1}{2}"
          Token(TkEndOfLine)@591..592 "\n"
          Token(TkNormalStart)@592..595 "---"
          Token(TkWhitespace)@595..596 " "
          Token(TkDocDetailMarkup)@596..598 "$$"
        Token(TkEndOfLine)@598..599 "\n"
        Token(TkNormalStart)@599..602 "---"
        Token(TkEndOfLine)@602..603 "\n"
        Token(TkNormalStart)@603..606 "---"
        Token(TkWhitespace)@606..607 " "
        Token(TkDocDetail)@607..611 "Text"
        Token(TkEndOfLine)@611..612 "\n"
        Token(TkNormalStart)@612..615 "---"
        Token(TkEndOfLine)@615..616 "\n"
        Token(TkNormalStart)@616..619 "---"
        Token(TkWhitespace)@619..620 " "
        Syntax(DocScope)@620..654
          Token(TkDocDetailMarkup)@620..622 "$$"
          Token(TkEndOfLine)@622..623 "\n"
          Token(TkNormalStart)@623..626 "---"
          Token(TkWhitespace)@626..627 " "
          Token(TkDocDetailCode)@627..638 "\\frac{1}{2}"
          Token(TkEndOfLine)@638..639 "\n"
          Token(TkNormalStart)@639..642 "---"
          Token(TkWhitespace)@642..643 " "
          Token(TkDocDetailMarkup)@643..645 "$$"
          Token(TkDocDetail)@645..646 " "
          Token(TkDocDetailArg)@646..654 "(anchor)"
    Token(TkEndOfLine)@654..655 "\n"
"###;

        let tree = LuaParser::parse(
            code,
            ParserConfig::with_desc_parser_type(DescParserType::MySt {
                primary_domain: None,
            }),
        );
        let result = format!("{:#?}", tree.get_red_root()).trim().to_string();
        let expected = expected.trim().to_string();
        assert_eq!(result, expected);
    }
}
