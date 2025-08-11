use crate::{DescItem, DescItemKind};
use emmylua_parser::{
    LuaAstNode, LuaDocDescription, LuaKind, LuaSyntaxElement, LuaTokenKind, Reader, SourceRange,
};
use rowan::Direction;
use std::cmp::min;
use unicode_general_category::{GeneralCategory, get_general_category};

pub fn is_ws(c: char) -> bool {
    matches!(c, ' ' | '\t')
}

pub fn desc_to_lines(
    text: &str,
    desc: LuaDocDescription,
    cursor_position: Option<usize>,
) -> Vec<SourceRange> {
    let mut lines = Vec::new();
    let mut line = SourceRange::EMPTY;
    let mut skip_current_line_content = false;
    let mut seen_doc_comments = false;

    let mut handle_token = |token: &LuaSyntaxElement| {
        let LuaSyntaxElement::Token(token) = token else {
            return;
        };

        match token.kind() {
            LuaKind::Token(LuaTokenKind::TkDocDetail) => {
                if skip_current_line_content {
                    return;
                }

                let range: SourceRange = token.text_range().into();
                if line.end_offset() == range.start_offset {
                    line.length += range.length;
                } else {
                    if line != SourceRange::EMPTY {
                        seen_doc_comments |= !text[line.start_offset..line.end_offset()]
                            .chars()
                            .all(|c| c == '-');
                        lines.push(line);
                    }
                    line = range;
                }
            }
            LuaKind::Token(LuaTokenKind::TkEndOfLine) => {
                seen_doc_comments |= !text[line.start_offset..line.end_offset()]
                    .chars()
                    .all(|c| c == '-');
                lines.push(line);
                line = SourceRange::EMPTY;
                skip_current_line_content = false
            }
            LuaKind::Token(LuaTokenKind::TkNormalStart | LuaTokenKind::TkDocContinue) => {
                let leading_marks = token.text().chars().take_while(|c| *c == '-').count();

                // Skip content for lines that don't start with three dashes.
                // Parser will see them as empty lines.
                //
                // Note: `leading_marks` can't be longer than three dashes.
                // If comment starts with four or more dashes, the first three
                // will end up in `TkNormalStart`, and the rest will be in `TkDocDetail`.
                skip_current_line_content = leading_marks != 3;

                if skip_current_line_content {
                    line = SourceRange::new(token.text_range().start().into(), 0);
                } else {
                    line = token.text_range().into();
                    line.start_offset += leading_marks;
                    line.length -= leading_marks;
                }
            }
            _ => {}
        }
    };

    let prev_token = desc
        .syntax()
        .siblings_with_tokens(Direction::Prev)
        .skip(1)
        .skip_while(|tk| tk.kind() == LuaTokenKind::TkWhitespace.into())
        .next();
    if let Some(prev_token) = prev_token {
        if prev_token.kind() == LuaTokenKind::TkNormalStart.into() {
            handle_token(&prev_token);
        }
    }
    for child in desc.syntax().children_with_tokens() {
        handle_token(&child);
    }

    if !line.is_empty() {
        seen_doc_comments |= !text[line.start_offset..line.end_offset()]
            .trim_end()
            .chars()
            .all(|c| c == '-');
        lines.push(line);
    }

    if !seen_doc_comments {
        // Comment block consists entirely of lines that only start with
        // two dashes, or lines that consist only of dashes and nothing else.
        return Vec::new();
    }

    // Strip lines that consist entirely of dashes from start and end
    // of the comment block.
    //
    // This handles cases where comment is adorned with long dashed lines:
    //
    // ```
    // ---------------
    // --- Comment ---
    // ---------------
    // ```
    let mut new_start = 0;
    for line in lines.iter() {
        let line_text = &text[line.start_offset..line.end_offset()];
        if line_text.trim_end().chars().all(|c| c == '-') {
            new_start += 1;
        } else {
            break;
        }
    }
    let mut new_end = lines.len();
    for line in lines[new_start..].iter().rev() {
        let line_text = &text[line.start_offset..line.end_offset()];
        if line_text.trim_end().chars().all(|c| c == '-') {
            new_end -= 1;
        } else {
            break;
        }
    }
    if new_start > 0 || new_end < lines.len() {
        lines = lines.drain(new_start..new_end).collect();
    }

    // Find and remove comment indentation.
    let mut common_indent = None;
    for line in lines.iter() {
        let text = &text[line.start_offset..line.end_offset()];

        if is_blank(text) {
            continue;
        }

        let indent = text.chars().take_while(|c| is_ws(*c)).count();
        common_indent = match common_indent {
            None => Some(indent),
            Some(common_indent) => Some(min(common_indent, indent)),
        };
    }

    let common_indent = common_indent.unwrap_or_default();
    if common_indent > 0 {
        for line in lines.iter_mut() {
            if line.length >= common_indent {
                line.start_offset += common_indent;
                line.length -= common_indent;
            }
        }
    }

    // Don't parse lines past user's cursor when calculating
    // Go To Definition or Completion. We handle this here so that
    // we don't affect common indent and other logic.
    if let Some(cursor_position) = cursor_position {
        for (i, line) in lines.iter().enumerate() {
            let start: usize = line.start_offset.into();
            if start > cursor_position {
                lines.truncate(i);
                break;
            }
        }
    }

    lines
}

pub trait ResultContainer {
    fn results(&self) -> &Vec<DescItem>;

    fn results_mut(&mut self) -> &mut Vec<DescItem>;

    fn cursor_position(&self) -> Option<usize>;

    fn emit_range(&mut self, range: SourceRange, kind: DescItemKind) {
        let should_emit = if let Some(cursor_position) = self.cursor_position() {
            kind == DescItemKind::Ref && range.contains_inclusive(cursor_position)
        } else {
            !range.is_empty()
        };

        if should_emit {
            let Some(last) = self.results_mut().last_mut() else {
                self.results_mut().push(DescItem {
                    range: range.into(),
                    kind,
                });
                return;
            };

            let end: usize = last.range.end().into();
            if last.kind == kind && end == range.start_offset {
                last.range = last.range.cover(range.into());
            } else {
                self.results_mut().push(DescItem {
                    range: range.into(),
                    kind,
                });
            }
        }
    }

    fn emit(&mut self, reader: &mut Reader, kind: DescItemKind) {
        self.emit_range(reader.current_range(), kind);
        reader.reset_buff();
    }
}

pub struct BacktrackPoint<'a> {
    prev_reader: Reader<'a>,
    prev_pos: usize,
}

impl<'a> BacktrackPoint<'a> {
    pub fn new<C: ResultContainer>(c: &mut C, reader: &mut Reader<'a>) -> Self {
        Self {
            prev_reader: reader.clone(),
            prev_pos: c.results().len(),
        }
    }

    pub fn commit<C: ResultContainer>(self, c: &mut C, reader: &mut Reader<'a>) {
        let (_c, _reader) = (c, reader); // We don't actually do anything.
        std::mem::forget(self);
    }

    pub fn rollback<C: ResultContainer>(self, c: &mut C, reader: &mut Reader<'a>) {
        *reader = self.prev_reader.clone();
        c.results_mut().truncate(self.prev_pos);
        std::mem::forget(self);
    }
}

impl<'a> Drop for BacktrackPoint<'a> {
    fn drop(&mut self) {
        panic!("backtrack point should be committed or rolled back");
    }
}

pub fn is_punct(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_punctuation()
    } else {
        matches!(
            get_general_category(c),
            // P | S
            GeneralCategory::ClosePunctuation
                | GeneralCategory::ConnectorPunctuation
                | GeneralCategory::DashPunctuation
                | GeneralCategory::FinalPunctuation
                | GeneralCategory::InitialPunctuation
                | GeneralCategory::OpenPunctuation
                | GeneralCategory::OtherPunctuation
                | GeneralCategory::CurrencySymbol
                | GeneralCategory::MathSymbol
                | GeneralCategory::ModifierSymbol
                | GeneralCategory::OtherSymbol
        )
    }
}

pub fn is_opening_quote(c: char) -> bool {
    if c.is_ascii() {
        matches!(c, '\'' | '"' | '<' | '(' | '[' | '{')
    } else {
        matches!(
            get_general_category(c),
            GeneralCategory::OpenPunctuation
                | GeneralCategory::InitialPunctuation
                | GeneralCategory::FinalPunctuation
        )
    }
}

pub fn is_closing_quote(c: char) -> bool {
    if c.is_ascii() {
        matches!(c, '\'' | '"' | '>' | ')' | ']' | '}')
    } else {
        matches!(
            get_general_category(c),
            GeneralCategory::ClosePunctuation
                | GeneralCategory::InitialPunctuation
                | GeneralCategory::FinalPunctuation
        )
    }
}

pub fn is_quote_match(l: char, r: char) -> bool {
    if !l.is_ascii() || !r.is_ascii() {
        return true;
    }

    match (l, r) {
        ('\'', '\'') => true,
        ('"', '"') => true,
        ('<', '>') => true,
        ('(', ')') => true,
        ('[', ']') => true,
        ('{', '}') => true,
        _ => false,
    }
}

pub fn is_blank(s: &str) -> bool {
    s.is_empty() || s.chars().all(|c| c.is_ascii_whitespace())
}

pub fn is_code_directive(name: &str) -> bool {
    matches!(
        name,
        "code-block" | "sourcecode" | "code" | "literalinclude" | "math"
    )
}

pub fn is_lua_role(name: &str) -> bool {
    matches!(
        name,
        "func"
            | "data"
            | "const"
            | "class"
            | "alias"
            | "enum"
            | "meth"
            | "attr"
            | "mod"
            | "obj"
            | "lua"
    )
}

pub fn sort_result(items: &mut Vec<DescItem>) {
    items.sort_by_key(|r| {
        let len: usize = r.range.len().into();

        (
            r.range.start(),               // Sort by start position,
            usize::MAX - len,              // longer tokens first,
            r.kind != DescItemKind::Scope, // scopes go first.
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use emmylua_parser::{LuaParser, ParserConfig};
    use googletest::prelude::*;

    fn get_desc(code: &str) -> LuaDocDescription {
        LuaParser::parse(code, ParserConfig::default())
            .get_chunk_node()
            .descendants::<LuaDocDescription>()
            .next()
            .unwrap()
    }

    fn run_desc_to_lines(code: &str) -> Vec<&str> {
        let desc = get_desc(code);
        let lines = desc_to_lines(code, desc, None);
        lines
            .iter()
            .map(|range| &code[range.start_offset..range.end_offset()])
            .collect()
    }

    #[gtest]
    fn test_desc_to_lines() {
        expect_eq!(
            run_desc_to_lines(
                r#"
                -- Desc
            "#
            ),
            vec![""; 0]
        );

        expect_eq!(
            run_desc_to_lines(
                r#"
                ----------
                -- Desc --
                ----------
            "#
            ),
            vec![""; 0]
        );

        expect_eq!(
            run_desc_to_lines(
                r#"
                ----------
                -- Desc --
                ----------
                -- Desc --
                ----------
            "#
            ),
            vec![""; 0]
        );

        expect_eq!(
            run_desc_to_lines(
                r#"
                --- Desc
            "#
            ),
            vec!["Desc"]
        );

        expect_eq!(
            run_desc_to_lines(
                r#"
                --------
                --- Desc
                --------
            "#
            ),
            vec!["Desc"]
        );

        expect_eq!(
            run_desc_to_lines(
                r#"
                --------
                --- Desc
                --------
                --- Desc
                --------
            "#
            ),
            vec![" Desc", "-----", " Desc"]
        );

        expect_eq!(
            run_desc_to_lines(
                r#"
                --- Desc
                ---Desc 2
            "#
            ),
            vec![" Desc", "Desc 2"]
        );

        expect_eq!(
            run_desc_to_lines(
                r#"
                ---Desc
                --- Desc 2
            "#
            ),
            vec!["Desc", " Desc 2"]
        );

        expect_eq!(
            run_desc_to_lines(
                r#"
                ---  Desc
                ---  Desc 2
            "#
            ),
            vec!["Desc", "Desc 2"]
        );

        expect_eq!(
            run_desc_to_lines(
                r#"
                --- @param x int Desc
            "#
            ),
            vec!["Desc"]
        );
    }
}
