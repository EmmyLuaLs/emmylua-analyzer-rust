use emmylua_parser::{LuaKind, LuaSyntaxKind, LuaSyntaxNode, LuaTokenKind};
use rowan::TextRange;

/// Count how many blank lines appear before a node.
pub fn count_blank_lines_before(node: &LuaSyntaxNode) -> usize {
    let mut blank_lines = 0;
    let mut consecutive_newlines = 0;

    // Walk tokens backwards, counting consecutive newlines
    if let Some(first_token) = node.first_token() {
        let mut token = first_token.prev_token();
        while let Some(t) = token {
            match t.kind().to_token() {
                LuaTokenKind::TkEndOfLine => {
                    consecutive_newlines += 1;
                    if consecutive_newlines > 1 {
                        blank_lines += 1;
                    }
                }
                LuaTokenKind::TkWhitespace => {
                    // Skip whitespace
                }
                _ => break,
            }
            token = t.prev_token();
        }
    }

    blank_lines
}

pub fn node_has_direct_comment_child(node: &LuaSyntaxNode) -> bool {
    node.children()
        .any(|child| child.kind() == LuaKind::Syntax(LuaSyntaxKind::Comment))
}

pub fn has_non_trivia_before_on_same_line_tokenwise(node: &LuaSyntaxNode) -> bool {
    let Some(first_token) = node.first_token() else {
        return false;
    };

    let mut previous = first_token.prev_token();

    while let Some(token) = previous {
        match token.kind().to_token() {
            LuaTokenKind::TkWhitespace => {
                previous = token.prev_token();
            }
            LuaTokenKind::TkEndOfLine => return false,
            _ => return true,
        }
    }

    false
}

pub fn source_line_prefix_width(node: &LuaSyntaxNode) -> usize {
    let mut width = 0usize;
    let Some(mut token) = node.first_token() else {
        return 0;
    };

    while let Some(prev) = token.prev_token() {
        let text = prev.text();
        let mut chars_since_break = 0usize;

        for ch in text.chars().rev() {
            if matches!(ch, '\n' | '\r') {
                return width;
            }
            chars_since_break += 1;
        }

        width += chars_since_break;
        token = prev;
    }

    width
}

pub fn trailing_gap_requests_alignment(
    node: &LuaSyntaxNode,
    comment_range: TextRange,
    required_min_gap: usize,
) -> bool {
    let mut gap_width = 0usize;
    let mut next = node.next_sibling_or_token();

    while let Some(element) = next {
        if element.text_range().start() >= comment_range.start() {
            break;
        }

        match element.kind() {
            LuaKind::Token(LuaTokenKind::TkEndOfLine) => return false,
            LuaKind::Token(LuaTokenKind::TkWhitespace) => {
                if let Some(token) = element.as_token() {
                    for ch in token.text().chars() {
                        if matches!(ch, '\n' | '\r') {
                            return false;
                        }
                        if matches!(ch, ' ' | '\t') {
                            gap_width += 1;
                        }
                    }
                }
            }
            _ => {
                if element.text_range().end() > comment_range.start() {
                    return false;
                }
            }
        }

        next = element.next_sibling_or_token();
    }

    gap_width > required_min_gap
}
