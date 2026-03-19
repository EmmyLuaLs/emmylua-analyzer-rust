use emmylua_parser::{LuaKind, LuaSyntaxKind, LuaSyntaxNode, LuaTokenKind};

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

pub fn node_has_direct_same_line_inline_comment(node: &LuaSyntaxNode) -> bool {
    node.children().any(|child| {
        child.kind() == LuaKind::Syntax(LuaSyntaxKind::Comment)
            && has_non_trivia_before_on_same_line(&child)
    })
}

pub fn node_has_direct_comment_child(node: &LuaSyntaxNode) -> bool {
    node.children()
        .any(|child| child.kind() == LuaKind::Syntax(LuaSyntaxKind::Comment))
}

pub fn has_non_trivia_before_on_same_line(node: &LuaSyntaxNode) -> bool {
    let mut previous = node.prev_sibling_or_token();

    while let Some(element) = previous {
        match element.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace) => {
                previous = element.prev_sibling_or_token();
            }
            LuaKind::Token(LuaTokenKind::TkEndOfLine) => return false,
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                previous = element.prev_sibling_or_token();
            }
            _ => return true,
        }
    }

    false
}
