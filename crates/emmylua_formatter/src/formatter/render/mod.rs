mod comments;
mod control;
mod doc_comments;
mod helpers;
mod statements;
#[cfg(test)]
mod test;

use crate::formatter::model::{StatementExprListLayoutKind, StatementExprListLayoutPlan};
use crate::ir::{self, AlignEntry, DocIR};
use emmylua_parser::*;

use super::FormatContext;
use crate::formatter::expr::ExprFormatOptions;
use crate::formatter::model::{
    FormatPlan, LayoutNodePlan, SyntaxNodeLayoutPlan, TokenSpacingExpected,
};
use crate::formatter::sequence::*;
use crate::formatter::trivia::*;

pub(super) use self::comments::{
    append_trailing_statement_suffix, comment_is_inline_after_anchor,
    extract_trailing_comment_rendered, has_inline_non_trivia_after, render_comment_with_spacing,
    render_direct_body_comment, source_order_token_is_trailing_statement_semicolon,
};
use self::control::{
    render_do_stat, render_for_range_stat, render_for_stat, render_func_stat, render_if_stat,
    render_local_func_stat, render_repeat_stat, render_while_stat,
};
use self::helpers::*;
pub(super) use self::statements::{
    format_statement_value_expr, has_direct_comment_before_token, render_assign_stat,
    render_break_stat, render_call_expr_stat, render_continue_stat, render_empty_stat,
    render_header_exprs_with_leading_docs, render_local_stat, render_return_stat,
};
use self::statements::{render_statement_align_split, render_statement_line_content};

pub fn render_ir(ctx: &FormatContext, chunk: &LuaChunk, plan: &FormatPlan) -> Vec<DocIR> {
    let mut docs = Vec::new();
    if let Some(token) = chunk.syntax().first_token()
        && token.kind() == LuaKind::Token(LuaTokenKind::TkShebang)
    {
        docs.push(ir::source_token(token));
        if !plan.layout.root_nodes.is_empty() {
            docs.push(ir::hard_line());
        }
    }

    docs.extend(render_aligned_block_layout_nodes(
        ctx,
        chunk.syntax(),
        plan.layout.root_nodes.as_slice(),
        plan,
    ));
    if plan.line_breaks.insert_final_newline {
        docs.push(ir::hard_line());
    }
    docs
}

pub fn render_closure_block_body(
    ctx: &FormatContext,
    expr: &LuaClosureExpr,
    plan: &FormatPlan,
) -> Vec<DocIR> {
    let root = expr.get_root();
    let closure_id = expr.get_syntax_id();
    if let Some(block_children) = plan.layout.closure_body_children.get(&closure_id) {
        return render_aligned_block_layout_nodes(ctx, &root, block_children.as_slice(), plan);
    }

    let Some(closure_plan) = find_syntax_plan_by_id(&plan.layout.root_nodes, closure_id) else {
        return Vec::new();
    };

    let Some(block_plan) = block_plan_from_parent_plan(closure_plan) else {
        return Vec::new();
    };

    render_aligned_block_layout_nodes(ctx, &root, block_plan.children.as_slice(), plan)
}

fn render_layout_node(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    node: &LayoutNodePlan,
    plan: &FormatPlan,
) -> Vec<DocIR> {
    if let Some(disabled) = render_format_disabled_layout_node(root, node, plan) {
        return disabled;
    }

    match node {
        LayoutNodePlan::Comment(comment) => {
            let Some(syntax) = find_node_by_id(root, comment.syntax_id) else {
                return Vec::new();
            };
            let Some(comment) = LuaComment::cast(syntax) else {
                return Vec::new();
            };
            render_comment_with_spacing(ctx, &comment, plan)
        }
        LayoutNodePlan::Syntax(syntax_plan) => match syntax_plan.kind {
            LuaSyntaxKind::Block => {
                render_aligned_block_layout_nodes(ctx, root, &syntax_plan.children, plan)
            }
            LuaSyntaxKind::LocalStat => render_local_stat(ctx, root, syntax_plan.syntax_id, plan),
            LuaSyntaxKind::ConstStat => render_local_stat(ctx, root, syntax_plan.syntax_id, plan),
            LuaSyntaxKind::AssignStat => render_assign_stat(ctx, root, syntax_plan.syntax_id, plan),
            LuaSyntaxKind::ReturnStat => render_return_stat(ctx, root, syntax_plan.syntax_id, plan),
            LuaSyntaxKind::BreakStat => render_break_stat(root, syntax_plan.syntax_id),
            LuaSyntaxKind::ContinueStat => render_continue_stat(root, syntax_plan.syntax_id),
            LuaSyntaxKind::WhileStat => render_while_stat(ctx, root, syntax_plan, plan),
            LuaSyntaxKind::ForStat => render_for_stat(ctx, root, syntax_plan, plan),
            LuaSyntaxKind::ForRangeStat => render_for_range_stat(ctx, root, syntax_plan, plan),
            LuaSyntaxKind::RepeatStat => render_repeat_stat(ctx, root, syntax_plan, plan),
            LuaSyntaxKind::IfStat => render_if_stat(ctx, root, syntax_plan, plan),
            LuaSyntaxKind::FuncStat => render_func_stat(ctx, root, syntax_plan, plan),
            LuaSyntaxKind::LocalFuncStat => render_local_func_stat(ctx, root, syntax_plan, plan),
            LuaSyntaxKind::DoStat => render_do_stat(ctx, root, syntax_plan, plan),
            LuaSyntaxKind::CallExprStat => {
                render_call_expr_stat(ctx, root, syntax_plan.syntax_id, plan)
            }
            LuaSyntaxKind::EmptyStat => render_empty_stat(root, syntax_plan.syntax_id),
            _ => render_unmigrated_syntax_leaf(root, syntax_plan.syntax_id),
        },
    }
}

fn render_format_disabled_layout_node(
    root: &LuaSyntaxNode,
    node: &LayoutNodePlan,
    plan: &FormatPlan,
) -> Option<Vec<DocIR>> {
    let syntax_id = match node {
        LayoutNodePlan::Comment(comment) => comment.syntax_id,
        LayoutNodePlan::Syntax(syntax) => syntax.syntax_id,
    };

    if !plan.layout.format_disabled.contains(&syntax_id) {
        return None;
    }

    let syntax = find_node_by_id(root, syntax_id)?;
    Some(vec![ir::source_node_trimmed(syntax)])
}

fn render_unmigrated_syntax_leaf(root: &LuaSyntaxNode, syntax_id: LuaSyntaxId) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_id) else {
        return Vec::new();
    };

    vec![ir::source_node_trimmed(node)]
}

fn block_plan_from_parent_plan(
    syntax_plan: &SyntaxNodeLayoutPlan,
) -> Option<&SyntaxNodeLayoutPlan> {
    syntax_plan.children.iter().find_map(|child| match child {
        LayoutNodePlan::Syntax(block) if block.kind == LuaSyntaxKind::Block => Some(block),
        _ => None,
    })
}

fn render_block_plan_without_excluded_comments(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    block_plan: Option<&SyntaxNodeLayoutPlan>,
    plan: &FormatPlan,
    excluded_comment_ids: &[LuaSyntaxId],
) -> Vec<DocIR> {
    let Some(block_plan) = block_plan else {
        return vec![ir::hard_line()];
    };

    let filtered_children;
    let block_children = if excluded_comment_ids.is_empty() {
        Some(block_plan.children.as_slice())
    } else {
        filtered_children = block_plan
            .children
            .iter()
            .filter(|child| match child {
                LayoutNodePlan::Comment(comment) => {
                    !excluded_comment_ids.contains(&comment.syntax_id)
                }
                _ => true,
            })
            .cloned()
            .collect::<Vec<_>>();
        Some(filtered_children.as_slice())
    };

    let docs = render_block_children(ctx, root, block_children, plan);
    if !matches!(docs.as_slice(), [DocIR::HardLine]) {
        return docs;
    }

    let Some(block_node) = find_node_by_id(root, block_plan.syntax_id) else {
        return docs;
    };
    let direct_comments: Vec<Vec<DocIR>> = block_node
        .children()
        .filter_map(LuaComment::cast)
        .filter(|comment| !excluded_comment_ids.contains(&LuaSyntaxId::from_node(comment.syntax())))
        .map(|comment| render_comment_with_spacing(ctx, &comment, plan))
        .collect();
    prepend_comment_lines_to_block_docs(docs, direct_comments)
}

fn render_block_children(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    block_children: Option<&[LayoutNodePlan]>,
    plan: &FormatPlan,
) -> Vec<DocIR> {
    let mut docs = Vec::new();

    if let Some(children) = block_children {
        let rendered_children = render_aligned_block_layout_nodes(ctx, root, children, plan);
        if !rendered_children.is_empty() {
            let mut body = vec![ir::hard_line()];
            body.extend(rendered_children);
            docs.push(ir::indent(body));
            docs.push(ir::hard_line());
        } else {
            docs.push(ir::hard_line());
        }
    } else {
        docs.push(ir::hard_line());
    }
    docs
}

fn prepend_comment_lines_to_block_docs(
    body_docs: Vec<DocIR>,
    comment_lines: Vec<Vec<DocIR>>,
) -> Vec<DocIR> {
    if comment_lines.is_empty() {
        return body_docs;
    }

    let mut prefix = vec![ir::hard_line()];
    for (index, comment) in comment_lines.into_iter().enumerate() {
        if index > 0 {
            prefix.push(ir::hard_line());
        }
        prefix.extend(comment);
    }

    match body_docs.as_slice() {
        [DocIR::HardLine] => vec![ir::indent(prefix), ir::hard_line()],
        [DocIR::Indent(inner), DocIR::HardLine] => {
            let mut combined = prefix;
            if !inner.is_empty() {
                combined.push(ir::hard_line());
                combined.extend(inner.iter().skip(1).cloned());
            }
            vec![ir::indent(combined), ir::hard_line()]
        }
        _ => body_docs,
    }
}

fn render_aligned_block_layout_nodes(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    nodes: &[LayoutNodePlan],
    plan: &FormatPlan,
) -> Vec<DocIR> {
    let mut docs = Vec::new();
    let mut index = 0usize;

    while index < nodes.len() {
        if layout_node_should_be_skipped_in_block(nodes, index) {
            index += 1;
            continue;
        }

        if layout_comment_is_inline_trailing(root, nodes, index) {
            index += 1;
            continue;
        }

        if index > 0 {
            let blank_lines = count_blank_lines_before_layout_node(root, &nodes[index])
                .min(ctx.config.layout.max_blank_lines);
            docs.push(ir::hard_line());
            for _ in 0..blank_lines {
                docs.push(ir::hard_line());
            }
        }

        if let Some((group_docs, next_index)) =
            try_render_aligned_statement_group(ctx, root, nodes, index, plan)
        {
            docs.extend(group_docs);
            index = next_index;
            continue;
        }

        docs.extend(render_layout_node(ctx, root, &nodes[index], plan));
        index += 1;
    }

    docs
}

fn layout_node_should_be_skipped_in_block(nodes: &[LayoutNodePlan], index: usize) -> bool {
    matches!(nodes.get(index), Some(LayoutNodePlan::Syntax(syntax_plan)) if syntax_plan.kind == LuaSyntaxKind::EmptyStat)
        && nodes.iter().enumerate().any(|(other_index, node)| {
            other_index != index
                && matches!(node, LayoutNodePlan::Syntax(other_plan) if other_plan.kind != LuaSyntaxKind::EmptyStat)
        })
}

fn try_render_aligned_statement_group(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    nodes: &[LayoutNodePlan],
    start: usize,
    plan: &FormatPlan,
) -> Option<(Vec<DocIR>, usize)> {
    if layout_node_is_format_disabled(&nodes[start], plan) {
        return None;
    }

    let anchor_kind = statement_alignment_node_kind(&nodes[start])?;
    let allow_eq_alignment = ctx.config.align.continuous_assign_statement;
    let mut entries = Vec::new();
    let mut has_aligned_split = false;
    let mut has_aligned_comment_signal = false;

    let mut end = start;
    while end < nodes.len() {
        if layout_comment_is_inline_trailing(root, nodes, end) {
            end += 1;
            continue;
        }

        let node = &nodes[end];
        if layout_node_is_format_disabled(node, plan) {
            break;
        }
        if end > start && count_blank_lines_before_layout_node(root, node) > 0 {
            break;
        }
        if end > start && !can_join_statement_alignment_group(ctx, root, anchor_kind, node, plan) {
            break;
        }

        match node {
            LayoutNodePlan::Comment(comment_plan) => {
                let syntax = find_node_by_id(root, comment_plan.syntax_id)?;
                let comment = LuaComment::cast(syntax)?;
                entries.push(AlignEntry::Line {
                    content: render_comment_with_spacing(ctx, &comment, plan),
                    trailing: None,
                });
            }
            LayoutNodePlan::Syntax(syntax_plan) => {
                let syntax = find_node_by_id(root, syntax_plan.syntax_id)?;
                let trailing_comment =
                    extract_trailing_comment_rendered(ctx, syntax_plan, &syntax, plan).map(
                        |(docs, _, align_hint)| {
                            if align_hint {
                                has_aligned_comment_signal = true;
                            }
                            docs
                        },
                    );

                if allow_eq_alignment
                    && let Some((before, after)) =
                        render_statement_align_split(ctx, root, syntax_plan, plan)
                {
                    has_aligned_split = true;
                    entries.push(AlignEntry::Aligned {
                        before,
                        after,
                        trailing: trailing_comment,
                    });
                } else {
                    entries.push(AlignEntry::Line {
                        content: render_statement_line_content(ctx, root, syntax_plan, plan)
                            .unwrap_or_else(|| render_layout_node(ctx, root, node, plan)),
                        trailing: trailing_comment,
                    });
                }
            }
        }

        end += 1;
    }

    if !has_aligned_split && !has_aligned_comment_signal {
        return None;
    }

    Some((vec![ir::align_group(entries)], end))
}

fn layout_node_is_format_disabled(node: &LayoutNodePlan, plan: &FormatPlan) -> bool {
    let syntax_id = match node {
        LayoutNodePlan::Comment(comment) => comment.syntax_id,
        LayoutNodePlan::Syntax(syntax) => syntax.syntax_id,
    };

    plan.layout.format_disabled.contains(&syntax_id)
}

fn layout_comment_is_inline_trailing(
    root: &LuaSyntaxNode,
    nodes: &[LayoutNodePlan],
    index: usize,
) -> bool {
    if index == 0 {
        return false;
    }

    let Some(LayoutNodePlan::Comment(comment_plan)) = nodes.get(index) else {
        return false;
    };
    let Some(comment_node) = find_node_by_id(root, comment_plan.syntax_id) else {
        return false;
    };

    has_non_trivia_before_on_same_line_tokenwise(&comment_node)
        && !comment_node.text().contains_char('\n')
        && !has_inline_non_trivia_after(&comment_node)
}

fn can_join_statement_alignment_group(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    anchor_kind: LuaSyntaxKind,
    node: &LayoutNodePlan,
    plan: &FormatPlan,
) -> bool {
    match node {
        LayoutNodePlan::Comment(_) => ctx.config.comments.align_across_standalone_comments,
        LayoutNodePlan::Syntax(syntax_plan) => {
            if let Some(kind) = statement_alignment_node_kind(node) {
                if ctx.config.comments.align_same_kind_only && kind != anchor_kind {
                    return false;
                }

                if ctx.config.align.continuous_assign_statement {
                    return true;
                }

                let Some(syntax) = find_node_by_id(root, syntax_plan.syntax_id) else {
                    return false;
                };
                extract_trailing_comment_rendered(ctx, syntax_plan, &syntax, plan).is_some()
            } else {
                false
            }
        }
    }
}

fn statement_alignment_node_kind(node: &LayoutNodePlan) -> Option<LuaSyntaxKind> {
    match node {
        LayoutNodePlan::Syntax(syntax_plan)
            if matches!(
                syntax_plan.kind,
                LuaSyntaxKind::LocalStat | LuaSyntaxKind::AssignStat
            ) =>
        {
            Some(syntax_plan.kind)
        }
        _ => None,
    }
}
