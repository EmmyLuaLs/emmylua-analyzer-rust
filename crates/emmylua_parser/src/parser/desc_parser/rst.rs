use crate::lexer::is_doc_whitespace;
use crate::parser::desc_parser::LuaDescParser;
use crate::parser::desc_parser::util::{
    BacktrackGuard, check_marks_are_consistent, directive_is_code, find_common_indent, is_blank,
    is_closing_quote, is_opening_quote, is_quote_match,
};
use crate::parser::{MarkEvent, MarkerEventContainer};
use crate::text::{Reader, ReaderWithMarks, SourceRange};
use crate::{LuaSyntaxKind, LuaTokenKind};
use std::cmp::min;
use std::ops::DerefMut;
use std::sync::LazyLock;

pub struct RstParser {
    primary_domain: Option<String>,
    default_role: Option<String>,
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

struct Line {
    events: Vec<MarkEvent>,
    range: SourceRange,
}

impl Default for Line {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            range: SourceRange::new(0, 0),
        }
    }
}

impl LuaDescParser for RstParser {
    fn parse(&mut self, text: &str, events: &[MarkEvent]) -> Vec<MarkEvent> {
        let mut lines = Vec::new();
        let mut line = Line::default();
        let mut seen_line = false;
        for event in events {
            match event {
                MarkEvent::EatToken {
                    kind: LuaTokenKind::TkDocDetail,
                    range,
                } => {
                    line.range = *range;
                    lines.push(std::mem::take(&mut line));
                    seen_line = true;
                }
                MarkEvent::EatToken {
                    kind: LuaTokenKind::TkEndOfLine,
                    ..
                } => {
                    line.events.push(*event);
                    if !seen_line {
                        lines.push(std::mem::take(&mut line));
                    }
                    seen_line = false;
                }
                event => {
                    line.events.push(*event);
                }
            }
        }
        if !line.events.is_empty() {
            lines.push(line);
        }

        let mut readers: Vec<_> = lines
            .iter_mut()
            .map(|line| {
                let text = &text[line.range.start_offset..line.range.end_offset()];
                ReaderWithMarks::new(Reader::new_with_range(text, line.range), &mut line.events)
            })
            .collect();

        let common_indent = find_common_indent(readers.iter().map(|reader| reader.tail_text()), 0);
        if common_indent > 0 {
            for reader in readers.iter_mut() {
                reader.consume_n_times(is_doc_whitespace, common_indent);
                reader.emit(LuaTokenKind::TkWhitespace);
            }
        }

        self.process_block(&mut readers);

        let result: Vec<_> = lines
            .into_iter()
            .map(|line| line.events)
            .flatten()
            .collect();

        check_marks_are_consistent(&result);

        result
    }
}

impl RstParser {
    pub fn new(primary_domain: Option<String>, default_role: Option<String>) -> Self {
        Self {
            primary_domain,
            default_role,
        }
    }

    fn process_block(&mut self, lines: &mut [ReaderWithMarks]) {
        let mut i = 0;
        let mut prev_line_ending = LineEnding::Normal;
        while i < lines.len() {
            (i, prev_line_ending) = self.consume_block(lines, i, prev_line_ending)
        }
    }

    fn consume_block(
        &mut self,
        lines: &mut [ReaderWithMarks],
        start: usize,
        prev_line_ending: LineEnding,
    ) -> (usize, LineEnding) {
        let line = &mut lines[start];

        if is_blank(line.tail_text()) {
            line.eat_while(|_| true);
            line.emit(LuaTokenKind::TkDocDetail);
            return (start + 1, prev_line_ending);
        }

        let res = match line.current_char() {
            // Indented literal text.
            ch if prev_line_ending == LineEnding::LiteralMark
                && (is_doc_whitespace(ch) || Self::is_indent_c(ch)) =>
            {
                self.try_process_literal_text(lines, start)
            }

            // Line block.
            '|' if is_doc_whitespace(line.next_char()) || line.next_char() == '\0' => {
                self.process_line_block(lines, start)
            }

            // Bullet list.
            '*' | '+' | '-' | '•' | '‣' | '⁃'
                if is_doc_whitespace(line.next_char()) || line.next_char() == '\0' =>
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
            ' ' => self.process_block_quote(lines, start),

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
        lines: &mut [ReaderWithMarks],
        start: usize,
    ) -> Result<usize, ()> {
        let ch = lines[start].current_char();

        let end = if is_doc_whitespace(ch) {
            Self::eat_indented_lines(lines, start, true)
        } else if Self::is_indent_c(ch) {
            Self::eat_prefixed_lines(lines, start, ch)
        } else {
            return Err(());
        };

        lines[start].mark(LuaSyntaxKind::DocScope);
        for line in &mut lines[start..end] {
            line.eat_while(|_| true);
            line.emit(LuaTokenKind::TkDocDetailCode);
        }
        lines[end - 1].push_node_end();

        Ok(end)
    }

    fn process_line_block(
        &mut self,
        lines: &mut [ReaderWithMarks],
        start: usize,
    ) -> Result<usize, ()> {
        let line = &mut lines[start];

        line.mark(LuaSyntaxKind::DocScope);

        line.bump();
        line.emit(LuaTokenKind::TkDocDetailMarkup);
        self.process_inline_content(line);

        let end = Self::eat_indented_lines(lines, start + 1, false);
        for line in lines[start + 1..end].iter_mut() {
            self.process_inline_content(line);
        }

        lines[end - 1].push_node_end();

        Ok(end)
    }

    fn process_bullet_list(
        &mut self,
        lines: &mut [ReaderWithMarks],
        start: usize,
    ) -> Result<usize, ()> {
        let line = &mut lines[start];

        line.mark(LuaSyntaxKind::DocScope);

        line.bump();
        line.emit(LuaTokenKind::TkDocDetailMarkup);

        let end = {
            if line.is_eof() {
                Self::eat_indented_lines(lines, start + 1, true)
            } else {
                let indent = line.eat_while(is_doc_whitespace) + 1;
                Self::eat_exactly_indented_lines(lines, start + 1, indent, true)
            }
        };
        self.process_block(&mut lines[start..end]);

        lines[end - 1].push_node_end();

        Ok(end)
    }

    fn try_process_numbered_list(
        &mut self,
        lines: &mut [ReaderWithMarks],
        start: usize,
    ) -> Result<usize, ()> {
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

        let mut line = line.backtrack_point();

        line.mark(LuaSyntaxKind::DocScope);

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
            _ => return Err(()),
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
            _ => return Err(()),
        };

        if !(is_doc_whitespace(line.current_char()) || line.is_eof()) {
            return Err(());
        }

        if let Some(next_line) = next_line {
            if !(is_doc_whitespace(next_line.current_char())
                || next_line.is_eof()
                || Self::is_list_start(
                    next_line.tail_text(),
                    list_enumerator_kind,
                    list_marker_kind,
                ))
            {
                return Err(());
            }
        }

        line.emit(LuaTokenKind::TkDocDetailMarkup);
        indent += line.eat_while(is_doc_whitespace);
        line.emit(LuaTokenKind::TkDocDetail);
        line.commit();

        let end = Self::eat_exactly_indented_lines(lines, start + 1, indent, true);
        self.process_block(&mut lines[start..end]);

        lines[end - 1].push_node_end();

        Ok(end)
    }

    fn try_process_field_list(
        &mut self,
        lines: &mut [ReaderWithMarks],
        start: usize,
    ) -> Result<usize, ()> {
        let line = &mut lines[start];
        let mut line = line.backtrack_point();

        line.mark(LuaSyntaxKind::DocScope);

        line.bump();
        line.emit(LuaTokenKind::TkDocDetailArgMarkup);
        eat_rst_flag_body(line.deref_mut());
        if line.current_char() != ':' {
            return Err(());
        }
        line.emit(LuaTokenKind::TkDocDetailArg);
        line.bump();
        line.emit(LuaTokenKind::TkDocDetailArgMarkup);
        line.eat_while(is_doc_whitespace);
        line.emit(LuaTokenKind::TkDocDetail);
        line.commit();

        let end = Self::eat_indented_lines(lines, start + 1, true);
        self.process_block(&mut lines[start..end]);

        lines[end - 1].push_node_end();

        Ok(end)
    }

    fn process_doctest_block(
        &mut self,
        lines: &mut [ReaderWithMarks],
        start: usize,
    ) -> Result<usize, ()> {
        let line = &mut lines[start];

        line.mark(LuaSyntaxKind::DocScope);

        line.bump();
        line.bump();
        line.bump();
        line.emit(LuaTokenKind::TkDocDetailMarkup);
        line.eat_while(is_doc_whitespace);
        line.emit(LuaTokenKind::TkDocDetail);
        line.eat_while(|_| true);
        line.emit(LuaTokenKind::TkDocDetailCode);

        for i in start + 1..lines.len() {
            let line = &mut lines[i];

            if is_blank(line.tail_text()) {
                line.eat_while(|_| true);
                line.emit(LuaTokenKind::TkDocDetail);
                line.push_node_end();
                return Ok(i + 1);
            }

            if line.tail_text().starts_with("...") || line.tail_text().starts_with(">>>") {
                line.bump();
                line.bump();
                line.bump();
                line.emit(LuaTokenKind::TkDocDetailMarkup);
                line.eat_while(is_doc_whitespace);
                line.emit(LuaTokenKind::TkDocDetail);
                line.eat_while(|_| true);
                line.emit(LuaTokenKind::TkDocDetailCode);
            } else {
                line.eat_while(|_| true);
                line.emit(LuaTokenKind::TkDocDetailCode);
            }
        }

        lines.last_mut().unwrap().push_node_end();
        Ok(lines.len())
    }

    fn try_process_explicit_markup(
        &mut self,
        lines: &mut [ReaderWithMarks],
        mut start: usize,
    ) -> Result<usize, ()> {
        let line = &mut lines[start];
        let mut line = line.backtrack_point();

        line.mark(LuaSyntaxKind::DocScope);

        line.bump();
        line.bump();
        if !is_doc_whitespace(line.current_char()) {
            return Err(());
        }
        line.emit(LuaTokenKind::TkDocDetailMarkup);
        line.eat_while(is_doc_whitespace);
        line.emit(LuaTokenKind::TkDocDetail);

        let is_code;
        match line.current_char() {
            // Footnote/citation
            '[' => {
                line.bump();
                line.emit(LuaTokenKind::TkDocDetailArgMarkup);
                line.eat_while(|c| c != ']');
                line.emit(LuaTokenKind::TkDocDetailArg);
                if line.current_char() != ']' {
                    return Err(());
                }
                line.bump();
                line.emit(LuaTokenKind::TkDocDetailArgMarkup);
                line.eat_while(is_doc_whitespace);
                line.emit(LuaTokenKind::TkDocDetail);
                self.process_inline_content(line.deref_mut());
                line.commit();

                is_code = false;
            }

            // Hyperlink target
            '_' => {
                line.eat_when('_');
                line.emit(LuaTokenKind::TkDocDetailArgMarkup);
                if !Self::eat_target_name(line.deref_mut()) || line.current_char() != ':' {
                    return Err(());
                }
                line.emit(LuaTokenKind::TkDocDetailArg);
                line.bump();
                line.emit(LuaTokenKind::TkDocDetailArgMarkup);
                line.eat_while(is_doc_whitespace);
                line.emit(LuaTokenKind::TkDocDetail);
                line.eat_while(|_| true);
                line.emit(LuaTokenKind::TkDocDetailInlineLink);
                line.commit();

                let end = Self::eat_indented_lines(lines, start + 1, true);
                for line in lines[start + 1..end].iter_mut() {
                    line.eat_while(|_| true);
                    line.emit(LuaTokenKind::TkDocDetailInlineLink);
                }

                lines[end - 1].push_node_end();

                return Ok(end);
            }

            // Directive or comment
            _ => {
                if !Self::eat_directive_name(line.deref_mut()) {
                    // Comment.
                    line.eat_while(|_| true);
                    line.emit(LuaTokenKind::TkDocDetail);
                    line.commit();

                    let end = Self::eat_indented_lines(lines, start + 1, true);
                    for line in lines[start + 1..end].iter_mut() {
                        line.eat_while(|_| true);
                        line.emit(LuaTokenKind::TkDocDetail);
                    }

                    lines[end - 1].push_node_end();

                    return Ok(end);
                }

                is_code = directive_is_code(line.current_text());
                line.emit(LuaTokenKind::TkDocDetailArg);
                line.bump();
                line.bump();
                line.emit(LuaTokenKind::TkDocDetailArgMarkup);
                line.eat_while(is_doc_whitespace);
                line.emit(LuaTokenKind::TkDocDetail);
                line.eat_while(|_| true);
                line.emit(LuaTokenKind::TkDocDetailCode);
                line.commit();
            }
        }

        start += 1;
        let end = Self::eat_indented_lines(lines, start, true);
        while start < end {
            let line = &mut lines[start];
            if is_blank(line.tail_text()) {
                line.eat_while(|_| true);
                line.emit(LuaTokenKind::TkDocDetail);
                start += 1;
                break;
            }

            line.eat_while(is_doc_whitespace);
            line.emit(LuaTokenKind::TkDocDetail);
            if line.current_char() == ':' {
                let mut line = line.backtrack_point();

                line.bump();
                line.emit(LuaTokenKind::TkDocDetailArgMarkup);
                eat_rst_flag_body(line.deref_mut());
                if line.current_char() == ':' {
                    line.emit(LuaTokenKind::TkDocDetailArg);
                    line.bump();
                    line.emit(LuaTokenKind::TkDocDetailArgMarkup);
                    line.eat_while(is_doc_whitespace);
                    line.emit(LuaTokenKind::TkDocDetail);
                    line.commit()
                }
            }
            line.eat_while(|_| true);
            line.emit(LuaTokenKind::TkDocDetailCode);

            start += 1;
        }

        if is_code {
            for line in lines[start..end].iter_mut() {
                line.eat_while(|_| true);
                line.emit(LuaTokenKind::TkDocDetailCode);
            }
        } else {
            self.process_block(&mut lines[start..end]);
        }

        lines[end - 1].push_node_end();

        Ok(end)
    }

    fn process_block_quote(
        &mut self,
        lines: &mut [ReaderWithMarks],
        start: usize,
    ) -> Result<usize, ()> {
        lines[start].mark(LuaSyntaxKind::DocScope);

        let end = Self::eat_indented_lines(lines, start, true);
        self.process_block(&mut lines[start..end]);

        lines[end - 1].push_node_end();

        Ok(end)
    }

    fn try_process_implicit_hyperlink_target(
        &mut self,
        lines: &mut [ReaderWithMarks],
        start: usize,
    ) -> Result<usize, ()> {
        let line = &mut lines[start];
        let mut line = line.backtrack_point();

        line.mark(LuaSyntaxKind::DocScope);

        line.bump();
        line.bump();
        if !is_doc_whitespace(line.current_char()) {
            return Err(());
        }
        line.emit(LuaTokenKind::TkDocDetailInlineLink);
        line.eat_while(is_doc_whitespace);
        line.emit(LuaTokenKind::TkDocDetail);
        line.eat_while(|_| true);
        line.emit(LuaTokenKind::TkDocDetailInlineLink);
        line.push_node_end();
        line.commit();

        Ok(start + 1)
    }

    fn process_paragraph(
        &mut self,
        lines: &mut [ReaderWithMarks],
        start: usize,
    ) -> (usize, LineEnding) {
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
                lines[start].mark(LuaSyntaxKind::DocScope);
                Self::eat_title_mark(&mut lines[start]);
                self.process_inline_content(&mut lines[start + 1]);
                Self::eat_title_mark(&mut lines[start + 2]);
                lines[start + 2].push_node_end();
                return (start + 3, LineEnding::Normal);
            } else if len >= 2
                && !Self::is_title_mark(lines[start].tail_text())
                && Self::is_title_mark(lines[start + 1].tail_text())
            {
                lines[start].mark(LuaSyntaxKind::DocScope);
                self.process_inline_content(&mut lines[start]);
                Self::eat_title_mark(&mut lines[start + 1]);
                lines[start + 1].push_node_end();
                return (start + 3, LineEnding::Normal);
            }
        }

        let mut line_ending = LineEnding::Normal;
        for i in start..end {
            line_ending = self.process_inline_content(&mut lines[i]);
        }

        (end, line_ending)
    }

    fn eat_indented_lines(
        lines: &mut [ReaderWithMarks],
        start: usize,
        allow_blank_lines: bool,
    ) -> usize {
        let mut end = start;
        let mut common_indent = None;
        for i in start..lines.len() {
            let line = &lines[i];
            let indent = line
                .tail_text()
                .chars()
                .take_while(|c| is_doc_whitespace(*c))
                .count();
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
                line.consume_n_times(is_doc_whitespace, common_indent);
                line.emit(LuaTokenKind::TkDocDetail);
            }
        }
        end
    }

    fn eat_exactly_indented_lines(
        lines: &mut [ReaderWithMarks],
        start: usize,
        min_indent: usize,
        allow_blank_lines: bool,
    ) -> usize {
        let mut end = start;
        for i in start..lines.len() {
            let line = &mut lines[i];
            let indent = line
                .tail_text()
                .chars()
                .take_while(|c| is_doc_whitespace(*c))
                .count();
            if indent >= min_indent {
                end = i + 1;
                line.consume_n_times(is_doc_whitespace, min_indent);
                line.emit(LuaTokenKind::TkDocDetail);
            } else if !allow_blank_lines || !is_blank(line.tail_text()) {
                break;
            }
        }
        end
    }

    fn eat_prefixed_lines(lines: &mut [ReaderWithMarks], start: usize, prefix: char) -> usize {
        let mut end = start;
        for i in start..lines.len() {
            let line = &mut lines[i];
            if line.current_char() == prefix {
                end = i + 1;
                line.bump();
                line.emit(LuaTokenKind::TkDocDetailMarkup);
            } else {
                break;
            }
        }
        end
    }

    #[must_use]
    fn eat_target_name(line: &mut ReaderWithMarks) -> bool {
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
    fn eat_directive_name(line: &mut ReaderWithMarks) -> bool {
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

    fn eat_title_mark(line: &mut ReaderWithMarks) {
        line.eat_while(is_doc_whitespace);
        line.emit(LuaTokenKind::TkDocDetail);
        line.eat_while(|c| !is_doc_whitespace(c));
        line.emit(LuaTokenKind::TkDocDetailMarkup);
        line.eat_while(|_| true);
        line.emit(LuaTokenKind::TkDocDetail);
    }

    fn process_inline_content(&mut self, reader: &mut ReaderWithMarks) -> LineEnding {
        let line_ending = {
            let line = reader.tail_text().trim_end();
            if line.ends_with("::") && !line.ends_with(":::") {
                LineEnding::LiteralMark
            } else {
                LineEnding::Normal
            }
        };

        let mut guard = BacktrackGuard::new(500);

        while !reader.is_eof() {
            match reader.current_char() {
                '\\' => {
                    reader.emit(LuaTokenKind::TkDocDetail);
                    reader.bump();
                    reader.bump();
                    reader.emit(LuaTokenKind::TkDocDetailInlineMarkup);
                }

                // Explicit role.
                ':' if reader.next_char() != ':' => {
                    if !Self::is_start_string(reader.prev_char(), reader.next_char()) {
                        reader.bump();
                        continue;
                    }

                    let mut bt = reader.backtrack_point();
                    bt.emit(LuaTokenKind::TkDocDetail);

                    let marker = bt.mark(LuaSyntaxKind::DocRef);

                    bt.bump();
                    bt.emit(LuaTokenKind::TkDocDetailInlineArgMarkup);
                    if !Self::eat_role_name(bt.deref_mut())
                        || bt.current_char() != ':'
                        || bt.next_char() != '`'
                    {
                        bt.rollback();
                        reader.bump();
                        guard.backtrack(reader);
                        continue;
                    }

                    let role_text = bt.current_text();
                    let is_lua_ref = role_text.starts_with("lua:")
                        || (self.primary_domain.as_deref() == Some("lua")
                            && !role_text.contains(":"));

                    bt.emit(LuaTokenKind::TkDocDetailInlineArg);
                    bt.bump();
                    bt.emit(LuaTokenKind::TkDocDetailInlineArgMarkup);

                    if !Self::eat_role_body(bt.deref_mut(), true, is_lua_ref) {
                        bt.rollback();
                        reader.bump();
                        guard.backtrack(reader);
                        continue;
                    }

                    marker.complete(bt.deref_mut());
                    bt.commit();
                }

                // Inline code
                '`' if reader.next_char() == '`' => {
                    let mut bt = reader.backtrack_point();
                    bt.emit(LuaTokenKind::TkDocDetail);

                    let prev = bt.prev_char();
                    bt.bump();
                    bt.bump();
                    let next = bt.current_char();

                    if !Self::is_start_string(prev, next) {
                        bt.rollback();
                        reader.bump();
                        guard.backtrack(reader);
                        continue;
                    }

                    bt.emit(LuaTokenKind::TkDocDetailInlineMarkup);

                    if !Self::eat_inline_code(bt.deref_mut()) {
                        bt.rollback();
                        reader.eat_when('`');
                        guard.backtrack(reader);
                        continue;
                    }

                    bt.commit();
                }

                // Role or hyperlink.
                '`' => {
                    if !Self::is_start_string(reader.prev_char(), reader.next_char()) {
                        reader.bump();
                        continue;
                    }

                    let mut bt = reader.backtrack_point();
                    bt.emit(LuaTokenKind::TkDocDetail);
                    let marker = bt.mark(LuaSyntaxKind::DocRef);
                    if !Self::eat_role_body(
                        bt.deref_mut(),
                        false,
                        self.default_role
                            .as_deref()
                            .is_some_and(|r| r.starts_with("lua:")),
                    ) {
                        bt.rollback();
                        reader.bump();
                        guard.backtrack(reader);
                        continue;
                    }
                    marker.complete(bt.deref_mut());
                    bt.commit();
                }

                // Hyperlink reference.
                '_' => {
                    if reader.next_char() != '`' {
                        let mut bt = reader.backtrack_point();
                        let prev = bt.prev_char();
                        let n_chars = bt.consume_char_n_times('_', 2);
                        if Self::is_end_string(prev, bt.current_char()) {
                            Self::emit_simple_reference(bt.deref_mut(), n_chars);
                            bt.commit();
                            continue;
                        } else {
                            bt.rollback();
                            reader.eat_when('_');
                            guard.backtrack(reader);
                            continue;
                        }
                    }

                    let mut bt = reader.backtrack_point();
                    bt.emit(LuaTokenKind::TkDocDetail);

                    let prev = bt.prev_char();
                    bt.bump();
                    bt.bump();
                    let next = bt.current_char();

                    if !Self::is_start_string(prev, next) {
                        bt.rollback();
                        reader.bump();
                        reader.bump();
                        guard.backtrack(reader);
                        continue;
                    }

                    bt.emit(LuaTokenKind::TkDocDetailInlineMarkup);

                    if !Self::eat_hyperlink_ref(bt.deref_mut()) {
                        bt.rollback();
                        reader.bump();
                        reader.bump();
                        guard.backtrack(reader);
                        continue;
                    }

                    bt.commit();
                }

                // Substitution.
                '|' => {
                    if !Self::is_start_string(reader.prev_char(), reader.next_char()) {
                        reader.bump();
                        continue;
                    }

                    let mut bt = reader.backtrack_point();
                    bt.emit(LuaTokenKind::TkDocDetail);
                    bt.bump();
                    bt.emit(LuaTokenKind::TkDocDetailInlineMarkup);

                    if !Self::eat_subst(bt.deref_mut()) {
                        bt.rollback();
                        reader.bump();
                        guard.backtrack(reader);
                        continue;
                    }

                    bt.commit();
                }

                // Footnote.
                '[' => {
                    if !Self::is_start_string(reader.prev_char(), reader.next_char()) {
                        reader.bump();
                        continue;
                    }

                    let mut bt = reader.backtrack_point();
                    bt.emit(LuaTokenKind::TkDocDetail);
                    bt.bump();
                    bt.emit(LuaTokenKind::TkDocDetailInlineMarkup);

                    if !Self::eat_footnote(bt.deref_mut()) {
                        bt.rollback();
                        reader.bump();
                        guard.backtrack(reader);
                        continue;
                    }

                    bt.commit();
                }

                // Emphasis.
                '*' => {
                    let mut bt = reader.backtrack_point();
                    bt.emit(LuaTokenKind::TkDocDetail);

                    let marker;

                    let is_strong = bt.next_char() == '*';

                    let prev = bt.prev_char();
                    if is_strong {
                        marker = bt.mark(LuaSyntaxKind::DocStrong);
                        bt.bump();
                        bt.bump();
                    } else {
                        marker = bt.mark(LuaSyntaxKind::DocEm);
                        bt.bump();
                    }
                    let next = bt.current_char();

                    if !Self::is_start_string(prev, next) {
                        bt.rollback();
                        reader.eat_when('*');
                        guard.backtrack(reader);
                        continue;
                    }

                    bt.emit(LuaTokenKind::TkDocDetailInlineMarkup);

                    if !Self::eat_em(bt.deref_mut(), is_strong) {
                        bt.rollback();
                        reader.eat_when('*');
                        guard.backtrack(reader);
                        continue;
                    }

                    marker.complete(bt.deref_mut());
                    bt.commit();
                }
                _ => {
                    reader.bump();
                }
            }
        }

        reader.emit(LuaTokenKind::TkDocDetail);

        line_ending
    }

    #[must_use]
    fn eat_role_name(reader: &mut ReaderWithMarks) -> bool {
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
    fn eat_inline_code(reader: &mut ReaderWithMarks) -> bool {
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
                    Self::eat_mark_end(reader, LuaTokenKind::TkDocDetailInlineCode, 2);
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
    fn eat_role_body(
        reader: &mut ReaderWithMarks,
        has_explicit_role: bool,
        is_lua_role: bool,
    ) -> bool {
        if reader.current_char() != '`' || reader.next_char() == '`' {
            return false;
        }

        while !reader.is_eof() {
            match reader.current_char() {
                '`' => {
                    let mut bt = reader.backtrack_point();

                    let prev = bt.prev_char();
                    bt.bump();

                    let code = bt.reset_buff_into_sub_reader();

                    let mark_len = bt.consume_n_times(|c| c == '_', 2) + 1;
                    if !Self::is_end_string(prev, bt.current_char()) {
                        bt.rollback();
                        reader.bump();
                        continue;
                    }

                    if mark_len > 1 && !has_explicit_role {
                        process_inline_code(
                            &mut ReaderWithMarks::new(code, bt.get_events()),
                            LuaTokenKind::TkDocDetailInlineLink,
                        );
                    } else if is_lua_role {
                        process_lua_ref(&mut ReaderWithMarks::new(code, bt.get_events()));
                    } else {
                        process_inline_code(
                            &mut ReaderWithMarks::new(code, bt.get_events()),
                            LuaTokenKind::TkDocDetailInlineCode,
                        );
                    }

                    bt.emit(LuaTokenKind::TkDocDetailInlineMarkup);
                    bt.commit();

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

        // process_inline_code(&mut ReaderWithMarks::new(code, bp.get_events()));

        false
    }

    fn emit_simple_reference(reader: &mut ReaderWithMarks, n_chars: usize) {
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
                        reader.emit(LuaTokenKind::TkDocDetail);
                        return;
                    }
                    break;
                }
                next = ch;
            }
        }

        reader.reset_buff();

        if !content_range.is_empty() {
            reader.get_events().push(MarkEvent::EatToken {
                kind: LuaTokenKind::TkDocDetail,
                range: content_range,
            });
        }

        let href_range = SourceRange::new(
            content_range.start_offset + content_range.length,
            range.length - n_chars - content_range.length,
        );

        let markup_range =
            SourceRange::new(content_range.start_offset + range.length - n_chars, n_chars);

        if !href_range.is_empty() {
            reader.get_events().push(MarkEvent::EatToken {
                kind: LuaTokenKind::TkDocDetailInlineLink,
                range: href_range,
            });
            reader.get_events().push(MarkEvent::EatToken {
                kind: LuaTokenKind::TkDocDetailInlineMarkup,
                range: markup_range,
            });
        } else {
            reader.get_events().push(MarkEvent::EatToken {
                kind: LuaTokenKind::TkDocDetail,
                range: markup_range,
            });
        }
    }

    #[must_use]
    fn eat_hyperlink_ref(reader: &mut ReaderWithMarks) -> bool {
        reader.bump(); // Should be at least 1 char long.

        while !reader.is_eof() {
            match reader.current_char() {
                '`' => {
                    if !Self::is_end_string(reader.prev_char(), reader.next_char()) {
                        reader.bump();
                        continue;
                    }
                    reader.emit(LuaTokenKind::TkDocDetailInlineLink);
                    reader.bump();
                    reader.emit(LuaTokenKind::TkDocDetailInlineMarkup);
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
    fn eat_subst(reader: &mut ReaderWithMarks) -> bool {
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
                        LuaTokenKind::TkDocDetailInlineCode
                    } else {
                        LuaTokenKind::TkDocDetailInlineLink
                    };
                    Self::eat_mark_end(reader, kind, mark_len);
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
    fn eat_footnote(reader: &mut ReaderWithMarks) -> bool {
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
                    Self::eat_mark_end(reader, LuaTokenKind::TkDocDetailInlineLink, 2);
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
    fn eat_em(reader: &mut ReaderWithMarks, is_strong: bool) -> bool {
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
                    Self::eat_mark_end(reader, LuaTokenKind::TkDocDetail, mark_len);
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

    fn eat_mark_end(line: &mut ReaderWithMarks, content_kind: LuaTokenKind, mark_len: usize) {
        let range = line.current_range();

        assert!(range.length > mark_len);

        let content_range = SourceRange::new(range.start_offset, range.length - mark_len);
        line.get_events().push(MarkEvent::EatToken {
            kind: content_kind,
            range: content_range,
        });

        let mark_range = SourceRange::new(range.start_offset + range.length - mark_len, mark_len);
        line.get_events().push(MarkEvent::EatToken {
            kind: LuaTokenKind::TkDocDetailInlineMarkup,
            range: mark_range,
        });
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
        static IS_S: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"^\p{Ps}|\p{Pi}|\p{Pf}|\p{Pd}|\p{Po}$").unwrap());
        if prev.is_whitespace() {
            return true;
        }
        if prev.is_ascii() {
            matches!(
                prev,
                '-' | ':' | '/' | '\'' | '"' | '<' | '(' | '[' | '{' | '\0'
            )
        } else {
            let mut tmp = [0; 4];
            IS_S.is_match(prev.encode_utf8(&mut tmp))
        }
    }

    fn is_end_string(prev: char, next: char) -> bool {
        // 2
        if prev.is_whitespace() {
            return false;
        }

        // 7
        static IS_E: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"^\p{Pe}|\p{Pi}|\p{Pf}|\p{Pd}|\p{Po}$").unwrap());
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
            let mut tmp = [0; 4];
            IS_E.is_match(next.encode_utf8(&mut tmp))
        }
    }
}

/// Eat contents of RST's flag field (directive parameters
/// are also flag fields). Reader should be set to a range that contains
/// the entire line after the initial colon.
pub fn eat_rst_flag_body(reader: &mut ReaderWithMarks) {
    while !reader.is_eof() {
        match reader.current_char() {
            '\\' => {
                reader.bump();
                reader.bump();
            }
            ':' if is_doc_whitespace(reader.next_char()) || reader.next_char() == '\0' => {
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
pub fn process_lua_ref(reader: &mut ReaderWithMarks) {
    let n_backticks = reader.eat_when('`');
    reader.emit(LuaTokenKind::TkDocDetailInlineMarkup);

    let text = reader.tail_text().trim_matches('`');
    let has_explicit_title = text.ends_with('>')
        && !text.ends_with("\\>")
        && (text.starts_with('<') || text.contains(" <"));

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
        reader.emit(LuaTokenKind::TkDocDetailInlineCode);
        while reader.tail_range().length > n_backticks + 1 {
            reader.bump();
        }
        reader.emit(LuaTokenKind::TkDocDetailRef);
        reader.bump();
        reader.emit(LuaTokenKind::TkDocDetailInlineCode);
        reader.eat_while(|_| true);
        reader.emit(LuaTokenKind::TkDocDetailInlineMarkup);
    } else {
        reader.consume_char_n_times('~', 1);
        reader.emit(LuaTokenKind::TkDocDetailInlineCode);
        while reader.tail_range().length > n_backticks {
            reader.bump();
        }
        reader.emit(LuaTokenKind::TkDocDetailRef);
        reader.eat_while(|_| true);
        reader.emit(LuaTokenKind::TkDocDetailInlineMarkup);
    }
}

/// Parse contents of backtick-enclosed code block,
/// supports both markdown and rst syntax.
///
/// Reader should be set to a range that only includes code block contents
/// and backticks around it.
pub fn process_inline_code(reader: &mut ReaderWithMarks, kind: LuaTokenKind) {
    let n_backticks = reader.eat_when('`');
    reader.emit(LuaTokenKind::TkDocDetailInlineMarkup);
    while reader.tail_range().length > n_backticks {
        reader.bump();
    }
    reader.emit(kind);
    reader.eat_while(|_| true);
    reader.emit(LuaTokenKind::TkDocDetailInlineMarkup);
}

#[cfg(test)]
mod tests {
    use crate::{DescParserType, LuaParser, ParserConfig};

    #[test]
    fn test_rst() {
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
--- .. code-block:: lua
---    :linenos:
---
---    print("foo!")
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
---
"#;

        let expected = r###"
Syntax(Chunk)@0..3941
  Syntax(Block)@0..3941
    Token(TkEndOfLine)@0..1 "\n"
    Syntax(Comment)@1..1213
      Token(TkNormalStart)@1..4 "---"
      Token(TkWhitespace)@4..5 " "
      Syntax(DocDescription)@5..1213
        Syntax(DocScope)@5..36
          Token(TkDocDetail)@5..18 "Inline markup"
          Token(TkEndOfLine)@18..19 "\n"
          Token(TkNormalStart)@19..22 "---"
          Token(TkWhitespace)@22..23 " "
          Token(TkDocDetailMarkup)@23..36 "============="
        Token(TkEndOfLine)@36..37 "\n"
        Token(TkNormalStart)@37..40 "---"
        Token(TkEndOfLine)@40..41 "\n"
        Token(TkNormalStart)@41..44 "---"
        Token(TkWhitespace)@44..45 " "
        Syntax(DocScope)@45..82
          Token(TkDocDetail)@45..61 "Not valid markup"
          Token(TkEndOfLine)@61..62 "\n"
          Token(TkNormalStart)@62..65 "---"
          Token(TkWhitespace)@65..66 " "
          Token(TkDocDetailMarkup)@66..82 "----------------"
        Token(TkEndOfLine)@82..83 "\n"
        Token(TkNormalStart)@83..86 "---"
        Token(TkEndOfLine)@86..87 "\n"
        Token(TkNormalStart)@87..90 "---"
        Token(TkWhitespace)@90..91 " "
        Syntax(DocScope)@91..144
          Token(TkDocDetailMarkup)@91..92 "-"
          Token(TkDocDetail)@92..144 " 2 * x a ** b (* BOM3 ..."
        Token(TkEndOfLine)@144..145 "\n"
        Token(TkNormalStart)@145..148 "---"
        Token(TkWhitespace)@148..149 " "
        Syntax(DocScope)@149..169
          Token(TkDocDetailMarkup)@149..150 "-"
          Token(TkDocDetail)@150..169 " || (breaks rule 3)"
        Token(TkEndOfLine)@169..170 "\n"
        Token(TkNormalStart)@170..173 "---"
        Token(TkWhitespace)@173..174 " "
        Syntax(DocScope)@174..257
          Token(TkDocDetailMarkup)@174..175 "-"
          Token(TkDocDetail)@175..257 " \"*\" '|' (*) [*] {*}  ..."
        Token(TkEndOfLine)@257..258 "\n"
        Token(TkNormalStart)@258..261 "---"
        Token(TkWhitespace)@261..262 " "
        Syntax(DocScope)@262..347
          Token(TkDocDetailMarkup)@262..263 "-"
          Token(TkDocDetail)@263..320 " 2*x a**b O(N**2) e** ..."
          Token(TkDocDetail)@320..347 " __init__() (breaks r ..."
        Token(TkEndOfLine)@347..348 "\n"
        Token(TkNormalStart)@348..351 "---"
        Token(TkEndOfLine)@351..352 "\n"
        Token(TkNormalStart)@352..355 "---"
        Token(TkEndOfLine)@355..356 "\n"
        Token(TkNormalStart)@356..359 "---"
        Token(TkWhitespace)@359..360 " "
        Syntax(DocScope)@360..389
          Token(TkDocDetail)@360..372 "Valid markup"
          Token(TkEndOfLine)@372..373 "\n"
          Token(TkNormalStart)@373..376 "---"
          Token(TkWhitespace)@376..377 " "
          Token(TkDocDetailMarkup)@377..389 "------------"
        Token(TkEndOfLine)@389..390 "\n"
        Token(TkNormalStart)@390..393 "---"
        Token(TkEndOfLine)@393..394 "\n"
        Token(TkNormalStart)@394..397 "---"
        Token(TkWhitespace)@397..398 " "
        Token(TkDocDetail)@398..405 "Style: "
        Syntax(DocEm)@405..409
          Token(TkDocDetailInlineMarkup)@405..406 "*"
          Token(TkDocDetail)@406..408 "em"
          Token(TkDocDetailInlineMarkup)@408..409 "*"
        Token(TkDocDetail)@409..411 ", "
        Syntax(DocStrong)@411..421
          Token(TkDocDetailInlineMarkup)@411..413 "**"
          Token(TkDocDetail)@413..419 "strong"
          Token(TkDocDetailInlineMarkup)@419..421 "**"
        Token(TkDocDetail)@421..423 ", "
        Syntax(DocStrong)@423..447
          Token(TkDocDetailInlineMarkup)@423..425 "**"
          Token(TkDocDetail)@425..445 "*strong, with stars*"
          Token(TkDocDetailInlineMarkup)@445..447 "**"
        Token(TkDocDetail)@447..457 ", *broken "
        Syntax(DocRef)@457..461
          Token(TkDocDetailInlineMarkup)@457..458 "`"
          Token(TkDocDetailInlineCode)@458..460 "em"
          Token(TkDocDetailInlineMarkup)@460..461 "`"
        Token(TkDocDetail)@461..462 "."
        Token(TkEndOfLine)@462..463 "\n"
        Token(TkNormalStart)@463..466 "---"
        Token(TkWhitespace)@466..467 " "
        Token(TkDocDetail)@467..481 "Implicit ref: "
        Syntax(DocRef)@481..492
          Token(TkDocDetailInlineMarkup)@481..482 "`"
          Token(TkDocDetailInlineCode)@482..491 "something"
          Token(TkDocDetailInlineMarkup)@491..492 "`"
        Token(TkDocDetail)@492..494 ", "
        Syntax(DocRef)@494..501
          Token(TkDocDetailInlineMarkup)@494..495 "`"
          Token(TkDocDetailInlineCode)@495..500 "a\\` b"
          Token(TkDocDetailInlineMarkup)@500..501 "`"
        Token(TkDocDetail)@501..521 ". Broken: `something"
        Token(TkEndOfLine)@521..522 "\n"
        Token(TkNormalStart)@522..525 "---"
        Token(TkWhitespace)@525..526 " "
        Token(TkDocDetail)@526..540 "Explicit ref: "
        Syntax(DocRef)@540..551
          Token(TkDocDetailInlineArgMarkup)@540..541 ":"
          Token(TkDocDetailInlineArg)@541..545 "role"
          Token(TkDocDetailInlineArgMarkup)@545..546 ":"
          Token(TkDocDetailInlineMarkup)@546..547 "`"
          Token(TkDocDetailInlineCode)@547..550 "ref"
          Token(TkDocDetailInlineMarkup)@550..551 "`"
        Token(TkDocDetail)@551..571 ". Broken: :role:`ref"
        Token(TkEndOfLine)@571..572 "\n"
        Token(TkNormalStart)@572..575 "---"
        Token(TkWhitespace)@575..576 " "
        Token(TkDocDetail)@576..585 "Lua ref: "
        Syntax(DocRef)@585..601
          Token(TkDocDetailInlineArgMarkup)@585..586 ":"
          Token(TkDocDetailInlineArg)@586..593 "lua:obj"
          Token(TkDocDetailInlineArgMarkup)@593..594 ":"
          Token(TkDocDetailInlineMarkup)@594..595 "`"
          Token(TkDocDetailRef)@595..600 "a.b.c"
          Token(TkDocDetailInlineMarkup)@600..601 "`"
        Token(TkDocDetail)@601..603 ", "
        Syntax(DocRef)@603..620
          Token(TkDocDetailInlineArgMarkup)@603..604 ":"
          Token(TkDocDetailInlineArg)@604..611 "lua:obj"
          Token(TkDocDetailInlineArgMarkup)@611..612 ":"
          Token(TkDocDetailInlineMarkup)@612..613 "`"
          Token(TkDocDetailInlineCode)@613..614 "~"
          Token(TkDocDetailRef)@614..619 "a.b.c"
          Token(TkDocDetailInlineMarkup)@619..620 "`"
        Token(TkDocDetail)@620..622 ", "
        Syntax(DocRef)@622..646
          Token(TkDocDetailInlineArgMarkup)@622..623 ":"
          Token(TkDocDetailInlineArg)@623..630 "lua:obj"
          Token(TkDocDetailInlineArgMarkup)@630..631 ":"
          Token(TkDocDetailInlineMarkup)@631..632 "`"
          Token(TkDocDetailInlineCode)@632..639 "title <"
          Token(TkDocDetailRef)@639..644 "a.b.c"
          Token(TkDocDetailInlineCode)@644..645 ">"
          Token(TkDocDetailInlineMarkup)@645..646 "`"
        Token(TkDocDetail)@646..647 "."
        Token(TkEndOfLine)@647..648 "\n"
        Token(TkNormalStart)@648..651 "---"
        Token(TkWhitespace)@651..652 " "
        Token(TkDocDetail)@652..664 "Code block: "
        Token(TkDocDetailInlineMarkup)@664..666 "``"
        Token(TkDocDetailInlineCode)@666..670 "code"
        Token(TkDocDetailInlineMarkup)@670..672 "``"
        Token(TkDocDetail)@672..674 ", "
        Token(TkDocDetailInlineMarkup)@674..676 "``"
        Token(TkDocDetailInlineCode)@676..692 "code `backticks`"
        Token(TkDocDetailInlineMarkup)@692..694 "``"
        Token(TkDocDetail)@694..695 ","
        Token(TkEndOfLine)@695..696 "\n"
        Token(TkNormalStart)@696..699 "---"
        Token(TkWhitespace)@699..700 " "
        Token(TkDocDetailInlineMarkup)@700..702 "``"
        Token(TkDocDetailInlineCode)@702..726 "escapes don't work here\\"
        Token(TkDocDetailInlineMarkup)@726..728 "``"
        Token(TkDocDetail)@728..730 ", "
        Token(TkDocDetailInlineMarkup)@730..732 "``"
        Token(TkDocDetailInlineCode)@732..748 "{ 1, 2, nil, 4 }"
        Token(TkDocDetailInlineMarkup)@748..750 "``"
        Token(TkDocDetail)@750..751 "."
        Token(TkEndOfLine)@751..752 "\n"
        Token(TkNormalStart)@752..755 "---"
        Token(TkWhitespace)@755..756 " "
        Token(TkDocDetail)@756..781 "Broken code block: `` ..."
        Syntax(DocEm)@781..786
          Token(TkDocDetailInlineMarkup)@781..782 "*"
          Token(TkDocDetail)@782..785 "bar"
          Token(TkDocDetailInlineMarkup)@785..786 "*"
        Token(TkEndOfLine)@786..787 "\n"
        Token(TkNormalStart)@787..790 "---"
        Token(TkWhitespace)@790..791 " "
        Token(TkDocDetail)@791..812 "Implicit hyperlinks: "
        Token(TkDocDetailInlineLink)@812..818 "target"
        Token(TkDocDetailInlineMarkup)@818..819 "_"
        Token(TkDocDetail)@819..821 ", "
        Token(TkDocDetailInlineLink)@821..830 "anonymous"
        Token(TkDocDetailInlineMarkup)@830..832 "__"
        Token(TkDocDetail)@832..848 ", not a link___."
        Token(TkEndOfLine)@848..849 "\n"
        Token(TkNormalStart)@849..852 "---"
        Token(TkWhitespace)@852..853 " "
        Token(TkDocDetail)@853..874 "Explicit hyperlinks: "
        Syntax(DocRef)@874..883
          Token(TkDocDetailInlineMarkup)@874..875 "`"
          Token(TkDocDetailInlineLink)@875..881 "target"
          Token(TkDocDetailInlineMarkup)@881..882 "`"
          Token(TkDocDetailInlineMarkup)@882..883 "_"
        Token(TkDocDetail)@883..885 ", "
        Syntax(DocRef)@885..898
          Token(TkDocDetailInlineMarkup)@885..886 "`"
          Token(TkDocDetailInlineLink)@886..895 "anonymous"
          Token(TkDocDetailInlineMarkup)@895..896 "`"
          Token(TkDocDetailInlineMarkup)@896..898 "__"
        Token(TkDocDetail)@898..899 "."
        Token(TkEndOfLine)@899..900 "\n"
        Token(TkNormalStart)@900..903 "---"
        Token(TkWhitespace)@903..904 " "
        Token(TkDocDetail)@904..919 "Malformed ref: "
        Syntax(DocRef)@919..938
          Token(TkDocDetailInlineArgMarkup)@919..920 ":"
          Token(TkDocDetailInlineArg)@920..927 "lua:obj"
          Token(TkDocDetailInlineArgMarkup)@927..928 ":"
          Token(TkDocDetailInlineMarkup)@928..929 "`"
          Token(TkDocDetailInlineCode)@929..930 "~"
          Token(TkDocDetailRef)@930..935 "a.b.c"
          Token(TkDocDetailInlineMarkup)@935..936 "`"
          Token(TkDocDetailInlineMarkup)@936..938 "__"
        Token(TkDocDetail)@938..961 " (still parsed as ref)."
        Token(TkEndOfLine)@961..962 "\n"
        Token(TkNormalStart)@962..965 "---"
        Token(TkWhitespace)@965..966 " "
        Token(TkDocDetail)@966..977 "Hyperlink: "
        Syntax(DocRef)@977..987
          Token(TkDocDetailInlineMarkup)@977..978 "`"
          Token(TkDocDetailInlineLink)@978..985 "foo bar"
          Token(TkDocDetailInlineMarkup)@985..986 "`"
          Token(TkDocDetailInlineMarkup)@986..987 "_"
        Token(TkDocDetail)@987..989 ", "
        Syntax(DocRef)@989..1000
          Token(TkDocDetailInlineMarkup)@989..990 "`"
          Token(TkDocDetailInlineLink)@990..997 "foo bar"
          Token(TkDocDetailInlineMarkup)@997..998 "`"
          Token(TkDocDetailInlineMarkup)@998..1000 "__"
        Token(TkDocDetail)@1000..1002 ", "
        Syntax(DocRef)@1002..1014
          Token(TkDocDetailInlineMarkup)@1002..1003 "`"
          Token(TkDocDetailInlineLink)@1003..1012 "foo <bar>"
          Token(TkDocDetailInlineMarkup)@1012..1013 "`"
          Token(TkDocDetailInlineMarkup)@1013..1014 "_"
        Token(TkDocDetail)@1014..1015 "."
        Token(TkEndOfLine)@1015..1016 "\n"
        Token(TkNormalStart)@1016..1019 "---"
        Token(TkWhitespace)@1019..1020 " "
        Token(TkDocDetail)@1020..1040 "Internal hyperlink: "
        Token(TkDocDetailInlineMarkup)@1040..1042 "_`"
        Token(TkDocDetailInlineLink)@1042..1049 "foo bar"
        Token(TkDocDetailInlineMarkup)@1049..1050 "`"
        Token(TkDocDetail)@1050..1068 ". Broken _`foo bar"
        Token(TkEndOfLine)@1068..1069 "\n"
        Token(TkNormalStart)@1069..1072 "---"
        Token(TkWhitespace)@1072..1073 " "
        Token(TkDocDetail)@1073..1083 "Footnote: "
        Token(TkDocDetailInlineMarkup)@1083..1084 "["
        Token(TkDocDetailInlineLink)@1084..1085 "1"
        Token(TkDocDetailInlineMarkup)@1085..1087 "]_"
        Token(TkDocDetail)@1087..1096 ", [2], [3"
        Token(TkEndOfLine)@1096..1097 "\n"
        Token(TkNormalStart)@1097..1100 "---"
        Token(TkWhitespace)@1100..1101 " "
        Token(TkDocDetail)@1101..1114 "Replacement: "
        Token(TkDocDetailInlineMarkup)@1114..1115 "|"
        Token(TkDocDetailInlineCode)@1115..1118 "foo"
        Token(TkDocDetailInlineMarkup)@1118..1119 "|"
        Token(TkDocDetail)@1119..1121 ", "
        Token(TkDocDetailInlineMarkup)@1121..1122 "|"
        Token(TkDocDetailInlineLink)@1122..1125 "bar"
        Token(TkDocDetailInlineMarkup)@1125..1127 "|_"
        Token(TkDocDetail)@1127..1129 ", "
        Token(TkDocDetailInlineMarkup)@1129..1130 "|"
        Token(TkDocDetailInlineLink)@1130..1133 "baz"
        Token(TkDocDetailInlineMarkup)@1133..1136 "|__"
        Token(TkDocDetail)@1136..1137 "."
        Token(TkEndOfLine)@1137..1138 "\n"
        Token(TkNormalStart)@1138..1141 "---"
        Token(TkWhitespace)@1141..1142 " "
        Syntax(DocEm)@1142..1163
          Token(TkDocDetailInlineMarkup)@1142..1143 "*"
          Token(TkDocDetail)@1143..1162 "2 * x  *a **b *.rst"
          Token(TkDocDetailInlineMarkup)@1162..1163 "*"
        Token(TkEndOfLine)@1163..1164 "\n"
        Token(TkNormalStart)@1164..1167 "---"
        Token(TkWhitespace)@1167..1168 " "
        Syntax(DocEm)@1168..1213
          Token(TkDocDetailInlineMarkup)@1168..1169 "*"
          Token(TkDocDetail)@1169..1212 "2*x a**b O(N**2) e**( ..."
          Token(TkDocDetailInlineMarkup)@1212..1213 "*"
    Token(TkEndOfLine)@1213..1214 "\n"
    Token(TkEndOfLine)@1214..1215 "\n"
    Syntax(Comment)@1215..3940
      Token(TkNormalStart)@1215..1218 "---"
      Token(TkWhitespace)@1218..1219 " "
      Syntax(DocDescription)@1219..3940
        Syntax(DocScope)@1219..1248
          Token(TkDocDetail)@1219..1231 "Block markup"
          Token(TkEndOfLine)@1231..1232 "\n"
          Token(TkNormalStart)@1232..1235 "---"
          Token(TkWhitespace)@1235..1236 " "
          Token(TkDocDetailMarkup)@1236..1248 "============"
        Token(TkEndOfLine)@1248..1249 "\n"
        Token(TkNormalStart)@1249..1252 "---"
        Token(TkEndOfLine)@1252..1253 "\n"
        Token(TkNormalStart)@1253..1256 "---"
        Token(TkWhitespace)@1256..1257 " "
        Syntax(DocScope)@1257..1272
          Token(TkDocDetail)@1257..1262 "Lists"
          Token(TkEndOfLine)@1262..1263 "\n"
          Token(TkNormalStart)@1263..1266 "---"
          Token(TkWhitespace)@1266..1267 " "
          Token(TkDocDetailMarkup)@1267..1272 "-----"
        Token(TkEndOfLine)@1272..1273 "\n"
        Token(TkNormalStart)@1273..1276 "---"
        Token(TkEndOfLine)@1276..1277 "\n"
        Token(TkNormalStart)@1277..1280 "---"
        Token(TkWhitespace)@1280..1281 " "
        Syntax(DocScope)@1281..1333
          Token(TkDocDetailMarkup)@1281..1282 "-"
          Token(TkDocDetail)@1282..1289 " List 1"
          Token(TkEndOfLine)@1289..1290 "\n"
          Token(TkNormalStart)@1290..1293 "---"
          Token(TkWhitespace)@1293..1294 " "
          Token(TkDocDetail)@1294..1296 "  "
          Token(TkDocDetail)@1296..1308 "Continuation"
          Token(TkEndOfLine)@1308..1309 "\n"
          Token(TkNormalStart)@1309..1312 "---"
          Token(TkEndOfLine)@1312..1313 "\n"
          Token(TkNormalStart)@1313..1316 "---"
          Token(TkWhitespace)@1316..1317 " "
          Token(TkDocDetail)@1317..1319 "  "
          Token(TkDocDetail)@1319..1333 "Continuation 2"
        Token(TkEndOfLine)@1333..1334 "\n"
        Token(TkNormalStart)@1334..1337 "---"
        Token(TkEndOfLine)@1337..1338 "\n"
        Token(TkNormalStart)@1338..1341 "---"
        Token(TkWhitespace)@1341..1342 " "
        Syntax(DocScope)@1342..1371
          Token(TkDocDetailMarkup)@1342..1343 "-"
          Token(TkDocDetail)@1343..1351 "  List 2"
          Token(TkEndOfLine)@1351..1352 "\n"
          Token(TkNormalStart)@1352..1355 "---"
          Token(TkWhitespace)@1355..1356 " "
          Token(TkDocDetail)@1356..1359 "   "
          Token(TkDocDetail)@1359..1371 "Continuation"
        Token(TkEndOfLine)@1371..1372 "\n"
        Token(TkNormalStart)@1372..1375 "---"
        Token(TkWhitespace)@1375..1376 " "
        Syntax(DocScope)@1376..1396
          Token(TkDocDetail)@1376..1378 "  "
          Token(TkDocDetail)@1378..1396 "Not a continuation"
        Token(TkEndOfLine)@1396..1397 "\n"
        Token(TkNormalStart)@1397..1400 "---"
        Token(TkEndOfLine)@1400..1401 "\n"
        Token(TkNormalStart)@1401..1404 "---"
        Token(TkWhitespace)@1404..1405 " "
        Syntax(DocScope)@1405..1435
          Token(TkDocDetailMarkup)@1405..1406 "-"
          Token(TkDocDetail)@1406..1411 " List"
          Token(TkEndOfLine)@1411..1412 "\n"
          Token(TkNormalStart)@1412..1415 "---"
          Token(TkEndOfLine)@1415..1416 "\n"
          Token(TkNormalStart)@1416..1419 "---"
          Token(TkWhitespace)@1419..1420 " "
          Token(TkDocDetail)@1420..1422 "  "
          Syntax(DocScope)@1422..1435
            Token(TkDocDetailMarkup)@1422..1423 "-"
            Token(TkDocDetail)@1423..1435 " Nested list"
        Token(TkEndOfLine)@1435..1436 "\n"
        Token(TkNormalStart)@1436..1439 "---"
        Token(TkEndOfLine)@1439..1440 "\n"
        Token(TkNormalStart)@1440..1443 "---"
        Token(TkWhitespace)@1443..1444 " "
        Syntax(DocScope)@1444..1450
          Token(TkDocDetailMarkup)@1444..1445 "-"
          Token(TkDocDetail)@1445..1450 " List"
        Token(TkEndOfLine)@1450..1451 "\n"
        Token(TkNormalStart)@1451..1454 "---"
        Token(TkWhitespace)@1454..1455 " "
        Token(TkDocDetail)@1455..1463 "Not list"
        Token(TkEndOfLine)@1463..1464 "\n"
        Token(TkNormalStart)@1464..1467 "---"
        Token(TkEndOfLine)@1467..1468 "\n"
        Token(TkNormalStart)@1468..1471 "---"
        Token(TkWhitespace)@1471..1472 " "
        Syntax(DocScope)@1472..1478
          Token(TkDocDetailMarkup)@1472..1473 "-"
          Token(TkDocDetail)@1473..1478 " List"
        Token(TkEndOfLine)@1478..1479 "\n"
        Token(TkNormalStart)@1479..1482 "---"
        Token(TkWhitespace)@1482..1483 " "
        Syntax(DocScope)@1483..1484
          Token(TkDocDetailMarkup)@1483..1484 "-"
        Token(TkEndOfLine)@1484..1485 "\n"
        Token(TkNormalStart)@1485..1488 "---"
        Token(TkWhitespace)@1488..1489 " "
        Syntax(DocScope)@1489..1495
          Token(TkDocDetailMarkup)@1489..1490 "-"
          Token(TkDocDetail)@1490..1495 " List"
        Token(TkEndOfLine)@1495..1496 "\n"
        Token(TkNormalStart)@1496..1499 "---"
        Token(TkEndOfLine)@1499..1500 "\n"
        Token(TkNormalStart)@1500..1503 "---"
        Token(TkEndOfLine)@1503..1504 "\n"
        Token(TkNormalStart)@1504..1507 "---"
        Token(TkWhitespace)@1507..1508 " "
        Syntax(DocScope)@1508..1541
          Token(TkDocDetail)@1508..1522 "Numbered lists"
          Token(TkEndOfLine)@1522..1523 "\n"
          Token(TkNormalStart)@1523..1526 "---"
          Token(TkWhitespace)@1526..1527 " "
          Token(TkDocDetailMarkup)@1527..1541 "--------------"
        Token(TkEndOfLine)@1541..1542 "\n"
        Token(TkNormalStart)@1542..1545 "---"
        Token(TkEndOfLine)@1545..1546 "\n"
        Token(TkNormalStart)@1546..1549 "---"
        Token(TkWhitespace)@1549..1550 " "
        Syntax(DocScope)@1550..1559
          Token(TkDocDetailMarkup)@1550..1552 "1."
          Token(TkDocDetail)@1552..1554 "  "
          Token(TkDocDetail)@1554..1559 "List."
        Token(TkEndOfLine)@1559..1560 "\n"
        Token(TkNormalStart)@1560..1563 "---"
        Token(TkEndOfLine)@1563..1564 "\n"
        Token(TkNormalStart)@1564..1567 "---"
        Token(TkWhitespace)@1567..1568 " "
        Syntax(DocScope)@1568..1577
          Token(TkDocDetailMarkup)@1568..1570 "1)"
          Token(TkDocDetail)@1570..1572 "  "
          Token(TkDocDetail)@1572..1577 "List."
        Token(TkEndOfLine)@1577..1578 "\n"
        Token(TkNormalStart)@1578..1581 "---"
        Token(TkEndOfLine)@1581..1582 "\n"
        Token(TkNormalStart)@1582..1585 "---"
        Token(TkWhitespace)@1585..1586 " "
        Syntax(DocScope)@1586..1595
          Token(TkDocDetailMarkup)@1586..1589 "(1)"
          Token(TkDocDetail)@1589..1590 " "
          Token(TkDocDetail)@1590..1595 "List."
        Token(TkEndOfLine)@1595..1596 "\n"
        Token(TkNormalStart)@1596..1599 "---"
        Token(TkEndOfLine)@1599..1600 "\n"
        Token(TkNormalStart)@1600..1603 "---"
        Token(TkWhitespace)@1603..1604 " "
        Token(TkDocDetail)@1604..1618 "A) This is not"
        Token(TkEndOfLine)@1618..1619 "\n"
        Token(TkNormalStart)@1619..1622 "---"
        Token(TkWhitespace)@1622..1623 " "
        Token(TkDocDetail)@1623..1629 "a list"
        Token(TkEndOfLine)@1629..1630 "\n"
        Token(TkNormalStart)@1630..1633 "---"
        Token(TkEndOfLine)@1633..1634 "\n"
        Token(TkNormalStart)@1634..1637 "---"
        Token(TkWhitespace)@1637..1638 " "
        Syntax(DocScope)@1638..1656
          Token(TkDocDetailMarkup)@1638..1640 "A)"
          Token(TkDocDetail)@1640..1641 " "
          Token(TkDocDetail)@1641..1656 "This is a list."
        Token(TkEndOfLine)@1656..1657 "\n"
        Token(TkNormalStart)@1657..1660 "---"
        Token(TkEndOfLine)@1660..1661 "\n"
        Token(TkNormalStart)@1661..1664 "---"
        Token(TkWhitespace)@1664..1665 " "
        Token(TkDocDetail)@1665..1689 "1) This is not a list..."
        Token(TkEndOfLine)@1689..1690 "\n"
        Token(TkNormalStart)@1690..1693 "---"
        Token(TkWhitespace)@1693..1694 " "
        Token(TkDocDetail)@1694..1745 "2. because list style ..."
        Token(TkEndOfLine)@1745..1746 "\n"
        Token(TkNormalStart)@1746..1749 "---"
        Token(TkEndOfLine)@1749..1750 "\n"
        Token(TkNormalStart)@1750..1753 "---"
        Token(TkWhitespace)@1753..1754 " "
        Syntax(DocScope)@1754..1774
          Token(TkDocDetailMarkup)@1754..1756 "1)"
          Token(TkDocDetail)@1756..1757 " "
          Token(TkDocDetail)@1757..1774 "This is a list..."
        Token(TkEndOfLine)@1774..1775 "\n"
        Token(TkNormalStart)@1775..1778 "---"
        Token(TkWhitespace)@1778..1779 " "
        Syntax(DocScope)@1779..1816
          Token(TkDocDetailMarkup)@1779..1781 "2)"
          Token(TkDocDetail)@1781..1782 " "
          Token(TkDocDetail)@1782..1816 "because list style do ..."
        Token(TkEndOfLine)@1816..1817 "\n"
        Token(TkNormalStart)@1817..1820 "---"
        Token(TkEndOfLine)@1820..1821 "\n"
        Token(TkNormalStart)@1821..1824 "---"
        Token(TkWhitespace)@1824..1825 " "
        Token(TkDocDetailInlineMarkup)@1825..1827 "\\A"
        Token(TkDocDetail)@1827..1862 ". Einstein was a real ..."
        Token(TkEndOfLine)@1862..1863 "\n"
        Token(TkNormalStart)@1863..1866 "---"
        Token(TkEndOfLine)@1866..1867 "\n"
        Token(TkNormalStart)@1867..1870 "---"
        Token(TkWhitespace)@1870..1871 " "
        Syntax(DocScope)@1871..1936
          Token(TkDocDetailMarkup)@1871..1873 "1."
          Token(TkDocDetail)@1873..1874 " "
          Token(TkDocDetail)@1874..1894 "Item 1 initial text."
          Token(TkEndOfLine)@1894..1895 "\n"
          Token(TkNormalStart)@1895..1898 "---"
          Token(TkEndOfLine)@1898..1899 "\n"
          Token(TkNormalStart)@1899..1902 "---"
          Token(TkWhitespace)@1902..1903 " "
          Token(TkDocDetail)@1903..1906 "   "
          Syntax(DocScope)@1906..1917
            Token(TkDocDetailMarkup)@1906..1908 "a)"
            Token(TkDocDetail)@1908..1909 " "
            Token(TkDocDetail)@1909..1917 "Item 1a."
          Token(TkEndOfLine)@1917..1918 "\n"
          Token(TkNormalStart)@1918..1921 "---"
          Token(TkWhitespace)@1921..1922 " "
          Token(TkDocDetail)@1922..1925 "   "
          Syntax(DocScope)@1925..1936
            Token(TkDocDetailMarkup)@1925..1927 "b)"
            Token(TkDocDetail)@1927..1928 " "
            Token(TkDocDetail)@1928..1936 "Item 1b."
        Token(TkEndOfLine)@1936..1937 "\n"
        Token(TkNormalStart)@1937..1940 "---"
        Token(TkEndOfLine)@1940..1941 "\n"
        Token(TkNormalStart)@1941..1944 "---"
        Token(TkWhitespace)@1944..1945 " "
        Syntax(DocScope)@1945..1978
          Token(TkDocDetailMarkup)@1945..1947 "2."
          Token(TkDocDetail)@1947..1948 " "
          Syntax(DocScope)@1948..1959
            Token(TkDocDetailMarkup)@1948..1950 "a)"
            Token(TkDocDetail)@1950..1951 " "
            Token(TkDocDetail)@1951..1959 "Item 2a."
          Token(TkEndOfLine)@1959..1960 "\n"
          Token(TkNormalStart)@1960..1963 "---"
          Token(TkWhitespace)@1963..1964 " "
          Token(TkDocDetail)@1964..1967 "   "
          Syntax(DocScope)@1967..1978
            Token(TkDocDetailMarkup)@1967..1969 "b)"
            Token(TkDocDetail)@1969..1970 " "
            Token(TkDocDetail)@1970..1978 "Item 2b."
        Token(TkEndOfLine)@1978..1979 "\n"
        Token(TkNormalStart)@1979..1982 "---"
        Token(TkEndOfLine)@1982..1983 "\n"
        Token(TkNormalStart)@1983..1986 "---"
        Token(TkWhitespace)@1986..1987 " "
        Syntax(DocScope)@1987..1994
          Token(TkDocDetailMarkup)@1987..1989 "1."
          Token(TkDocDetail)@1989..1990 " "
          Token(TkDocDetail)@1990..1994 "List"
        Token(TkEndOfLine)@1994..1995 "\n"
        Token(TkNormalStart)@1995..1998 "---"
        Token(TkWhitespace)@1998..1999 " "
        Syntax(DocScope)@1999..2001
          Token(TkDocDetailMarkup)@1999..2001 "2."
        Token(TkEndOfLine)@2001..2002 "\n"
        Token(TkNormalStart)@2002..2005 "---"
        Token(TkWhitespace)@2005..2006 " "
        Syntax(DocScope)@2006..2013
          Token(TkDocDetailMarkup)@2006..2008 "3."
          Token(TkDocDetail)@2008..2009 " "
          Token(TkDocDetail)@2009..2013 "List"
        Token(TkEndOfLine)@2013..2014 "\n"
        Token(TkNormalStart)@2014..2017 "---"
        Token(TkEndOfLine)@2017..2018 "\n"
        Token(TkNormalStart)@2018..2021 "---"
        Token(TkEndOfLine)@2021..2022 "\n"
        Token(TkNormalStart)@2022..2025 "---"
        Token(TkWhitespace)@2025..2026 " "
        Syntax(DocScope)@2026..2051
          Token(TkDocDetail)@2026..2036 "Field list"
          Token(TkEndOfLine)@2036..2037 "\n"
          Token(TkNormalStart)@2037..2040 "---"
          Token(TkWhitespace)@2040..2041 " "
          Token(TkDocDetailMarkup)@2041..2051 "----------"
        Token(TkEndOfLine)@2051..2052 "\n"
        Token(TkNormalStart)@2052..2055 "---"
        Token(TkEndOfLine)@2055..2056 "\n"
        Token(TkNormalStart)@2056..2059 "---"
        Token(TkWhitespace)@2059..2060 " "
        Syntax(DocScope)@2060..2075
          Token(TkDocDetailArgMarkup)@2060..2061 ":"
          Token(TkDocDetailArg)@2061..2066 "Field"
          Token(TkDocDetailArgMarkup)@2066..2067 ":"
          Token(TkDocDetail)@2067..2068 " "
          Token(TkDocDetail)@2068..2075 "content"
        Token(TkEndOfLine)@2075..2076 "\n"
        Token(TkNormalStart)@2076..2079 "---"
        Token(TkWhitespace)@2079..2080 " "
        Syntax(DocScope)@2080..2097
          Token(TkDocDetailArgMarkup)@2080..2081 ":"
          Token(TkDocDetailArg)@2081..2088 "Field:2"
          Token(TkDocDetailArgMarkup)@2088..2089 ":"
          Token(TkDocDetail)@2089..2090 " "
          Token(TkDocDetail)@2090..2097 "Content"
        Token(TkEndOfLine)@2097..2098 "\n"
        Token(TkNormalStart)@2098..2101 "---"
        Token(TkWhitespace)@2101..2102 " "
        Syntax(DocScope)@2102..2146
          Token(TkDocDetailArgMarkup)@2102..2103 ":"
          Token(TkDocDetailArg)@2103..2110 "Field:3"
          Token(TkDocDetailArgMarkup)@2110..2111 ":"
          Token(TkDocDetail)@2111..2112 " "
          Token(TkDocDetail)@2112..2119 "Content"
          Token(TkEndOfLine)@2119..2120 "\n"
          Token(TkNormalStart)@2120..2123 "---"
          Token(TkWhitespace)@2123..2124 " "
          Token(TkDocDetail)@2124..2134 "          "
          Token(TkDocDetail)@2134..2146 "Continuation"
        Token(TkEndOfLine)@2146..2147 "\n"
        Token(TkNormalStart)@2147..2150 "---"
        Token(TkWhitespace)@2150..2151 " "
        Syntax(DocScope)@2151..2170
          Token(TkDocDetailArgMarkup)@2151..2152 ":"
          Token(TkDocDetailArg)@2152..2161 "Field\\: 4"
          Token(TkDocDetailArgMarkup)@2161..2162 ":"
          Token(TkDocDetail)@2162..2163 " "
          Token(TkDocDetail)@2163..2170 "Content"
        Token(TkEndOfLine)@2170..2171 "\n"
        Token(TkNormalStart)@2171..2174 "---"
        Token(TkEndOfLine)@2174..2175 "\n"
        Token(TkNormalStart)@2175..2178 "---"
        Token(TkEndOfLine)@2178..2179 "\n"
        Token(TkNormalStart)@2179..2182 "---"
        Token(TkWhitespace)@2182..2183 " "
        Syntax(DocScope)@2183..2208
          Token(TkDocDetail)@2183..2193 "Line block"
          Token(TkEndOfLine)@2193..2194 "\n"
          Token(TkNormalStart)@2194..2197 "---"
          Token(TkWhitespace)@2197..2198 " "
          Token(TkDocDetailMarkup)@2198..2208 "----------"
        Token(TkEndOfLine)@2208..2209 "\n"
        Token(TkNormalStart)@2209..2212 "---"
        Token(TkEndOfLine)@2212..2213 "\n"
        Token(TkNormalStart)@2213..2216 "---"
        Token(TkWhitespace)@2216..2217 " "
        Syntax(DocScope)@2217..2257
          Token(TkDocDetailMarkup)@2217..2218 "|"
          Token(TkDocDetail)@2218..2257 " Lend us a couple of  ..."
        Token(TkEndOfLine)@2257..2258 "\n"
        Token(TkNormalStart)@2258..2261 "---"
        Token(TkWhitespace)@2261..2262 " "
        Syntax(DocScope)@2262..2285
          Token(TkDocDetailMarkup)@2262..2263 "|"
          Token(TkDocDetail)@2263..2285 " I'm absolutely skint."
        Token(TkEndOfLine)@2285..2286 "\n"
        Token(TkNormalStart)@2286..2289 "---"
        Token(TkWhitespace)@2289..2290 " "
        Syntax(DocScope)@2290..2374
          Token(TkDocDetailMarkup)@2290..2291 "|"
          Token(TkDocDetail)@2291..2347 " But I'm expecting a  ..."
          Token(TkEndOfLine)@2347..2348 "\n"
          Token(TkNormalStart)@2348..2351 "---"
          Token(TkWhitespace)@2351..2352 " "
          Token(TkDocDetail)@2352..2354 "  "
          Token(TkDocDetail)@2354..2374 "as soon as it comes."
        Token(TkEndOfLine)@2374..2375 "\n"
        Token(TkNormalStart)@2375..2378 "---"
        Token(TkWhitespace)@2378..2379 " "
        Syntax(DocScope)@2379..2392
          Token(TkDocDetailMarkup)@2379..2380 "|"
          Token(TkDocDetail)@2380..2392 " Love, Ewan."
        Token(TkEndOfLine)@2392..2393 "\n"
        Token(TkNormalStart)@2393..2396 "---"
        Token(TkEndOfLine)@2396..2397 "\n"
        Token(TkNormalStart)@2397..2400 "---"
        Token(TkWhitespace)@2400..2401 " "
        Syntax(DocScope)@2401..2422
          Token(TkDocDetail)@2401..2403 "  "
          Token(TkDocDetail)@2403..2422 "Not a continuation."
        Token(TkEndOfLine)@2422..2423 "\n"
        Token(TkNormalStart)@2423..2426 "---"
        Token(TkEndOfLine)@2426..2427 "\n"
        Token(TkNormalStart)@2427..2430 "---"
        Token(TkEndOfLine)@2430..2431 "\n"
        Token(TkNormalStart)@2431..2434 "---"
        Token(TkWhitespace)@2434..2435 " "
        Syntax(DocScope)@2435..2464
          Token(TkDocDetail)@2435..2447 "Block quotes"
          Token(TkEndOfLine)@2447..2448 "\n"
          Token(TkNormalStart)@2448..2451 "---"
          Token(TkWhitespace)@2451..2452 " "
          Token(TkDocDetailMarkup)@2452..2464 "------------"
        Token(TkEndOfLine)@2464..2465 "\n"
        Token(TkNormalStart)@2465..2468 "---"
        Token(TkEndOfLine)@2468..2469 "\n"
        Token(TkNormalStart)@2469..2472 "---"
        Token(TkWhitespace)@2472..2473 " "
        Token(TkDocDetail)@2473..2530 "This is an ordinary p ..."
        Token(TkEndOfLine)@2530..2531 "\n"
        Token(TkNormalStart)@2531..2534 "---"
        Token(TkEndOfLine)@2534..2535 "\n"
        Token(TkNormalStart)@2535..2538 "---"
        Token(TkWhitespace)@2538..2539 " "
        Syntax(DocScope)@2539..2628
          Token(TkDocDetail)@2539..2543 "    "
          Token(TkDocDetail)@2543..2597 "\"It is my business to ..."
          Token(TkEndOfLine)@2597..2598 "\n"
          Token(TkNormalStart)@2598..2601 "---"
          Token(TkEndOfLine)@2601..2602 "\n"
          Token(TkNormalStart)@2602..2605 "---"
          Token(TkWhitespace)@2605..2606 " "
          Token(TkDocDetail)@2606..2610 "    "
          Token(TkDocDetail)@2610..2628 "-- Sherlock Holmes"
        Token(TkEndOfLine)@2628..2629 "\n"
        Token(TkNormalStart)@2629..2632 "---"
        Token(TkEndOfLine)@2632..2633 "\n"
        Token(TkNormalStart)@2633..2636 "---"
        Token(TkWhitespace)@2636..2637 " "
        Syntax(DocScope)@2637..2649
          Token(TkDocDetailMarkup)@2637..2638 "*"
          Token(TkDocDetail)@2638..2649 " List item."
        Token(TkEndOfLine)@2649..2650 "\n"
        Token(TkNormalStart)@2650..2653 "---"
        Token(TkEndOfLine)@2653..2654 "\n"
        Token(TkNormalStart)@2654..2657 "---"
        Token(TkWhitespace)@2657..2658 " "
        Token(TkDocDetail)@2658..2660 ".."
        Token(TkEndOfLine)@2660..2661 "\n"
        Token(TkNormalStart)@2661..2664 "---"
        Token(TkEndOfLine)@2664..2665 "\n"
        Token(TkNormalStart)@2665..2668 "---"
        Token(TkWhitespace)@2668..2669 " "
        Syntax(DocScope)@2669..2687
          Token(TkDocDetail)@2669..2673 "    "
          Token(TkDocDetail)@2673..2687 "Block quote 3."
        Token(TkEndOfLine)@2687..2688 "\n"
        Token(TkNormalStart)@2688..2691 "---"
        Token(TkEndOfLine)@2691..2692 "\n"
        Token(TkNormalStart)@2692..2695 "---"
        Token(TkEndOfLine)@2695..2696 "\n"
        Token(TkNormalStart)@2696..2699 "---"
        Token(TkWhitespace)@2699..2700 " "
        Syntax(DocScope)@2700..2733
          Token(TkDocDetail)@2700..2714 "Doctest blocks"
          Token(TkEndOfLine)@2714..2715 "\n"
          Token(TkNormalStart)@2715..2718 "---"
          Token(TkWhitespace)@2718..2719 " "
          Token(TkDocDetailMarkup)@2719..2733 "--------------"
        Token(TkEndOfLine)@2733..2734 "\n"
        Token(TkNormalStart)@2734..2737 "---"
        Token(TkEndOfLine)@2737..2738 "\n"
        Token(TkNormalStart)@2738..2741 "---"
        Token(TkWhitespace)@2741..2742 " "
        Syntax(DocScope)@2742..2860
          Token(TkDocDetailMarkup)@2742..2745 ">>>"
          Token(TkDocDetail)@2745..2746 " "
          Token(TkDocDetailCode)@2746..2778 "print('this is a Doct ..."
          Token(TkEndOfLine)@2778..2779 "\n"
          Token(TkNormalStart)@2779..2782 "---"
          Token(TkWhitespace)@2782..2783 " "
          Token(TkDocDetailCode)@2783..2806 "this is a Doctest block"
          Token(TkEndOfLine)@2806..2807 "\n"
          Token(TkNormalStart)@2807..2810 "---"
          Token(TkWhitespace)@2810..2811 " "
          Token(TkDocDetailMarkup)@2811..2814 ">>>"
          Token(TkDocDetail)@2814..2815 " "
          Token(TkDocDetailCode)@2815..2831 "print('foo bar')"
          Token(TkEndOfLine)@2831..2832 "\n"
          Token(TkNormalStart)@2832..2835 "---"
          Token(TkWhitespace)@2835..2836 " "
          Token(TkDocDetailMarkup)@2836..2839 "..."
          Token(TkDocDetail)@2839..2840 " "
          Token(TkDocDetailCode)@2840..2844 "None"
          Token(TkEndOfLine)@2844..2845 "\n"
          Token(TkNormalStart)@2845..2848 "---"
          Token(TkWhitespace)@2848..2849 " "
          Token(TkDocDetailCode)@2849..2856 "foo bar"
          Token(TkEndOfLine)@2856..2857 "\n"
          Token(TkNormalStart)@2857..2860 "---"
        Token(TkEndOfLine)@2860..2861 "\n"
        Token(TkNormalStart)@2861..2864 "---"
        Token(TkWhitespace)@2864..2865 " "
        Syntax(DocScope)@2865..2900
          Token(TkDocDetail)@2865..2880 "Explicit markup"
          Token(TkEndOfLine)@2880..2881 "\n"
          Token(TkNormalStart)@2881..2884 "---"
          Token(TkWhitespace)@2884..2885 " "
          Token(TkDocDetailMarkup)@2885..2900 "---------------"
        Token(TkEndOfLine)@2900..2901 "\n"
        Token(TkNormalStart)@2901..2904 "---"
        Token(TkEndOfLine)@2904..2905 "\n"
        Token(TkNormalStart)@2905..2908 "---"
        Token(TkWhitespace)@2908..2909 " "
        Syntax(DocScope)@2909..2948
          Token(TkDocDetailMarkup)@2909..2911 ".."
          Token(TkDocDetail)@2911..2912 " "
          Token(TkDocDetail)@2912..2919 "Comment"
          Token(TkEndOfLine)@2919..2920 "\n"
          Token(TkNormalStart)@2920..2923 "---"
          Token(TkEndOfLine)@2923..2924 "\n"
          Token(TkNormalStart)@2924..2927 "---"
          Token(TkWhitespace)@2927..2928 " "
          Token(TkDocDetail)@2928..2931 "   "
          Token(TkDocDetail)@2931..2948 "With continuation"
        Token(TkEndOfLine)@2948..2949 "\n"
        Token(TkNormalStart)@2949..2952 "---"
        Token(TkEndOfLine)@2952..2953 "\n"
        Token(TkNormalStart)@2953..2956 "---"
        Token(TkWhitespace)@2956..2957 " "
        Syntax(DocScope)@2957..2972
          Token(TkDocDetailMarkup)@2957..2959 ".."
          Token(TkDocDetail)@2959..2960 " "
          Token(TkDocDetailArgMarkup)@2960..2961 "["
          Token(TkDocDetailArg)@2961..2962 "1"
          Token(TkDocDetailArgMarkup)@2962..2963 "]"
          Token(TkDocDetail)@2963..2964 " "
          Token(TkDocDetail)@2964..2972 "Footnote"
        Token(TkEndOfLine)@2972..2973 "\n"
        Token(TkNormalStart)@2973..2976 "---"
        Token(TkEndOfLine)@2976..2977 "\n"
        Token(TkNormalStart)@2977..2980 "---"
        Token(TkWhitespace)@2980..2981 " "
        Syntax(DocScope)@2981..3060
          Token(TkDocDetailMarkup)@2981..2983 ".."
          Token(TkDocDetail)@2983..2984 " "
          Token(TkDocDetailArgMarkup)@2984..2985 "["
          Token(TkDocDetailArg)@2985..2986 "#"
          Token(TkDocDetailArgMarkup)@2986..2987 "]"
          Token(TkDocDetail)@2987..2988 " "
          Token(TkDocDetail)@2988..3001 "Long footnote"
          Token(TkEndOfLine)@3001..3002 "\n"
          Token(TkNormalStart)@3002..3005 "---"
          Token(TkWhitespace)@3005..3006 " "
          Token(TkDocDetail)@3006..3009 "   "
          Token(TkDocDetailCode)@3009..3027 "with continuation."
          Token(TkEndOfLine)@3027..3028 "\n"
          Token(TkNormalStart)@3028..3031 "---"
          Token(TkEndOfLine)@3031..3032 "\n"
          Token(TkNormalStart)@3032..3035 "---"
          Token(TkWhitespace)@3035..3036 " "
          Token(TkDocDetail)@3036..3039 "   "
          Syntax(DocScope)@3039..3060
            Token(TkDocDetailMarkup)@3039..3040 "-"
            Token(TkDocDetail)@3040..3060 " And nested content."
        Token(TkEndOfLine)@3060..3061 "\n"
        Token(TkNormalStart)@3061..3064 "---"
        Token(TkEndOfLine)@3064..3065 "\n"
        Token(TkNormalStart)@3065..3068 "---"
        Token(TkWhitespace)@3068..3069 " "
        Syntax(DocScope)@3069..3080
          Token(TkDocDetailMarkup)@3069..3071 ".."
          Token(TkDocDetail)@3071..3072 " "
          Token(TkDocDetailArgMarkup)@3072..3073 "_"
          Token(TkDocDetailArg)@3073..3079 "target"
          Token(TkDocDetailArgMarkup)@3079..3080 ":"
        Token(TkEndOfLine)@3080..3081 "\n"
        Token(TkNormalStart)@3081..3084 "---"
        Token(TkEndOfLine)@3084..3085 "\n"
        Token(TkNormalStart)@3085..3088 "---"
        Token(TkWhitespace)@3088..3089 " "
        Syntax(DocScope)@3089..3119
          Token(TkDocDetailMarkup)@3089..3091 ".."
          Token(TkDocDetail)@3091..3092 " "
          Token(TkDocDetailArgMarkup)@3092..3093 "_"
          Token(TkDocDetailArg)@3093..3107 "hyperlink-name"
          Token(TkDocDetailArgMarkup)@3107..3108 ":"
          Token(TkDocDetail)@3108..3109 " "
          Token(TkDocDetailInlineLink)@3109..3119 "link-block"
        Token(TkEndOfLine)@3119..3120 "\n"
        Token(TkNormalStart)@3120..3123 "---"
        Token(TkEndOfLine)@3123..3124 "\n"
        Token(TkNormalStart)@3124..3127 "---"
        Token(TkWhitespace)@3127..3128 " "
        Syntax(DocScope)@3128..3215
          Token(TkDocDetailMarkup)@3128..3130 ".."
          Token(TkDocDetail)@3130..3131 " "
          Token(TkDocDetailArgMarkup)@3131..3132 "_"
          Token(TkDocDetailArg)@3132..3182 "`FAQTS: Computers: Pr ..."
          Token(TkDocDetailArgMarkup)@3182..3183 ":"
          Token(TkEndOfLine)@3183..3184 "\n"
          Token(TkNormalStart)@3184..3187 "---"
          Token(TkWhitespace)@3187..3188 " "
          Token(TkDocDetail)@3188..3191 "   "
          Token(TkDocDetailInlineLink)@3191..3215 "http://python.faqts.com/"
        Token(TkEndOfLine)@3215..3216 "\n"
        Token(TkNormalStart)@3216..3219 "---"
        Token(TkEndOfLine)@3219..3220 "\n"
        Token(TkNormalStart)@3220..3223 "---"
        Token(TkWhitespace)@3223..3224 " "
        Syntax(DocScope)@3224..3300
          Token(TkDocDetailMarkup)@3224..3226 ".."
          Token(TkDocDetail)@3226..3227 " "
          Token(TkDocDetailArgMarkup)@3227..3228 "_"
          Token(TkDocDetailArg)@3228..3242 "entirely-below"
          Token(TkDocDetailArgMarkup)@3242..3243 ":"
          Token(TkEndOfLine)@3243..3244 "\n"
          Token(TkNormalStart)@3244..3247 "---"
          Token(TkWhitespace)@3247..3248 " "
          Token(TkDocDetail)@3248..3251 "   "
          Token(TkDocDetailInlineLink)@3251..3268 "https://docutils."
          Token(TkEndOfLine)@3268..3269 "\n"
          Token(TkNormalStart)@3269..3272 "---"
          Token(TkWhitespace)@3272..3273 " "
          Token(TkDocDetail)@3273..3276 "   "
          Token(TkDocDetailInlineLink)@3276..3300 "sourceforge.net/rst.html"
        Token(TkEndOfLine)@3300..3301 "\n"
        Token(TkNormalStart)@3301..3304 "---"
        Token(TkEndOfLine)@3304..3305 "\n"
        Token(TkNormalStart)@3305..3308 "---"
        Token(TkWhitespace)@3308..3309 " "
        Syntax(DocScope)@3309..3342
          Token(TkDocDetailMarkup)@3309..3311 ".."
          Token(TkDocDetail)@3311..3312 " "
          Token(TkDocDetailArgMarkup)@3312..3313 "_"
          Token(TkDocDetailArg)@3313..3341 "Chapter One\\: \"Tadpol ..."
          Token(TkDocDetailArgMarkup)@3341..3342 ":"
        Token(TkEndOfLine)@3342..3343 "\n"
        Token(TkNormalStart)@3343..3346 "---"
        Token(TkEndOfLine)@3346..3347 "\n"
        Token(TkNormalStart)@3347..3350 "---"
        Token(TkWhitespace)@3350..3351 " "
        Token(TkDocDetail)@3351..3379 "It's not easy being g ..."
        Token(TkEndOfLine)@3379..3380 "\n"
        Token(TkNormalStart)@3380..3383 "---"
        Token(TkEndOfLine)@3383..3384 "\n"
        Token(TkNormalStart)@3384..3387 "---"
        Token(TkWhitespace)@3387..3388 " "
        Syntax(DocScope)@3388..3402
          Token(TkDocDetailMarkup)@3388..3390 ".."
          Token(TkDocDetail)@3390..3391 " "
          Token(TkDocDetailArg)@3391..3400 "directive"
          Token(TkDocDetailArgMarkup)@3400..3402 "::"
        Token(TkEndOfLine)@3402..3403 "\n"
        Token(TkNormalStart)@3403..3406 "---"
        Token(TkEndOfLine)@3406..3407 "\n"
        Token(TkNormalStart)@3407..3410 "---"
        Token(TkWhitespace)@3410..3411 " "
        Syntax(DocScope)@3411..3515
          Token(TkDocDetailMarkup)@3411..3413 ".."
          Token(TkDocDetail)@3413..3414 " "
          Token(TkDocDetailArg)@3414..3423 "directive"
          Token(TkDocDetailArgMarkup)@3423..3425 "::"
          Token(TkDocDetail)@3425..3426 " "
          Token(TkDocDetailCode)@3426..3430 "args"
          Token(TkEndOfLine)@3430..3431 "\n"
          Token(TkNormalStart)@3431..3434 "---"
          Token(TkWhitespace)@3434..3435 " "
          Token(TkDocDetail)@3435..3438 "   "
          Token(TkDocDetailArgMarkup)@3438..3439 ":"
          Token(TkDocDetailArg)@3439..3444 "param"
          Token(TkDocDetailArgMarkup)@3444..3445 ":"
          Token(TkDocDetail)@3445..3446 " "
          Token(TkDocDetailCode)@3446..3451 "value"
          Token(TkEndOfLine)@3451..3452 "\n"
          Token(TkNormalStart)@3452..3455 "---"
          Token(TkWhitespace)@3455..3456 " "
          Token(TkDocDetail)@3456..3459 "   "
          Token(TkDocDetailCode)@3459..3466 "param 2"
          Token(TkEndOfLine)@3466..3467 "\n"
          Token(TkNormalStart)@3467..3470 "---"
          Token(TkEndOfLine)@3470..3471 "\n"
          Token(TkNormalStart)@3471..3474 "---"
          Token(TkWhitespace)@3474..3475 " "
          Token(TkDocDetail)@3475..3478 "   "
          Token(TkDocDetail)@3478..3486 "Content."
          Token(TkEndOfLine)@3486..3487 "\n"
          Token(TkNormalStart)@3487..3490 "---"
          Token(TkEndOfLine)@3490..3491 "\n"
          Token(TkNormalStart)@3491..3494 "---"
          Token(TkWhitespace)@3494..3495 " "
          Token(TkDocDetail)@3495..3498 "   "
          Syntax(DocScope)@3498..3515
            Token(TkDocDetailMarkup)@3498..3499 "-"
            Token(TkDocDetail)@3499..3515 " Nested content."
        Token(TkEndOfLine)@3515..3516 "\n"
        Token(TkNormalStart)@3516..3519 "---"
        Token(TkEndOfLine)@3519..3520 "\n"
        Token(TkNormalStart)@3520..3523 "---"
        Token(TkWhitespace)@3523..3524 " "
        Syntax(DocScope)@3524..3585
          Token(TkDocDetailMarkup)@3524..3526 ".."
          Token(TkDocDetail)@3526..3527 " "
          Token(TkDocDetailArg)@3527..3537 "code-block"
          Token(TkDocDetailArgMarkup)@3537..3539 "::"
          Token(TkDocDetail)@3539..3540 " "
          Token(TkDocDetailCode)@3540..3543 "lua"
          Token(TkEndOfLine)@3543..3544 "\n"
          Token(TkNormalStart)@3544..3547 "---"
          Token(TkWhitespace)@3547..3548 " "
          Token(TkDocDetail)@3548..3551 "   "
          Token(TkDocDetailArgMarkup)@3551..3552 ":"
          Token(TkDocDetailArg)@3552..3559 "linenos"
          Token(TkDocDetailArgMarkup)@3559..3560 ":"
          Token(TkEndOfLine)@3560..3561 "\n"
          Token(TkNormalStart)@3561..3564 "---"
          Token(TkEndOfLine)@3564..3565 "\n"
          Token(TkNormalStart)@3565..3568 "---"
          Token(TkWhitespace)@3568..3569 " "
          Token(TkDocDetail)@3569..3572 "   "
          Token(TkDocDetailCode)@3572..3585 "print(\"foo!\")"
        Token(TkEndOfLine)@3585..3586 "\n"
        Token(TkNormalStart)@3586..3589 "---"
        Token(TkEndOfLine)@3589..3590 "\n"
        Token(TkNormalStart)@3590..3593 "---"
        Token(TkEndOfLine)@3593..3594 "\n"
        Token(TkNormalStart)@3594..3597 "---"
        Token(TkWhitespace)@3597..3598 " "
        Syntax(DocScope)@3598..3653
          Token(TkDocDetail)@3598..3623 "Implicit hyperlink ta ..."
          Token(TkEndOfLine)@3623..3624 "\n"
          Token(TkNormalStart)@3624..3627 "---"
          Token(TkWhitespace)@3627..3628 " "
          Token(TkDocDetailMarkup)@3628..3653 "--------------------- ..."
        Token(TkEndOfLine)@3653..3654 "\n"
        Token(TkNormalStart)@3654..3657 "---"
        Token(TkEndOfLine)@3657..3658 "\n"
        Token(TkNormalStart)@3658..3661 "---"
        Token(TkWhitespace)@3661..3662 " "
        Syntax(DocScope)@3662..3702
          Token(TkDocDetailInlineLink)@3662..3664 "__"
          Token(TkDocDetail)@3664..3665 " "
          Token(TkDocDetailInlineLink)@3665..3702 "anonymous-hyperlink-t ..."
        Token(TkEndOfLine)@3702..3703 "\n"
        Token(TkNormalStart)@3703..3706 "---"
        Token(TkEndOfLine)@3706..3707 "\n"
        Token(TkNormalStart)@3707..3710 "---"
        Token(TkEndOfLine)@3710..3711 "\n"
        Token(TkNormalStart)@3711..3714 "---"
        Token(TkWhitespace)@3714..3715 " "
        Syntax(DocScope)@3715..3748
          Token(TkDocDetail)@3715..3729 "Literal blocks"
          Token(TkEndOfLine)@3729..3730 "\n"
          Token(TkNormalStart)@3730..3733 "---"
          Token(TkWhitespace)@3733..3734 " "
          Token(TkDocDetailMarkup)@3734..3748 "--------------"
        Token(TkEndOfLine)@3748..3749 "\n"
        Token(TkNormalStart)@3749..3752 "---"
        Token(TkEndOfLine)@3752..3753 "\n"
        Token(TkNormalStart)@3753..3756 "---"
        Token(TkWhitespace)@3756..3757 " "
        Token(TkDocDetail)@3757..3759 "::"
        Token(TkEndOfLine)@3759..3760 "\n"
        Token(TkNormalStart)@3760..3763 "---"
        Token(TkEndOfLine)@3763..3764 "\n"
        Token(TkNormalStart)@3764..3767 "---"
        Token(TkWhitespace)@3767..3768 " "
        Token(TkDocDetail)@3768..3770 "  "
        Syntax(DocScope)@3770..3783
          Token(TkDocDetailCode)@3770..3783 "This is code!"
        Token(TkEndOfLine)@3783..3784 "\n"
        Token(TkNormalStart)@3784..3787 "---"
        Token(TkEndOfLine)@3787..3788 "\n"
        Token(TkNormalStart)@3788..3791 "---"
        Token(TkWhitespace)@3791..3792 " "
        Token(TkDocDetail)@3792..3803 "Some code::"
        Token(TkEndOfLine)@3803..3804 "\n"
        Token(TkNormalStart)@3804..3807 "---"
        Token(TkEndOfLine)@3807..3808 "\n"
        Token(TkNormalStart)@3808..3811 "---"
        Token(TkWhitespace)@3811..3812 " "
        Token(TkDocDetail)@3812..3814 "  "
        Syntax(DocScope)@3814..3845
          Token(TkDocDetailCode)@3814..3821 "Code..."
          Token(TkEndOfLine)@3821..3822 "\n"
          Token(TkNormalStart)@3822..3825 "---"
          Token(TkEndOfLine)@3825..3826 "\n"
          Token(TkNormalStart)@3826..3829 "---"
          Token(TkWhitespace)@3829..3830 " "
          Token(TkDocDetail)@3830..3832 "  "
          Token(TkDocDetailCode)@3832..3845 "...continues."
        Token(TkEndOfLine)@3845..3846 "\n"
        Token(TkNormalStart)@3846..3849 "---"
        Token(TkEndOfLine)@3849..3850 "\n"
        Token(TkNormalStart)@3850..3853 "---"
        Token(TkWhitespace)@3853..3854 " "
        Token(TkDocDetail)@3854..3856 "::"
        Token(TkEndOfLine)@3856..3857 "\n"
        Token(TkNormalStart)@3857..3860 "---"
        Token(TkEndOfLine)@3860..3861 "\n"
        Token(TkNormalStart)@3861..3864 "---"
        Token(TkWhitespace)@3864..3865 " "
        Token(TkDocDetailMarkup)@3865..3866 "-"
        Syntax(DocScope)@3866..3908
          Token(TkDocDetailCode)@3866..3885 " This is also code!"
          Token(TkEndOfLine)@3885..3886 "\n"
          Token(TkNormalStart)@3886..3889 "---"
          Token(TkWhitespace)@3889..3890 " "
          Token(TkDocDetailMarkup)@3890..3891 "-"
          Token(TkEndOfLine)@3891..3892 "\n"
          Token(TkNormalStart)@3892..3895 "---"
          Token(TkWhitespace)@3895..3896 " "
          Token(TkDocDetailMarkup)@3896..3897 "-"
          Token(TkDocDetailCode)@3897..3908 " Continues."
        Token(TkEndOfLine)@3908..3909 "\n"
        Token(TkNormalStart)@3909..3912 "---"
        Token(TkEndOfLine)@3912..3913 "\n"
        Token(TkNormalStart)@3913..3916 "---"
        Token(TkWhitespace)@3916..3917 " "
        Syntax(DocScope)@3917..3936
          Token(TkDocDetailMarkup)@3917..3918 "-"
          Token(TkDocDetail)@3918..3936 " And this is list."
        Token(TkEndOfLine)@3936..3937 "\n"
        Token(TkNormalStart)@3937..3940 "---"
    Token(TkEndOfLine)@3940..3941 "\n"
"###;

        let tree = LuaParser::parse(
            code,
            ParserConfig::with_desc_parser_type(DescParserType::Rst {
                primary_domain: None,
                default_role: None,
            }),
        );
        let result = format!("{:#?}", tree.get_red_root()).trim().to_string();
        let expected = expected.trim().to_string();
        assert_eq!(result, expected);
    }
}
