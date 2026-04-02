use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaComment, LuaDoStat, LuaElseClauseStat, LuaElseIfClauseStat,
    LuaExpr, LuaForRangeStat, LuaForStat, LuaFuncStat, LuaIfStat, LuaKind, LuaLocalFuncStat,
    LuaLocalName, LuaRepeatStat, LuaStat, LuaSyntaxId, LuaSyntaxKind, LuaSyntaxNode,
    LuaSyntaxToken, LuaTokenKind, LuaWhileStat,
};
use rowan::NodeOrToken;

use crate::ir::{self, DocIR};

use super::super::expr;
use super::super::model::{RootFormatPlan, StatementExprListLayoutKind, SyntaxNodeLayoutPlan};
use super::FormatContext;
use super::helpers::{
    find_direct_child_plan_by_id, find_node_by_id, leading_inline_block_comment,
    next_significant_is_block, next_significant_is_inline_comment, previous_significant_token,
    render_expr, token_left_spacing_docs, token_right_spacing_docs,
};
use super::{
    comment_is_inline_after_anchor, first_direct_token, format_statement_value_expr,
    has_direct_comment_before_token, render_block_plan_without_excluded_comments,
    render_comment_with_spacing, render_direct_body_comment, render_header_exprs_with_leading_docs,
};

pub(super) fn render_while_stat(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_plan.syntax_id) else {
        return Vec::new();
    };
    let Some(stat) = LuaWhileStat::cast(node) else {
        return Vec::new();
    };

    render_while_source_order(ctx, root, stat.syntax(), syntax_plan, plan)
}

fn render_while_source_order(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let children = syntax.children_with_tokens().collect::<Vec<_>>();
    let mut docs = Vec::new();

    for (index, child) in children.iter().enumerate() {
        match child {
            NodeOrToken::Token(token) => {
                let kind = token.kind().to_token();
                if matches!(kind, LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine) {
                    continue;
                }

                if kind == LuaTokenKind::TkEnd {
                    continue;
                }

                if previous_significant_token(&children, index).is_some()
                    && !previous_significant_is_comment(&children, index)
                {
                    docs.extend(token_left_spacing_docs(plan, Some(token)));
                }
                docs.push(ir::source_token(token.clone()));

                if !next_significant_is_inline_comment(&children, index) {
                    docs.extend(token_right_spacing_docs(plan, Some(token)));
                }
            }
            NodeOrToken::Node(node) => match node.kind().into() {
                LuaSyntaxKind::Comment => {
                    if let Some(comment) = LuaComment::cast(node.clone()) {
                        let anchor = previous_significant_token(&children, index);
                        if comment_is_inline_after_anchor(root, anchor.as_ref(), comment.syntax()) {
                            docs.extend(inline_anchor_comment_separator_docs(
                                plan,
                                anchor.as_ref(),
                            ));
                            docs.push(ir::line_suffix(render_comment_with_spacing(
                                ctx, &comment, plan,
                            )));
                            if !next_significant_is_block(&children, index) {
                                docs.push(ir::hard_line());
                            }
                        } else if matches!(
                            anchor.as_ref().map(|token| token.kind().to_token()),
                            Some(LuaTokenKind::TkDo)
                        ) {
                            docs.extend(render_direct_body_comment(comment, ctx, plan));
                        } else {
                            if !docs.is_empty() {
                                docs.push(ir::hard_line());
                            }
                            docs.extend(render_comment_with_spacing(ctx, &comment, plan));
                            if matches!(
                                next_significant_token_kind(&children, index),
                                Some(LuaTokenKind::TkDo)
                            ) {
                                docs.push(ir::hard_line());
                            }
                        }
                    }
                }
                LuaSyntaxKind::Block => {
                    let block_plan =
                        find_direct_child_plan_by_id(syntax_plan, LuaSyntaxId::from_node(node));
                    let anchor = previous_significant_token(&children, index);
                    let inline_comment = anchor.as_ref().and_then(|anchor_token| {
                        leading_inline_block_comment(root, node, anchor_token)
                    });
                    if let Some(comment) = inline_comment.as_ref() {
                        docs.extend(inline_anchor_comment_separator_docs(plan, anchor.as_ref()));
                        docs.push(ir::line_suffix(render_comment_with_spacing(
                            ctx, comment, plan,
                        )));
                    }

                    let excluded_comment_ids = inline_comment
                        .as_ref()
                        .map(|comment| vec![LuaSyntaxId::from_node(comment.syntax())])
                        .unwrap_or_default();
                    docs.extend(render_block_plan_without_excluded_comments(
                        ctx,
                        root,
                        block_plan,
                        plan,
                        excluded_comment_ids.as_slice(),
                    ));
                }
                _ => {
                    if let Some(expr) = LuaExpr::cast(node.clone()) {
                        docs.extend(render_expr(ctx, plan, &expr));
                    }
                }
            },
        }
    }

    docs.push(ir::syntax_token(LuaTokenKind::TkEnd));
    docs
}

pub(super) fn render_for_stat(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_plan.syntax_id) else {
        return Vec::new();
    };
    let Some(stat) = LuaForStat::cast(node) else {
        return Vec::new();
    };

    let Some(expr_list_plan) = plan
        .layout
        .control_header_expr_lists
        .get(&syntax_plan.syntax_id)
        .copied()
    else {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    };

    render_for_source_order(
        ctx,
        root,
        stat.syntax(),
        syntax_plan,
        plan,
        &stat,
        expr_list_plan,
    )
}

fn render_for_source_order(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
    stat: &LuaForStat,
    expr_list_plan: super::super::model::StatementExprListLayoutPlan,
) -> Vec<DocIR> {
    let children = syntax.children_with_tokens().collect::<Vec<_>>();
    let exprs: Vec<_> = stat.get_iter_expr().collect();
    let comma_token = first_direct_token(stat.syntax(), LuaTokenKind::TkComma);
    let mut docs = Vec::new();
    let mut rendered_exprs = false;

    for (index, child) in children.iter().enumerate() {
        match child {
            NodeOrToken::Token(token) => {
                let kind = token.kind().to_token();
                if matches!(kind, LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine) {
                    continue;
                }

                if kind == LuaTokenKind::TkEnd {
                    continue;
                }

                if rendered_exprs && kind == LuaTokenKind::TkComma {
                    continue;
                }

                if previous_significant_token(&children, index).is_some()
                    && !previous_significant_is_comment(&children, index)
                {
                    docs.extend(token_left_spacing_docs(plan, Some(token)));
                }
                docs.push(ir::source_token(token.clone()));

                if !next_significant_is_inline_comment(&children, index) {
                    docs.extend(token_right_spacing_docs(plan, Some(token)));
                }
            }
            NodeOrToken::Node(node) => match node.kind().into() {
                LuaSyntaxKind::Comment => {
                    if let Some(comment) = LuaComment::cast(node.clone()) {
                        let anchor = previous_significant_token(&children, index);
                        if comment_is_inline_after_anchor(root, anchor.as_ref(), comment.syntax()) {
                            docs.extend(inline_anchor_comment_separator_docs(
                                plan,
                                anchor.as_ref(),
                            ));
                            docs.push(ir::line_suffix(render_comment_with_spacing(
                                ctx, &comment, plan,
                            )));
                            if !next_significant_is_block(&children, index) {
                                docs.push(ir::hard_line());
                            }
                        } else if matches!(
                            anchor.as_ref().map(|token| token.kind().to_token()),
                            Some(LuaTokenKind::TkDo)
                        ) {
                            docs.extend(render_direct_body_comment(comment, ctx, plan));
                        } else {
                            if !docs.is_empty() {
                                docs.push(ir::hard_line());
                            }
                            docs.extend(render_comment_with_spacing(ctx, &comment, plan));
                            if matches!(
                                next_significant_token_kind(&children, index),
                                Some(LuaTokenKind::TkDo)
                            ) {
                                docs.push(ir::hard_line());
                            }
                        }
                    }
                }
                LuaSyntaxKind::Block => {
                    let block_plan =
                        find_direct_child_plan_by_id(syntax_plan, LuaSyntaxId::from_node(node));
                    let anchor = previous_significant_token(&children, index);
                    let inline_comment = anchor.as_ref().and_then(|anchor_token| {
                        leading_inline_block_comment(root, node, anchor_token)
                    });
                    if let Some(comment) = inline_comment.as_ref() {
                        docs.extend(inline_anchor_comment_separator_docs(plan, anchor.as_ref()));
                        docs.push(ir::line_suffix(render_comment_with_spacing(
                            ctx, comment, plan,
                        )));
                    }

                    let excluded_comment_ids = inline_comment
                        .as_ref()
                        .map(|comment| vec![LuaSyntaxId::from_node(comment.syntax())])
                        .unwrap_or_default();
                    docs.extend(render_block_plan_without_excluded_comments(
                        ctx,
                        root,
                        block_plan,
                        plan,
                        excluded_comment_ids.as_slice(),
                    ));
                }
                _ => {
                    if LuaExpr::cast(node.clone()).is_some() {
                        if rendered_exprs {
                            continue;
                        }
                        docs.extend(render_source_order_header_expr_list(
                            ctx,
                            plan,
                            expr_list_plan,
                            comma_token.as_ref(),
                            exprs.as_slice(),
                        ));
                        rendered_exprs = true;
                    }
                }
            },
        }
    }

    docs.push(ir::syntax_token(LuaTokenKind::TkEnd));
    docs
}

pub(super) fn render_for_range_stat(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_plan.syntax_id) else {
        return Vec::new();
    };
    let Some(stat) = LuaForRangeStat::cast(node) else {
        return Vec::new();
    };

    let Some(expr_list_plan) = plan
        .layout
        .control_header_expr_lists
        .get(&syntax_plan.syntax_id)
        .copied()
    else {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    };

    render_for_range_source_order(
        ctx,
        root,
        stat.syntax(),
        syntax_plan,
        plan,
        &stat,
        expr_list_plan,
    )
}

fn render_for_range_source_order(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
    stat: &LuaForRangeStat,
    expr_list_plan: super::super::model::StatementExprListLayoutPlan,
) -> Vec<DocIR> {
    let children = syntax.children_with_tokens().collect::<Vec<_>>();
    let exprs: Vec<_> = stat.get_expr_list().collect();
    let mut docs = Vec::new();
    let mut rendered_exprs = false;

    let comma_token = first_direct_token(stat.syntax(), LuaTokenKind::TkComma);

    for (index, child) in children.iter().enumerate() {
        match child {
            NodeOrToken::Token(token) => {
                let kind = token.kind().to_token();
                if matches!(kind, LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine) {
                    continue;
                }

                if kind == LuaTokenKind::TkEnd {
                    continue;
                }

                if rendered_exprs && kind == LuaTokenKind::TkComma {
                    continue;
                }

                if previous_significant_token(&children, index).is_some()
                    && !previous_significant_is_comment(&children, index)
                {
                    docs.extend(token_left_spacing_docs(plan, Some(token)));
                }
                docs.push(ir::source_token(token.clone()));

                if !next_significant_is_inline_comment(&children, index) {
                    docs.extend(token_right_spacing_docs(plan, Some(token)));
                }
            }
            NodeOrToken::Node(node) => match node.kind().into() {
                LuaSyntaxKind::Comment => {
                    if let Some(comment) = LuaComment::cast(node.clone()) {
                        let anchor = previous_significant_token(&children, index);
                        if comment_is_inline_after_anchor(root, anchor.as_ref(), comment.syntax()) {
                            docs.extend(inline_anchor_comment_separator_docs(
                                plan,
                                anchor.as_ref(),
                            ));
                            docs.push(ir::line_suffix(render_comment_with_spacing(
                                ctx, &comment, plan,
                            )));
                            if !next_significant_is_block(&children, index) {
                                docs.push(ir::hard_line());
                            }
                        } else if matches!(
                            anchor.as_ref().map(|token| token.kind().to_token()),
                            Some(LuaTokenKind::TkDo)
                        ) {
                            docs.extend(render_direct_body_comment(comment, ctx, plan));
                        } else {
                            if !docs.is_empty() {
                                docs.push(ir::hard_line());
                            }
                            docs.extend(render_comment_with_spacing(ctx, &comment, plan));
                            if matches!(
                                next_significant_token_kind(&children, index),
                                Some(LuaTokenKind::TkIn | LuaTokenKind::TkDo)
                            ) {
                                docs.push(ir::hard_line());
                            }
                        }
                    }
                }
                LuaSyntaxKind::Block => {
                    let block_plan =
                        find_direct_child_plan_by_id(syntax_plan, LuaSyntaxId::from_node(node));
                    let anchor = previous_significant_token(&children, index);
                    let inline_comment = anchor.as_ref().and_then(|anchor_token| {
                        leading_inline_block_comment(root, node, anchor_token)
                    });
                    if let Some(comment) = inline_comment.as_ref() {
                        docs.extend(inline_anchor_comment_separator_docs(plan, anchor.as_ref()));
                        docs.push(ir::line_suffix(render_comment_with_spacing(
                            ctx, comment, plan,
                        )));
                    }

                    let excluded_comment_ids = inline_comment
                        .as_ref()
                        .map(|comment| vec![LuaSyntaxId::from_node(comment.syntax())])
                        .unwrap_or_default();
                    docs.extend(render_block_plan_without_excluded_comments(
                        ctx,
                        root,
                        block_plan,
                        plan,
                        excluded_comment_ids.as_slice(),
                    ));
                }
                _ => {
                    if LuaExpr::cast(node.clone()).is_some() {
                        if rendered_exprs {
                            continue;
                        }
                        docs.extend(render_source_order_header_expr_list(
                            ctx,
                            plan,
                            expr_list_plan,
                            comma_token.as_ref(),
                            exprs.as_slice(),
                        ));
                        rendered_exprs = true;
                    }
                }
            },
        }
    }

    docs.push(ir::syntax_token(LuaTokenKind::TkEnd));
    docs
}

pub(super) fn render_repeat_stat(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_plan.syntax_id) else {
        return Vec::new();
    };
    let Some(stat) = LuaRepeatStat::cast(node) else {
        return Vec::new();
    };

    render_repeat_source_order(ctx, root, stat.syntax(), syntax_plan, plan)
}

fn render_repeat_source_order(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let children = syntax.children_with_tokens().collect::<Vec<_>>();
    let mut docs = Vec::new();

    for (index, child) in children.iter().enumerate() {
        match child {
            NodeOrToken::Token(token) => {
                let kind = token.kind().to_token();
                if matches!(kind, LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine) {
                    continue;
                }

                if previous_significant_token(&children, index).is_some()
                    && !previous_significant_is_comment(&children, index)
                    && !(kind == LuaTokenKind::TkUntil
                        && previous_significant_is_block(&children, index))
                {
                    docs.extend(token_left_spacing_docs(plan, Some(token)));
                }
                docs.push(ir::source_token(token.clone()));

                if !next_significant_is_inline_comment(&children, index) {
                    docs.extend(token_right_spacing_docs(plan, Some(token)));
                }
            }
            NodeOrToken::Node(node) => match node.kind().into() {
                LuaSyntaxKind::Comment => {
                    if let Some(comment) = LuaComment::cast(node.clone()) {
                        let anchor = previous_significant_token(&children, index);
                        if comment_is_inline_after_anchor(root, anchor.as_ref(), comment.syntax()) {
                            docs.extend(inline_anchor_comment_separator_docs(
                                plan,
                                anchor.as_ref(),
                            ));
                            docs.push(ir::line_suffix(render_comment_with_spacing(
                                ctx, &comment, plan,
                            )));
                            if !next_significant_is_block(&children, index) {
                                docs.push(ir::hard_line());
                            }
                        } else if matches!(
                            anchor.as_ref().map(|token| token.kind().to_token()),
                            Some(LuaTokenKind::TkRepeat)
                        ) {
                            docs.extend(render_direct_body_comment(comment, ctx, plan));
                        } else {
                            if !docs.is_empty() {
                                docs.push(ir::hard_line());
                            }
                            docs.extend(render_comment_with_spacing(ctx, &comment, plan));
                            if matches!(
                                next_significant_token_kind(&children, index),
                                Some(LuaTokenKind::TkUntil)
                            ) {
                                docs.push(ir::hard_line());
                            }
                        }
                    }
                }
                LuaSyntaxKind::Block => {
                    let block_plan =
                        find_direct_child_plan_by_id(syntax_plan, LuaSyntaxId::from_node(node));
                    let anchor = previous_significant_token(&children, index);
                    let inline_comment = anchor.as_ref().and_then(|anchor_token| {
                        leading_inline_block_comment(root, node, anchor_token)
                    });
                    if let Some(comment) = inline_comment.as_ref() {
                        docs.extend(inline_anchor_comment_separator_docs(plan, anchor.as_ref()));
                        docs.push(ir::line_suffix(render_comment_with_spacing(
                            ctx, comment, plan,
                        )));
                    }

                    let excluded_comment_ids = inline_comment
                        .as_ref()
                        .map(|comment| vec![LuaSyntaxId::from_node(comment.syntax())])
                        .unwrap_or_default();
                    docs.extend(render_block_plan_without_excluded_comments(
                        ctx,
                        root,
                        block_plan,
                        plan,
                        excluded_comment_ids.as_slice(),
                    ));
                }
                _ => {
                    if let Some(expr) = LuaExpr::cast(node.clone()) {
                        docs.extend(render_expr(ctx, plan, &expr));
                    }
                }
            },
        }
    }

    docs
}

pub(super) fn render_func_stat(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_plan.syntax_id) else {
        return Vec::new();
    };
    let Some(stat) = LuaFuncStat::cast(node) else {
        return Vec::new();
    };
    let Some(closure) = stat.get_closure() else {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    };

    let params_open_token = closure
        .get_params_list()
        .and_then(|params| first_direct_token(params.syntax(), LuaTokenKind::TkLeftParen));
    if has_direct_comment_before_token(stat.syntax(), params_open_token.as_ref()) {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    }

    render_named_function_stat_source_order(ctx, root, stat.syntax(), syntax_plan, plan)
}

pub(super) fn render_local_func_stat(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_plan.syntax_id) else {
        return Vec::new();
    };
    let Some(stat) = LuaLocalFuncStat::cast(node) else {
        return Vec::new();
    };
    let Some(closure) = stat.get_closure() else {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    };

    let params_open_token = closure
        .get_params_list()
        .and_then(|params| first_direct_token(params.syntax(), LuaTokenKind::TkLeftParen));
    if has_direct_comment_before_token(stat.syntax(), params_open_token.as_ref()) {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    }

    render_named_function_stat_source_order(ctx, root, stat.syntax(), syntax_plan, plan)
}

fn render_named_function_stat_source_order(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let children = syntax.children_with_tokens().collect::<Vec<_>>();
    let mut docs = Vec::new();

    for (index, child) in children.iter().enumerate() {
        match child {
            NodeOrToken::Token(token) => {
                let kind = token.kind().to_token();
                if matches!(kind, LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine) {
                    continue;
                }

                let previous_token = previous_significant_token(&children, index);
                if previous_token.is_some()
                    && !previous_significant_is_comment(&children, index)
                    && !matches!(
                        (
                            previous_token.as_ref().map(|token| token.kind().to_token()),
                            kind
                        ),
                        (
                            Some(LuaTokenKind::TkLocal | LuaTokenKind::TkGlobal),
                            LuaTokenKind::TkFunction
                        )
                    )
                {
                    docs.extend(token_left_spacing_docs(plan, Some(token)));
                }
                docs.push(ir::source_token(token.clone()));

                if !next_significant_is_inline_comment(&children, index) {
                    docs.extend(token_right_spacing_docs(plan, Some(token)));
                }
            }
            NodeOrToken::Node(node) => match node.kind().into() {
                LuaSyntaxKind::ClosureExpr => {
                    let closure_plan =
                        find_direct_child_plan_by_id(syntax_plan, LuaSyntaxId::from_node(node));
                    let Some(closure_plan) = closure_plan else {
                        docs.push(ir::source_node_trimmed(node.clone()));
                        continue;
                    };
                    let Some(closure) = emmylua_parser::LuaClosureExpr::cast(node.clone()) else {
                        continue;
                    };
                    docs.extend(render_named_function_closure_tail_source_order(
                        ctx,
                        root,
                        &closure,
                        closure_plan,
                        plan,
                    ));
                }
                _ => {
                    if let Some(expr) = LuaExpr::cast(node.clone()) {
                        docs.extend(render_expr(ctx, plan, &expr));
                    } else if let Some(local_name) = LuaLocalName::cast(node.clone()) {
                        docs.extend(format_local_name_ir(&local_name));
                    }
                }
            },
        }
    }

    docs
}

pub(super) fn render_do_stat(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_plan.syntax_id) else {
        return Vec::new();
    };
    let Some(stat) = LuaDoStat::cast(node) else {
        return Vec::new();
    };

    render_do_source_order(ctx, root, stat.syntax(), syntax_plan, plan)
}

fn render_do_source_order(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let children = syntax.children_with_tokens().collect::<Vec<_>>();
    let mut docs = Vec::new();

    for (index, child) in children.iter().enumerate() {
        match child {
            NodeOrToken::Token(token) => {
                let kind = token.kind().to_token();
                if matches!(kind, LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine) {
                    continue;
                }

                if kind == LuaTokenKind::TkEnd {
                    continue;
                }

                if previous_significant_token(&children, index).is_some()
                    && !previous_significant_is_comment(&children, index)
                {
                    docs.extend(token_left_spacing_docs(plan, Some(token)));
                }
                docs.push(ir::source_token(token.clone()));

                if !next_significant_is_inline_comment(&children, index) {
                    docs.extend(token_right_spacing_docs(plan, Some(token)));
                }
            }
            NodeOrToken::Node(node) => match node.kind().into() {
                LuaSyntaxKind::Comment => {
                    if let Some(comment) = LuaComment::cast(node.clone()) {
                        let anchor = previous_significant_token(&children, index);
                        if comment_is_inline_after_anchor(root, anchor.as_ref(), comment.syntax()) {
                            docs.extend(inline_anchor_comment_separator_docs(
                                plan,
                                anchor.as_ref(),
                            ));
                            docs.push(ir::line_suffix(render_comment_with_spacing(
                                ctx, &comment, plan,
                            )));
                            if !next_significant_is_block(&children, index) {
                                docs.push(ir::hard_line());
                            }
                        } else if matches!(
                            anchor.as_ref().map(|token| token.kind().to_token()),
                            Some(LuaTokenKind::TkDo)
                        ) {
                            docs.extend(render_direct_body_comment(comment, ctx, plan));
                        } else {
                            if !docs.is_empty() {
                                docs.push(ir::hard_line());
                            }
                            docs.extend(render_comment_with_spacing(ctx, &comment, plan));
                            if matches!(
                                next_significant_token_kind(&children, index),
                                Some(LuaTokenKind::TkEnd)
                            ) {
                                docs.push(ir::hard_line());
                            }
                        }
                    }
                }
                LuaSyntaxKind::Block => {
                    let block_plan =
                        find_direct_child_plan_by_id(syntax_plan, LuaSyntaxId::from_node(node));
                    let anchor = previous_significant_token(&children, index);
                    let inline_comment = anchor.as_ref().and_then(|anchor_token| {
                        leading_inline_block_comment(root, node, anchor_token)
                    });
                    if let Some(comment) = inline_comment.as_ref() {
                        docs.extend(inline_anchor_comment_separator_docs(plan, anchor.as_ref()));
                        docs.push(ir::line_suffix(render_comment_with_spacing(
                            ctx, comment, plan,
                        )));
                    }

                    let excluded_comment_ids = inline_comment
                        .as_ref()
                        .map(|comment| vec![LuaSyntaxId::from_node(comment.syntax())])
                        .unwrap_or_default();
                    docs.extend(render_block_plan_without_excluded_comments(
                        ctx,
                        root,
                        block_plan,
                        plan,
                        excluded_comment_ids.as_slice(),
                    ));
                }
                _ => {}
            },
        }
    }

    docs.push(ir::syntax_token(LuaTokenKind::TkEnd));
    docs
}

pub(super) fn render_if_stat(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_plan.syntax_id) else {
        return Vec::new();
    };
    let Some(stat) = LuaIfStat::cast(node) else {
        return Vec::new();
    };

    if let Some(preserved) = try_preserve_single_line_if_body(ctx, &stat) {
        return preserved;
    }

    render_if_clause_source_order(ctx, root, stat.syntax(), syntax_plan, plan)
}

fn render_if_clause_source_order(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let children = syntax.children_with_tokens().collect::<Vec<_>>();
    let mut docs = Vec::new();

    for (index, child) in children.iter().enumerate() {
        match child {
            NodeOrToken::Token(token) => {
                let kind = token.kind().to_token();
                if matches!(kind, LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine) {
                    continue;
                }

                if previous_significant_token(&children, index).is_some()
                    && !previous_significant_is_comment(&children, index)
                {
                    docs.extend(token_left_spacing_docs(plan, Some(token)));
                }
                docs.push(ir::source_token(token.clone()));

                if !next_significant_is_inline_comment(&children, index) {
                    docs.extend(token_right_spacing_docs(plan, Some(token)));
                }
            }
            NodeOrToken::Node(node) => match node.kind().into() {
                LuaSyntaxKind::Comment => {
                    if let Some(comment) = LuaComment::cast(node.clone()) {
                        let anchor = previous_significant_token(&children, index);
                        if comment_is_inline_after_anchor(root, anchor.as_ref(), comment.syntax()) {
                            docs.extend(inline_anchor_comment_separator_docs(
                                plan,
                                anchor.as_ref(),
                            ));
                            docs.push(ir::line_suffix(render_comment_with_spacing(
                                ctx, &comment, plan,
                            )));
                            if !next_significant_is_block(&children, index) {
                                docs.push(ir::hard_line());
                            }
                        } else if matches!(
                            anchor.as_ref().map(|token| token.kind().to_token()),
                            Some(LuaTokenKind::TkThen | LuaTokenKind::TkElse)
                        ) {
                            docs.extend(render_direct_body_comment(comment, ctx, plan));
                        } else {
                            if !docs.is_empty() {
                                docs.push(ir::hard_line());
                            }
                            docs.extend(render_comment_with_spacing(ctx, &comment, plan));
                            if matches!(
                                next_significant_token_kind(&children, index),
                                Some(LuaTokenKind::TkThen)
                            ) {
                                docs.push(ir::hard_line());
                            }
                        }
                    }
                }
                LuaSyntaxKind::Block => {
                    let block_plan =
                        find_direct_child_plan_by_id(syntax_plan, LuaSyntaxId::from_node(node));
                    let anchor = previous_significant_token(&children, index);
                    let inline_comment = anchor.as_ref().and_then(|anchor_token| {
                        leading_inline_block_comment(root, node, anchor_token)
                    });
                    if let Some(comment) = inline_comment.as_ref() {
                        docs.extend(inline_anchor_comment_separator_docs(plan, anchor.as_ref()));
                        docs.push(ir::line_suffix(render_comment_with_spacing(
                            ctx, comment, plan,
                        )));
                    }

                    let excluded_comment_ids = inline_comment
                        .as_ref()
                        .map(|comment| vec![LuaSyntaxId::from_node(comment.syntax())])
                        .unwrap_or_default();
                    docs.extend(render_block_plan_without_excluded_comments(
                        ctx,
                        root,
                        block_plan,
                        plan,
                        excluded_comment_ids.as_slice(),
                    ));
                }
                LuaSyntaxKind::ElseIfClauseStat => {
                    let clause_plan =
                        find_direct_child_plan_by_id(syntax_plan, LuaSyntaxId::from_node(node));
                    let Some(clause_plan) = clause_plan else {
                        docs.push(ir::source_node_trimmed(node.clone()));
                        continue;
                    };
                    let Some(clause) = LuaElseIfClauseStat::cast(node.clone()) else {
                        continue;
                    };
                    docs.extend(render_if_clause_source_order(
                        ctx,
                        root,
                        clause.syntax(),
                        clause_plan,
                        plan,
                    ));
                }
                LuaSyntaxKind::ElseClauseStat => {
                    let clause_plan =
                        find_direct_child_plan_by_id(syntax_plan, LuaSyntaxId::from_node(node));
                    let Some(clause_plan) = clause_plan else {
                        docs.push(ir::source_node_trimmed(node.clone()));
                        continue;
                    };
                    let Some(clause) = LuaElseClauseStat::cast(node.clone()) else {
                        continue;
                    };
                    docs.extend(render_if_clause_source_order(
                        ctx,
                        root,
                        clause.syntax(),
                        clause_plan,
                        plan,
                    ));
                }
                _ => {
                    if let Some(expr) = LuaExpr::cast(node.clone()) {
                        docs.extend(render_expr(ctx, plan, &expr));
                    }
                }
            },
        }
    }

    docs
}

fn render_named_function_closure_tail_source_order(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    closure: &emmylua_parser::LuaClosureExpr,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let children = closure.syntax().children_with_tokens().collect::<Vec<_>>();
    let mut docs = Vec::new();
    let mut saw_tail_comment = false;
    let mut saw_block = false;

    for (index, child) in children.iter().enumerate() {
        match child {
            NodeOrToken::Token(token) => {
                let kind = token.kind().to_token();
                if matches!(kind, LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine) {
                    continue;
                }

                if kind == LuaTokenKind::TkEnd {
                    continue;
                }

                if previous_significant_token(&children, index).is_some()
                    && !previous_significant_is_comment(&children, index)
                {
                    docs.extend(token_left_spacing_docs(plan, Some(token)));
                }
                docs.push(ir::source_token(token.clone()));

                if !next_significant_is_inline_comment(&children, index) {
                    docs.extend(token_right_spacing_docs(plan, Some(token)));
                }
            }
            NodeOrToken::Node(node) => match node.kind().into() {
                LuaSyntaxKind::Comment => {
                    saw_tail_comment = true;
                    if let Some(comment) = LuaComment::cast(node.clone()) {
                        let anchor = previous_significant_token(&children, index);
                        if comment_is_inline_after_anchor(root, anchor.as_ref(), comment.syntax()) {
                            docs.extend(inline_anchor_comment_separator_docs(
                                plan,
                                anchor.as_ref(),
                            ));
                            docs.push(ir::line_suffix(render_comment_with_spacing(
                                ctx, &comment, plan,
                            )));
                            if !next_significant_is_block(&children, index) {
                                docs.push(ir::hard_line());
                            }
                        } else if matches!(
                            anchor.as_ref().map(|token| token.kind().to_token()),
                            Some(LuaTokenKind::TkRightParen)
                        ) {
                            docs.extend(render_direct_body_comment(comment, ctx, plan));
                        } else {
                            if !docs.is_empty() {
                                docs.push(ir::hard_line());
                            }
                            docs.extend(render_comment_with_spacing(ctx, &comment, plan));
                            if matches!(
                                next_significant_token_kind(&children, index),
                                Some(LuaTokenKind::TkEnd)
                            ) {
                                docs.push(ir::hard_line());
                            }
                        }
                    }
                }
                LuaSyntaxKind::ParamList => {
                    if let Some(params) = emmylua_parser::LuaParamList::cast(node.clone()) {
                        let open = first_direct_token(params.syntax(), LuaTokenKind::TkLeftParen);
                        docs.extend(token_left_spacing_docs(plan, open.as_ref()));
                        docs.extend(expr::format_param_list_ir(ctx, plan, &params));
                    }
                }
                LuaSyntaxKind::Block => {
                    saw_block = true;
                    let block_plan =
                        find_direct_child_plan_by_id(syntax_plan, LuaSyntaxId::from_node(node));
                    let anchor = previous_significant_token(&children, index);
                    let inline_comment = anchor.as_ref().and_then(|anchor_token| {
                        leading_inline_block_comment(root, node, anchor_token)
                    });
                    if let Some(comment) = inline_comment.as_ref() {
                        docs.extend(inline_anchor_comment_separator_docs(plan, anchor.as_ref()));
                        docs.push(ir::line_suffix(render_comment_with_spacing(
                            ctx, comment, plan,
                        )));
                    }

                    let excluded_comment_ids = inline_comment
                        .as_ref()
                        .map(|comment| vec![LuaSyntaxId::from_node(comment.syntax())])
                        .unwrap_or_default();
                    docs.extend(render_block_plan_without_excluded_comments(
                        ctx,
                        root,
                        block_plan,
                        plan,
                        excluded_comment_ids.as_slice(),
                    ));
                }
                _ => {}
            },
        }
    }

    if !saw_block && !saw_tail_comment {
        docs.push(ir::space());
    }
    docs.push(ir::syntax_token(LuaTokenKind::TkEnd));
    docs
}

fn format_local_name_ir(local_name: &LuaLocalName) -> Vec<DocIR> {
    let mut docs = Vec::new();
    if let Some(token) = local_name.get_name_token() {
        docs.push(ir::source_token(token.syntax().clone()));
    }
    if let Some(attrib) = local_name.get_attrib() {
        docs.push(ir::space());
        docs.push(ir::text("<"));
        if let Some(name_token) = attrib.get_name_token() {
            docs.push(ir::source_token(name_token.syntax().clone()));
        }
        docs.push(ir::text(">"));
    }
    docs
}

fn render_source_order_header_expr_list(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    expr_list_plan: super::super::model::StatementExprListLayoutPlan,
    comma_token: Option<&LuaSyntaxToken>,
    exprs: &[LuaExpr],
) -> Vec<DocIR> {
    let expr_docs: Vec<Vec<DocIR>> = exprs
        .iter()
        .enumerate()
        .map(|(index, expr)| {
            format_statement_value_expr(
                ctx,
                plan,
                expr,
                index == 0
                    && matches!(
                        expr_list_plan.kind,
                        StatementExprListLayoutKind::PreserveFirstMultiline
                    ),
            )
        })
        .collect();

    render_header_exprs_with_leading_docs(
        ctx,
        plan,
        expr_list_plan,
        Vec::new(),
        comma_token,
        expr_docs,
    )
}

fn inline_anchor_comment_separator_docs(
    plan: &RootFormatPlan,
    anchor_token: Option<&LuaSyntaxToken>,
) -> Vec<DocIR> {
    if matches!(
        anchor_token.map(|token| token.kind().to_token()),
        Some(LuaTokenKind::TkIn)
    ) {
        return vec![ir::space()];
    }

    if token_right_spacing_docs(plan, anchor_token).is_empty() {
        vec![ir::space()]
    } else {
        Vec::new()
    }
}

fn previous_significant_is_comment(
    children: &[NodeOrToken<LuaSyntaxNode, LuaSyntaxToken>],
    index: usize,
) -> bool {
    children[..index]
        .iter()
        .rev()
        .find_map(|child| match child {
            NodeOrToken::Token(token)
                if matches!(
                    token.kind().to_token(),
                    LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine
                ) =>
            {
                None
            }
            NodeOrToken::Node(node) => Some(node.kind() == LuaKind::Syntax(LuaSyntaxKind::Comment)),
            NodeOrToken::Token(_) => Some(false),
        })
        .unwrap_or(false)
}

fn previous_significant_is_block(
    children: &[NodeOrToken<LuaSyntaxNode, LuaSyntaxToken>],
    index: usize,
) -> bool {
    children[..index]
        .iter()
        .rev()
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
            NodeOrToken::Token(_) => Some(false),
        })
        .unwrap_or(false)
}

fn next_significant_token_kind(
    children: &[NodeOrToken<LuaSyntaxNode, LuaSyntaxToken>],
    index: usize,
) -> Option<LuaTokenKind> {
    children[index + 1..].iter().find_map(|child| match child {
        NodeOrToken::Token(token)
            if matches!(
                token.kind().to_token(),
                LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine
            ) =>
        {
            None
        }
        NodeOrToken::Token(token) => Some(token.kind().to_token()),
        NodeOrToken::Node(_) => None,
    })
}

fn is_block_like_expr(expr: &LuaExpr) -> bool {
    matches!(expr, LuaExpr::ClosureExpr(_) | LuaExpr::TableExpr(_))
}

fn try_preserve_single_line_if_body(ctx: &FormatContext, stat: &LuaIfStat) -> Option<Vec<DocIR>> {
    if stat.syntax().text().contains_char('\n') {
        return None;
    }

    let text_len: u32 = stat.syntax().text().len().into();
    let reserve_width = if ctx.config.layout.max_line_width > 40 {
        8
    } else {
        4
    };
    if text_len as usize + reserve_width > ctx.config.layout.max_line_width {
        return None;
    }

    if stat.get_else_clause().is_some() || stat.get_else_if_clause_list().next().is_some() {
        return None;
    }

    let block = stat.get_block()?;
    let mut stats = block.get_stats();
    let only_stat = stats.next()?;
    if stats.next().is_some() {
        return None;
    }

    if !is_simple_single_line_if_body(&only_stat) {
        return None;
    }

    Some(vec![ir::source_node(stat.syntax().clone())])
}

fn is_simple_single_line_if_body(stat: &LuaStat) -> bool {
    match stat {
        LuaStat::ReturnStat(_)
        | LuaStat::BreakStat(_)
        | LuaStat::GotoStat(_)
        | LuaStat::CallExprStat(_) => true,
        LuaStat::LocalStat(local) => {
            let exprs: Vec<_> = local.get_value_exprs().collect();
            exprs.len() <= 1 && exprs.iter().all(|expr| !is_block_like_expr(expr))
        }
        LuaStat::AssignStat(assign) => {
            let (_, exprs) = assign.get_var_and_expr_list();
            exprs.len() <= 1 && exprs.iter().all(|expr| !is_block_like_expr(expr))
        }
        _ => false,
    }
}
