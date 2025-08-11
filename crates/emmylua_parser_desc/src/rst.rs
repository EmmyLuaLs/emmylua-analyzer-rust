use crate::LuaDocDescription;
use crate::lang::{CodeBlockLang, process_code};
use crate::util::{
    BacktrackPoint, ResultContainer, desc_to_lines, is_blank, is_closing_quote, is_code_directive,
    is_lua_role, is_opening_quote, is_quote_match, is_ws,
};
use crate::{DescItem, DescItemKind, LuaDescParser};
use emmylua_parser::{LexerState, Reader, SourceRange};
use std::cmp::min;
use unicode_general_category::{GeneralCategory, get_general_category};

pub struct RstParser {
    primary_domain: Option<String>,
    default_role: Option<String>,
    result: Vec<DescItem>,
    cursor_position: Option<usize>,
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum LineEnding {
    Normal,
    LiteralMark, // Line ends with double colon.
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum ListEnumeratorKind {
    Auto,
    Number,
    SmallLetter,
    CapitalLetter,
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum ListMarkerKind {
    Dot,
    Paren,
    Enclosed,
}

impl LuaDescParser for RstParser {
    fn parse(&mut self, text: &str, desc: LuaDocDescription) -> Vec<DescItem> {
        assert!(self.result.is_empty());

        let lines = desc_to_lines(text, desc, self.cursor_position);
        let mut readers: Vec<_> = lines
            .iter()
            .map(|range| {
                let line = &text[range.start_offset..range.end_offset()];
                Reader::new_with_range(line, *range)
            })
            .collect();

        self.process_block(&mut readers);

        std::mem::take(&mut self.result)
    }
}

impl ResultContainer for RstParser {
    fn results(&self) -> &Vec<DescItem> {
        &self.result
    }

    fn results_mut(&mut self) -> &mut Vec<DescItem> {
        &mut self.result
    }

    fn cursor_position(&self) -> Option<usize> {
        self.cursor_position
    }
}

impl RstParser {
    pub fn new(
        primary_domain: Option<String>,
        default_role: Option<String>,
        cursor_position: Option<usize>,
    ) -> Self {
        Self {
            primary_domain,
            default_role,
            result: Vec::new(),
            cursor_position,
        }
    }

    fn process_block(&mut self, lines: &mut [Reader]) {
        let mut i = 0;
        let mut prev_line_ending = LineEnding::Normal;
        while i < lines.len() {
            (i, prev_line_ending) = self.consume_block(lines, i, prev_line_ending)
        }
    }

    fn consume_block(
        &mut self,
        lines: &mut [Reader],
        start: usize,
        prev_line_ending: LineEnding,
    ) -> (usize, LineEnding) {
        let line = &mut lines[start];

        if is_blank(line.tail_text()) {
            line.eat_till_end();
            line.reset_buff();
            return (start + 1, prev_line_ending);
        }

        let res = match line.current_char() {
            // Indented literal text.
            ch if prev_line_ending == LineEnding::LiteralMark
                && (is_ws(ch) || Self::is_indent_c(ch)) =>
            {
                self.try_process_literal_text(lines, start)
            }

            // Line block.
            '|' if is_ws(line.next_char()) || line.next_char() == '\0' => {
                self.process_line_block(lines, start)
            }

            // Bullet list.
            '*' | '+' | '-' | '•' | '‣' | '⁃'
                if is_ws(line.next_char()) || line.next_char() == '\0' =>
            {
                self.process_bullet_list(lines, start)
            }

            // Maybe numbered list.
            '0'..='9' | 'a'..='z' | 'A'..='Z' | '#' | '(' => {
                self.try_process_numbered_list(lines, start)
            }

            // Maybe field list.
            ':' if line.next_char() != ':' => self.try_process_field_list(lines, start),

            // Doctest block.
            '>' if line.tail_text().starts_with(">>>") => self.process_doctest_block(lines, start),

            // Maybe explicit markup start.
            '.' if line.next_char() == '.' => self.try_process_explicit_markup(lines, start),

            // Block quote.
            ' ' | '\t' => self.process_block_quote(lines, start),

            // Maybe implicit hyperlink target.
            '_' if line.next_char() == '_' => {
                self.try_process_implicit_hyperlink_target(lines, start)
            }

            // Normal line, will be processed as inline contents.
            _ => Err(()),
        };

        match res {
            Ok(end) => (end, LineEnding::Normal),
            Err(_) => {
                // Paragraph.
                self.process_paragraph(lines, start)
            }
        }
    }

    fn try_process_literal_text(
        &mut self,
        lines: &mut [Reader],
        start: usize,
    ) -> Result<usize, ()> {
        //   Line
        //
        // > Line

        let ch = lines[start].current_char();

        let end = if is_ws(ch) {
            self.gather_indented_lines(lines, start, true)
        } else if Self::is_indent_c(ch) {
            self.gather_prefixed_lines(lines, start, ch)
        } else {
            return Err(());
        };

        let scope_start = lines[start].current_range().start_offset;
        for line in &mut lines[start..end] {
            line.eat_till_end();
            self.emit(line, DescItemKind::CodeBlock);
        }
        let scope_end = lines[end - 1].current_range().end_offset();
        self.emit_range(
            SourceRange::from_start_end(scope_start, scope_end),
            DescItemKind::Scope,
        );

        Ok(end)
    }

    fn process_line_block(&mut self, lines: &mut [Reader], start: usize) -> Result<usize, ()> {
        // | Line.
        //   Line continuation.

        let line = &mut lines[start];

        if line.current_char() != '|' || !(is_ws(line.next_char()) || line.next_char() == '\0') {
            return Err(());
        }

        let scope_start = line.current_range().start_offset;

        line.bump();
        self.emit(line, DescItemKind::Markup);
        self.process_inline_content(line);

        let end = self.gather_indented_lines(lines, start + 1, false);
        for line in lines[start + 1..end].iter_mut() {
            self.process_inline_content(line);
        }

        let scope_end = lines[end - 1].current_range().end_offset();
        self.emit_range(
            SourceRange::from_start_end(scope_start, scope_end),
            DescItemKind::Scope,
        );

        Ok(end)
    }

    fn process_bullet_list(&mut self, lines: &mut [Reader], start: usize) -> Result<usize, ()> {
        // - Line
        let line = &mut lines[start];

        if !matches!(line.current_char(), '*' | '+' | '-' | '•' | '‣' | '⁃')
            || !(is_ws(line.next_char()) || line.next_char() == '\0')
        {
            return Err(());
        }

        let scope_start = line.current_range().start_offset;

        line.bump();
        self.emit(line, DescItemKind::Markup);

        let end = {
            if line.is_eof() {
                self.gather_indented_lines(lines, start + 1, true)
            } else {
                let indent = line.eat_while(is_ws) + 1;
                self.gather_exactly_indented_lines(lines, start + 1, indent, true)
            }
        };
        self.process_block(&mut lines[start..end]);

        let scope_end = lines[end - 1].current_range().end_offset();
        self.emit_range(
            SourceRange::from_start_end(scope_start, scope_end),
            DescItemKind::Scope,
        );

        Ok(end)
    }

    fn try_process_numbered_list(
        &mut self,
        lines: &mut [Reader],
        start: usize,
    ) -> Result<usize, ()> {
        // 1) Line
        // #) Line
        // a) Line
        //
        // 1. Line
        // 1) Line
        // (1) Line

        let line;
        let next_line;
        if start + 1 < lines.len() {
            let [got_line, got_next_line] = lines.get_disjoint_mut([start, start + 1]).unwrap();
            line = got_line;
            next_line = Some(got_next_line);
        } else {
            line = &mut lines[start];
            next_line = None;
        }

        let bt = BacktrackPoint::new(self, line);
        let scope_start = line.current_range().start_offset;

        let mut indent = 0;

        let starts_with_paren = line.current_char() == '(';
        if starts_with_paren {
            line.bump();
            indent += 1;
        }

        let list_enumerator_kind = match line.current_char() {
            '#' => {
                line.bump();
                indent += 1;
                ListEnumeratorKind::Auto
            }
            '0'..='9' => {
                indent += line.eat_while(|c| c.is_ascii_digit());
                ListEnumeratorKind::Number
            }
            'a'..='z' => {
                line.bump();
                indent += 1;
                ListEnumeratorKind::SmallLetter
            }
            'A'..='Z' => {
                line.bump();
                indent += 1;
                ListEnumeratorKind::CapitalLetter
            }
            _ => {
                bt.rollback(self, line);
                return Err(());
            }
        };

        let list_marker_kind = match line.current_char() {
            ')' => {
                line.bump();
                indent += 1;
                if starts_with_paren {
                    ListMarkerKind::Enclosed
                } else {
                    ListMarkerKind::Paren
                }
            }
            '.' if !starts_with_paren => {
                line.bump();
                indent += 1;
                ListMarkerKind::Dot
            }
            _ => {
                bt.rollback(self, line);
                return Err(());
            }
        };

        if !(is_ws(line.current_char()) || line.is_eof()) {
            bt.rollback(self, line);
            return Err(());
        }

        if let Some(next_line) = next_line {
            if !(is_ws(next_line.current_char())
                || next_line.is_eof()
                || Self::is_list_start(
                    next_line.tail_text(),
                    list_enumerator_kind,
                    list_marker_kind,
                ))
            {
                bt.rollback(self, line);
                return Err(());
            }
        }

        self.emit(line, DescItemKind::Markup);
        indent += line.eat_while(is_ws);
        line.reset_buff();
        bt.commit(self, line);

        let end = self.gather_exactly_indented_lines(lines, start + 1, indent, true);
        self.process_block(&mut lines[start..end]);

        let scope_end = lines[end - 1].current_range().end_offset();
        self.emit_range(
            SourceRange::from_start_end(scope_start, scope_end),
            DescItemKind::Scope,
        );

        Ok(end)
    }

    fn try_process_field_list(&mut self, lines: &mut [Reader], start: usize) -> Result<usize, ()> {
        // :Flag: Line.
        //        Line continuation.

        let line = &mut lines[start];

        if line.current_char() != ':' || line.next_char() != ':' {
            return Err(());
        }

        let bt = BacktrackPoint::new(self, line);
        let scope_start = line.current_range().start_offset;

        line.bump();
        self.emit(line, DescItemKind::Markup);
        eat_rst_flag_body(line);
        if line.current_char() != ':' {
            bt.rollback(self, line);
            return Err(());
        }
        self.emit(line, DescItemKind::Arg);
        line.bump();
        self.emit(line, DescItemKind::Markup);
        line.eat_while(is_ws);
        line.reset_buff();
        bt.commit(self, line);

        let end = self.gather_indented_lines(lines, start + 1, true);
        self.process_block(&mut lines[start..end]);

        let scope_end = lines[end - 1].current_range().end_offset();
        self.emit_range(
            SourceRange::from_start_end(scope_start, scope_end),
            DescItemKind::Scope,
        );

        Ok(end)
    }

    fn process_doctest_block(&mut self, lines: &mut [Reader], start: usize) -> Result<usize, ()> {
        // >>> Code.
        // ... Continuation
        // Result.

        let line = &mut lines[start];

        if !line.tail_text().starts_with(">>>") {
            return Err(());
        }

        let scope_start = line.current_range().start_offset;

        line.bump();
        line.bump();
        line.bump();
        self.emit(line, DescItemKind::Markup);
        line.eat_while(is_ws);
        line.reset_buff();
        line.eat_till_end();
        self.emit(line, DescItemKind::CodeBlock);

        for i in start + 1..lines.len() {
            let line = &mut lines[i];

            if is_blank(line.tail_text()) {
                line.eat_till_end();
                line.reset_buff();
                let scope_end = line.current_range().end_offset();
                self.emit_range(
                    SourceRange::from_start_end(scope_start, scope_end),
                    DescItemKind::Scope,
                );
                return Ok(i + 1);
            }

            if line.tail_text().starts_with("...") || line.tail_text().starts_with(">>>") {
                line.bump();
                line.bump();
                line.bump();
                self.emit(line, DescItemKind::Markup);
                line.eat_while(is_ws);
                line.reset_buff();
                line.eat_till_end();
                self.emit(line, DescItemKind::CodeBlock);
            } else {
                line.eat_till_end();
                self.emit(line, DescItemKind::CodeBlock);
            }
        }

        let scope_end = lines.last_mut().unwrap().current_range().end_offset();
        self.emit_range(
            SourceRange::from_start_end(scope_start, scope_end),
            DescItemKind::Scope,
        );
        Ok(lines.len())
    }

    fn try_process_explicit_markup(
        &mut self,
        lines: &mut [Reader],
        mut start: usize,
    ) -> Result<usize, ()> {
        // .. Line.
        //    Line continuation.

        let line = &mut lines[start];

        if line.current_char() != '.' || line.next_char() != '.' {
            return Err(());
        }

        let bt = BacktrackPoint::new(self, line);
        let scope_start = line.current_range().start_offset;

        line.bump();
        line.bump();
        if !is_ws(line.current_char()) {
            bt.rollback(self, line);
            return Err(());
        }
        self.emit(line, DescItemKind::Markup);
        line.eat_while(is_ws);
        line.reset_buff();

        let is_code;
        let lang;
        match line.current_char() {
            // Footnote/citation
            '[' => {
                line.bump();
                self.emit(line, DescItemKind::Markup);
                line.eat_while(|c| c != ']');
                self.emit(line, DescItemKind::Arg);
                if line.current_char() != ']' {
                    bt.rollback(self, line);
                    return Err(());
                }
                line.bump();
                self.emit(line, DescItemKind::Markup);
                line.eat_while(is_ws);
                line.reset_buff();
                self.process_inline_content(line);
                bt.commit(self, line);

                is_code = false;
                lang = None;
            }

            // Hyperlink target
            '_' => {
                line.eat_when('_');
                self.emit(line, DescItemKind::Markup);
                if !Self::eat_target_name(line) || line.current_char() != ':' {
                    bt.rollback(self, line);
                    return Err(());
                }
                self.emit(line, DescItemKind::Arg);
                line.bump();
                self.emit(line, DescItemKind::Markup);
                line.eat_while(is_ws);
                line.reset_buff();
                line.eat_till_end();
                self.emit(line, DescItemKind::Link);
                bt.commit(self, line);

                let end = self.gather_indented_lines(lines, start + 1, true);
                for line in lines[start + 1..end].iter_mut() {
                    line.eat_till_end();
                    self.emit(line, DescItemKind::Link);
                }

                let scope_end = lines[end - 1].current_range().end_offset();
                self.emit_range(
                    SourceRange::from_start_end(scope_start, scope_end),
                    DescItemKind::Scope,
                );

                return Ok(end);
            }

            // Directive or comment
            _ => {
                if !Self::eat_directive_name(line) {
                    // Comment.
                    line.eat_till_end();
                    line.reset_buff();
                    bt.commit(self, line);

                    let end = self.gather_indented_lines(lines, start + 1, true);
                    for line in lines[start + 1..end].iter_mut() {
                        line.eat_till_end();
                        line.reset_buff();
                    }

                    let scope_end = lines[end - 1].current_range().end_offset();
                    self.emit_range(
                        SourceRange::from_start_end(scope_start, scope_end),
                        DescItemKind::Scope,
                    );

                    return Ok(end);
                }

                is_code = is_code_directive(line.current_text());
                self.emit(line, DescItemKind::Arg);
                line.bump();
                line.bump();
                self.emit(line, DescItemKind::Markup);
                line.eat_while(is_ws);
                line.reset_buff();
                line.eat_till_end();
                lang = if is_code {
                    CodeBlockLang::try_parse(line.current_text().trim())
                } else {
                    None
                };
                self.emit(line, DescItemKind::CodeBlock);
                bt.commit(self, line);
            }
        }

        start += 1;
        let end = self.gather_indented_lines(lines, start, true);
        while start < end {
            let line = &mut lines[start];
            if is_blank(line.tail_text()) {
                line.eat_till_end();
                line.reset_buff();
                start += 1;
                break;
            }

            line.eat_while(is_ws);
            line.reset_buff();
            if line.current_char() == ':' {
                let bt = BacktrackPoint::new(self, line);

                line.bump();
                self.emit(line, DescItemKind::Markup);
                eat_rst_flag_body(line);
                if line.current_char() == ':' {
                    self.emit(line, DescItemKind::Arg);
                    line.bump();
                    self.emit(line, DescItemKind::Markup);
                    line.eat_while(is_ws);
                    line.reset_buff();
                    bt.commit(self, line);
                } else {
                    bt.rollback(self, line);
                }
            }
            line.eat_till_end();
            self.emit(line, DescItemKind::CodeBlock);

            start += 1;
        }

        if lang.is_some() && self.cursor_position.is_none() {
            let mut state = LexerState::Normal;
            for line in lines[start..end].iter_mut() {
                line.eat_till_end();
                let line_range = line.current_range();
                let prev_reader = line.reset_buff_into_sub_reader();
                state = process_code(
                    self,
                    line_range,
                    prev_reader,
                    state,
                    lang.unwrap_or(CodeBlockLang::None),
                );
            }
        } else if is_code {
            for line in lines[start..end].iter_mut() {
                line.eat_till_end();
                self.emit(line, DescItemKind::CodeBlock);
            }
        } else {
            self.process_block(&mut lines[start..end]);
        }

        let scope_end = lines[end - 1].current_range().end_offset();
        self.emit_range(
            SourceRange::from_start_end(scope_start, scope_end),
            DescItemKind::Scope,
        );

        Ok(end)
    }

    fn process_block_quote(&mut self, lines: &mut [Reader], start: usize) -> Result<usize, ()> {
        // Indented lines.

        let scope_start = lines[start].current_range().start_offset;

        let end = self.gather_indented_lines(lines, start, true);
        self.process_block(&mut lines[start..end]);

        let scope_end = lines[end - 1].current_range().end_offset();
        self.emit_range(
            SourceRange::from_start_end(scope_start, scope_end),
            DescItemKind::Scope,
        );

        Ok(end)
    }

    fn try_process_implicit_hyperlink_target(
        &mut self,
        lines: &mut [Reader],
        start: usize,
    ) -> Result<usize, ()> {
        // __ Hyperlink.

        let line = &mut lines[start];

        if line.current_char() != '_' || line.next_char() != '_' {
            return Err(());
        }

        let bt = BacktrackPoint::new(self, line);

        let scope_start = line.current_range().start_offset;

        line.bump();
        line.bump();
        if !is_ws(line.current_char()) {
            bt.rollback(self, line);
            return Err(());
        }
        self.emit(line, DescItemKind::Link);
        line.eat_while(is_ws);
        line.reset_buff();
        line.eat_till_end();
        self.emit(line, DescItemKind::Link);

        bt.commit(self, line);

        let scope_end = line.current_range().end_offset();
        self.emit_range(
            SourceRange::from_start_end(scope_start, scope_end),
            DescItemKind::Scope,
        );

        Ok(start + 1)
    }

    fn process_paragraph(&mut self, lines: &mut [Reader], start: usize) -> (usize, LineEnding) {
        // Non-indented lines.

        let mut end = start + 1;
        while end < lines.len() && !is_blank(lines[end].tail_text()) {
            end += 1;

            // Detect titles.
            let len = end - start;
            if len >= 3
                && Self::is_title_mark(lines[start].tail_text())
                && !Self::is_title_mark(lines[start + 1].tail_text())
                && Self::is_title_mark(lines[start + 2].tail_text())
            {
                let scope_start = lines[start].current_range().start_offset;
                self.process_title_mark(&mut lines[start]);
                self.process_inline_content(&mut lines[start + 1]);
                self.process_title_mark(&mut lines[start + 2]);

                let scope_end = lines[start + 2].current_range().end_offset();
                self.emit_range(
                    SourceRange::from_start_end(scope_start, scope_end),
                    DescItemKind::Scope,
                );

                return (start + 3, LineEnding::Normal);
            } else if len >= 2
                && !Self::is_title_mark(lines[start].tail_text())
                && Self::is_title_mark(lines[start + 1].tail_text())
            {
                let scope_start = lines[start].current_range().start_offset;
                self.process_inline_content(&mut lines[start]);
                self.process_title_mark(&mut lines[start + 1]);

                let scope_end = lines[start + 1].current_range().end_offset();
                self.emit_range(
                    SourceRange::from_start_end(scope_start, scope_end),
                    DescItemKind::Scope,
                );

                return (start + 3, LineEnding::Normal);
            }
        }

        let mut line_ending = LineEnding::Normal;
        for i in start..end {
            line_ending = self.process_inline_content(&mut lines[i]);
        }

        (end, line_ending)
    }

    fn gather_indented_lines(
        &mut self,
        lines: &mut [Reader],
        start: usize,
        allow_blank_lines: bool,
    ) -> usize {
        let mut end = start;
        let mut common_indent = None;
        for i in start..lines.len() {
            let line = &lines[i];
            let indent = line.tail_text().chars().take_while(|c| is_ws(*c)).count();
            if indent >= 1 {
                end = i + 1;
                common_indent = match common_indent {
                    None => Some(indent),
                    Some(common_indent) => Some(min(common_indent, indent)),
                };
            } else if !allow_blank_lines || !is_blank(line.tail_text()) {
                break;
            }
        }
        if common_indent.is_some_and(|c| c > 0) {
            let common_indent = common_indent.unwrap();
            for line in lines[start..end].iter_mut() {
                line.consume_n_times(is_ws, common_indent);
                line.reset_buff();
            }
        }
        end
    }

    fn gather_exactly_indented_lines(
        &mut self,
        lines: &mut [Reader],
        start: usize,
        min_indent: usize,
        allow_blank_lines: bool,
    ) -> usize {
        let mut end = start;
        for i in start..lines.len() {
            let line = &mut lines[i];
            let indent = line.tail_text().chars().take_while(|c| is_ws(*c)).count();
            if indent >= min_indent {
                end = i + 1;
                line.consume_n_times(is_ws, min_indent);
                line.reset_buff();
            } else if !allow_blank_lines || !is_blank(line.tail_text()) {
                break;
            }
        }
        end
    }

    fn gather_prefixed_lines(&mut self, lines: &mut [Reader], start: usize, prefix: char) -> usize {
        let mut end = start;
        for i in start..lines.len() {
            let line = &mut lines[i];
            if line.current_char() == prefix {
                end = i + 1;
                line.bump();
                self.emit(line, DescItemKind::Markup);
            } else {
                break;
            }
        }
        end
    }

    #[must_use]
    fn eat_target_name(line: &mut Reader) -> bool {
        // .. _Target name: Hyperlink.
        //     ^^^^^^^^^^^

        if line.current_char() == '`' {
            line.bump();
            while !line.is_eof() {
                match line.current_char() {
                    '\\' => {
                        line.bump();
                        line.bump();
                    }
                    '`' => {
                        line.bump();
                        return true;
                    }
                    _ => {
                        line.bump();
                    }
                }
            }
        } else {
            while !line.is_eof() {
                match line.current_char() {
                    ':' if matches!(line.next_char(), ' ' | '\t' | '\0') => {
                        return true;
                    }
                    '\\' => {
                        line.bump();
                        line.bump();
                    }
                    _ => {
                        line.bump();
                    }
                }
            }
        }

        false
    }

    #[must_use]
    fn eat_directive_name(line: &mut Reader) -> bool {
        // .. Directive name:: Arguments.
        //    ^^^^^^^^^^^^^^

        while !line.is_eof() {
            match line.current_char() {
                ':' if line.next_char() == ':' => {
                    return true;
                }
                '.' | ':' | '+' | '_' | '-' | 'a'..='z' | 'A'..='Z' | '0'..='9' => {
                    line.bump();
                }
                _ => {
                    return false;
                }
            }
        }

        false
    }

    fn process_title_mark(&mut self, line: &mut Reader) {
        line.eat_while(is_ws);
        line.reset_buff();
        line.eat_while(|c| !is_ws(c));
        self.emit(line, DescItemKind::Markup);
        line.eat_till_end();
        line.reset_buff();
    }

    fn process_inline_content(&mut self, reader: &mut Reader) -> LineEnding {
        let line_ending = {
            let line = reader.tail_text().trim_end();
            if line.ends_with("::") {
                LineEnding::LiteralMark
            } else {
                LineEnding::Normal
            }
        };

        if self
            .cursor_position
            .is_some_and(|offset| !reader.tail_range().contains_inclusive(offset))
        {
            // No point in calculating this when all we care
            // is what's under the user's cursor.
            return line_ending;
        }

        while !reader.is_eof() {
            match reader.current_char() {
                '\\' => {
                    reader.reset_buff();
                    reader.bump();
                    reader.bump();
                    self.emit(reader, DescItemKind::Markup);
                }

                // Explicit role.
                ':' if reader.next_char() != ':' => {
                    if !Self::is_start_string(reader.prev_char(), reader.next_char()) {
                        reader.bump();
                        continue;
                    }

                    reader.reset_buff();
                    let bt = BacktrackPoint::new(self, reader);

                    reader.bump();
                    self.emit(reader, DescItemKind::Markup);
                    if !Self::eat_role_name(reader)
                        || reader.current_char() != ':'
                        || reader.next_char() != '`'
                    {
                        bt.rollback(self, reader);
                        reader.bump();
                        // guard.backtrack(reader);
                        continue;
                    }

                    let role_text = reader.current_text();
                    let is_lua_ref = role_text.starts_with("lua:")
                        || (self.primary_domain.as_deref() == Some("lua")
                            && !role_text.contains(":")
                            && is_lua_role(role_text));

                    self.emit(reader, DescItemKind::Arg);
                    reader.bump();
                    self.emit(reader, DescItemKind::Markup);

                    if !self.try_handle_role_body(reader, true, is_lua_ref, self.cursor_position) {
                        bt.rollback(self, reader);
                        reader.bump();
                        // guard.backtrack(reader);
                        continue;
                    };

                    bt.commit(self, reader);
                }

                // Inline code
                '`' if reader.next_char() == '`'
                    && !self.cursor_position.is_some_and(|offset| {
                        reader.current_range().end_offset() + 1 == offset
                    }) =>
                {
                    let bt = BacktrackPoint::new(self, reader);
                    reader.reset_buff();

                    let prev = reader.prev_char();
                    reader.bump();
                    reader.bump();
                    let next = reader.current_char();

                    if !Self::is_start_string(prev, next) {
                        bt.rollback(self, reader);
                        reader.bump();
                        // guard.backtrack(reader);
                        continue;
                    }

                    self.emit(reader, DescItemKind::Markup);

                    if !self.try_handle_inline_code(reader) {
                        bt.rollback(self, reader);
                        reader.eat_when('`');
                        // guard.backtrack(reader);
                        continue;
                    }

                    bt.commit(self, reader);
                }

                // Role or hyperlink.
                '`' => {
                    let allow_broken_start_sequence = self
                        .cursor_position
                        .is_some_and(|offset| reader.current_range().end_offset() + 1 == offset);
                    let next_char = if allow_broken_start_sequence {
                        'x' // Any non-whitespace will do.
                    } else {
                        reader.next_char()
                    };

                    if !Self::is_start_string(reader.prev_char(), next_char) {
                        reader.bump();
                        continue;
                    }

                    let bt = BacktrackPoint::new(self, reader);
                    reader.reset_buff();

                    if !self.try_handle_role_body(
                        reader,
                        false,
                        self.default_role
                            .as_deref()
                            .is_some_and(|r| r.starts_with("lua:")),
                        self.cursor_position,
                    ) {
                        bt.rollback(self, reader);
                        reader.bump();
                        // guard.backtrack(reader);
                        continue;
                    };

                    bt.commit(self, reader);
                }

                // Hyperlink reference.
                '_' => {
                    if reader.next_char() != '`' {
                        let bt = BacktrackPoint::new(self, reader);
                        let prev = reader.prev_char();
                        let n_chars = reader.consume_char_n_times('_', 2);
                        if Self::is_end_string(prev, reader.current_char()) {
                            self.handle_simple_ref(reader, n_chars);
                            bt.commit(self, reader);
                            continue;
                        } else {
                            bt.rollback(self, reader);
                            reader.eat_when('_');
                            // guard.backtrack(reader);
                            continue;
                        }
                    }

                    let bt = BacktrackPoint::new(self, reader);
                    reader.reset_buff();

                    let prev = reader.prev_char();
                    reader.bump();
                    reader.bump();
                    let next = reader.current_char();

                    if !Self::is_start_string(prev, next) {
                        bt.rollback(self, reader);
                        reader.bump();
                        reader.bump();
                        // guard.backtrack(reader);
                        continue;
                    }

                    self.emit(reader, DescItemKind::Markup);

                    if !self.try_handle_hyperlink_ref(reader) {
                        bt.rollback(self, reader);
                        reader.bump();
                        reader.bump();
                        // guard.backtrack(reader);
                        continue;
                    }

                    bt.commit(self, reader);
                }

                // Substitution.
                '|' => {
                    if !Self::is_start_string(reader.prev_char(), reader.next_char()) {
                        reader.bump();
                        continue;
                    }

                    let bt = BacktrackPoint::new(self, reader);
                    reader.reset_buff();
                    reader.bump();
                    self.emit(reader, DescItemKind::Markup);

                    if !self.try_handle_subst(reader) {
                        bt.rollback(self, reader);
                        reader.bump();
                        // guard.backtrack(reader);
                        continue;
                    }

                    bt.commit(self, reader);
                }

                // Footnote.
                '[' => {
                    if !Self::is_start_string(reader.prev_char(), reader.next_char()) {
                        reader.bump();
                        continue;
                    }

                    let bt = BacktrackPoint::new(self, reader);
                    reader.reset_buff();
                    reader.bump();
                    self.emit(reader, DescItemKind::Markup);

                    if !self.try_handle_footnote(reader) {
                        bt.rollback(self, reader);
                        reader.bump();
                        // guard.backtrack(reader);
                        continue;
                    }

                    bt.commit(self, reader);
                }

                // Emphasis.
                '*' => {
                    let bt = BacktrackPoint::new(self, reader);
                    reader.reset_buff();

                    let is_strong = reader.next_char() == '*';

                    let prev = reader.prev_char();
                    let start_range = reader.current_range().start_offset;
                    if is_strong {
                        reader.bump();
                        reader.bump();
                    } else {
                        reader.bump();
                    }
                    let next = reader.current_char();

                    if !Self::is_start_string(prev, next) {
                        bt.rollback(self, reader);
                        reader.eat_when('*');
                        // guard.backtrack(reader);
                        continue;
                    }

                    self.emit(reader, DescItemKind::Markup);

                    if !self.try_handle_em(reader, is_strong) {
                        bt.rollback(self, reader);
                        reader.eat_when('*');
                        // guard.backtrack(reader);
                        continue;
                    }

                    let end_range = reader.current_range().end_offset();
                    self.emit_range(
                        SourceRange::from_start_end(start_range, end_range),
                        if is_strong {
                            DescItemKind::Strong
                        } else {
                            DescItemKind::Em
                        },
                    );

                    bt.commit(self, reader);
                }
                _ => {
                    reader.bump();
                }
            }
        }

        reader.reset_buff();

        line_ending
    }

    #[must_use]
    fn eat_role_name(reader: &mut Reader) -> bool {
        // :RoleName:`Role content`
        //  ^^^^^^^^

        while !reader.is_eof() {
            match reader.current_char() {
                ':' if !reader.next_char().is_ascii_alphanumeric() => {
                    return true;
                }
                '.' | ':' | '+' | '_' | '-' | 'a'..='z' | 'A'..='Z' | '0'..='9' => {
                    reader.bump();
                }
                _ => {
                    return false;
                }
            }
        }

        false
    }

    #[must_use]
    fn try_handle_inline_code(&mut self, reader: &mut Reader) -> bool {
        reader.bump(); // Should be at least 1 char long.
        while !reader.is_eof() {
            match reader.current_char() {
                '`' if reader.next_char() == '`' => {
                    let mut prev = reader.prev_char();
                    let n_stars = reader.eat_when('`');
                    if n_stars < 2 {
                        continue;
                    } else if n_stars > 2 {
                        prev = '`';
                    }
                    if !Self::is_end_string(prev, reader.current_char()) {
                        continue;
                    }
                    self.emit_mark_end(reader, Some(DescItemKind::Code), 2);
                    return true;
                }
                _ => {
                    reader.bump();
                }
            }
        }

        false
    }

    #[must_use]
    fn try_handle_role_body(
        &mut self,
        reader: &mut Reader,
        has_explicit_role: bool,
        is_lua_role: bool,
        cursor_position: Option<usize>,
    ) -> bool {
        if reader.current_char() != '`' {
            return false;
        }
        reader.bump();

        while !reader.is_eof() {
            match reader.current_char() {
                '`' => {
                    let bt = BacktrackPoint::new(self, reader);

                    let prev = reader.prev_char();
                    reader.bump();

                    let code = reader.reset_buff_into_sub_reader();

                    let mark_len = reader.consume_n_times(|c| c == '_', 2) + 1;
                    if !Self::is_end_string(prev, reader.current_char()) {
                        bt.rollback(self, reader);
                        reader.bump();
                        continue;
                    }

                    if mark_len > 1 && !has_explicit_role {
                        process_inline_code(self, code, DescItemKind::Link);
                    } else if is_lua_role {
                        process_lua_ref(self, code);
                    } else {
                        process_inline_code(self, code, DescItemKind::Code);
                    }

                    self.emit(reader, DescItemKind::Markup);
                    bt.commit(self, reader);

                    return true;
                }
                '\\' => {
                    reader.bump();
                    reader.bump();
                }
                _ => {
                    reader.bump();
                }
            }
        }

        if let Some(cursor_position) = cursor_position {
            if reader.current_range().contains_inclusive(cursor_position) {
                process_lua_ref(self, reader.reset_buff_into_sub_reader());
                return true;
            }
        }

        false
    }

    fn handle_simple_ref(&mut self, reader: &mut Reader, n_chars: usize) {
        let range = reader.current_range();

        let mut content_range = SourceRange::new(range.start_offset, range.length - n_chars);

        {
            let mut next = '\0';
            for ch in reader.current_text().chars().rev().skip(n_chars) {
                if ch.is_ascii_alphanumeric()
                    || (matches!(ch, '.' | ':' | '+' | '_' | '-') && !next.is_ascii_alphanumeric())
                {
                    content_range.length -= ch.len_utf8();
                } else {
                    if !Self::is_start_string(ch, next) {
                        reader.reset_buff();
                        return;
                    }
                    break;
                }
                next = ch;
            }
        }

        reader.reset_buff();

        let href_range = SourceRange::new(
            content_range.start_offset + content_range.length,
            range.length - n_chars - content_range.length,
        );

        let markup_range =
            SourceRange::new(content_range.start_offset + range.length - n_chars, n_chars);

        self.emit_range(href_range, DescItemKind::Link);
        self.emit_range(markup_range, DescItemKind::Markup);
    }

    #[must_use]
    fn try_handle_hyperlink_ref(&mut self, reader: &mut Reader) -> bool {
        reader.bump(); // Should be at least 1 char long.

        while !reader.is_eof() {
            match reader.current_char() {
                '`' => {
                    if !Self::is_end_string(reader.prev_char(), reader.next_char()) {
                        reader.bump();
                        continue;
                    }
                    self.emit(reader, DescItemKind::Link);
                    reader.bump();
                    self.emit(reader, DescItemKind::Markup);
                    return true;
                }
                '\\' => {
                    reader.bump();
                    reader.bump();
                }
                _ => {
                    reader.bump();
                }
            }
        }

        false
    }

    #[must_use]
    fn try_handle_subst(&mut self, reader: &mut Reader) -> bool {
        reader.bump(); // Should be at least 1 char long.
        while !reader.is_eof() {
            match reader.current_char() {
                '|' => {
                    let prev = reader.prev_char();
                    reader.bump();
                    let mark_len = reader.consume_n_times(|c| c == '_', 2) + 1;
                    if !Self::is_end_string(prev, reader.current_char()) {
                        continue;
                    }
                    let kind = if mark_len == 1 {
                        DescItemKind::Code
                    } else {
                        DescItemKind::Link
                    };
                    self.emit_mark_end(reader, Some(kind), mark_len);
                    return true;
                }
                '\\' => {
                    reader.bump();
                    reader.bump();
                }
                _ => {
                    reader.bump();
                }
            }
        }

        false
    }

    #[must_use]
    fn try_handle_footnote(&mut self, reader: &mut Reader) -> bool {
        reader.bump(); // Should be at least 1 char long.
        while !reader.is_eof() {
            match reader.current_char() {
                ']' if reader.next_char() == '_' => {
                    let prev = reader.prev_char();
                    reader.bump();
                    reader.bump();
                    if !Self::is_end_string(prev, reader.current_char()) {
                        continue;
                    }
                    self.emit_mark_end(reader, Some(DescItemKind::Link), 2);
                    return true;
                }
                '\\' => {
                    reader.bump();
                    reader.bump();
                }
                _ => {
                    reader.bump();
                }
            }
        }

        false
    }

    #[must_use]
    fn try_handle_em(&mut self, reader: &mut Reader, is_strong: bool) -> bool {
        let mark_len = 1 + is_strong as usize;
        reader.bump(); // Should be at least 1 char long.
        while !reader.is_eof() {
            match reader.current_char() {
                '*' => {
                    let mut prev = reader.prev_char();
                    let n_stars = reader.eat_when('*');
                    if n_stars < mark_len {
                        continue;
                    } else if n_stars > mark_len {
                        prev = '*';
                    }
                    if !Self::is_end_string(prev, reader.current_char()) {
                        continue;
                    }
                    self.emit_mark_end(reader, None, mark_len);
                    return true;
                }
                '\\' => {
                    reader.bump();
                    reader.bump();
                }
                _ => {
                    reader.bump();
                }
            }
        }

        false
    }

    fn emit_mark_end(
        &mut self,
        line: &mut Reader,
        content_kind: Option<DescItemKind>,
        mark_len: usize,
    ) {
        let range = line.current_range();

        assert!(range.length > mark_len);

        let content_range = SourceRange::new(range.start_offset, range.length - mark_len);
        if let Some(content_kind) = content_kind {
            self.emit_range(content_range, content_kind);
        }

        let mark_range = SourceRange::new(range.start_offset + range.length - mark_len, mark_len);
        self.emit_range(mark_range, DescItemKind::Markup);
        line.reset_buff();
    }

    fn is_indent_c(c: char) -> bool {
        // Any punctuation character can start character-indented block.
        c.is_ascii_punctuation()
    }

    fn is_list_start(
        line: &str,
        list_enumerator_kind: ListEnumeratorKind,
        list_marker_kind: ListMarkerKind,
    ) -> bool {
        let mut chars = line.chars();

        if list_marker_kind == ListMarkerKind::Enclosed && chars.next() != Some('(') {
            return false;
        }

        let ch = match list_enumerator_kind {
            ListEnumeratorKind::Auto => {
                if chars.next() != Some('#') {
                    return false;
                }
                chars.next()
            }
            ListEnumeratorKind::Number => {
                if !matches!(chars.next(), Some('0'..='9')) {
                    return false;
                }
                loop {
                    let ch = chars.next();
                    if !matches!(ch, Some('0'..='9')) {
                        break ch;
                    }
                }
            }
            ListEnumeratorKind::SmallLetter => {
                if !matches!(chars.next(), Some('a'..='z')) {
                    return false;
                }
                chars.next()
            }
            ListEnumeratorKind::CapitalLetter => {
                if !matches!(chars.next(), Some('A'..='Z')) {
                    return false;
                }
                chars.next()
            }
        };

        let expected_ch = match list_marker_kind {
            ListMarkerKind::Dot => '.',
            ListMarkerKind::Paren | ListMarkerKind::Enclosed => ')',
        };

        ch == Some(expected_ch) && matches!(chars.next(), None | Some(' ' | '\t'))
    }

    fn is_title_mark(s: &str) -> bool {
        // This is a heuristic to avoid calculating width of title text.
        let s = s.trim_end();
        s.len() >= 3 && s.chars().all(|c| c.is_ascii_punctuation())
    }

    fn is_start_string(prev: char, next: char) -> bool {
        // 1
        if next.is_whitespace() {
            return false;
        }

        // 5
        if is_opening_quote(prev) && is_closing_quote(next) && is_quote_match(prev, next) {
            return false;
        }

        // 6
        if prev.is_whitespace() {
            return true;
        }
        if prev.is_ascii() {
            matches!(
                prev,
                '-' | ':' | '/' | '\'' | '"' | '<' | '(' | '[' | '{' | '\0'
            )
        } else {
            matches!(
                get_general_category(prev),
                GeneralCategory::OpenPunctuation
                    | GeneralCategory::InitialPunctuation
                    | GeneralCategory::FinalPunctuation
                    | GeneralCategory::DashPunctuation
                    | GeneralCategory::OtherPunctuation
            )
        }
    }

    fn is_end_string(prev: char, next: char) -> bool {
        // 2
        if prev.is_whitespace() {
            return false;
        }

        // 7
        if next.is_whitespace() {
            return true;
        }
        if next.is_ascii() {
            matches!(
                next,
                '-' | '.'
                    | ','
                    | ':'
                    | ';'
                    | '!'
                    | '?'
                    | '\\'
                    | '/'
                    | '\''
                    | '"'
                    | ')'
                    | ']'
                    | '}'
                    | '>'
                    | '\0'
            )
        } else {
            matches!(
                get_general_category(prev),
                GeneralCategory::ClosePunctuation
                    | GeneralCategory::InitialPunctuation
                    | GeneralCategory::FinalPunctuation
                    | GeneralCategory::DashPunctuation
                    | GeneralCategory::OtherPunctuation
            )
        }
    }
}

/// Eat contents of RST's flag field (directive parameters
/// are also flag fields). Reader should be set to a range that contains
/// the entire line after the initial colon.
pub fn eat_rst_flag_body(reader: &mut Reader) {
    while !reader.is_eof() {
        match reader.current_char() {
            '\\' => {
                reader.bump();
                reader.bump();
            }
            ':' if is_ws(reader.next_char()) || reader.next_char() == '\0' => {
                break;
            }
            _ => {
                reader.bump();
            }
        }
    }
}

/// Parse contents of backtick-enclosed lua reference,
/// supports both markdown and rst syntax.
///
/// Reader should be set to a range that only includes reference contents
/// and backticks around it.
pub fn process_lua_ref<C: ResultContainer>(container: &mut C, mut reader: Reader) {
    if reader.tail_text().chars().all(|c| c == '`') || !reader.tail_text().ends_with("`") {
        // Happens when auto complete called on an empty/incomplete reference.
        reader.bump();
        container.emit(&mut reader, DescItemKind::Markup);
        reader.eat_till_end();
        container.emit(&mut reader, DescItemKind::Ref);
        return;
    }

    let n_backticks = reader.eat_when('`');
    container.emit(&mut reader, DescItemKind::Markup);

    let text = reader.tail_text().trim_matches('`');
    let has_explicit_title = text.ends_with('>') && (text.starts_with('<') || text.contains(" <"));

    if has_explicit_title {
        while !reader.is_eof() {
            if reader.current_char() == '<' && matches!(reader.prev_char(), ' ' | '`' | '\0') {
                reader.bump();
                break;
            } else {
                reader.bump();
            }
        }
        reader.consume_char_n_times('~', 1);
        container.emit(&mut reader, DescItemKind::Code);
        while reader.tail_range().length > n_backticks + 1 {
            reader.bump();
        }
        container.emit(&mut reader, DescItemKind::Ref);
        reader.bump();
        container.emit(&mut reader, DescItemKind::Code);
        reader.eat_while(|_| true);
        container.emit(&mut reader, DescItemKind::Markup);
    } else {
        reader.consume_char_n_times('~', 1);
        container.emit(&mut reader, DescItemKind::Code);
        while reader.tail_range().length > n_backticks {
            reader.bump();
        }
        container.emit(&mut reader, DescItemKind::Ref);
        reader.eat_while(|_| true);
        container.emit(&mut reader, DescItemKind::Markup);
    }
}

/// Parse contents of backtick-enclosed code block,
/// supports both markdown and rst syntax.
///
/// Reader should be set to a range that only includes code block contents
/// and backticks around it.
pub fn process_inline_code<C: ResultContainer>(
    container: &mut C,
    mut reader: Reader,
    kind: DescItemKind,
) {
    let n_backticks = reader.eat_when('`');
    container.emit(&mut reader, DescItemKind::Markup);
    while reader.tail_range().length > n_backticks {
        reader.bump();
    }
    container.emit(&mut reader, kind);
    reader.eat_while(|_| true);
    container.emit(&mut reader, DescItemKind::Markup);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused)]
    use crate::testlib::{print_result, test};
    use googletest::prelude::*;

    #[gtest]
    fn test_rst() -> Result<()> {
        let code = r#"
--- Inline markup
--- =============
---
--- Not valid markup
--- ----------------
---
--- - 2 * x a ** b (* BOM32_* ` `` _ __ | (breaks rule 1)
--- - || (breaks rule 3)
--- - "*" '|' (*) [*] {*} <*> “*” »*« ›*‹ «*» »*» ›*› (breaks rule 5)
--- - 2*x a**b O(N**2) e**(x*y) f(x)*f(y) a|b file*.* __init__ __init__() (breaks rule 6)
---
---
--- Valid markup
--- ------------
---
--- Style: *em*, **strong**, ***strong, with stars***, *broken `em`.
--- Implicit ref: `something`, `a\` b`. Broken: `something
--- Explicit ref: :role:`ref`. Broken: :role:`ref
--- Lua ref: :lua:obj:`a.b.c`, :lua:obj:`~a.b.c`, :lua:obj:`title <a.b.c>`.
--- Code block: ``code``, ``code `backticks```,
--- ``escapes don't work here\``, ``{ 1, 2, nil, 4 }``.
--- Broken code block: ``foo *bar*
--- Implicit hyperlinks: target_, anonymous__, not a link___.
--- Explicit hyperlinks: `target`_, `anonymous`__.
--- Malformed ref: :lua:obj:`~a.b.c`__ (still parsed as ref).
--- Hyperlink: `foo bar`_, `foo bar`__, `foo <bar>`_.
--- Internal hyperlink: _`foo bar`. Broken _`foo bar
--- Footnote: [1]_, [2], [3
--- Replacement: |foo|, |bar|_, |baz|__.
--- *2 * x  *a **b *.rst*
--- *2*x a**b O(N**2) e**(x*y) f(x)*f(y) a*(1+2)*
---
--- Block markup
--- ============
---
--- Lists
--- -----
---
--- - List 1
---   Continuation
---
---   Continuation 2
---
--- -  List 2
---    Continuation
---   Not a continuation
---
--- - List
---
---   - Nested list
---
--- - List
--- Not list
---
--- - List
--- -
--- - List
---
---
--- Numbered lists
--- --------------
---
--- 1.  List.
---
--- 1)  List.
---
--- (1) List.
---
--- A) This is not
--- a list
---
--- A) This is a list.
---
--- 1) This is not a list...
--- 2. because list style changes without a blank line.
---
--- 1) This is a list...
--- 2) because list style doesn't change.
---
--- \A. Einstein was a really smart dude.
---
--- 1. Item 1 initial text.
---
---    a) Item 1a.
---    b) Item 1b.
---
--- 2. a) Item 2a.
---    b) Item 2b.
---
--- 1. List
--- 2.
--- 3. List
---
---
--- Field list
--- ----------
---
--- :Field: content
--- :Field:2: Content
--- :Field:3: Content
---           Continuation
--- :Field\: 4: Content
---
---
--- Line block
--- ----------
---
--- | Lend us a couple of bob till Thursday.
--- | I'm absolutely skint.
--- | But I'm expecting a postal order and I can pay you back
---   as soon as it comes.
--- | Love, Ewan.
---
---   Not a continuation.
---
---
--- Block quotes
--- ------------
---
--- This is an ordinary paragraph, introducing a block quote.
---
---     "It is my business to know things.  That is my trade."
---
---     -- Sherlock Holmes
---
--- * List item.
---
--- ..
---
---     Block quote 3.
---
---
--- Doctest blocks
--- --------------
---
--- >>> print('this is a Doctest block')
--- this is a Doctest block
--- >>> print('foo bar')
--- ... None
--- foo bar
---
--- Explicit markup
--- ---------------
---
--- .. Comment
---
---    With continuation
---
--- .. [1] Footnote
---
--- .. [#] Long footnote
---    with continuation.
---
---    - And nested content.
---
--- .. _target:
---
--- .. _hyperlink-name: link-block
---
--- .. _`FAQTS: Computers: Programming: Languages: Python`:
---    http://python.faqts.com/
---
--- .. _entirely-below:
---    https://docutils.
---    sourceforge.net/rst.html
---
--- .. _Chapter One\: "Tadpole Days":
---
--- It's not easy being green...
---
--- .. directive::
---
--- .. directive:: args
---    :param: value
---    param 2
---
---    Content.
---
---    - Nested content.
---
--- .. code-block:: python
---
---    def foo():
---        pass
---
--- .. code-block:: lua
---    :linenos:
---
---    function foo(x)
---        print([[
---            long string
---        ]])
---    end
---
---
--- Implicit hyperlink target
--- -------------------------
---
--- __ anonymous-hyperlink-target-link-block
---
---
--- Literal blocks
--- --------------
---
--- ::
---
---   This is code!
---
--- Some code::
---
---   Code...
---
---   ...continues.
---
--- ::
---
--- - This is also code!
--- -
--- - Continues.
---
--- - And this is list.
"#;

        let expected = r#"
--- <Scope>Inline markup
--- <Markup>=============</Markup></Scope>
---
--- <Scope>Not valid markup
--- <Markup>----------------</Markup></Scope>
---
--- <Scope><Markup>-</Markup> 2 * x a ** b (* BOM32_* ` `` _ __ | (breaks rule 1)</Scope>
--- <Scope><Markup>-</Markup> || (breaks rule 3)</Scope>
--- <Scope><Markup>-</Markup> "*" '|' (*) [*] {*} <*> “*” »*« ›*‹ «*» »*» ›*› (breaks rule 5)</Scope>
--- <Scope><Markup>-</Markup> 2*x a**b O(N**2) e**(x*y) f(x)*f(y) a|b file*.* __init__ __init__() (breaks rule 6)</Scope>
---
---
--- <Scope>Valid markup
--- <Markup>------------</Markup></Scope>
---
--- Style: <Em><Markup>*</Markup>em<Markup>*</Markup></Em>, <Strong><Markup>**</Markup>strong<Markup>**</Markup></Strong>, <Strong><Markup>**</Markup>*strong, with stars*<Markup>**</Markup></Strong>, *broken <Markup>`</Markup><Code>em</Code><Markup>`</Markup>.
--- Implicit ref: <Markup>`</Markup><Code>something</Code><Markup>`</Markup>, <Markup>`</Markup><Code>a\` b</Code><Markup>`</Markup>. Broken: `something
--- Explicit ref: <Markup>:</Markup><Arg>role</Arg><Markup>:`</Markup><Code>ref</Code><Markup>`</Markup>. Broken: :role:`ref
--- Lua ref: <Markup>:</Markup><Arg>lua:obj</Arg><Markup>:`</Markup><Ref>a.b.c</Ref><Markup>`</Markup>, <Markup>:</Markup><Arg>lua:obj</Arg><Markup>:`</Markup><Code>~</Code><Ref>a.b.c</Ref><Markup>`</Markup>, <Markup>:</Markup><Arg>lua:obj</Arg><Markup>:`</Markup><Code>title <</Code><Ref>a.b.c</Ref><Code>></Code><Markup>`</Markup>.
--- Code block: <Markup>``</Markup><Code>code</Code><Markup>``</Markup>, <Markup>``</Markup><Code>code `backticks`</Code><Markup>``</Markup>,
--- <Markup>``</Markup><Code>escapes don't work here\</Code><Markup>``</Markup>, <Markup>``</Markup><Code>{ 1, 2, nil, 4 }</Code><Markup>``</Markup>.
--- Broken code block: ``foo <Em><Markup>*</Markup>bar<Markup>*</Markup></Em>
--- Implicit hyperlinks: <Link>target</Link><Markup>_</Markup>, <Link>anonymous</Link><Markup>__</Markup>, not a link___.
--- Explicit hyperlinks: <Markup>`</Markup><Link>target</Link><Markup>`_</Markup>, <Markup>`</Markup><Link>anonymous</Link><Markup>`__</Markup>.
--- Malformed ref: <Markup>:</Markup><Arg>lua:obj</Arg><Markup>:`</Markup><Code>~</Code><Ref>a.b.c</Ref><Markup>`__</Markup> (still parsed as ref).
--- Hyperlink: <Markup>`</Markup><Link>foo bar</Link><Markup>`_</Markup>, <Markup>`</Markup><Link>foo bar</Link><Markup>`__</Markup>, <Markup>`</Markup><Link>foo <bar></Link><Markup>`_</Markup>.
--- Internal hyperlink: <Markup>_`</Markup><Link>foo bar</Link><Markup>`</Markup>. Broken _`foo bar
--- Footnote: <Markup>[</Markup><Link>1</Link><Markup>]_</Markup>, [2], [3
--- Replacement: <Markup>|</Markup><Code>foo</Code><Markup>|</Markup>, <Markup>|</Markup><Link>bar</Link><Markup>|_</Markup>, <Markup>|</Markup><Link>baz</Link><Markup>|__</Markup>.
--- <Em><Markup>*</Markup>2 * x  *a **b *.rst<Markup>*</Markup></Em>
--- <Em><Markup>*</Markup>2*x a**b O(N**2) e**(x*y) f(x)*f(y) a*(1+2)<Markup>*</Markup></Em>
---
--- <Scope>Block markup
--- <Markup>============</Markup></Scope>
---
--- <Scope>Lists
--- <Markup>-----</Markup></Scope>
---
--- <Scope><Markup>-</Markup> List 1
---   Continuation
---
---   Continuation 2</Scope>
---
--- <Scope><Markup>-</Markup>  List 2
---    Continuation</Scope>
--- <Scope>  Not a continuation</Scope>
---
--- <Scope><Markup>-</Markup> List
---
---   <Scope><Markup>-</Markup> Nested list</Scope></Scope>
---
--- <Scope><Markup>-</Markup> List</Scope>
--- Not list
---
--- <Scope><Markup>-</Markup> List</Scope>
--- <Scope><Markup>-</Markup></Scope>
--- <Scope><Markup>-</Markup> List</Scope>
---
---
--- <Scope>Numbered lists
--- <Markup>--------------</Markup></Scope>
---
--- <Scope><Markup>1.</Markup>  List.</Scope>
---
--- <Scope><Markup>1)</Markup>  List.</Scope>
---
--- <Scope><Markup>(1)</Markup> List.</Scope>
---
--- A) This is not
--- a list
---
--- <Scope><Markup>A)</Markup> This is a list.</Scope>
---
--- 1) This is not a list...
--- 2. because list style changes without a blank line.
---
--- <Scope><Markup>1)</Markup> This is a list...</Scope>
--- <Scope><Markup>2)</Markup> because list style doesn't change.</Scope>
---
--- <Markup>\A</Markup>. Einstein was a really smart dude.
---
--- <Scope><Markup>1.</Markup> Item 1 initial text.
---
---    <Scope><Markup>a)</Markup> Item 1a.</Scope>
---    <Scope><Markup>b)</Markup> Item 1b.</Scope></Scope>
---
--- <Scope><Markup>2.</Markup> <Scope><Markup>a)</Markup> Item 2a.</Scope>
---    <Scope><Markup>b)</Markup> Item 2b.</Scope></Scope>
---
--- <Scope><Markup>1.</Markup> List</Scope>
--- <Scope><Markup>2.</Markup></Scope>
--- <Scope><Markup>3.</Markup> List</Scope>
---
---
--- <Scope>Field list
--- <Markup>----------</Markup></Scope>
---
--- :Field: content
--- :Field:2: Content
--- :Field:3: Content
---           Continuation
--- :Field<Markup>\:</Markup> 4: Content
---
---
--- <Scope>Line block
--- <Markup>----------</Markup></Scope>
---
--- <Scope><Markup>|</Markup> Lend us a couple of bob till Thursday.</Scope>
--- <Scope><Markup>|</Markup> I'm absolutely skint.</Scope>
--- <Scope><Markup>|</Markup> But I'm expecting a postal order and I can pay you back
---   as soon as it comes.</Scope>
--- <Scope><Markup>|</Markup> Love, Ewan.</Scope>
---
--- <Scope>  Not a continuation.</Scope>
---
---
--- <Scope>Block quotes
--- <Markup>------------</Markup></Scope>
---
--- This is an ordinary paragraph, introducing a block quote.
---
--- <Scope>    "It is my business to know things.  That is my trade."
---
---     -- Sherlock Holmes</Scope>
---
--- <Scope><Markup>*</Markup> List item.</Scope>
---
--- ..
---
--- <Scope>    Block quote 3.</Scope>
---
---
--- <Scope>Doctest blocks
--- <Markup>--------------</Markup></Scope>
---
--- <Scope><Markup>>>></Markup> <CodeBlock>print('this is a Doctest block')</CodeBlock>
--- <CodeBlock>this is a Doctest block</CodeBlock>
--- <Markup>>>></Markup> <CodeBlock>print('foo bar')</CodeBlock>
--- <Markup>...</Markup> <CodeBlock>None</CodeBlock>
--- <CodeBlock>foo bar</CodeBlock>
---</Scope>
--- <Scope>Explicit markup
--- <Markup>---------------</Markup></Scope>
---
--- <Scope><Markup>..</Markup> Comment
---
---    With continuation</Scope>
---
--- <Scope><Markup>..</Markup> <Markup>[</Markup><Arg>1</Arg><Markup>]</Markup> Footnote</Scope>
---
--- <Scope><Markup>..</Markup> <Markup>[</Markup><Arg>#</Arg><Markup>]</Markup> Long footnote
---    <CodeBlock>with continuation.</CodeBlock>
---
---    <Scope><Markup>-</Markup> And nested content.</Scope></Scope>
---
--- <Scope><Markup>..</Markup> <Markup>_</Markup><Arg>target</Arg><Markup>:</Markup></Scope>
---
--- <Scope><Markup>..</Markup> <Markup>_</Markup><Arg>hyperlink-name</Arg><Markup>:</Markup> <Link>link-block</Link></Scope>
---
--- <Scope><Markup>..</Markup> <Markup>_</Markup><Arg>`FAQTS: Computers: Programming: Languages: Python`</Arg><Markup>:</Markup>
---    <Link>http://python.faqts.com/</Link></Scope>
---
--- <Scope><Markup>..</Markup> <Markup>_</Markup><Arg>entirely-below</Arg><Markup>:</Markup>
---    <Link>https://docutils.</Link>
---    <Link>sourceforge.net/rst.html</Link></Scope>
---
--- <Scope><Markup>..</Markup> <Markup>_</Markup><Arg>Chapter One\: "Tadpole Days"</Arg><Markup>:</Markup></Scope>
---
--- It's not easy being green...
---
--- <Scope><Markup>..</Markup> <Arg>directive</Arg><Markup>::</Markup></Scope>
---
--- <Scope><Markup>..</Markup> <Arg>directive</Arg><Markup>::</Markup> <CodeBlock>args</CodeBlock>
---    <Markup>:</Markup><Arg>param</Arg><Markup>:</Markup> <CodeBlock>value</CodeBlock>
---    <CodeBlock>param 2</CodeBlock>
---
---    Content.
---
---    <Scope><Markup>-</Markup> Nested content.</Scope></Scope>
---
--- <Scope><Markup>..</Markup> <Arg>code-block</Arg><Markup>::</Markup> <CodeBlock>python</CodeBlock>
---
---    <CodeBlock>def foo():</CodeBlock>
---    <CodeBlock>    pass</CodeBlock></Scope>
---
--- <Scope><Markup>..</Markup> <Arg>code-block</Arg><Markup>::</Markup> <CodeBlock>lua</CodeBlock>
---    <Markup>:</Markup><Arg>linenos</Arg><Markup>:</Markup>
---
---    <CodeBlockHl(Keyword)>function</CodeBlockHl(Keyword)> <CodeBlockHl(Function)>foo</CodeBlockHl(Function)><CodeBlockHl(Operators)>(</CodeBlockHl(Operators)><CodeBlockHl(Variable)>x</CodeBlockHl(Variable)><CodeBlockHl(Operators)>)</CodeBlockHl(Operators)>
---        <CodeBlockHl(Function)>print</CodeBlockHl(Function)><CodeBlockHl(Operators)>(</CodeBlockHl(Operators)><CodeBlockHl(String)>[[</CodeBlockHl(String)>
---    <CodeBlockHl(String)>        long string</CodeBlockHl(String)>
---    <CodeBlockHl(String)>    ]]</CodeBlockHl(String)><CodeBlockHl(Operators)>)</CodeBlockHl(Operators)>
---    <CodeBlockHl(Keyword)>end</CodeBlockHl(Keyword)></Scope>
---
---
--- <Scope>Implicit hyperlink target
--- <Markup>-------------------------</Markup></Scope>
---
--- <Scope><Link>__</Link> <Link>anonymous-hyperlink-target-link-block</Link></Scope>
---
---
--- <Scope>Literal blocks
--- <Markup>--------------</Markup></Scope>
---
--- ::
---
---   <Scope><CodeBlock>This is code!</CodeBlock></Scope>
---
--- Some code::
---
---   <Scope><CodeBlock>Code...</CodeBlock>
---
---   <CodeBlock>...continues.</CodeBlock></Scope>
---
--- ::
---
--- <Markup>-</Markup><Scope><CodeBlock> This is also code!</CodeBlock>
--- <Markup>-</Markup>
--- <Markup>-</Markup><CodeBlock> Continues.</CodeBlock></Scope>
---
--- <Scope><Markup>-</Markup> And this is list.</Scope>
"#;

        test(&code, Box::new(RstParser::new(None, None, None)), &expected).or_fail()?;
        Ok(())
    }

    #[gtest]
    fn test_rst_no_indent() -> Result<()> {
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
---```lua
---
---<Scope> local t = 213</Scope>
---```
---
---<Scope> <Scope><Markup>..</Markup> <Arg>code-block</Arg><Markup>::</Markup> <CodeBlock>lua</CodeBlock>
---
---    <CodeBlockHl(Keyword)>local</CodeBlockHl(Keyword)> <CodeBlockHl(Variable)>t</CodeBlockHl(Variable)> <CodeBlockHl(Operators)>=</CodeBlockHl(Operators)> <CodeBlockHl(Number)>123</CodeBlockHl(Number)>
---    <CodeBlockHl(Variable)>yes</CodeBlockHl(Variable)> <CodeBlockHl(Operators)>=</CodeBlockHl(Operators)> <CodeBlockHl(Number)>1123</CodeBlockHl(Number)></Scope></Scope>
local t = 123
"#;

        test(
            &code,
            Box::new(RstParser::new(None, Some("lua:obj".to_string()), None)),
            &expected,
        )
        .or_fail()?;
        Ok(())
    }

    #[gtest]
    fn test_rst_default_role() -> Result<()> {
        let code = r#"--- See `ref`"#;

        let expected = r#"--- See <Markup>`</Markup><Ref>ref</Ref><Markup>`</Markup>"#;

        test(
            &code,
            Box::new(RstParser::new(None, Some("lua:obj".to_string()), None)),
            &expected,
        )
        .or_fail()?;
        Ok(())
    }

    #[gtest]
    fn test_rst_primary_domain() -> Result<()> {
        let code = r#"--- See :obj:`ref`"#;

        let expected = r#"
            --- See <Markup>:</Markup><Arg>obj</Arg><Markup>:`</Markup><Ref>ref</Ref><Markup>`</Markup>
        "#;

        test(
            &code,
            Box::new(RstParser::new(Some("lua".to_string()), None, None)),
            &expected,
        )
        .or_fail()?;
        Ok(())
    }

    #[gtest]
    fn test_rst_search_at_offset() -> Result<()> {
        let code = r#"--- See :lua:obj:`x` :lua:obj:`ref`"#;
        let expected = r#"--- See :lua:obj:`x` :lua:obj:`<Ref>ref</Ref>`"#;
        test(
            &code,
            Box::new(RstParser::new(None, None, Some(31))),
            &expected,
        )
        .or_fail()?;
        test(
            &code,
            Box::new(RstParser::new(None, None, Some(32))),
            &expected,
        )
        .or_fail()?;
        test(
            &code,
            Box::new(RstParser::new(None, None, Some(34))),
            &expected,
        )
        .or_fail()?;

        let code = r#"--- See :lua:obj:`x` :lua:obj:`"#;
        let expected = r#"--- See :lua:obj:`x` :lua:obj:`<Ref></Ref>"#;
        test(
            &code,
            Box::new(RstParser::new(None, None, Some(31))),
            &expected,
        )
        .or_fail()?;

        let code = r#"--- See :lua:obj:`x` :lua:obj:``..."#;
        let expected = r#"--- See :lua:obj:`x` :lua:obj:`<Ref>`</Ref>..."#;
        test(
            &code,
            Box::new(RstParser::new(None, None, Some(31))),
            &expected,
        )
        .or_fail()?;
        Ok(())
    }

    #[gtest]
    fn test_rst_search_at_offset_default_role() -> Result<()> {
        let code = r#"--- See `ab`"#;
        let expected = r#"--- See `<Ref>ab</Ref>`"#;
        test(
            &code,
            Box::new(RstParser::new(None, Some("lua:obj".to_string()), Some(9))),
            &expected,
        )
        .or_fail()?;
        test(
            &code,
            Box::new(RstParser::new(None, Some("lua:obj".to_string()), Some(10))),
            &expected,
        )
        .or_fail()?;
        test(
            &code,
            Box::new(RstParser::new(None, Some("lua:obj".to_string()), Some(11))),
            &expected,
        )
        .or_fail()?;

        let code = r#"--- See `"#;
        let expected = r#"--- See `<Ref></Ref>"#;
        test(
            &code,
            Box::new(RstParser::new(None, Some("lua:obj".to_string()), Some(9))),
            &expected,
        )
        .or_fail()?;

        let code = r#"--- See `..."#;
        let expected = r#"--- See `<Ref>...</Ref>"#;
        test(
            &code,
            Box::new(RstParser::new(None, Some("lua:obj".to_string()), Some(9))),
            &expected,
        )
        .or_fail()?;

        let code = r#"--- See ``"#;
        let expected = r#"--- See `<Ref>`</Ref>"#;
        test(
            &code,
            Box::new(RstParser::new(None, Some("lua:obj".to_string()), Some(9))),
            &expected,
        )
        .or_fail()?;
        Ok(())
    }
}
