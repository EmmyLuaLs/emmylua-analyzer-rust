use emmylua_parser::{LuaAstNode, LuaComment, LuaKind, LuaSyntaxKind, LuaSyntaxNode, LuaTokenKind};
use rowan::TextRange;

use crate::ir::{self, DocIR};

/// Format a Comment node.
///
/// Dispatches between three comment types:
/// - Doc comments (`---@...`): walk the syntax tree, normalize whitespace
/// - Long comments (`--[[ ... ]]`): preserve content as-is
/// - Normal comments (`-- ...`): preserve text with trimming
pub fn format_comment(comment: &LuaComment) -> Vec<DocIR> {
    let text = comment.syntax().text().to_string();

    // Long comments (--[[ ... ]]): preserve content exactly (like long strings)
    if text.starts_with("--[[") || text.starts_with("--[=") {
        return vec![ir::text(text.trim_end())];
    }

    // Doc comments: walk the parsed syntax tree to normalize whitespace
    if comment.get_doc_tags().next().is_some() || comment.get_description().is_some() {
        return format_doc_comment(comment);
    }

    // Normal single-line comment: preserve text
    let text = text.trim_end();
    vec![ir::text(text)]
}

/// Format a doc comment by walking its syntax tree token-by-token.
///
/// Only flat formatting is used (Text, Space, HardLine) — no Group/SoftLine
/// since comments cannot have breaking rules.
fn format_doc_comment(comment: &LuaComment) -> Vec<DocIR> {
    let mut docs = Vec::new();
    let mut last_was_space = false;
    walk_doc_tokens(comment.syntax(), &mut docs, &mut last_was_space);
    // Trim trailing whitespace
    while matches!(docs.last(), Some(DocIR::Space)) {
        docs.pop();
    }
    docs
}

/// Recursively walk a doc comment node, emitting flat IR for each token.
fn walk_doc_tokens(node: &LuaSyntaxNode, docs: &mut Vec<DocIR>, last_was_space: &mut bool) {
    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Token(token) => {
                let kind: LuaTokenKind = token.kind().into();
                match kind {
                    LuaTokenKind::TkWhitespace => {
                        if !*last_was_space {
                            docs.push(ir::space());
                            *last_was_space = true;
                        }
                    }
                    LuaTokenKind::TkEndOfLine => {
                        // Remove trailing space before line break
                        if *last_was_space {
                            docs.pop();
                        }
                        docs.push(ir::hard_line());
                        *last_was_space = true; // prevent space at start of next line
                    }
                    _ => {
                        docs.push(ir::text(token.text()));
                        *last_was_space = false;
                    }
                }
            }
            rowan::NodeOrToken::Node(child_node) => {
                walk_doc_tokens(&child_node, docs, last_was_space);
            }
        }
    }
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
/// Extract a trailing comment on the same line after a syntax node.
/// Returns the raw comment docs (NOT wrapped in LineSuffix) and the text range.
pub fn extract_trailing_comment(node: &LuaSyntaxNode) -> Option<(Vec<DocIR>, TextRange)> {
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
                return Some((vec![ir::text(comment_text)], range));
            }
            _ => return None,
        }
        next = sibling.next_sibling_or_token();
    }

    None
}

/// Format a trailing comment as LineSuffix (for non-grouped use).
pub fn format_trailing_comment(node: &LuaSyntaxNode) -> Option<(DocIR, TextRange)> {
    let (docs, range) = extract_trailing_comment(node)?;
    let mut suffix_content = vec![ir::space()];
    suffix_content.extend(docs);
    Some((ir::line_suffix(suffix_content), range))
}
