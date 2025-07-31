use crate::desc_parser::{DescItem, DescRangeKind};
use crate::lexer::LuaTokenData;
use crate::text::{Reader, SourceRange};
use crate::{LuaAstNode, LuaDocDescription, LuaKind, LuaSyntaxElement, LuaTokenKind};
use std::cmp::min;

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

    for child in desc.syntax().children_with_tokens() {
        let LuaSyntaxElement::Token(token) = child else {
            continue;
        };

        match token.kind() {
            LuaKind::Token(LuaTokenKind::TkDocDetail) => {
                let range: SourceRange = token.text_range().into();
                if line.end_offset() == range.start_offset {
                    line.length += range.length;
                } else {
                    if line != SourceRange::EMPTY {
                        lines.push(line);
                    }
                    line = range;
                }
            }
            LuaKind::Token(LuaTokenKind::TkEndOfLine) => {
                lines.push(line);
                line = SourceRange::EMPTY;
            }
            LuaKind::Token(LuaTokenKind::TkNormalStart | LuaTokenKind::TkDocContinue) => {
                line = token.text_range().into();
                let leading_marks = token.text().chars().take_while(|c| *c == '-').count();
                line.start_offset += leading_marks;
                line.length -= leading_marks;
            }
            _ => {}
        }
    }

    if !line.is_empty() {
        lines.push(line);
    }

    let mut common_indent = None;
    for line in lines.iter().skip(1) {
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
        for line in lines.iter_mut().skip(1) {
            if line.length >= common_indent {
                line.start_offset += common_indent;
                line.length -= common_indent;
            }
        }
    }

    // Don't parse lines past user's cursor when calculating
    // Go To Definition or Completion. We handle this here so that
    // we don't affect common indent.
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

    fn emit_range(&mut self, range: SourceRange, kind: DescRangeKind) {
        let should_emit = if let Some(cursor_position) = self.cursor_position() {
            kind == DescRangeKind::Ref && range.contains_inclusive(cursor_position)
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

    fn emit(&mut self, reader: &mut Reader, kind: DescRangeKind) {
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
        false // TODO: P|S
    }
}

pub fn is_opening_quote(c: char) -> bool {
    if c.is_ascii() {
        matches!(c, '\'' | '"' | '<' | '(' | '[' | '{')
    } else {
        false // TODO: Ps|Pi|Pf
    }
}

pub fn is_closing_quote(c: char) -> bool {
    if c.is_ascii() {
        matches!(c, '\'' | '"' | '>' | ')' | ']' | '}')
    } else {
        false // TODO: Pe|Pi|Pf
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

pub fn process_lua_code<'a, C: ResultContainer>(
    c: &mut C,
    range: SourceRange,
    tokens: Vec<LuaTokenData>,
) {
    let mut pos = range.start_offset;
    for token in tokens {
        if pos < token.range.start_offset {
            c.emit_range(
                SourceRange::from_start_end(pos, token.range.start_offset),
                DescRangeKind::CodeBlock,
            )
        }
        if !matches!(
            token.kind,
            LuaTokenKind::TkEof | LuaTokenKind::TkEndOfLine | LuaTokenKind::TkWhitespace
        ) {
            c.emit_range(token.range, DescRangeKind::CodeBlockHl(token.kind));
            pos = token.range.end_offset();
        } else {
            pos = token.range.start_offset;
        }
    }

    if pos < range.end_offset() {
        c.emit_range(
            SourceRange::from_start_end(pos, range.end_offset()),
            DescRangeKind::CodeBlock,
        )
    }
}

pub fn sort_result(items: &mut Vec<DescItem>) {
    items.sort_by_key(|r| {
        let len: usize = r.range.len().into();

        (
            r.range.start(),                // Sort by start position,
            usize::MAX - len,               // longer tokens first,
            r.kind != DescRangeKind::Scope, // scopes go first.
        )
    });
}
