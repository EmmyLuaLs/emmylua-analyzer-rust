use crate::LuaTokenKind;
use crate::lexer::is_doc_whitespace;
use crate::parser::MarkEvent;
use crate::text::ReaderWithMarks;
use std::cmp::min;
use std::sync::LazyLock;

pub fn find_common_indent<'a>(
    lines: impl IntoIterator<Item = &'a str>,
    line_depth: usize,
) -> usize {
    let mut common_indent = None;

    for line in lines {
        if line.len() < line_depth {
            continue;
        }
        let line = &line[line_depth..];

        if is_blank(line) {
            continue;
        }

        let indent = line.chars().take_while(|c| is_doc_whitespace(*c)).count();
        common_indent = match common_indent {
            None => Some(indent),
            Some(common_indent) => Some(min(common_indent, indent)),
        };
    }

    common_indent.unwrap_or_default()
}

pub fn is_punct(c: char) -> bool {
    // Regex crate ships with unicode tables for detecting punctuation.
    // Unfortunately, these tables are not public, so the only way to
    // access them is through matching a symbol with a regex. We don't add
    // another crate with this info to save on binary size.
    static IS_P: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"^\pP|\pS").unwrap());

    if c.is_ascii() {
        c.is_ascii_punctuation()
    } else {
        let mut tmp = [0; 4];
        IS_P.is_match(c.encode_utf8(&mut tmp))
    }
}

pub fn is_opening_quote(c: char) -> bool {
    static IS_Q: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^\p{Ps}|\p{Pi}|\p{Pf}$").unwrap());

    if c.is_ascii() {
        matches!(c, '\'' | '"' | '<' | '(' | '[' | '{')
    } else {
        let mut tmp = [0; 4];
        let res = IS_Q.is_match(c.encode_utf8(&mut tmp));
        res
    }
}

pub fn is_closing_quote(c: char) -> bool {
    static IS_Q: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^\p{Pe}|\p{Pi}|\p{Pf}$").unwrap());

    if c.is_ascii() {
        matches!(c, '\'' | '"' | '>' | ')' | ']' | '}')
    } else {
        let mut tmp = [0; 4];
        let res = IS_Q.is_match(c.encode_utf8(&mut tmp));
        res
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

#[cfg(debug_assertions)]
pub fn check_marks_are_consistent(events: &[MarkEvent]) {
    let mut level = 0usize;
    for event in events {
        match event {
            MarkEvent::NodeStart { kind, .. } => {
                if *kind != crate::LuaSyntaxKind::None {
                    level += 1
                }
            }
            MarkEvent::NodeEnd => {
                level = level
                    .checked_sub(1)
                    .expect("more node ends than node starts");
            }
            _ => {}
        }
    }
    assert_eq!(level, 0, "more node starts than node ends");
}

#[cfg(not(debug_assertions))]
pub fn check_marks_are_consistent(events: &[MarkEvent]) {
    let _unused = events;
}

pub fn directive_is_code(name: &str) -> bool {
    matches!(
        name,
        "code-block" | "sourcecode" | "code" | "literalinclude" | "math"
    )
}

pub struct BacktrackGuard {
    n_backtracks: usize,
}

impl BacktrackGuard {
    pub fn new(n_backtracks: usize) -> Self {
        BacktrackGuard { n_backtracks }
    }

    pub fn backtrack(&mut self, reader: &mut ReaderWithMarks) {
        if self.n_backtracks == 0 {
            reader.eat_while(|_| true);
            reader.emit(LuaTokenKind::TkDocDetail);
        } else {
            self.n_backtracks -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::desc_parser::util::find_common_indent;

    #[test]
    fn test_find_common_indent() {
        let s = r"";
        let common_indent = find_common_indent(s.lines(), 0);
        assert_eq!(common_indent, 0);

        let s = r"x";
        let common_indent = find_common_indent(s.lines(), 0);
        assert_eq!(common_indent, 0);

        let s = r"  x";
        let common_indent = find_common_indent(s.lines(), 0);
        assert_eq!(common_indent, 2);

        let s = r"x\n  x";
        let common_indent = find_common_indent(s.lines(), 0);
        assert_eq!(common_indent, 0);

        let s = r#"  x\n  x"#;
        let common_indent = find_common_indent(s.lines(), 0);
        assert_eq!(common_indent, 2);

        let s = r#"  x\n\n  x"#;
        let common_indent = find_common_indent(s.lines(), 0);
        assert_eq!(common_indent, 2);

        let s = r#"> x\n> x"#;
        let common_indent = find_common_indent(s.lines(), 1);
        assert_eq!(common_indent, 1);
    }
}
