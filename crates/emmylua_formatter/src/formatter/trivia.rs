use emmylua_parser::{LuaKind, LuaSyntaxKind, LuaSyntaxNode, LuaTokenKind};

pub fn count_blank_lines_before(node: &LuaSyntaxNode) -> usize {
    let mut blank_lines = 0;
    let mut consecutive_newlines = 0;

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
                LuaTokenKind::TkWhitespace => {}
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
            LuaTokenKind::TkWhitespace => previous = token.prev_token(),
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
