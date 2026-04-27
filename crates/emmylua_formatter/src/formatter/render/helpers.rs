use emmylua_parser::{
    LuaAstNode, LuaComment, LuaExpr, LuaKind, LuaSyntaxId, LuaSyntaxKind, LuaSyntaxNode,
    LuaSyntaxToken, LuaTokenKind,
};
use rowan::NodeOrToken;

use crate::formatter::render::comment_is_inline_after_anchor;
use crate::ir::{self, DocIR};

use super::super::expr;
use super::super::model::{
    LayoutNodePlan, RootFormatPlan, SyntaxNodeLayoutPlan, TokenSpacingExpected,
};
use super::super::sequence::{
    SequenceEntry, render_sequence, sequence_ends_with_comment, sequence_has_comment,
    sequence_starts_with_inline_comment,
};
use super::super::trivia::{
    count_blank_lines_before, has_non_trivia_before_on_same_line_tokenwise,
};
use super::FormatContext;

pub(super) fn render_expr(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    expr: &LuaExpr,
) -> Vec<DocIR> {
    expr::format_expr(ctx, plan, expr)
}

pub(super) fn find_direct_child_plan_by_id(
    syntax_plan: &SyntaxNodeLayoutPlan,
    syntax_id: LuaSyntaxId,
) -> Option<&SyntaxNodeLayoutPlan> {
    syntax_plan.children.iter().find_map(|child| match child {
        LayoutNodePlan::Syntax(plan) if plan.syntax_id == syntax_id => Some(plan),
        _ => None,
    })
}

pub(super) fn next_significant_is_inline_comment(
    children: &[NodeOrToken<LuaSyntaxNode, LuaSyntaxToken>],
    index: usize,
) -> bool {
    children[index + 1..]
        .iter()
        .find_map(|child| match child {
            NodeOrToken::Token(token)
                if matches!(
                    token.kind().to_token(),
                    LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine
                ) =>
            {
                None
            }
            NodeOrToken::Node(node) if node.kind() == LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                LuaComment::cast(node.clone())
                    .map(|comment| has_non_trivia_before_on_same_line_tokenwise(comment.syntax()))
            }
            _ => Some(false),
        })
        .unwrap_or(false)
}

pub(super) fn previous_significant_token(
    children: &[NodeOrToken<LuaSyntaxNode, LuaSyntaxToken>],
    index: usize,
) -> Option<LuaSyntaxToken> {
    children[..index]
        .iter()
        .rev()
        .find_map(|child| match child {
            NodeOrToken::Token(token)
                if !matches!(
                    token.kind().to_token(),
                    LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine
                ) =>
            {
                Some(token.clone())
            }
            NodeOrToken::Node(node) => last_significant_token_in_node(node),
            _ => None,
        })
}

pub(super) fn last_significant_token_in_node(node: &LuaSyntaxNode) -> Option<LuaSyntaxToken> {
    let children = node.children_with_tokens().collect::<Vec<_>>();
    children.into_iter().rev().find_map(|child| match child {
        NodeOrToken::Token(token)
            if !matches!(
                token.kind().to_token(),
                LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine
            ) =>
        {
            Some(token)
        }
        NodeOrToken::Node(node) => last_significant_token_in_node(&node),
        _ => None,
    })
}

pub(super) fn next_significant_is_block(
    children: &[NodeOrToken<LuaSyntaxNode, LuaSyntaxToken>],
    index: usize,
) -> bool {
    children[index + 1..]
        .iter()
        .find_map(|child| match child {
            NodeOrToken::Token(token)
                if matches!(
                    token.kind().to_token(),
                    LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine
                ) =>
            {
                None
            }
            NodeOrToken::Node(node) => Some(node.kind() == LuaKind::Syntax(LuaSyntaxKind::Block)),
            _ => Some(false),
        })
        .unwrap_or(false)
}

pub(super) fn leading_inline_block_comment(
    root: &LuaSyntaxNode,
    block_node: &LuaSyntaxNode,
    anchor_token: &LuaSyntaxToken,
) -> Option<LuaComment> {
    for child in block_node.children_with_tokens() {
        match child {
            NodeOrToken::Token(token)
                if matches!(token.kind().to_token(), LuaTokenKind::TkWhitespace) =>
            {
                continue;
            }
            NodeOrToken::Token(token)
                if matches!(token.kind().to_token(), LuaTokenKind::TkEndOfLine) =>
            {
                return None;
            }
            NodeOrToken::Node(node) if node.kind() == LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                let comment = LuaComment::cast(node)?;
                return comment_is_inline_after_anchor(root, Some(anchor_token), comment.syntax())
                    .then_some(comment);
            }
            _ => return None,
        }
    }

    None
}

pub(super) fn find_syntax_plan_by_id(
    nodes: &[LayoutNodePlan],
    syntax_id: LuaSyntaxId,
) -> Option<&SyntaxNodeLayoutPlan> {
    for node in nodes {
        if let LayoutNodePlan::Syntax(plan) = node {
            if plan.syntax_id == syntax_id {
                return Some(plan);
            }

            if let Some(found) = find_syntax_plan_by_id(&plan.children, syntax_id) {
                return Some(found);
            }
        }
    }

    None
}

pub(super) fn token_or_kind_doc(
    token: Option<&LuaSyntaxToken>,
    fallback_kind: LuaTokenKind,
) -> DocIR {
    token
        .map(|token| ir::source_token(token.clone()))
        .unwrap_or_else(|| ir::syntax_token(fallback_kind))
}

pub(super) fn first_direct_token(
    node: &LuaSyntaxNode,
    kind: LuaTokenKind,
) -> Option<LuaSyntaxToken> {
    node.children_with_tokens().find_map(|element| {
        let token = element.into_token()?;
        (token.kind().to_token() == kind).then_some(token)
    })
}

pub(super) fn token_left_spacing_docs(
    plan: &RootFormatPlan,
    token: Option<&LuaSyntaxToken>,
) -> Vec<DocIR> {
    let Some(token) = token else {
        return Vec::new();
    };
    spacing_docs_from_expected(plan.spacing.left_expected(LuaSyntaxId::from_token(token)))
}

pub(super) fn token_right_spacing_docs(
    plan: &RootFormatPlan,
    token: Option<&LuaSyntaxToken>,
) -> Vec<DocIR> {
    let Some(token) = token else {
        return Vec::new();
    };
    spacing_docs_from_expected(plan.spacing.right_expected(LuaSyntaxId::from_token(token)))
}

pub(super) fn spacing_docs_from_expected(expected: Option<&TokenSpacingExpected>) -> Vec<DocIR> {
    match expected {
        Some(TokenSpacingExpected::Space(count)) | Some(TokenSpacingExpected::MaxSpace(count)) => {
            (0..*count).map(|_| ir::space()).collect()
        }
        None => Vec::new(),
    }
}

pub(super) fn comma_token_docs(token: Option<&LuaSyntaxToken>) -> Vec<DocIR> {
    vec![token_or_kind_doc(token, LuaTokenKind::TkComma)]
}

pub(super) fn comma_flat_separator(
    plan: &RootFormatPlan,
    token: Option<&LuaSyntaxToken>,
) -> Vec<DocIR> {
    let mut docs = comma_token_docs(token);
    docs.extend(token_right_spacing_docs(plan, token));
    docs
}

pub(super) fn comma_fill_separator(token: Option<&LuaSyntaxToken>) -> Vec<DocIR> {
    let mut docs = comma_token_docs(token);
    docs.push(ir::soft_line());
    docs
}

pub(super) fn separator_entry_from_token(
    plan: &RootFormatPlan,
    token: Option<&LuaSyntaxToken>,
) -> SequenceEntry {
    SequenceEntry::Separator {
        docs: token
            .map(|token| vec![ir::source_token(token.clone())])
            .unwrap_or_else(|| comma_token_docs(None)),
        after_docs: token_right_spacing_docs(plan, token),
    }
}

pub(super) fn render_trivia_aware_sequence_tail(
    _plan: &RootFormatPlan,
    leading_docs: Vec<DocIR>,
    entries: &[SequenceEntry],
) -> Vec<DocIR> {
    let mut tail = if sequence_starts_with_inline_comment(entries) {
        Vec::new()
    } else {
        leading_docs
    };
    if sequence_has_comment(entries) {
        if sequence_starts_with_inline_comment(entries) {
            render_sequence(&mut tail, entries, false);
        } else {
            tail.push(ir::hard_line());
            render_sequence(&mut tail, entries, true);
        }
    } else {
        render_sequence(&mut tail, entries, false);
    }
    tail
}

pub(super) fn render_trivia_aware_split_sequence_tail(
    plan: &RootFormatPlan,
    leading_docs: Vec<DocIR>,
    lhs_entries: &[SequenceEntry],
    split_token: Option<&LuaSyntaxToken>,
    rhs_entries: &[SequenceEntry],
) -> Vec<DocIR> {
    let mut tail = leading_docs;
    if !lhs_entries.is_empty() {
        render_sequence(&mut tail, lhs_entries, false);
    }

    if let Some(split_token) = split_token {
        if sequence_ends_with_comment(lhs_entries) {
            tail.push(ir::hard_line());
            tail.push(ir::source_token(split_token.clone()));
        } else if sequence_has_comment(lhs_entries) {
            tail.push(ir::space());
            tail.push(ir::source_token(split_token.clone()));
        } else {
            tail.extend(token_left_spacing_docs(plan, Some(split_token)));
            tail.push(ir::source_token(split_token.clone()));
        }

        if !rhs_entries.is_empty() {
            if sequence_has_comment(rhs_entries) {
                if sequence_starts_with_inline_comment(rhs_entries) {
                    render_sequence(&mut tail, rhs_entries, false);
                } else {
                    tail.push(ir::hard_line());
                    render_sequence(&mut tail, rhs_entries, true);
                }
            } else {
                tail.extend(token_right_spacing_docs(plan, Some(split_token)));
                render_sequence(&mut tail, rhs_entries, false);
            }
        }
    }

    tail
}

pub(super) fn count_blank_lines_before_layout_node(
    root: &LuaSyntaxNode,
    node: &LayoutNodePlan,
) -> usize {
    let syntax_id = match node {
        LayoutNodePlan::Comment(comment) => comment.syntax_id,
        LayoutNodePlan::Syntax(syntax) => syntax.syntax_id,
    };
    let Some(node) = find_node_by_id(root, syntax_id) else {
        return 0;
    };

    count_blank_lines_before(&node)
}

pub(super) fn find_node_by_id(
    root: &LuaSyntaxNode,
    syntax_id: LuaSyntaxId,
) -> Option<LuaSyntaxNode> {
    if LuaSyntaxId::from_node(root) == syntax_id {
        return Some(root.clone());
    }

    syntax_id.to_node_from_root(root)
}
