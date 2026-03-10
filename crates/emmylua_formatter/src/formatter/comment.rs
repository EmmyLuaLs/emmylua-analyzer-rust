use emmylua_parser::{LuaAstNode, LuaComment, LuaKind, LuaSyntaxKind, LuaSyntaxNode, LuaTokenKind};
use rowan::TextRange;

use crate::ir::{self, DocIR};

/// Format a Comment node.
///
/// Comment is a syntax node in the CST (LuaSyntaxKind::Comment),
/// which can be a single-line comment (`-- ...`) or a multi-line comment (`--[[ ... ]]`).
/// We preserve the original comment text and only handle indentation (managed by Printer's indent).
pub fn format_comment(comment: &LuaComment) -> Vec<DocIR> {
    let text = comment.syntax().text().to_string();
    let text = text.trim_end();

    // Multi-line comment: split by lines, each line as a Text + HardLine
    let lines: Vec<&str> = text.lines().collect();

    if lines.len() <= 1 {
        // Single-line comment
        return vec![ir::text(text)];
    }

    // Multi-line content (doc comments or --[[ ]] block comments)
    let mut docs = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            docs.push(ir::hard_line());
        }
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            // Preserve empty lines
            continue;
        }
        docs.push(ir::text(trimmed));
    }

    docs
}

/// Collect "orphan" comments in a syntax node.
///
/// When a Block is empty (e.g. `if x then -- comment end`),
/// comments may become direct children of the parent statement node rather than the Block.
/// This function collects those comments and returns the formatted IR.
pub fn collect_orphan_comments(node: &LuaSyntaxNode) -> Vec<DocIR> {
    let mut docs = Vec::new();
    for child in node.children() {
        if child.kind() == LuaKind::Syntax(LuaSyntaxKind::Comment)
            && let Some(comment) = LuaComment::cast(child)
        {
            if !docs.is_empty() {
                docs.push(ir::hard_line());
            }
            docs.extend(format_comment(&comment));
        }
    }
    docs
}
///
/// Find a Comment node on the same line after a statement node;
/// if found, attach it to the end of line using LineSuffix.
pub fn format_trailing_comment(node: &LuaSyntaxNode) -> Option<(DocIR, TextRange)> {
    let mut next = node.next_sibling_or_token();

    // Look ahead at most 4 elements (skipping whitespace, commas, semicolons)
    for _ in 0..4 {
        let sibling = next.as_ref()?;
        match sibling.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace) => {}
            LuaKind::Token(LuaTokenKind::TkSemicolon) => {}
            LuaKind::Token(LuaTokenKind::TkComma) => {}
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                let comment_node = sibling.as_node()?;
                let comment_text = comment_node.text().to_string();
                let comment_text = comment_text.trim_end().to_string();

                // Only single-line comments are treated as trailing comments
                if comment_text.contains('\n') {
                    return None;
                }

                let range = comment_node.text_range();
                return Some((
                    ir::line_suffix(vec![ir::space(), ir::text(comment_text)]),
                    range,
                ));
            }
            _ => return None,
        }
        next = sibling.next_sibling_or_token();
    }

    None
}
