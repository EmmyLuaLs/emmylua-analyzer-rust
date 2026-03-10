use emmylua_parser::{LuaSyntaxNode, LuaTokenKind};

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
