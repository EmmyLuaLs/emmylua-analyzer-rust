use crate::formatter::render::comments_ast::render_comment_via_ast;
use rowan::TextRange;

use super::*;

pub(crate) type RenderedTrailingComment = (Vec<DocIR>, TextRange, bool);

pub(crate) fn extract_trailing_comment_rendered(
    ctx: &FormatContext,
    syntax_plan: &SyntaxNodeLayoutPlan,
    node: &LuaSyntaxNode,
    plan: &FormatPlan,
) -> Option<RenderedTrailingComment> {
    let comment = find_inline_trailing_comment_node(node)?;
    if comment.text().contains_char('\n') {
        return None;
    }
    let comment = LuaComment::cast(comment.clone())?;
    let docs = render_comment_with_spacing(ctx, &comment, plan);
    let align_hint = matches!(
        syntax_plan.kind,
        LuaSyntaxKind::LocalStat | LuaSyntaxKind::AssignStat
    ) && trailing_gap_requests_alignment(
        node,
        comment.get_range(),
        ctx.config.comments.line_comment_min_spaces_before.max(1),
    );
    Some((docs, comment.get_range(), align_hint))
}

pub(crate) fn append_trailing_comment_suffix(
    ctx: &FormatContext,
    plan: &FormatPlan,
    docs: &mut Vec<DocIR>,
    node: &LuaSyntaxNode,
) {
    let Some(comment_node) = find_inline_trailing_comment_node(node) else {
        return;
    };
    let Some(comment) = LuaComment::cast(comment_node) else {
        return;
    };

    let content_width = crate::ir::ir_flat_width(docs);
    let padding = if ctx.config.comments.line_comment_min_column == 0 {
        ctx.config.comments.line_comment_min_spaces_before.max(1)
    } else {
        ctx.config
            .comments
            .line_comment_min_spaces_before
            .max(1)
            .max(
                ctx.config
                    .comments
                    .line_comment_min_column
                    .saturating_sub(content_width),
            )
    };
    let mut suffix = (0..padding).map(|_| ir::space()).collect::<Vec<_>>();
    suffix.extend(render_comment_with_spacing(ctx, &comment, plan));
    docs.push(ir::line_suffix(suffix));
}

pub(crate) fn append_trailing_statement_suffix(
    ctx: &FormatContext,
    plan: &FormatPlan,
    docs: &mut Vec<DocIR>,
    node: &LuaSyntaxNode,
) {
    append_trailing_statement_semicolon(ctx, plan, docs, node);
    append_trailing_comment_suffix(ctx, plan, docs, node);
}

fn append_trailing_statement_semicolon(
    ctx: &FormatContext,
    plan: &FormatPlan,
    docs: &mut Vec<DocIR>,
    node: &LuaSyntaxNode,
) {
    if !ctx.config.output.preserve_statement_semicolon {
        return;
    }

    let Some(semicolon) = trailing_statement_semicolon_token(node) else {
        return;
    };

    docs.extend(token_left_spacing_docs(plan, Some(&semicolon)));
    docs.push(ir::source_token(semicolon));
}

pub(crate) fn source_order_token_is_trailing_statement_semicolon(
    node: &LuaSyntaxNode,
    token: &LuaSyntaxToken,
) -> bool {
    trailing_statement_semicolon_token(node)
        .is_some_and(|semicolon| semicolon.text_range() == token.text_range())
}

fn trailing_statement_semicolon_token(node: &LuaSyntaxNode) -> Option<LuaSyntaxToken> {
    let children = node.children_with_tokens().collect::<Vec<_>>();
    for child in children.into_iter().rev() {
        match child.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine) => continue,
            LuaKind::Syntax(LuaSyntaxKind::Comment) => continue,
            LuaKind::Token(LuaTokenKind::TkSemicolon) => return child.into_token(),
            _ => return None,
        }
    }

    None
}

fn find_inline_trailing_comment_node(node: &LuaSyntaxNode) -> Option<LuaSyntaxNode> {
    for child in node.children() {
        if child.kind() != LuaKind::Syntax(LuaSyntaxKind::Comment) {
            continue;
        }

        if has_inline_non_trivia_before(&child) && !has_non_trivia_after_in_node(&child) {
            return Some(child);
        }
    }

    let mut next = node.next_sibling_or_token();
    for _ in 0..4 {
        let sibling = next.as_ref()?;
        match sibling.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace)
            | LuaKind::Token(LuaTokenKind::TkSemicolon)
            | LuaKind::Token(LuaTokenKind::TkComma) => {}
            LuaKind::Syntax(LuaSyntaxKind::Comment) => return sibling.as_node().cloned(),
            _ => return None,
        }
        next = sibling.next_sibling_or_token();
    }

    None
}

fn has_non_trivia_after_in_node(node: &LuaSyntaxNode) -> bool {
    let mut next = node.next_sibling_or_token();
    while let Some(element) = next {
        match element.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine) => {
                next = element.next_sibling_or_token();
            }
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                next = element.next_sibling_or_token();
            }
            _ => return true,
        }
    }

    false
}

pub(crate) fn has_inline_non_trivia_before(node: &LuaSyntaxNode) -> bool {
    let mut previous = node.prev_sibling_or_token();
    while let Some(element) = previous {
        match element.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace) => {
                previous = element.prev_sibling_or_token()
            }
            LuaKind::Token(LuaTokenKind::TkEndOfLine) => return false,
            LuaKind::Syntax(LuaSyntaxKind::Comment) => previous = element.prev_sibling_or_token(),
            _ => return true,
        }
    }
    false
}

pub(crate) fn has_inline_non_trivia_after(node: &LuaSyntaxNode) -> bool {
    let mut next = node.next_sibling_or_token();
    while let Some(element) = next {
        match element.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace) => next = element.next_sibling_or_token(),
            LuaKind::Token(LuaTokenKind::TkEndOfLine) => return false,
            LuaKind::Syntax(LuaSyntaxKind::Comment) => next = element.next_sibling_or_token(),
            _ => return true,
        }
    }
    false
}

pub(crate) fn render_comment_with_spacing(
    ctx: &FormatContext,
    comment: &LuaComment,
    plan: &FormatPlan,
) -> Vec<DocIR> {
    if should_preserve_comment_raw(comment) {
        return vec![ir::source_node_trimmed(comment.syntax().clone())];
    }

    render_comment_via_ast(ctx, comment, plan)
}

pub(crate) fn render_direct_body_comment(
    comment: LuaComment,
    ctx: &FormatContext,
    plan: &FormatPlan,
) -> Vec<DocIR> {
    vec![
        ir::indent({
            let mut docs = vec![ir::hard_line()];
            docs.extend(render_comment_with_spacing(ctx, &comment, plan));
            docs
        }),
        ir::hard_line(),
    ]
}

pub(crate) fn comment_is_inline_after_anchor(
    root: &LuaSyntaxNode,
    anchor_token: Option<&LuaSyntaxToken>,
    comment: &LuaSyntaxNode,
) -> bool {
    let Some(anchor_token) = anchor_token else {
        return false;
    };

    let start: usize = anchor_token.text_range().end().into();
    let end: usize = comment.text_range().start().into();
    if end < start {
        return false;
    }

    let text = root.text().to_string();
    !text[start..end].chars().any(|ch| matches!(ch, '\n' | '\r'))
}

fn should_preserve_comment_raw(comment: &LuaComment) -> bool {
    let raw = comment.syntax().text().to_string();
    if raw.starts_with("----") {
        return true;
    }

    if raw
        .lines()
        .any(|line| raw_comment_starts_like_long_comment(line.trim_start()))
    {
        return true;
    }

    let Some(first_token) = comment.syntax().first_token() else {
        return false;
    };

    matches!(
        first_token.kind().to_token(),
        LuaTokenKind::TkLongCommentStart | LuaTokenKind::TkDocLongStart
    ) || dash_prefix_len(first_token.text()) > 3
}

fn raw_comment_starts_like_long_comment(raw: &str) -> bool {
    let Some(after_dash) = raw.strip_prefix("--") else {
        return false;
    };
    let Some(after_open) = after_dash.strip_prefix('[') else {
        return false;
    };
    let equals_count = after_open.bytes().take_while(|byte| *byte == b'=').count();
    after_open[equals_count..].starts_with('[')
}

fn dash_prefix_len(prefix_text: &str) -> usize {
    prefix_text.bytes().take_while(|byte| *byte == b'-').count()
}
