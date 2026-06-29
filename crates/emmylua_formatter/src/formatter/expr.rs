use emmylua_parser::{
    BinaryOperator, LuaAssignStat, LuaAstNode, LuaAstToken, LuaBinaryExpr, LuaCallArgList,
    LuaCallExpr, LuaClosureExpr, LuaComment, LuaExpr, LuaIndexExpr, LuaIndexKey, LuaKind,
    LuaLiteralExpr, LuaLiteralToken, LuaLocalStat, LuaNameExpr, LuaParamList, LuaParenExpr,
    LuaSingleArgExpr, LuaStat, LuaSyntaxId, LuaSyntaxKind, LuaSyntaxNode, LuaSyntaxToken,
    LuaTableExpr, LuaTableField, LuaTernaryExpr, LuaTokenKind, LuaUnaryExpr,
};
use rowan::TextRange;

use crate::config::{
    ExpandStrategy, QuoteStyle, SimpleLambdaSingleLine, SingleArgCallParens, TrailingComma,
};
use crate::ir;
use crate::printer::measure_docs;
use ir::{AlignEntry, DocIR};

use super::FormatContext;
use super::model::{ExprSequenceLayoutPlan, FormatPlan, TokenSpacingExpected};
use super::render;
use super::sequence::{
    DelimitedSequenceLayout, SequenceLayoutCandidates, SequenceLayoutPolicy,
    choose_sequence_layout, format_delimited_sequence,
};
use super::spacing::{SpaceRule, space_around_binary_op};
use super::trivia::{
    count_blank_lines_before, has_non_trivia_before_on_same_line_tokenwise,
    node_has_direct_comment_child, source_line_prefix_width, trailing_gap_requests_alignment,
};

#[derive(Clone, Copy, Default)]
pub struct ExprFormatOptions {
    pub prefer_chain_break: bool,
}

pub fn format_expr(ctx: &FormatContext, plan: &FormatPlan, expr: &LuaExpr) -> Vec<DocIR> {
    format_expr_with_options(ctx, plan, expr, ExprFormatOptions::default())
}

pub fn format_expr_with_options(
    ctx: &FormatContext,
    plan: &FormatPlan,
    expr: &LuaExpr,
    options: ExprFormatOptions,
) -> Vec<DocIR> {
    if expr_is_chain_root(expr)
        && expr_may_need_chain_format(expr)
        && !chain_has_direct_comments(expr)
        && let Some(chain_docs) = try_format_chain_expr(ctx, plan, expr, options)
    {
        return chain_docs;
    }

    match expr {
        LuaExpr::NameExpr(expr) => format_name_expr(expr),
        LuaExpr::LiteralExpr(expr) => format_literal_expr(ctx, expr),
        LuaExpr::BinaryExpr(expr) => format_binary_expr(ctx, plan, expr),
        LuaExpr::UnaryExpr(expr) => format_unary_expr(ctx, plan, expr),
        LuaExpr::ParenExpr(expr) => format_paren_expr(ctx, plan, expr),
        LuaExpr::IndexExpr(expr) => format_index_expr(ctx, plan, expr),
        LuaExpr::CallExpr(expr) => format_call_expr(ctx, plan, expr),
        LuaExpr::TableExpr(expr) => format_table_expr(ctx, plan, expr),
        LuaExpr::ClosureExpr(expr) => format_closure_expr(ctx, plan, expr),
        LuaExpr::TernaryExpr(expr) => format_ternary_expr(ctx, plan, expr),
    }
}

fn format_name_expr(expr: &LuaNameExpr) -> Vec<DocIR> {
    expr.get_name_token()
        .map(|token| vec![ir::source_token(token.syntax().clone())])
        .unwrap_or_default()
}

type EqSplitDocs = (Vec<DocIR>, Vec<DocIR>);
type ChainSegments = (LuaExpr, Vec<ChainSegment>);

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChainSegmentKind {
    Access,
    Call,
}

#[derive(Clone)]
struct ChainSegment {
    docs: Vec<DocIR>,
    kind: ChainSegmentKind,
    attach_to_root: bool,
}

fn format_literal_expr(ctx: &FormatContext, expr: &LuaLiteralExpr) -> Vec<DocIR> {
    let Some(LuaLiteralToken::String(token)) = expr.get_literal() else {
        return vec![ir::source_node_trimmed(expr.syntax().clone())];
    };

    let text = token.syntax().text().to_string();
    let Some(original_quote) = text.chars().next() else {
        return vec![ir::source_node_trimmed(expr.syntax().clone())];
    };
    if token.syntax().kind() == LuaTokenKind::TkLongString.into()
        || !matches!(original_quote, '\'' | '"')
    {
        return vec![ir::source_node_trimmed(expr.syntax().clone())];
    }

    let preferred_quote = match ctx.config.output.quote_style {
        QuoteStyle::Preserve => return vec![ir::source_node_trimmed(expr.syntax().clone())],
        QuoteStyle::Double => '"',
        QuoteStyle::Single => '\'',
    };
    if preferred_quote == original_quote {
        return vec![ir::source_node_trimmed(expr.syntax().clone())];
    }

    let raw_body = &text[1..text.len() - 1];
    if raw_short_string_contains_unescaped_quote(raw_body, preferred_quote) {
        return vec![ir::source_node_trimmed(expr.syntax().clone())];
    }

    vec![ir::text(rewrite_short_string_quotes(
        raw_body,
        original_quote,
        preferred_quote,
    ))]
}

fn format_binary_expr(ctx: &FormatContext, plan: &FormatPlan, expr: &LuaBinaryExpr) -> Vec<DocIR> {
    if node_has_direct_comment_child(expr.syntax()) {
        return vec![ir::source_node_trimmed(expr.syntax().clone())];
    }

    if let Some(flattened) = try_format_flat_binary_chain(ctx, plan, expr) {
        return flattened;
    }

    let Some((left, right)) = expr.get_exprs() else {
        return vec![ir::source_node_trimmed(expr.syntax().clone())];
    };
    let Some(op_token) = expr.get_op_token() else {
        return vec![ir::source_node_trimmed(expr.syntax().clone())];
    };

    let left_docs = format_expr(ctx, plan, &left);
    let right_docs = format_expr(ctx, plan, &right);
    let space_rule = space_around_binary_op(op_token.get_op(), ctx.config);
    let force_space_before = op_token.get_op() == BinaryOperator::OpConcat
        && space_rule == SpaceRule::NoSpace
        && left
            .syntax()
            .last_token()
            .is_some_and(|token| token.kind() == LuaTokenKind::TkFloat.into());

    if ir::ir_has_forced_line_break(&left_docs)
        && should_attach_short_binary_tail(op_token.get_op(), &right, &right_docs)
    {
        let mut docs = left_docs;
        if force_space_before {
            docs.push(ir::space());
        } else {
            docs.push(space_rule.to_ir());
        }
        docs.push(ir::source_token(op_token.syntax().clone()));
        docs.push(space_rule.to_ir());
        docs.extend(right_docs);
        return docs;
    }

    if should_attach_short_binary_tail(op_token.get_op(), &right, &right_docs)
        && should_attach_after_multiline_left_expr(&left)
    {
        let mut docs = left_docs;
        if force_space_before {
            docs.push(ir::space());
        } else {
            docs.push(space_rule.to_ir());
        }
        docs.push(ir::source_token(op_token.syntax().clone()));
        docs.push(space_rule.to_ir());
        docs.extend(right_docs);
        return docs;
    }

    vec![ir::group(vec![
        ir::list(left_docs),
        ir::indent(vec![
            continuation_break_ir(force_space_before || space_rule != SpaceRule::NoSpace),
            ir::source_token(op_token.syntax().clone()),
            space_rule.to_ir(),
            ir::list(right_docs),
        ]),
    ])]
}

fn raw_short_string_contains_unescaped_quote(raw_body: &str, quote: char) -> bool {
    let mut consecutive_backslashes = 0usize;

    for ch in raw_body.chars() {
        if ch == '\\' {
            consecutive_backslashes += 1;
            continue;
        }

        let is_escaped = consecutive_backslashes % 2 == 1;
        consecutive_backslashes = 0;

        if ch == quote && !is_escaped {
            return true;
        }
    }

    false
}

fn rewrite_short_string_quotes(raw_body: &str, original_quote: char, quote: char) -> String {
    let mut result = String::with_capacity(raw_body.len() + 2);
    result.push(quote);
    let mut consecutive_backslashes = 0usize;

    for ch in raw_body.chars() {
        if ch == '\\' {
            consecutive_backslashes += 1;
            continue;
        }

        if ch == original_quote && consecutive_backslashes % 2 == 1 {
            for _ in 0..(consecutive_backslashes - 1) {
                result.push('\\');
            }
        } else {
            for _ in 0..consecutive_backslashes {
                result.push('\\');
            }
        }

        consecutive_backslashes = 0;
        result.push(ch);
    }

    for _ in 0..consecutive_backslashes {
        result.push('\\');
    }

    result.push(quote);
    result
}

fn should_attach_short_binary_tail(
    op: BinaryOperator,
    right: &LuaExpr,
    right_docs: &[DocIR],
) -> bool {
    if ir::ir_has_forced_line_break(right_docs) {
        return false;
    }

    match op {
        BinaryOperator::OpAnd | BinaryOperator::OpOr => {
            ir::ir_flat_width(right_docs) <= 24
                && matches!(
                    right,
                    LuaExpr::LiteralExpr(_)
                        | LuaExpr::NameExpr(_)
                        | LuaExpr::ParenExpr(_)
                        | LuaExpr::IndexExpr(_)
                        | LuaExpr::CallExpr(_)
                )
        }
        BinaryOperator::OpEq
        | BinaryOperator::OpNe
        | BinaryOperator::OpLt
        | BinaryOperator::OpLe
        | BinaryOperator::OpGt
        | BinaryOperator::OpGe => {
            ir::ir_flat_width(right_docs) <= 16
                && matches!(
                    right,
                    LuaExpr::LiteralExpr(_) | LuaExpr::NameExpr(_) | LuaExpr::ParenExpr(_)
                )
        }
        _ => false,
    }
}

fn should_attach_after_multiline_left_expr(left: &LuaExpr) -> bool {
    left.syntax().text().contains_char('\n')
        && matches!(
            left,
            LuaExpr::CallExpr(_)
                | LuaExpr::ParenExpr(_)
                | LuaExpr::IndexExpr(_)
                | LuaExpr::TableExpr(_)
                | LuaExpr::ClosureExpr(_)
        )
}

fn format_unary_expr(ctx: &FormatContext, plan: &FormatPlan, expr: &LuaUnaryExpr) -> Vec<DocIR> {
    let mut docs = Vec::new();
    if let Some(op_token) = expr.get_op_token() {
        docs.push(ir::source_token(op_token.syntax().clone()));
    }
    if let Some(inner) = expr.get_expr() {
        docs.extend(token_gap_spacing_docs(
            plan,
            expr.get_op_token().as_ref().map(|token| token.syntax()),
            inner.syntax().first_token().as_ref(),
        ));
        docs.extend(format_expr(ctx, plan, &inner));
    }
    docs
}

fn format_paren_expr(ctx: &FormatContext, plan: &FormatPlan, expr: &LuaParenExpr) -> Vec<DocIR> {
    if node_has_direct_comment_child(expr.syntax()) {
        return vec![ir::source_node_trimmed(expr.syntax().clone())];
    }

    let mut docs = vec![ir::syntax_token(LuaTokenKind::TkLeftParen)];
    if ctx.config.spacing.space_inside_parens {
        docs.push(ir::space());
    }
    if let Some(inner) = expr.get_expr() {
        docs.extend(format_expr(ctx, plan, &inner));
    }
    if ctx.config.spacing.space_inside_parens {
        docs.push(ir::space());
    }
    docs.push(ir::syntax_token(LuaTokenKind::TkRightParen));
    docs
}

fn format_index_expr(ctx: &FormatContext, plan: &FormatPlan, expr: &LuaIndexExpr) -> Vec<DocIR> {
    if expr
        .get_index_token()
        .is_some_and(|token| token.is_left_bracket())
        && node_has_direct_comment_child(expr.syntax())
    {
        return vec![ir::source_node_trimmed(expr.syntax().clone())];
    }

    let prefix_docs = expr
        .get_prefix_expr()
        .map(|prefix| format_expr(ctx, plan, &prefix))
        .unwrap_or_default();
    let access_docs = format_index_access_ir(ctx, plan, expr);
    let indent_tail = matches!(
        access_docs.first(),
        Some(DocIR::SyntaxToken(
            LuaTokenKind::TkDot | LuaTokenKind::TkColon
        ))
    );

    let bridge = analyze_expr_bridge(expr.syntax());
    if bridge.has_line_break || !bridge.comment_fragments.is_empty() {
        return format_expr_with_bridge_tail(ctx, prefix_docs, access_docs, bridge, indent_tail);
    }

    let mut docs = prefix_docs;
    docs.extend(access_docs);
    docs
}

pub fn format_param_list_ir(
    ctx: &FormatContext,
    plan: &FormatPlan,
    params: &LuaParamList,
) -> Vec<DocIR> {
    let collected = collect_param_entries(ctx, params);

    if collected.has_comments {
        return format_param_list_with_comments(ctx, plan, params, collected);
    }

    let param_docs: Vec<Vec<DocIR>> = collected
        .entries
        .into_iter()
        .map(|entry| entry.doc)
        .collect();
    let (open, close) = paren_tokens(params.syntax());
    let comma = first_direct_token(params.syntax(), LuaTokenKind::TkComma);
    format_delimited_sequence(
        ctx,
        DelimitedSequenceLayout {
            open: token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftParen),
            close: token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightParen),
            items: param_docs,
            strategy: ctx.config.layout.func_params_expand.clone(),
            preserve_multiline: false,
            flat_separator: comma_flat_separator(plan, comma.as_ref()),
            fill_separator: comma_fill_separator(comma.as_ref()),
            break_separator: comma_break_separator(comma.as_ref()),
            flat_open_padding: token_right_spacing_docs(plan, open.as_ref()),
            flat_close_padding: token_left_spacing_docs(plan, close.as_ref()),
            grouped_padding: grouped_padding_from_pair(plan, open.as_ref(), close.as_ref()),
            flat_trailing: vec![],
            grouped_trailing: trailing_comma_ir(ctx.config.output.trailing_comma.clone()),
        },
    )
}

enum CompactCallArgListAttempt {
    Formatted(Vec<DocIR>),
    ReuseDocs(Vec<Vec<DocIR>>),
    CommentsPresent,
}

#[derive(Default)]
struct CollectedParamEntries {
    entries: Vec<ParamEntry>,
    comments_after_open: Vec<DelimitedComment>,
    comments_before_close: Vec<Vec<DocIR>>,
    has_comments: bool,
    consumed_comment_ranges: Vec<TextRange>,
}

struct DelimitedComment {
    docs: Vec<DocIR>,
    same_line_after_open: bool,
    blank_lines_before: usize,
}

struct ParamEntry {
    leading_comments: Vec<Vec<DocIR>>,
    doc: Vec<DocIR>,
    trailing_comment: Option<Vec<DocIR>>,
    trailing_align_hint: bool,
}

fn collect_param_entries(ctx: &FormatContext, params: &LuaParamList) -> CollectedParamEntries {
    let mut collected = CollectedParamEntries::default();
    let mut pending_comments = Vec::new();
    let mut seen_param = false;

    for child in params.syntax().children() {
        if let Some(comment) = LuaComment::cast(child.clone()) {
            if collected
                .consumed_comment_ranges
                .iter()
                .any(|range| *range == comment.syntax().text_range())
            {
                continue;
            }
            let docs = vec![ir::source_node_trimmed(comment.syntax().clone())];
            collected.has_comments = true;
            if !seen_param {
                collected.comments_after_open.push(DelimitedComment {
                    docs,
                    same_line_after_open: has_non_trivia_before_on_same_line_tokenwise(
                        comment.syntax(),
                    ),
                    blank_lines_before: 0,
                });
            } else {
                pending_comments.push(docs);
            }
            continue;
        }

        if let Some(param) = emmylua_parser::LuaParamName::cast(child) {
            let trailing_comment = extract_trailing_comment_text(param.syntax());
            if trailing_comment.is_some() {
                collected.has_comments = true;
            }
            let trailing_align_hint = trailing_comment.as_ref().is_some_and(|(_, range)| {
                trailing_gap_requests_alignment(
                    param.syntax(),
                    *range,
                    ctx.config.comments.line_comment_min_spaces_before.max(1),
                )
            });
            if let Some((_, range)) = &trailing_comment {
                collected.consumed_comment_ranges.push(*range);
            }
            let doc = if param.is_dots() {
                vec![ir::text("...")]
            } else if let Some(token) = param.get_name_token() {
                vec![ir::source_token(token.syntax().clone())]
            } else {
                continue;
            };
            collected.entries.push(ParamEntry {
                leading_comments: std::mem::take(&mut pending_comments),
                doc,
                trailing_comment: trailing_comment.map(|(docs, _)| docs),
                trailing_align_hint,
            });
            seen_param = true;
        }
    }

    if !pending_comments.is_empty() {
        collected.comments_before_close = pending_comments;
    }

    collected
}

fn format_param_list_with_comments(
    ctx: &FormatContext,
    _plan: &FormatPlan,
    params: &LuaParamList,
    collected: CollectedParamEntries,
) -> Vec<DocIR> {
    let (open, close) = paren_tokens(params.syntax());
    let comma = first_direct_token(params.syntax(), LuaTokenKind::TkComma);
    let mut docs = vec![token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftParen)];
    let trailing = trailing_comma_ir(ctx.config.output.trailing_comma.clone());

    if !collected.comments_after_open.is_empty() || !collected.entries.is_empty() {
        let entry_count = collected.entries.len();
        let mut inner = Vec::new();
        let trailing_widths = aligned_trailing_comment_widths(
            ctx.config.should_align_param_line_comments()
                && collected
                    .entries
                    .iter()
                    .any(|entry| entry.trailing_align_hint),
            collected.entries.iter().enumerate().map(|(index, entry)| {
                let mut content = entry.doc.clone();
                if index + 1 < entry_count {
                    content.extend(comma_token_docs(comma.as_ref()));
                } else {
                    content.push(trailing.clone());
                }
                (content, entry.trailing_comment.is_some())
            }),
        );

        let mut first_inner_line_started = false;
        for comment in collected.comments_after_open {
            if comment.same_line_after_open && !first_inner_line_started {
                let mut suffix = trailing_comment_prefix(ctx);
                suffix.extend(comment.docs);
                docs.push(ir::line_suffix(suffix));
            } else {
                inner.push(ir::hard_line());
                inner.extend(comment.docs);
                first_inner_line_started = true;
            }
        }
        for (index, entry) in collected.entries.into_iter().enumerate() {
            inner.push(ir::hard_line());
            for comment_docs in entry.leading_comments {
                inner.extend(comment_docs);
                inner.push(ir::hard_line());
            }
            let mut line_content = entry.doc;
            inner.extend(line_content.clone());
            if index + 1 < entry_count {
                inner.extend(comma_token_docs(comma.as_ref()));
                line_content.extend(comma_token_docs(comma.as_ref()));
            } else {
                inner.push(trailing.clone());
                line_content.push(trailing.clone());
            }
            if let Some(comment_docs) = entry.trailing_comment {
                let mut suffix = trailing_comment_prefix_for_width(
                    ctx,
                    ir::ir_flat_width(&line_content),
                    trailing_widths[index],
                );
                suffix.extend(comment_docs);
                inner.push(ir::line_suffix(suffix));
            }
        }

        for comment_docs in collected.comments_before_close {
            inner.push(ir::hard_line());
            inner.extend(comment_docs);
        }

        docs.push(ir::indent(inner));
        docs.push(ir::hard_line());
    }

    docs.push(token_or_kind_doc(
        close.as_ref(),
        LuaTokenKind::TkRightParen,
    ));
    docs
}

fn format_call_expr(ctx: &FormatContext, plan: &FormatPlan, expr: &LuaCallExpr) -> Vec<DocIR> {
    let prefix_expr = expr.get_prefix_expr();
    let prefix_is_multiline = prefix_expr
        .as_ref()
        .is_some_and(|prefix| prefix.syntax().text().contains_char('\n'));
    let mut docs = prefix_expr
        .map(|prefix| format_expr(ctx, plan, &prefix))
        .unwrap_or_default();

    let Some(args_list) = expr.get_args_list() else {
        return docs;
    };

    if let Some(single_arg_docs) = format_single_arg_call_without_parens(ctx, plan, &args_list) {
        docs.push(ir::space());
        docs.extend(single_arg_docs);
        return docs;
    }

    let (open, _) = paren_tokens(args_list.syntax());
    docs.extend(token_left_spacing_docs(plan, open.as_ref()));
    if docs.is_empty() && ctx.config.spacing.space_before_call_paren {
        docs.push(ir::space());
    }
    let arg_docs = format_call_arg_list(ctx, plan, &args_list);

    let bridge = analyze_expr_bridge(expr.syntax());
    if bridge.has_line_break || !bridge.comment_fragments.is_empty() {
        return format_expr_with_bridge_tail(ctx, docs, arg_docs, bridge, false);
    }

    if prefix_is_multiline && ir::ir_has_forced_line_break(&arg_docs) {
        docs.push(ir::indent(arg_docs));
    } else {
        docs.extend(arg_docs);
    }
    docs
}

struct ExprBridgePlan {
    has_line_break: bool,
    comment_fragments: Vec<InlineCommentFragment>,
}

fn format_expr_with_bridge_tail(
    ctx: &FormatContext,
    prefix_docs: Vec<DocIR>,
    tail_docs: Vec<DocIR>,
    bridge: ExprBridgePlan,
    indent_tail: bool,
) -> Vec<DocIR> {
    let mut bridged_tail = Vec::new();
    let mut has_content_before_tail = false;
    let mut has_inline_suffix_comment = false;

    for (index, fragment) in bridge.comment_fragments.into_iter().enumerate() {
        if fragment.same_line_before && index == 0 {
            let mut suffix = trailing_comment_prefix(ctx);
            suffix.extend(fragment.docs);
            bridged_tail.push(ir::line_suffix(suffix));
            has_inline_suffix_comment = true;
        } else {
            bridged_tail.push(ir::hard_line());
            bridged_tail.extend(fragment.docs);
            has_content_before_tail = true;
        }
    }

    if bridge.has_line_break || has_content_before_tail || has_inline_suffix_comment {
        bridged_tail.push(ir::hard_line());
    }
    bridged_tail.extend(tail_docs);

    let mut docs = prefix_docs;
    if indent_tail {
        docs.push(ir::indent(bridged_tail));
    } else {
        docs.extend(bridged_tail);
    }
    docs
}

fn analyze_expr_bridge(syntax: &LuaSyntaxNode) -> ExprBridgePlan {
    let mut seen_prefix_expr = false;
    let mut comment_fragments = Vec::new();
    let mut has_line_break = false;

    for element in syntax.children_with_tokens() {
        match element {
            rowan::NodeOrToken::Node(node) => {
                if !seen_prefix_expr {
                    if LuaExpr::cast(node).is_some() {
                        seen_prefix_expr = true;
                    }
                    continue;
                }

                if let Some(comment) = LuaComment::cast(node) {
                    comment_fragments.push(InlineCommentFragment {
                        docs: vec![ir::source_node_trimmed(comment.syntax().clone())],
                        same_line_before: has_non_trivia_before_on_same_line_tokenwise(
                            comment.syntax(),
                        ),
                    });
                    continue;
                }

                break;
            }
            rowan::NodeOrToken::Token(token) => {
                if !seen_prefix_expr {
                    continue;
                }

                match token.kind().into() {
                    LuaTokenKind::TkWhitespace => {}
                    LuaTokenKind::TkEndOfLine => has_line_break = true,
                    _ => break,
                }
            }
        }
    }

    ExprBridgePlan {
        has_line_break,
        comment_fragments,
    }
}

fn format_call_arg_list(
    ctx: &FormatContext,
    plan: &FormatPlan,
    args_list: &LuaCallArgList,
) -> Vec<DocIR> {
    let args: Vec<_> = args_list.get_args().collect();
    let layout_plan = expr_sequence_layout_plan(plan, args_list.syntax());
    if call_arg_list_has_direct_comments(args_list) {
        let collected = collect_call_arg_entries(ctx, plan, args_list);
        if collected.has_comments {
            return format_call_arg_list_with_comments(ctx, plan, args_list, collected);
        }
    }

    let preserve_multiline_args = layout_plan.preserve_multiline;
    let attach_first_arg = preserve_multiline_args && layout_plan.first_arg_multiline_block;
    let arg_docs: Vec<Vec<DocIR>> = args
        .iter()
        .enumerate()
        .map(|(index, arg)| {
            format_call_arg_value_ir(
                ctx,
                plan,
                arg,
                attach_first_arg,
                preserve_multiline_args,
                index,
            )
        })
        .collect();

    format_call_arg_list_from_docs(
        ctx,
        plan,
        args_list,
        attach_first_arg,
        layout_plan,
        arg_docs,
    )
}

fn format_call_arg_list_from_docs(
    ctx: &FormatContext,
    plan: &FormatPlan,
    args_list: &LuaCallArgList,
    attach_first_arg: bool,
    layout_plan: ExprSequenceLayoutPlan,
    arg_docs: Vec<Vec<DocIR>>,
) -> Vec<DocIR> {
    let first_arg_is_multiline_table = layout_plan.first_arg_multiline_table;
    let allow_inline_tail_after_first_arg =
        layout_plan.first_arg_multiline_block || first_arg_is_multiline_table;
    let (open, close) = paren_tokens(args_list.syntax());
    let comma = first_direct_token(args_list.syntax(), LuaTokenKind::TkComma);

    if arg_docs.len() == 1
        && matches!(args_list.get_args().next(), Some(LuaExpr::TableExpr(_)))
        && ir::ir_has_forced_line_break(&arg_docs[0])
    {
        let mut docs = vec![token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftParen)];
        docs.extend(arg_docs[0].clone());
        docs.push(token_or_kind_doc(
            close.as_ref(),
            LuaTokenKind::TkRightParen,
        ));
        return docs;
    }

    if attach_first_arg {
        return format_call_args_with_attached_first_arg(
            ctx,
            plan,
            allow_inline_tail_after_first_arg,
            layout_plan,
            arg_docs,
            token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftParen),
            token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightParen),
            comma.as_ref(),
        );
    }

    if let Some(block_index) = layout_plan.single_inline_block_arg_index {
        return format_call_args_with_inline_block_item(
            ctx,
            plan,
            layout_plan,
            arg_docs,
            block_index,
            token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftParen),
            token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightParen),
            comma.as_ref(),
        );
    }

    if arg_docs
        .iter()
        .skip(1)
        .any(|docs| ir::ir_has_forced_line_break(docs))
    {
        return format_call_args_with_block_items(
            ctx,
            layout_plan,
            arg_docs,
            token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftParen),
            token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightParen),
            comma.as_ref(),
        );
    }

    format_delimited_sequence(
        ctx,
        DelimitedSequenceLayout {
            open: token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftParen),
            close: token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightParen),
            items: arg_docs,
            strategy: ctx.config.layout.call_args_expand.clone(),
            preserve_multiline: layout_plan.preserve_multiline,
            flat_separator: comma_flat_separator(plan, comma.as_ref()),
            fill_separator: comma_fill_separator(comma.as_ref()),
            break_separator: comma_break_separator(comma.as_ref()),
            flat_open_padding: token_right_spacing_docs(plan, open.as_ref()),
            flat_close_padding: token_left_spacing_docs(plan, close.as_ref()),
            grouped_padding: grouped_padding_from_pair(plan, open.as_ref(), close.as_ref()),
            flat_trailing: vec![],
            grouped_trailing: trailing_comma_ir(ctx.config.output.trailing_comma.clone()),
        },
    )
}

fn format_call_args_with_inline_block_item(
    ctx: &FormatContext,
    plan: &FormatPlan,
    layout_plan: ExprSequenceLayoutPlan,
    arg_docs: Vec<Vec<DocIR>>,
    block_index: usize,
    open: DocIR,
    close: DocIR,
    comma: Option<&LuaSyntaxToken>,
) -> Vec<DocIR> {
    let prefix_items = &arg_docs[..block_index];
    let block_item = &arg_docs[block_index];
    let tail_items = &arg_docs[block_index + 1..];

    let mut anchored_docs = vec![open.clone()];
    if !prefix_items.is_empty() {
        append_docs_with_separator(
            &mut anchored_docs,
            prefix_items,
            &comma_flat_separator(plan, comma),
        );
        anchored_docs.extend(comma_flat_separator(plan, comma));
    }
    anchored_docs.extend(block_item.clone());
    if !tail_items.is_empty() {
        anchored_docs.push(ir::indent(vec![ir::fill(
            build_fill_parts_with_leading_separator(tail_items, &comma_fill_separator(comma)),
        )]));
        anchored_docs.push(ir::hard_line());
        anchored_docs.push(close.clone());
    } else {
        anchored_docs.push(close.clone());
    }

    let anchored = vec![ir::group_break(anchored_docs)];
    let one_per_line = build_one_per_line_call_args(&arg_docs, open, close, comma);

    choose_sequence_layout(
        ctx,
        SequenceLayoutCandidates {
            fill: Some(anchored),
            one_per_line: Some(one_per_line),
            ..Default::default()
        },
        SequenceLayoutPolicy {
            allow_fill: true,
            prefer_balanced_break_lines: true,
            first_line_prefix_width: layout_plan.first_line_prefix_width,
            ..Default::default()
        },
    )
}

fn format_call_args_with_block_items(
    ctx: &FormatContext,
    layout_plan: ExprSequenceLayoutPlan,
    arg_docs: Vec<Vec<DocIR>>,
    open: DocIR,
    close: DocIR,
    comma: Option<&LuaSyntaxToken>,
) -> Vec<DocIR> {
    if arg_docs.is_empty() {
        return vec![open, close];
    }

    let mut blocked_inner = Vec::new();
    let mut current_chunk: Vec<Vec<DocIR>> = Vec::new();

    for (index, item_docs) in arg_docs.iter().enumerate() {
        let is_last = index + 1 == arg_docs.len();
        let item_is_multiline = ir::ir_has_forced_line_break(item_docs);
        let mut item_with_trailing = item_docs.clone();
        if !is_last {
            item_with_trailing.extend(comma_token_docs(comma));
        }

        if item_is_multiline {
            if !current_chunk.is_empty() {
                blocked_inner.push(ir::hard_line());
                blocked_inner.extend(render_blocked_call_arg_chunk(std::mem::take(
                    &mut current_chunk,
                )));
            }
            blocked_inner.push(ir::hard_line());
            blocked_inner.extend(item_with_trailing);
        } else {
            current_chunk.push(item_with_trailing);
        }
    }

    if !current_chunk.is_empty() {
        blocked_inner.push(ir::hard_line());
        blocked_inner.extend(render_blocked_call_arg_chunk(current_chunk));
    }

    let blocked = vec![ir::group_break(vec![
        open.clone(),
        ir::indent(blocked_inner),
        ir::hard_line(),
        close.clone(),
    ])];
    let one_per_line = build_one_per_line_call_args(&arg_docs, open, close, comma);

    choose_sequence_layout(
        ctx,
        SequenceLayoutCandidates {
            fill: Some(blocked),
            one_per_line: Some(one_per_line),
            ..Default::default()
        },
        SequenceLayoutPolicy {
            allow_fill: true,
            prefer_balanced_break_lines: true,
            first_line_prefix_width: layout_plan.first_line_prefix_width,
            ..Default::default()
        },
    )
}

fn render_blocked_call_arg_chunk(items_with_trailing: Vec<Vec<DocIR>>) -> Vec<DocIR> {
    if items_with_trailing.is_empty() {
        return Vec::new();
    }

    let item_count = items_with_trailing.len();
    let mut parts = Vec::with_capacity(items_with_trailing.len().saturating_mul(2));
    for (index, item_docs) in items_with_trailing.into_iter().enumerate() {
        parts.push(ir::list(item_docs));
        if index + 1 < item_count {
            parts.push(ir::soft_line());
        }
    }
    vec![ir::fill(parts)]
}

fn build_one_per_line_call_args(
    arg_docs: &[Vec<DocIR>],
    open: DocIR,
    close: DocIR,
    comma: Option<&LuaSyntaxToken>,
) -> Vec<DocIR> {
    let item_count = arg_docs.len();
    let mut one_per_line_inner = Vec::new();
    for (index, item_docs) in arg_docs.iter().enumerate() {
        one_per_line_inner.push(ir::hard_line());
        one_per_line_inner.extend(item_docs.clone());
        if index + 1 < item_count {
            one_per_line_inner.extend(comma_token_docs(comma));
        }
    }

    vec![ir::group_break(vec![
        open,
        ir::indent(one_per_line_inner),
        ir::hard_line(),
        close,
    ])]
}

fn format_call_arg_value_ir(
    ctx: &FormatContext,
    plan: &FormatPlan,
    arg: &LuaExpr,
    attach_first_arg: bool,
    preserve_multiline_args: bool,
    index: usize,
) -> Vec<DocIR> {
    if preserve_multiline_args && arg.syntax().text().contains_char('\n') {
        if let LuaExpr::TableExpr(table) = arg {
            return format_multiline_table_expr(ctx, plan, table);
        }

        if attach_first_arg && index == 0 {
            return format_expr(ctx, plan, arg);
        }
    }

    let docs = format_expr(ctx, plan, arg);
    if let LuaExpr::TableExpr(table) = arg
        && ir::ir_has_forced_line_break(&docs)
    {
        return format_multiline_table_expr(ctx, plan, table);
    }

    docs
}

fn format_call_args_with_attached_first_arg(
    ctx: &FormatContext,
    plan: &FormatPlan,
    allow_inline_tail_after_first_arg: bool,
    layout_plan: ExprSequenceLayoutPlan,
    arg_docs: Vec<Vec<DocIR>>,
    open: DocIR,
    close: DocIR,
    comma: Option<&LuaSyntaxToken>,
) -> Vec<DocIR> {
    if arg_docs.is_empty() {
        return vec![open, close];
    }

    if arg_docs.len() == 1 {
        let mut docs = vec![open];
        docs.extend(arg_docs[0].clone());
        docs.push(close);
        return docs;
    }

    let Some((first_arg, tail_items)) = arg_docs.split_first() else {
        return vec![open, close];
    };
    let tail_is_single_line = allow_inline_tail_after_first_arg
        && tail_items
            .iter()
            .all(|item_docs| !ir::ir_has_forced_line_break(item_docs));

    if tail_is_single_line
        && let Some(attached_inline_tail) =
            try_format_attached_inline_tail(ctx, plan, first_arg, tail_items, &open, &close, comma)
    {
        return attached_inline_tail;
    }

    let flat = tail_is_single_line.then(|| {
        let mut docs = vec![open.clone()];
        docs.extend(first_arg.clone());
        docs.extend(comma_flat_separator(plan, comma));
        append_docs_with_separator(&mut docs, tail_items, &comma_flat_separator(plan, comma));
        docs.push(close.clone());
        docs
    });

    let mut fill_docs = vec![open.clone()];
    fill_docs.extend(first_arg.clone());
    fill_docs.extend(comma_token_docs(comma));
    fill_docs.push(ir::indent(vec![
        ir::soft_line(),
        ir::fill(build_fill_parts(tail_items, &comma_fill_separator(comma))),
    ]));
    fill_docs.push(close.clone());

    choose_sequence_layout(
        ctx,
        SequenceLayoutCandidates {
            flat,
            fill: Some(vec![ir::group(fill_docs)]),
            one_per_line: Some(build_attached_first_arg_one_per_line(
                &arg_docs, open, close, comma,
            )),
            ..Default::default()
        },
        SequenceLayoutPolicy {
            allow_fill: true,
            prefer_balanced_break_lines: true,
            first_line_prefix_width: layout_plan.first_line_prefix_width,
            ..Default::default()
        },
    )
}

fn try_format_attached_inline_tail(
    ctx: &FormatContext,
    plan: &FormatPlan,
    first_arg: &[DocIR],
    tail_items: &[Vec<DocIR>],
    open: &DocIR,
    close: &DocIR,
    comma: Option<&LuaSyntaxToken>,
) -> Option<Vec<DocIR>> {
    if !ir::ir_has_forced_line_break(first_arg) {
        return None;
    }

    let mut trailing_docs = comma_flat_separator(plan, comma);
    append_docs_with_separator(
        &mut trailing_docs,
        tail_items,
        &comma_flat_separator(plan, comma),
    );
    trailing_docs.push(close.clone());

    let last_line_width = measure_docs(ctx.config, first_arg)
        .line_widths
        .last()
        .copied()
        .unwrap_or_else(|| ir::ir_flat_width(first_arg));
    let trailing_width = ir::ir_flat_width(&trailing_docs);
    if last_line_width + trailing_width > ctx.config.layout.max_line_width {
        return None;
    }

    let mut docs = vec![open.clone()];
    docs.extend(first_arg.to_vec());
    docs.extend(trailing_docs);
    Some(docs)
}

fn build_attached_first_arg_one_per_line(
    arg_docs: &[Vec<DocIR>],
    open: DocIR,
    close: DocIR,
    comma: Option<&LuaSyntaxToken>,
) -> Vec<DocIR> {
    let Some((first_arg, tail_items)) = arg_docs.split_first() else {
        return vec![ir::group_break(vec![open, close])];
    };

    let mut one_per_line_docs = vec![open];
    one_per_line_docs.extend(first_arg.clone());
    if tail_items.is_empty() {
        one_per_line_docs.push(close);
        return vec![ir::group_break(one_per_line_docs)];
    }

    one_per_line_docs.extend(comma_token_docs(comma));
    let mut rest = Vec::new();
    let rest_count = tail_items.len();
    for (index, item_docs) in tail_items.iter().enumerate() {
        rest.push(ir::hard_line());
        rest.extend(item_docs.clone());
        if index + 1 < rest_count {
            rest.extend(comma_token_docs(comma));
        }
    }

    one_per_line_docs.push(ir::indent(rest));
    one_per_line_docs.push(ir::hard_line());
    one_per_line_docs.push(close);
    vec![ir::group_break(one_per_line_docs)]
}

fn append_docs_with_separator(docs: &mut Vec<DocIR>, items: &[Vec<DocIR>], separator: &[DocIR]) {
    for (index, item_docs) in items.iter().enumerate() {
        if index > 0 {
            docs.extend(separator.to_vec());
        }
        docs.extend(item_docs.clone());
    }
}

fn call_arg_list_has_direct_comments(args_list: &LuaCallArgList) -> bool {
    args_list
        .syntax()
        .children()
        .any(|child| LuaComment::cast(child).is_some())
}

#[derive(Default)]
struct CollectedCallArgEntries {
    entries: Vec<CallArgEntry>,
    comments_after_open: Vec<DelimitedComment>,
    comments_before_close: Vec<Vec<DocIR>>,
    has_comments: bool,
    consumed_comment_ranges: Vec<TextRange>,
}

struct CallArgEntry {
    leading_comments: Vec<Vec<DocIR>>,
    doc: Vec<DocIR>,
    trailing_comment: Option<Vec<DocIR>>,
    trailing_align_hint: bool,
}

fn collect_call_arg_entries(
    ctx: &FormatContext,
    plan: &FormatPlan,
    args_list: &LuaCallArgList,
) -> CollectedCallArgEntries {
    let mut collected = CollectedCallArgEntries::default();
    let mut pending_comments = Vec::new();
    let mut seen_arg = false;

    for child in args_list.syntax().children() {
        if let Some(comment) = LuaComment::cast(child.clone()) {
            if collected
                .consumed_comment_ranges
                .iter()
                .any(|range| *range == comment.syntax().text_range())
            {
                continue;
            }
            let docs = vec![ir::source_node_trimmed(comment.syntax().clone())];
            collected.has_comments = true;
            if !seen_arg {
                collected.comments_after_open.push(DelimitedComment {
                    docs,
                    same_line_after_open: has_non_trivia_before_on_same_line_tokenwise(
                        comment.syntax(),
                    ),
                    blank_lines_before: 0,
                });
            } else {
                pending_comments.push(docs);
            }
            continue;
        }

        if let Some(arg) = LuaExpr::cast(child) {
            let trailing_comment = extract_trailing_comment(ctx, plan, arg.syntax());
            if trailing_comment.is_some() {
                collected.has_comments = true;
            }
            let trailing_align_hint = trailing_comment.as_ref().is_some_and(|(_, range)| {
                trailing_gap_requests_alignment(
                    arg.syntax(),
                    *range,
                    ctx.config.comments.line_comment_min_spaces_before.max(1),
                )
            });
            if let Some((_, range)) = &trailing_comment {
                collected.consumed_comment_ranges.push(*range);
            }
            collected.entries.push(CallArgEntry {
                leading_comments: std::mem::take(&mut pending_comments),
                doc: format_expr(ctx, plan, &arg),
                trailing_comment: trailing_comment.map(|(docs, _)| docs),
                trailing_align_hint,
            });
            seen_arg = true;
        }
    }

    if !pending_comments.is_empty() {
        collected.comments_before_close = pending_comments;
    }

    collected
}

fn format_call_arg_list_with_comments(
    ctx: &FormatContext,
    plan: &FormatPlan,
    args_list: &LuaCallArgList,
    collected: CollectedCallArgEntries,
) -> Vec<DocIR> {
    let (open, close) = paren_tokens(args_list.syntax());
    let comma = first_direct_token(args_list.syntax(), LuaTokenKind::TkComma);
    let mut docs = vec![token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftParen)];
    let trailing = trailing_comma_ir(ctx.config.output.trailing_comma.clone());

    if !collected.comments_after_open.is_empty() || !collected.entries.is_empty() {
        let entry_count = collected.entries.len();
        let mut inner = Vec::new();
        let trailing_widths = aligned_trailing_comment_widths(
            ctx.config.should_align_call_arg_line_comments()
                && collected
                    .entries
                    .iter()
                    .any(|entry| entry.trailing_align_hint),
            collected.entries.iter().enumerate().map(|(index, entry)| {
                let mut content = entry.doc.clone();
                if index + 1 < entry_count {
                    content.extend(comma_token_docs(comma.as_ref()));
                } else {
                    content.push(trailing.clone());
                }
                (content, entry.trailing_comment.is_some())
            }),
        );

        let mut first_inner_line_started = false;
        for comment in collected.comments_after_open {
            if comment.same_line_after_open && !first_inner_line_started {
                let mut suffix = trailing_comment_prefix(ctx);
                suffix.extend(comment.docs);
                docs.push(ir::line_suffix(suffix));
            } else {
                inner.push(ir::hard_line());
                inner.extend(comment.docs);
                first_inner_line_started = true;
            }
        }
        for (index, entry) in collected.entries.into_iter().enumerate() {
            inner.push(ir::hard_line());
            for comment_docs in entry.leading_comments {
                inner.extend(comment_docs);
                inner.push(ir::hard_line());
            }
            let mut line_content = entry.doc;
            inner.extend(line_content.clone());
            if index + 1 < entry_count {
                inner.extend(comma_token_docs(comma.as_ref()));
                line_content.extend(comma_token_docs(comma.as_ref()));
            } else {
                inner.push(trailing.clone());
                line_content.push(trailing.clone());
            }
            if let Some(comment_docs) = entry.trailing_comment {
                let mut suffix = trailing_comment_prefix_for_width(
                    ctx,
                    ir::ir_flat_width(&line_content),
                    trailing_widths[index],
                );
                suffix.extend(comment_docs);
                inner.push(ir::line_suffix(suffix));
            }
        }

        for comment_docs in collected.comments_before_close {
            inner.push(ir::hard_line());
            inner.extend(comment_docs);
        }

        docs.push(ir::indent(inner));
        docs.push(ir::hard_line());
    } else {
        docs.extend(token_right_spacing_docs(plan, open.as_ref()));
    }

    docs.push(token_or_kind_doc(
        close.as_ref(),
        LuaTokenKind::TkRightParen,
    ));
    docs
}

fn format_single_arg_call_without_parens(
    ctx: &FormatContext,
    plan: &FormatPlan,
    args_list: &LuaCallArgList,
) -> Option<Vec<DocIR>> {
    let single_arg = match ctx.config.output.single_arg_call_parens {
        SingleArgCallParens::Always => None,
        SingleArgCallParens::Preserve => args_list
            .is_single_arg_no_parens()
            .then(|| args_list.get_single_arg_expr())
            .flatten(),
        SingleArgCallParens::Omit => args_list.get_single_arg_expr(),
    }?;

    Some(match single_arg {
        LuaSingleArgExpr::TableExpr(table) => format_table_expr(ctx, plan, &table),
        LuaSingleArgExpr::LiteralExpr(lit)
            if matches!(lit.get_literal(), Some(LuaLiteralToken::String(_))) =>
        {
            vec![ir::source_node_trimmed(lit.syntax().clone())]
        }
        LuaSingleArgExpr::LiteralExpr(_) => return None,
    })
}

fn format_table_expr(ctx: &FormatContext, plan: &FormatPlan, expr: &LuaTableExpr) -> Vec<DocIR> {
    let has_direct_comments = expr
        .syntax()
        .children()
        .any(|child| LuaComment::cast(child).is_some());

    if expr.is_empty() && !has_direct_comments {
        let (open, close) = brace_tokens(expr.syntax());
        return vec![
            token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftBrace),
            token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightBrace),
        ];
    }

    let mut collected = collect_table_entries(ctx, plan, expr);
    let has_assign_fields = collected
        .entries
        .iter()
        .any(|entry| entry.field.is_assign_field());
    let has_assign_alignment = ctx.config.align.table_field
        && has_assign_fields
        && table_group_requests_alignment(&collected.entries);

    if has_assign_alignment {
        populate_table_eq_splits(ctx, plan, &mut collected.entries);
    }

    if collected.has_comments {
        return format_table_with_comments(ctx, plan, expr, collected);
    }

    let field_docs: Vec<Vec<DocIR>> = collected
        .entries
        .iter()
        .map(|entry| entry.doc.clone())
        .collect();
    let has_multiline_field = field_docs
        .iter()
        .any(|docs| ir::ir_has_forced_line_break(docs));
    let prefer_declaration_expand = should_prefer_expanded_declaration_table(expr);
    let (open, close) = brace_tokens(expr.syntax());
    let comma = first_direct_token(expr.syntax(), LuaTokenKind::TkComma);
    let layout_plan = expr_sequence_layout_plan(plan, expr.syntax());
    let effective_expand = if prefer_declaration_expand || has_multiline_field {
        ExpandStrategy::Always
    } else if expr.is_empty() {
        ExpandStrategy::Never
    } else {
        ctx.config.layout.table_expand.clone()
    };

    if has_assign_alignment {
        let layout = DelimitedSequenceLayout {
            open: token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftBrace),
            close: token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightBrace),
            items: field_docs.clone(),
            strategy: effective_expand.clone(),
            preserve_multiline: layout_plan.preserve_multiline,
            flat_separator: comma_flat_separator(plan, comma.as_ref()),
            fill_separator: comma_fill_separator(comma.as_ref()),
            break_separator: comma_break_separator(comma.as_ref()),
            flat_open_padding: token_right_spacing_docs(plan, open.as_ref()),
            flat_close_padding: token_left_spacing_docs(plan, close.as_ref()),
            grouped_padding: grouped_padding_from_pair(plan, open.as_ref(), close.as_ref()),
            flat_trailing: vec![],
            grouped_trailing: trailing_comma_ir(ctx.config.trailing_table_comma()),
        };

        return match effective_expand {
            ExpandStrategy::Always => wrap_table_multiline_docs(
                token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftBrace),
                token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightBrace),
                build_table_expanded_inner(
                    ctx,
                    &collected.entries,
                    &trailing_comma_ir(ctx.config.trailing_table_comma()),
                    true,
                    ctx.config.should_align_table_line_comments(),
                ),
            ),
            ExpandStrategy::Never => format_delimited_sequence(ctx, layout),
            ExpandStrategy::Auto => {
                if has_multiline_field {
                    return wrap_table_multiline_docs(
                        token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftBrace),
                        token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightBrace),
                        build_table_expanded_inner(
                            ctx,
                            &collected.entries,
                            &trailing_comma_ir(ctx.config.trailing_table_comma()),
                            true,
                            ctx.config.should_align_table_line_comments(),
                        ),
                    );
                }

                if layout_plan.preserve_multiline {
                    return wrap_table_multiline_docs(
                        token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftBrace),
                        token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightBrace),
                        build_table_expanded_inner(
                            ctx,
                            &collected.entries,
                            &trailing_comma_ir(ctx.config.trailing_table_comma()),
                            true,
                            ctx.config.should_align_table_line_comments(),
                        ),
                    );
                }

                let mut flat_layout = layout;
                flat_layout.strategy = ExpandStrategy::Never;
                let flat_docs = format_delimited_sequence(ctx, flat_layout);
                if ir::ir_flat_width(&flat_docs) + source_line_prefix_width(expr.syntax())
                    <= ctx.config.layout.max_line_width
                {
                    flat_docs
                } else {
                    wrap_table_multiline_docs(
                        token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftBrace),
                        token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightBrace),
                        build_table_expanded_inner(
                            ctx,
                            &collected.entries,
                            &trailing_comma_ir(ctx.config.trailing_table_comma()),
                            true,
                            ctx.config.should_align_table_line_comments(),
                        ),
                    )
                }
            }
        };
    }

    let layout = DelimitedSequenceLayout {
        open: token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftBrace),
        close: token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightBrace),
        items: field_docs,
        strategy: effective_expand.clone(),
        preserve_multiline: layout_plan.preserve_multiline,
        flat_separator: comma_flat_separator(plan, comma.as_ref()),
        fill_separator: comma_fill_separator(comma.as_ref()),
        break_separator: comma_break_separator(comma.as_ref()),
        flat_open_padding: token_right_spacing_docs(plan, open.as_ref()),
        flat_close_padding: token_left_spacing_docs(plan, close.as_ref()),
        grouped_padding: grouped_padding_from_pair(plan, open.as_ref(), close.as_ref()),
        flat_trailing: vec![],
        grouped_trailing: trailing_comma_ir(ctx.config.trailing_table_comma()),
    };

    if has_assign_fields
        && !has_multiline_field
        && !prefer_declaration_expand
        && matches!(ctx.config.layout.table_expand, ExpandStrategy::Auto)
    {
        if layout_plan.preserve_multiline {
            return wrap_table_multiline_docs(
                token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftBrace),
                token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightBrace),
                build_table_expanded_inner(
                    ctx,
                    &collected.entries,
                    &trailing_comma_ir(ctx.config.trailing_table_comma()),
                    false,
                    false,
                ),
            );
        }

        let mut flat_layout = layout.clone();
        flat_layout.strategy = ExpandStrategy::Never;
        let flat_docs = format_delimited_sequence(ctx, flat_layout);
        if ir::ir_flat_width(&flat_docs) + source_line_prefix_width(expr.syntax())
            <= ctx.config.layout.max_line_width
        {
            return flat_docs;
        }

        return wrap_table_multiline_docs(
            token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftBrace),
            token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightBrace),
            build_table_expanded_inner(
                ctx,
                &collected.entries,
                &trailing_comma_ir(ctx.config.trailing_table_comma()),
                false,
                false,
            ),
        );
    }

    format_delimited_sequence(ctx, layout)
}

fn should_prefer_expanded_declaration_table(expr: &LuaTableExpr) -> bool {
    let table_syntax = expr.syntax();
    let Some(statement) = table_syntax.ancestors().find(|node| {
        matches!(
            node.kind(),
            LuaKind::Syntax(LuaSyntaxKind::LocalStat | LuaSyntaxKind::AssignStat)
        )
    }) else {
        return false;
    };

    if !is_first_statement_value_expr(table_syntax, &statement) {
        return false;
    }

    has_tightly_attached_doc_declaration_comment(&statement)
}

fn is_first_statement_value_expr(expr: &LuaSyntaxNode, statement: &LuaSyntaxNode) -> bool {
    if let Some(local_stat) = LuaLocalStat::cast(statement.clone()) {
        return local_stat
            .get_value_exprs()
            .next()
            .is_some_and(|first| first.syntax() == expr);
    }

    if let Some(assign_stat) = LuaAssignStat::cast(statement.clone()) {
        return assign_stat
            .get_var_and_expr_list()
            .1
            .first()
            .is_some_and(|first| first.syntax() == expr);
    }

    false
}

fn has_tightly_attached_doc_declaration_comment(statement: &LuaSyntaxNode) -> bool {
    let mut previous = statement.prev_sibling_or_token();
    let mut blank_lines = 0usize;
    let mut seen_newline = false;

    while let Some(element) = previous {
        match element.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace) => {
                previous = element.prev_sibling_or_token();
            }
            LuaKind::Token(LuaTokenKind::TkEndOfLine) => {
                if seen_newline {
                    blank_lines += 1;
                }
                seen_newline = true;
                if blank_lines > 0 {
                    return false;
                }
                previous = element.prev_sibling_or_token();
            }
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                let Some(comment) = element.as_node() else {
                    return false;
                };
                return comment_contains_enum_or_class_tag(comment);
            }
            _ => return false,
        }
    }

    false
}

fn comment_contains_enum_or_class_tag(comment: &LuaSyntaxNode) -> bool {
    comment.descendants_with_tokens().any(|element| {
        let Some(token) = element.into_token() else {
            return false;
        };

        matches!(
            token.kind().to_token(),
            LuaTokenKind::TkTagEnum | LuaTokenKind::TkTagClass
        )
    })
}

fn format_multiline_table_expr(
    ctx: &FormatContext,
    plan: &FormatPlan,
    expr: &LuaTableExpr,
) -> Vec<DocIR> {
    let collected = collect_table_entries(ctx, plan, expr);

    if collected.has_comments
        || (ctx.config.align.table_field && table_group_requests_alignment(&collected.entries))
    {
        return format_table_with_comments(ctx, plan, expr, collected);
    }

    let field_docs: Vec<Vec<DocIR>> = collected
        .entries
        .into_iter()
        .map(|entry| entry.doc)
        .collect();
    let (open, close) = brace_tokens(expr.syntax());
    let comma = first_direct_token(expr.syntax(), LuaTokenKind::TkComma);

    format_delimited_sequence(
        ctx,
        DelimitedSequenceLayout {
            open: token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftBrace),
            close: token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightBrace),
            items: field_docs,
            strategy: ExpandStrategy::Always,
            preserve_multiline: false,
            flat_separator: comma_flat_separator(plan, comma.as_ref()),
            fill_separator: comma_fill_separator(comma.as_ref()),
            break_separator: comma_break_separator(comma.as_ref()),
            flat_open_padding: token_right_spacing_docs(plan, open.as_ref()),
            flat_close_padding: token_left_spacing_docs(plan, close.as_ref()),
            grouped_padding: grouped_padding_from_pair(plan, open.as_ref(), close.as_ref()),
            flat_trailing: vec![],
            grouped_trailing: trailing_comma_ir(ctx.config.trailing_table_comma()),
        },
    )
}

#[derive(Default)]
struct CollectedTableEntries {
    entries: Vec<TableEntry>,
    comments_after_open: Vec<DelimitedComment>,
    comments_before_close: Vec<TableSeparatedComment>,
    has_comments: bool,
    consumed_comment_ranges: Vec<TextRange>,
}

struct TableSeparatedComment {
    docs: Vec<DocIR>,
    blank_lines_before: usize,
}

struct TableEntry {
    field: LuaTableField,
    leading_comments: Vec<TableSeparatedComment>,
    blank_lines_before: usize,
    doc: Vec<DocIR>,
    eq_split: Option<EqSplitDocs>,
    align_hint: bool,
    comment_align_hint: bool,
    trailing_comment: Option<Vec<DocIR>>,
}

fn collect_table_entries(
    ctx: &FormatContext,
    plan: &FormatPlan,
    expr: &LuaTableExpr,
) -> CollectedTableEntries {
    let mut collected = CollectedTableEntries::default();
    let mut pending_comments: Vec<TableSeparatedComment> = Vec::new();
    let mut seen_field = false;

    for child in expr.syntax().children() {
        if let Some(comment) = LuaComment::cast(child.clone()) {
            if collected
                .consumed_comment_ranges
                .iter()
                .any(|range| *range == comment.syntax().text_range())
            {
                continue;
            }
            let docs = if table_leading_comment_should_be_formatted(&comment) {
                render::render_comment_with_spacing(ctx, &comment, plan)
            } else {
                vec![ir::source_node_trimmed(comment.syntax().clone())]
            };
            let blank_lines_before = count_blank_lines_before(comment.syntax());
            collected.has_comments = true;
            if !seen_field {
                collected.comments_after_open.push(DelimitedComment {
                    docs,
                    same_line_after_open: has_non_trivia_before_on_same_line_tokenwise(
                        comment.syntax(),
                    ),
                    blank_lines_before,
                });
            } else {
                pending_comments.push(TableSeparatedComment {
                    docs,
                    blank_lines_before,
                });
            }
            continue;
        }

        if let Some(field) = LuaTableField::cast(child) {
            let blank_lines_before = count_blank_lines_before(field.syntax());
            let trailing_comment = extract_trailing_comment(ctx, plan, field.syntax());
            if trailing_comment.is_some() {
                collected.has_comments = true;
            }
            if blank_lines_before > 0 {
                collected.has_comments = true;
            }
            if let Some((_, range)) = &trailing_comment {
                collected.consumed_comment_ranges.push(*range);
            }
            let comment_align_hint = trailing_comment.as_ref().is_some_and(|(_, range)| {
                trailing_gap_requests_alignment(
                    field.syntax(),
                    *range,
                    ctx.config.comments.line_comment_min_spaces_before.max(1),
                )
            });
            collected.entries.push(TableEntry {
                field: field.clone(),
                leading_comments: std::mem::take(&mut pending_comments),
                blank_lines_before,
                doc: format_table_field_ir(ctx, plan, &field),
                eq_split: None,
                align_hint: field_requests_alignment(&field),
                comment_align_hint,
                trailing_comment: trailing_comment.map(|(docs, _)| docs),
            });
            seen_field = true;
        }
    }

    if !pending_comments.is_empty() {
        collected.comments_before_close = pending_comments;
    }

    collected
}

fn table_leading_comment_should_be_formatted(comment: &LuaComment) -> bool {
    comment
        .syntax()
        .first_token()
        .is_some_and(|token| matches!(token.kind().to_token(), LuaTokenKind::TkDocStart))
}

fn format_table_with_comments(
    ctx: &FormatContext,
    plan: &FormatPlan,
    expr: &LuaTableExpr,
    mut collected: CollectedTableEntries,
) -> Vec<DocIR> {
    let (open, close) = brace_tokens(expr.syntax());
    let mut docs = vec![token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftBrace)];
    let trailing = trailing_comma_ir(ctx.config.trailing_table_comma());
    let should_align_eq = ctx.config.align.table_field
        && collected
            .entries
            .iter()
            .any(|entry| entry.field.is_assign_field())
        && table_group_requests_alignment(&collected.entries);

    if should_align_eq {
        populate_table_eq_splits(ctx, plan, &mut collected.entries);
    }

    if !collected.comments_after_open.is_empty() || !collected.entries.is_empty() {
        let mut inner = Vec::new();

        let mut first_inner_line_started = false;
        for comment in collected.comments_after_open {
            if comment.same_line_after_open && !first_inner_line_started {
                let mut suffix = trailing_comment_prefix(ctx);
                suffix.extend(comment.docs);
                docs.push(ir::line_suffix(suffix));
            } else {
                push_table_blank_lines(ctx, &mut inner, comment.blank_lines_before);
                inner.extend(comment.docs);
                first_inner_line_started = true;
            }
        }

        inner.extend(build_table_expanded_inner(
            ctx,
            &collected.entries,
            &trailing,
            should_align_eq,
            ctx.config.should_align_table_line_comments(),
        ));

        for comment in collected.comments_before_close {
            push_table_blank_lines(ctx, &mut inner, comment.blank_lines_before);
            inner.extend(comment.docs);
        }

        docs.push(ir::indent(inner));
        docs.push(ir::hard_line());
    }

    docs.push(token_or_kind_doc(
        close.as_ref(),
        LuaTokenKind::TkRightBrace,
    ));
    docs
}

fn format_table_field_ir(
    ctx: &FormatContext,
    plan: &FormatPlan,
    field: &LuaTableField,
) -> Vec<DocIR> {
    let mut docs = Vec::new();

    if field.is_assign_field() {
        docs.extend(format_table_field_key_ir(ctx, plan, field));
        let assign_space = if ctx.config.spacing.space_around_assign_operator {
            ir::space()
        } else {
            ir::list(vec![])
        };
        docs.push(assign_space.clone());
        docs.push(ir::syntax_token(LuaTokenKind::TkAssign));
        docs.push(assign_space);
    }

    if let Some(value) = field.get_value_expr() {
        docs.extend(format_table_field_value_ir(
            ctx,
            plan,
            &value,
            field.is_assign_field() && ctx.config.layout.prefer_chain_break_on_statement_tail,
        ));
    }

    docs
}

fn format_table_field_key_ir(
    ctx: &FormatContext,
    plan: &FormatPlan,
    field: &LuaTableField,
) -> Vec<DocIR> {
    let Some(key) = field.get_field_key() else {
        return Vec::new();
    };

    match key {
        LuaIndexKey::Name(name) => vec![ir::source_token(name.syntax().clone())],
        LuaIndexKey::String(string) => vec![
            ir::syntax_token(LuaTokenKind::TkLeftBracket),
            ir::source_token(string.syntax().clone()),
            ir::syntax_token(LuaTokenKind::TkRightBracket),
        ],
        LuaIndexKey::Integer(number) => vec![
            ir::syntax_token(LuaTokenKind::TkLeftBracket),
            ir::source_token(number.syntax().clone()),
            ir::syntax_token(LuaTokenKind::TkRightBracket),
        ],
        LuaIndexKey::Expr(expr) => vec![
            ir::syntax_token(LuaTokenKind::TkLeftBracket),
            ir::list(format_expr(ctx, plan, &expr)),
            ir::syntax_token(LuaTokenKind::TkRightBracket),
        ],
        LuaIndexKey::Idx(_) => Vec::new(),
    }
}

fn format_table_field_value_ir(
    ctx: &FormatContext,
    plan: &FormatPlan,
    value: &LuaExpr,
    prefer_chain_break: bool,
) -> Vec<DocIR> {
    if let LuaExpr::TableExpr(table) = value
        && value.syntax().text().contains_char('\n')
    {
        return format_multiline_table_expr(ctx, plan, table);
    }

    format_expr_with_options(ctx, plan, value, ExprFormatOptions { prefer_chain_break })
}

fn format_table_field_eq_split(
    ctx: &FormatContext,
    plan: &FormatPlan,
    field: &LuaTableField,
) -> Option<EqSplitDocs> {
    if !field.is_assign_field() {
        return None;
    }

    let before = format_table_field_key_ir(ctx, plan, field);
    if before.is_empty() {
        return None;
    }

    let assign_space = if ctx.config.spacing.space_around_assign_operator {
        ir::space()
    } else {
        ir::list(vec![])
    };
    let mut after = vec![
        ir::syntax_token(LuaTokenKind::TkAssign),
        assign_space.clone(),
    ];
    if let Some(value) = field.get_value_expr() {
        after.extend(format_table_field_value_ir(
            ctx,
            plan,
            &value,
            ctx.config.layout.prefer_chain_break_on_statement_tail,
        ));
    }
    Some((before, after))
}

fn populate_table_eq_splits(ctx: &FormatContext, plan: &FormatPlan, entries: &mut [TableEntry]) {
    for entry in entries {
        if entry.eq_split.is_none() && entry.field.is_assign_field() {
            entry.eq_split = format_table_field_eq_split(ctx, plan, &entry.field);
        }
    }
}

fn field_requests_alignment(field: &LuaTableField) -> bool {
    if !field.is_assign_field() {
        return false;
    }

    let Some(assign_token) = field.syntax().children_with_tokens().find_map(|element| {
        let token = element.into_token()?;
        (token.kind() == LuaTokenKind::TkAssign.into()).then_some(token)
    }) else {
        return false;
    };

    let mut gap_width = 0usize;
    let mut previous = assign_token.prev_token();

    while let Some(token) = previous {
        match token.kind().to_token() {
            LuaTokenKind::TkWhitespace => {
                for ch in token.text().chars().rev() {
                    if matches!(ch, '\n' | '\r') {
                        return false;
                    }
                    if matches!(ch, ' ' | '\t') {
                        gap_width += 1;
                    }
                }
                previous = token.prev_token();
            }
            LuaTokenKind::TkEndOfLine => return false,
            _ => break,
        }
    }

    gap_width > 1
}

fn table_group_requests_alignment(entries: &[TableEntry]) -> bool {
    entries.iter().any(|entry| entry.align_hint)
}

fn table_comment_group_requests_alignment(entries: &[TableEntry]) -> bool {
    entries
        .iter()
        .any(|entry| entry.trailing_comment.is_some() && entry.comment_align_hint)
}

fn wrap_table_multiline_docs(open: DocIR, close: DocIR, inner: Vec<DocIR>) -> Vec<DocIR> {
    let mut docs = vec![open];
    if !inner.is_empty() {
        docs.push(ir::indent(inner));
        docs.push(ir::hard_line());
    }
    docs.push(close);
    docs
}

fn build_table_expanded_inner(
    ctx: &FormatContext,
    entries: &[TableEntry],
    trailing: &DocIR,
    align_eq: bool,
    align_comments: bool,
) -> Vec<DocIR> {
    let mut inner = Vec::new();
    let last_field_idx = entries.iter().rposition(|_| true);
    let trailing_comment_widths = aligned_trailing_comment_widths_for_widths(
        align_comments && entries.iter().any(|entry| entry.trailing_comment.is_some()),
        entries.iter().enumerate().map(|(index, entry)| {
            (
                table_entry_comment_alignment_width(ctx, &entry.doc, last_field_idx == Some(index)),
                entry.trailing_comment.is_some(),
            )
        }),
    );

    if align_eq {
        let mut index = 0usize;
        while index < entries.len() {
            if entries[index].eq_split.is_some() {
                let group_start = index;
                let mut group_end = index + 1;
                while group_end < entries.len()
                    && entries[group_end].eq_split.is_some()
                    && entries[group_end].leading_comments.is_empty()
                    && entries[group_end].blank_lines_before == 0
                {
                    group_end += 1;
                }

                if group_end - group_start >= 2
                    && table_group_requests_alignment(&entries[group_start..group_end])
                {
                    push_table_entry_prefix(ctx, &mut inner, &entries[group_start]);

                    let comment_widths = if align_comments {
                        aligned_table_comment_widths(
                            ctx,
                            entries,
                            group_start,
                            group_end,
                            last_field_idx,
                            trailing,
                        )
                    } else {
                        vec![None; group_end - group_start]
                    };

                    let mut align_entries = Vec::new();
                    for current in group_start..group_end {
                        let entry = &entries[current];
                        if let Some((before, after)) = &entry.eq_split {
                            let is_last = last_field_idx == Some(current);
                            let mut after_docs = after.clone();
                            if is_last {
                                after_docs.push(trailing.clone());
                            } else {
                                after_docs.push(ir::syntax_token(LuaTokenKind::TkComma));
                            }

                            if let Some(comment_docs) = &entry.trailing_comment {
                                if let Some(padding) = comment_widths[current - group_start] {
                                    after_docs
                                        .push(aligned_table_comment_suffix(comment_docs, padding));
                                    align_entries.push(AlignEntry::Aligned {
                                        before: before.clone(),
                                        after: after_docs,
                                        trailing: None,
                                    });
                                } else {
                                    let mut suffix = trailing_comment_prefix(ctx);
                                    suffix.extend(comment_docs.clone());
                                    after_docs.push(ir::line_suffix(suffix));
                                    align_entries.push(AlignEntry::Aligned {
                                        before: before.clone(),
                                        after: after_docs,
                                        trailing: None,
                                    });
                                }
                            } else {
                                align_entries.push(AlignEntry::Aligned {
                                    before: before.clone(),
                                    after: after_docs,
                                    trailing: None,
                                });
                            }
                        }
                    }
                    inner.push(ir::align_group(align_entries));
                    index = group_end;
                    continue;
                }
            }

            push_table_entry_line(
                ctx,
                &mut inner,
                &entries[index],
                index,
                last_field_idx,
                trailing,
                trailing_comment_widths.get(index).copied().flatten(),
            );
            index += 1;
        }

        return inner;
    }

    for (index, entry) in entries.iter().enumerate() {
        push_table_entry_line(
            ctx,
            &mut inner,
            entry,
            index,
            last_field_idx,
            trailing,
            trailing_comment_widths.get(index).copied().flatten(),
        );
    }

    inner
}

fn push_table_entry_line(
    ctx: &FormatContext,
    inner: &mut Vec<DocIR>,
    entry: &TableEntry,
    index: usize,
    last_field_idx: Option<usize>,
    trailing: &DocIR,
    aligned_content_width: Option<usize>,
) {
    push_table_entry_prefix(ctx, inner, entry);
    inner.extend(entry.doc.clone());
    if last_field_idx == Some(index) {
        inner.push(trailing.clone());
    } else {
        inner.push(ir::syntax_token(LuaTokenKind::TkComma));
    }
    if let Some(comment_docs) = &entry.trailing_comment {
        let mut suffix = trailing_comment_prefix_for_width(
            ctx,
            table_entry_comment_alignment_width(ctx, &entry.doc, last_field_idx == Some(index)),
            aligned_content_width,
        );
        suffix.extend(comment_docs.clone());
        inner.push(ir::line_suffix(suffix));
    }
}

fn push_table_entry_prefix(ctx: &FormatContext, inner: &mut Vec<DocIR>, entry: &TableEntry) {
    if let Some((first_comment, rest)) = entry.leading_comments.split_first() {
        push_table_blank_lines(ctx, inner, first_comment.blank_lines_before);
        inner.extend(first_comment.docs.clone());
        for comment in rest {
            push_table_blank_lines(ctx, inner, comment.blank_lines_before);
            inner.extend(comment.docs.clone());
        }
        push_table_blank_lines(ctx, inner, entry.blank_lines_before);
    } else {
        push_table_blank_lines(ctx, inner, entry.blank_lines_before);
    }
}

fn push_table_blank_lines(ctx: &FormatContext, inner: &mut Vec<DocIR>, blank_lines_before: usize) {
    inner.push(ir::hard_line());
    for _ in 0..blank_lines_before.min(ctx.config.layout.max_blank_lines) {
        inner.push(ir::hard_line());
    }
}

fn table_entry_comment_alignment_width(
    ctx: &FormatContext,
    entry_docs: &[DocIR],
    is_last: bool,
) -> usize {
    let separator_width = if is_last {
        match ctx.config.trailing_table_comma() {
            TrailingComma::Never => 0,
            TrailingComma::Multiline | TrailingComma::Always => 1,
        }
    } else {
        1
    };

    ir::ir_flat_width(entry_docs) + separator_width
}

fn aligned_table_comment_widths(
    ctx: &FormatContext,
    entries: &[TableEntry],
    group_start: usize,
    group_end: usize,
    last_field_idx: Option<usize>,
    trailing: &DocIR,
) -> Vec<Option<usize>> {
    let mut widths = vec![None; group_end - group_start];
    let mut subgroup_start = group_start;

    while subgroup_start < group_end {
        while subgroup_start < group_end && entries[subgroup_start].trailing_comment.is_none() {
            subgroup_start += 1;
        }
        if subgroup_start >= group_end {
            break;
        }

        let mut subgroup_end = subgroup_start + 1;
        while subgroup_end < group_end && entries[subgroup_end].trailing_comment.is_some() {
            subgroup_end += 1;
        }

        if table_comment_group_requests_alignment(&entries[subgroup_start..subgroup_end]) {
            let max_content_width = (subgroup_start..subgroup_end)
                .filter_map(|index| {
                    let entry = &entries[index];
                    let (before, after) = entry.eq_split.as_ref()?;
                    let mut content = before.clone();
                    content.push(ir::space());
                    content.extend(after.clone());
                    if last_field_idx == Some(index) {
                        content.push(trailing.clone());
                    } else {
                        content.push(ir::syntax_token(LuaTokenKind::TkComma));
                    }
                    Some(ir::ir_flat_width(&content))
                })
                .max()
                .unwrap_or(0);

            for index in subgroup_start..subgroup_end {
                let entry = &entries[index];
                if let Some((before, after)) = &entry.eq_split {
                    let mut content = before.clone();
                    content.push(ir::space());
                    content.extend(after.clone());
                    if last_field_idx == Some(index) {
                        content.push(trailing.clone());
                    } else {
                        content.push(ir::syntax_token(LuaTokenKind::TkComma));
                    }
                    widths[index - group_start] = Some(trailing_comment_padding_for_config(
                        ctx,
                        ir::ir_flat_width(&content),
                        max_content_width,
                    ));
                }
            }
        }

        subgroup_start = subgroup_end;
    }

    widths
}

fn aligned_table_comment_suffix(comment_docs: &[DocIR], padding: usize) -> DocIR {
    let mut suffix = Vec::new();
    suffix.extend((0..padding).map(|_| ir::space()));
    suffix.extend(comment_docs.iter().cloned());
    ir::line_suffix(suffix)
}

fn trailing_comment_padding_for_config(
    ctx: &FormatContext,
    content_width: usize,
    aligned_content_width: usize,
) -> usize {
    let natural_padding = aligned_content_width.saturating_sub(content_width)
        + ctx.config.comments.line_comment_min_spaces_before.max(1);

    if ctx.config.comments.line_comment_min_column == 0 {
        natural_padding
    } else {
        natural_padding.max(
            ctx.config
                .comments
                .line_comment_min_column
                .saturating_sub(content_width),
        )
    }
}

fn format_closure_expr(
    ctx: &FormatContext,
    plan: &FormatPlan,
    expr: &LuaClosureExpr,
) -> Vec<DocIR> {
    if let Some(inline_docs) = try_format_simple_inline_closure_expr(ctx, plan, expr) {
        return inline_docs;
    }

    let shell_plan = collect_closure_shell_plan(ctx, plan, expr);
    render_closure_shell(ctx, plan, expr, shell_plan)
}

fn format_ternary_expr(
    ctx: &FormatContext,
    plan: &FormatPlan,
    expr: &LuaTernaryExpr,
) -> Vec<DocIR> {
    let Some(cond_expr) = expr.get_condition_expr() else {
        return vec![ir::source_node(expr.syntax().clone())];
    };
    let Some((true_expr, false_expr)) = expr.get_true_false_exprs() else {
        return vec![ir::source_node(expr.syntax().clone())];
    };
    let mut docs = format_expr(ctx, plan, &cond_expr);
    docs.push(ir::space());
    docs.push(ir::syntax_token(LuaTokenKind::TkTernary));
    docs.push(ir::space());
    docs.extend(format_expr(ctx, plan, &true_expr));
    docs.push(ir::space());
    docs.push(ir::syntax_token(LuaTokenKind::TkColon));
    docs.push(ir::space());
    docs.extend(format_expr(ctx, plan, &false_expr));
    docs
}

fn try_format_simple_inline_closure_expr(
    ctx: &FormatContext,
    plan: &FormatPlan,
    expr: &LuaClosureExpr,
) -> Option<Vec<DocIR>> {
    let source_is_single_line = !expr.syntax().text().contains_char('\n');
    match ctx.config.output.simple_lambda_single_line {
        SimpleLambdaSingleLine::Never => return None,
        SimpleLambdaSingleLine::Preserve if !source_is_single_line => return None,
        SimpleLambdaSingleLine::Always | SimpleLambdaSingleLine::Preserve => {}
    }

    if node_has_direct_comment_child(expr.syntax()) {
        return None;
    }

    let shell_plan = collect_closure_shell_plan(ctx, plan, expr);
    if !shell_plan.before_params_comments.is_empty() || !shell_plan.before_body_comments.is_empty()
    {
        return None;
    }
    if ir::ir_has_forced_line_break(&shell_plan.params) {
        return None;
    }

    let block = expr.get_block()?;
    if node_has_direct_comment_child(block.syntax()) {
        return None;
    }

    let mut stats = block.get_stats();
    let LuaStat::ReturnStat(return_stat) = stats.next()? else {
        return None;
    };
    if stats.next().is_some() || node_has_direct_comment_child(return_stat.syntax()) {
        return None;
    }

    let mut returned_exprs = return_stat.get_expr_list();
    let returned_expr = returned_exprs.next()?;
    if returned_exprs.next().is_some() {
        return None;
    }

    let returned_docs = format_expr(ctx, plan, &returned_expr);
    if ir::ir_has_forced_line_break(&returned_docs) {
        return None;
    }

    let mut docs = vec![ir::syntax_token(LuaTokenKind::TkFunction)];
    if let Some(params) = expr.get_params_list() {
        let (open, _) = paren_tokens(params.syntax());
        docs.extend(token_left_spacing_docs(plan, open.as_ref()));
    }
    docs.extend(shell_plan.params);
    docs.push(ir::space());
    docs.push(ir::syntax_token(LuaTokenKind::TkReturn));
    docs.push(ir::space());
    docs.extend(returned_docs);
    docs.push(ir::space());
    docs.push(ir::syntax_token(LuaTokenKind::TkEnd));

    (ir::ir_flat_width(&docs) + source_line_prefix_width(expr.syntax())
        <= ctx.config.layout.max_line_width)
        .then_some(docs)
}

struct InlineCommentFragment {
    docs: Vec<DocIR>,
    same_line_before: bool,
}

struct ClosureShellPlan {
    params: Vec<DocIR>,
    before_params_comments: Vec<InlineCommentFragment>,
    before_body_comments: Vec<InlineCommentFragment>,
}

fn collect_closure_shell_plan(
    ctx: &FormatContext,
    plan: &FormatPlan,
    expr: &LuaClosureExpr,
) -> ClosureShellPlan {
    let mut params = vec![
        ir::syntax_token(LuaTokenKind::TkLeftParen),
        ir::syntax_token(LuaTokenKind::TkRightParen),
    ];
    let mut before_params_comments = Vec::new();
    let mut before_body_comments = Vec::new();
    let mut seen_params = false;

    for child in expr.syntax().children() {
        if let Some(params_list) = LuaParamList::cast(child.clone()) {
            params = format_param_list_ir(ctx, plan, &params_list);
            seen_params = true;
        } else if let Some(comment) = LuaComment::cast(child) {
            let fragment = InlineCommentFragment {
                docs: vec![ir::source_node_trimmed(comment.syntax().clone())],
                same_line_before: has_non_trivia_before_on_same_line_tokenwise(comment.syntax()),
            };
            if seen_params {
                before_body_comments.push(fragment);
            } else {
                before_params_comments.push(fragment);
            }
        }
    }

    ClosureShellPlan {
        params,
        before_params_comments,
        before_body_comments,
    }
}

fn render_closure_shell(
    ctx: &FormatContext,
    root_plan: &FormatPlan,
    expr: &LuaClosureExpr,
    plan: ClosureShellPlan,
) -> Vec<DocIR> {
    let mut docs = vec![ir::syntax_token(LuaTokenKind::TkFunction)];
    let mut broke_before_params = false;

    for comment in plan.before_params_comments {
        if comment.same_line_before && !broke_before_params {
            let mut suffix = trailing_comment_prefix(ctx);
            suffix.extend(comment.docs);
            docs.push(ir::line_suffix(suffix));
        } else {
            docs.push(ir::hard_line());
            docs.extend(comment.docs);
        }
        broke_before_params = true;
    }

    if broke_before_params {
        docs.push(ir::hard_line());
    } else if let Some(params) = expr.get_params_list() {
        let (open, _) = paren_tokens(params.syntax());
        docs.extend(token_left_spacing_docs(root_plan, open.as_ref()));
    }
    docs.extend(plan.params);

    let mut body_comment_lines = Vec::new();
    let mut saw_same_line_body_comment = false;
    for comment in plan.before_body_comments {
        if comment.same_line_before && body_comment_lines.is_empty() {
            let mut suffix = trailing_comment_prefix(ctx);
            suffix.extend(comment.docs);
            docs.push(ir::line_suffix(suffix));
            saw_same_line_body_comment = true;
        } else {
            body_comment_lines.push(comment.docs);
        }
    }

    let rendered_body_docs = render::render_closure_block_body(ctx, expr, root_plan);
    let has_body_content = !body_comment_lines.is_empty() || !rendered_body_docs.is_empty();

    if has_body_content {
        let mut block_docs = vec![ir::hard_line()];
        for comment_docs in body_comment_lines {
            block_docs.extend(comment_docs);
            block_docs.push(ir::hard_line());
        }
        block_docs.extend(block_docs_from_rendered_body(rendered_body_docs));
        docs.push(ir::indent(block_docs));
        docs.push(ir::hard_line());
    } else if saw_same_line_body_comment {
        docs.push(ir::hard_line());
    }

    if !saw_same_line_body_comment && expr.get_block().is_none() && !has_body_content {
        if closure_end_starts_on_new_line(expr.syntax()) {
            docs.push(ir::hard_line());
        } else {
            docs.push(ir::space());
        }
    }

    docs.push(ir::syntax_token(LuaTokenKind::TkEnd));
    docs
}

fn block_docs_from_rendered_body(mut body_docs: Vec<DocIR>) -> Vec<DocIR> {
    while matches!(body_docs.first(), Some(DocIR::HardLine)) {
        body_docs.remove(0);
    }
    while matches!(body_docs.last(), Some(DocIR::HardLine)) {
        body_docs.pop();
    }
    body_docs
}

fn closure_end_starts_on_new_line(syntax: &LuaSyntaxNode) -> bool {
    let Some(end_token) = first_direct_token(syntax, LuaTokenKind::TkEnd) else {
        return false;
    };
    let mut previous = end_token.prev_token();

    while let Some(token) = previous {
        match token.kind().to_token() {
            LuaTokenKind::TkWhitespace => previous = token.prev_token(),
            LuaTokenKind::TkEndOfLine => return true,
            _ => return false,
        }
    }

    false
}

fn expr_is_chain_root(expr: &LuaExpr) -> bool {
    let Some(parent) = expr.syntax().parent() else {
        return true;
    };

    let Some(parent_expr) = LuaExpr::cast(parent) else {
        return true;
    };

    match parent_expr {
        LuaExpr::CallExpr(parent_call) => parent_call.get_prefix_expr().is_none_or(|prefix| {
            LuaSyntaxId::from_node(prefix.syntax()) != LuaSyntaxId::from_node(expr.syntax())
        }),
        LuaExpr::IndexExpr(parent_index) => parent_index.get_prefix_expr().is_none_or(|prefix| {
            LuaSyntaxId::from_node(prefix.syntax()) != LuaSyntaxId::from_node(expr.syntax())
        }),
        _ => true,
    }
}

fn expr_may_need_chain_format(expr: &LuaExpr) -> bool {
    match expr {
        LuaExpr::CallExpr(call) => call
            .get_prefix_expr()
            .is_some_and(|prefix| prefix_contains_call_chain(&prefix)),
        LuaExpr::IndexExpr(index) => index
            .get_prefix_expr()
            .is_some_and(|prefix| prefix_can_extend_chain(&prefix)),
        _ => false,
    }
}

fn prefix_contains_call_chain(prefix: &LuaExpr) -> bool {
    match prefix {
        LuaExpr::CallExpr(_) => true,
        LuaExpr::IndexExpr(index) => index
            .get_prefix_expr()
            .is_some_and(|base| prefix_contains_call_chain(&base)),
        _ => false,
    }
}

fn prefix_can_extend_chain(prefix: &LuaExpr) -> bool {
    match prefix {
        LuaExpr::CallExpr(_) => true,
        LuaExpr::IndexExpr(index) => index
            .get_prefix_expr()
            .is_some_and(|base| matches!(base, LuaExpr::CallExpr(_) | LuaExpr::IndexExpr(_))),
        _ => false,
    }
}

fn chain_has_direct_comments(expr: &LuaExpr) -> bool {
    if node_has_direct_comment_child(expr.syntax()) {
        return true;
    }

    match expr {
        LuaExpr::CallExpr(call) => call
            .get_prefix_expr()
            .is_some_and(|prefix| chain_has_direct_comments(&prefix)),
        LuaExpr::IndexExpr(index) => index
            .get_prefix_expr()
            .is_some_and(|prefix| chain_has_direct_comments(&prefix)),
        _ => false,
    }
}

fn try_format_chain_expr(
    ctx: &FormatContext,
    plan: &FormatPlan,
    expr: &LuaExpr,
    options: ExprFormatOptions,
) -> Option<Vec<DocIR>> {
    let (root, mut segments) = collect_chain_segments(ctx, plan, expr)?;
    if segments.len() <= 1 {
        return None;
    }
    let preserve_multiline_chain = chain_has_explicit_segment_break(expr);

    let mut root_docs = format_expr(ctx, plan, &root);
    let mut head_ends_with_call = matches!(root, LuaExpr::CallExpr(_));
    if segments
        .first()
        .is_some_and(|segment| segment.attach_to_root)
    {
        let first_segment = segments.remove(0);
        root_docs.extend(first_segment.docs);
        head_ends_with_call = true;
    }

    if segments.is_empty() {
        return Some(root_docs);
    }

    let segment_docs: Vec<Vec<DocIR>> = segments
        .iter()
        .map(|segment| segment.docs.clone())
        .collect();
    let flat_tail = segment_docs.concat();
    let flat = {
        let mut docs = root_docs.clone();
        docs.extend(flat_tail);
        docs
    };

    if options.prefer_chain_break
        && let Some((head_docs, tail_segments)) =
            preferred_chain_break_parts(&root_docs, &segments, head_ends_with_call)
        && tail_segments
            .iter()
            .all(|docs| !ir::ir_has_forced_line_break(docs))
    {
        return Some(format_preferred_broken_chain(
            ctx,
            expr,
            head_docs,
            tail_segments,
        ));
    }

    if preserve_multiline_chain {
        return Some(format_preserved_multiline_chain(
            ctx,
            expr,
            root_docs,
            segment_docs,
        ));
    }

    if segment_docs
        .iter()
        .any(|docs| ir::ir_has_forced_line_break(docs))
    {
        return Some(flat);
    }

    let fill_tail = ir::fill(build_fill_parts(&segment_docs, &[ir::soft_line_or_empty()]));
    let one_per_line_tail = ir::intersperse(segment_docs.clone(), vec![ir::hard_line()]);

    let fill = vec![ir::group(vec![
        ir::list(root_docs.clone()),
        ir::indent(vec![ir::soft_line(), fill_tail]),
    ])];
    let one_per_line = vec![ir::group_break(vec![
        ir::list(root_docs),
        ir::indent(vec![ir::hard_line(), ir::list(one_per_line_tail)]),
    ])];

    Some(choose_sequence_layout(
        ctx,
        SequenceLayoutCandidates {
            flat: Some(flat),
            fill: Some(fill),
            one_per_line: Some(one_per_line),
            ..Default::default()
        },
        SequenceLayoutPolicy {
            allow_fill: true,
            prefer_balanced_break_lines: true,
            first_line_prefix_width: chain_expr_first_line_prefix_width(expr),
            ..Default::default()
        },
    ))
}

fn chain_expr_first_line_prefix_width(expr: &LuaExpr) -> usize {
    if expr.get_parent::<LuaCallArgList>().is_some() || expr.get_parent::<LuaBinaryExpr>().is_some()
    {
        0
    } else {
        source_line_prefix_width(expr.syntax())
    }
}

fn format_preserved_multiline_chain(
    ctx: &FormatContext,
    expr: &LuaExpr,
    root_docs: Vec<DocIR>,
    segments: Vec<Vec<DocIR>>,
) -> Vec<DocIR> {
    let mut docs = root_docs.clone();
    let mut start_index = 0usize;
    let first_line_prefix_width = source_line_prefix_width(expr.syntax());

    if let Some(first_segment) = segments.first()
        && source_keeps_first_chain_segment_attached(expr)
        && !ir::ir_has_forced_line_break(first_segment)
        && first_line_prefix_width
            + ir::ir_flat_width(&root_docs)
            + ir::ir_flat_width(first_segment)
            <= ctx.config.layout.max_line_width
    {
        docs.extend(first_segment.clone());
        start_index = 1;
    }

    if start_index >= segments.len() {
        return docs;
    }

    let remaining_tail = ir::intersperse(segments[start_index..].to_vec(), vec![ir::hard_line()]);
    docs.push(ir::indent(vec![ir::hard_line(), ir::list(remaining_tail)]));
    docs
}

fn format_preferred_broken_chain(
    ctx: &FormatContext,
    expr: &LuaExpr,
    mut head_docs: Vec<DocIR>,
    tail_segments: Vec<Vec<DocIR>>,
) -> Vec<DocIR> {
    if tail_segments.is_empty() {
        return head_docs;
    }

    let remaining_tail = ir::intersperse(tail_segments, vec![ir::hard_line()]);
    head_docs.push(ir::indent(vec![ir::hard_line(), ir::list(remaining_tail)]));
    let _ = ctx;
    let _ = expr;
    head_docs
}

type PreferredChainBreakParts = (Vec<DocIR>, Vec<Vec<DocIR>>);

fn preferred_chain_break_parts(
    root_docs: &[DocIR],
    segments: &[ChainSegment],
    head_ends_with_call: bool,
) -> Option<PreferredChainBreakParts> {
    if head_ends_with_call {
        if segments.len() < 2 {
            return None;
        }

        let head_docs = root_docs.to_vec();
        let tail_segments = segments
            .iter()
            .map(|segment| segment.docs.clone())
            .collect();
        return Some((head_docs, tail_segments));
    }

    if segments.len() < 3 {
        return None;
    }

    let first_call_index = segments
        .iter()
        .position(|segment| segment.kind == ChainSegmentKind::Call)?;
    if first_call_index + 1 >= segments.len() {
        return None;
    }

    let mut head_docs = root_docs.to_vec();
    for segment in &segments[..=first_call_index] {
        head_docs.extend(segment.docs.clone());
    }

    let tail_segments = segments[first_call_index + 1..]
        .iter()
        .map(|segment| segment.docs.clone())
        .collect();
    Some((head_docs, tail_segments))
}

fn chain_has_explicit_segment_break(expr: &LuaExpr) -> bool {
    match expr {
        LuaExpr::CallExpr(call) => {
            let Some(prefix) = call.get_prefix_expr() else {
                return false;
            };

            if let LuaExpr::IndexExpr(index) = &prefix
                && let Some(base) = index.get_prefix_expr()
            {
                return token_starts_on_new_line(chain_index_boundary_token(index).as_ref())
                    || chain_has_explicit_segment_break(&base);
            }

            token_starts_on_new_line(chain_call_boundary_token(call).as_ref())
                || chain_has_explicit_segment_break(&prefix)
        }
        LuaExpr::IndexExpr(index) => {
            let Some(prefix) = index.get_prefix_expr() else {
                return false;
            };

            token_starts_on_new_line(chain_index_boundary_token(index).as_ref())
                || chain_has_explicit_segment_break(&prefix)
        }
        _ => false,
    }
}

fn source_keeps_first_chain_segment_attached(expr: &LuaExpr) -> bool {
    let source_text = expr.syntax().text().to_string();
    let Some(first_line) = source_text.lines().next() else {
        return false;
    };

    first_line.contains(':') || first_line.contains('.') || first_line.contains('[')
}

fn chain_index_boundary_token(index: &LuaIndexExpr) -> Option<LuaSyntaxToken> {
    first_direct_token(index.syntax(), LuaTokenKind::TkColon)
        .or_else(|| first_direct_token(index.syntax(), LuaTokenKind::TkDot))
        .or_else(|| first_direct_token(index.syntax(), LuaTokenKind::TkLeftBracket))
}

fn chain_call_boundary_token(call: &LuaCallExpr) -> Option<LuaSyntaxToken> {
    let args_list = call.get_args_list()?;
    first_direct_token(args_list.syntax(), LuaTokenKind::TkLeftParen)
}

fn token_starts_on_new_line(token: Option<&LuaSyntaxToken>) -> bool {
    let Some(token) = token else {
        return false;
    };
    let mut previous = token.prev_token();

    while let Some(prev) = previous {
        match prev.kind().to_token() {
            LuaTokenKind::TkWhitespace => previous = prev.prev_token(),
            LuaTokenKind::TkEndOfLine => return true,
            _ => return false,
        }
    }

    false
}

fn collect_chain_segments(
    ctx: &FormatContext,
    plan: &FormatPlan,
    expr: &LuaExpr,
) -> Option<ChainSegments> {
    match expr {
        LuaExpr::CallExpr(call) => collect_call_chain_segments(ctx, plan, call),
        LuaExpr::IndexExpr(index) => {
            let prefix = index.get_prefix_expr()?;
            let (root, mut segments) = collect_chain_segments(ctx, plan, &prefix)
                .unwrap_or_else(|| (prefix.clone(), Vec::new()));
            segments.push(ChainSegment {
                docs: format_index_access_ir(ctx, plan, index),
                kind: ChainSegmentKind::Access,
                attach_to_root: false,
            });
            Some((root, segments))
        }
        _ => None,
    }
}

fn collect_call_chain_segments(
    ctx: &FormatContext,
    plan: &FormatPlan,
    call: &LuaCallExpr,
) -> Option<ChainSegments> {
    let prefix = call.get_prefix_expr()?;

    if let LuaExpr::IndexExpr(index) = &prefix
        && let Some(base) = index.get_prefix_expr()
    {
        let (root, mut segments) =
            collect_chain_segments(ctx, plan, &base).unwrap_or_else(|| (base.clone(), Vec::new()));
        let mut segment = format_index_access_ir(ctx, plan, index);
        segment.extend(format_call_suffix_ir(ctx, plan, call));
        segments.push(ChainSegment {
            docs: segment,
            kind: ChainSegmentKind::Call,
            attach_to_root: false,
        });
        return Some((root, segments));
    }

    let (root, mut segments) =
        collect_chain_segments(ctx, plan, &prefix).unwrap_or_else(|| (prefix.clone(), Vec::new()));
    let attach_to_root = segments.is_empty() && call_has_empty_paren_args(call);
    segments.push(ChainSegment {
        docs: format_call_suffix_ir(ctx, plan, call),
        kind: ChainSegmentKind::Call,
        attach_to_root,
    });
    Some((root, segments))
}

fn call_has_empty_paren_args(call: &LuaCallExpr) -> bool {
    let Some(args_list) = call.get_args_list() else {
        return false;
    };

    args_list.get_args().next().is_none() && !call_arg_list_has_direct_comments(&args_list)
}

fn format_call_suffix_ir(ctx: &FormatContext, plan: &FormatPlan, expr: &LuaCallExpr) -> Vec<DocIR> {
    let Some(args_list) = expr.get_args_list() else {
        return Vec::new();
    };
    let args: Vec<_> = args_list.get_args().collect();

    if let Some(single_arg_docs) = format_single_arg_call_without_parens(ctx, plan, &args_list) {
        let mut docs = vec![ir::space()];
        docs.extend(single_arg_docs);
        return docs;
    }

    let layout_plan = expr_sequence_layout_plan(plan, args_list.syntax());
    if !layout_plan.preserve_multiline {
        match format_compact_call_arg_list(ctx, plan, &args_list, &args) {
            CompactCallArgListAttempt::Formatted(docs) => docs,
            CompactCallArgListAttempt::ReuseDocs(arg_docs) => {
                let attach_first_arg = false;
                format_call_arg_list_from_docs(
                    ctx,
                    plan,
                    &args_list,
                    attach_first_arg,
                    layout_plan,
                    arg_docs,
                )
            }
            CompactCallArgListAttempt::CommentsPresent => {
                format_call_arg_list(ctx, plan, &args_list)
            }
        }
    } else {
        format_call_arg_list(ctx, plan, &args_list)
    }
}

fn format_compact_call_arg_list(
    ctx: &FormatContext,
    plan: &FormatPlan,
    args_list: &LuaCallArgList,
    args: &[LuaExpr],
) -> CompactCallArgListAttempt {
    if call_arg_list_has_direct_comments(args_list) {
        let collected = collect_call_arg_entries(ctx, plan, args_list);
        if collected.has_comments {
            return CompactCallArgListAttempt::CommentsPresent;
        }
    }

    let arg_docs: Vec<Vec<DocIR>> = args.iter().map(|arg| format_expr(ctx, plan, arg)).collect();
    if arg_docs
        .iter()
        .any(|docs| ir::ir_has_forced_line_break(docs))
    {
        return CompactCallArgListAttempt::ReuseDocs(arg_docs);
    }

    let (open, close) = paren_tokens(args_list.syntax());
    let comma = first_direct_token(args_list.syntax(), LuaTokenKind::TkComma);
    CompactCallArgListAttempt::Formatted(format_delimited_sequence(
        ctx,
        DelimitedSequenceLayout {
            open: token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftParen),
            close: token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightParen),
            items: arg_docs,
            strategy: ExpandStrategy::Never,
            preserve_multiline: false,
            flat_separator: comma_flat_separator(plan, comma.as_ref()),
            fill_separator: comma_fill_separator(comma.as_ref()),
            break_separator: comma_break_separator(comma.as_ref()),
            flat_open_padding: token_right_spacing_docs(plan, open.as_ref()),
            flat_close_padding: token_left_spacing_docs(plan, close.as_ref()),
            grouped_padding: grouped_padding_from_pair(plan, open.as_ref(), close.as_ref()),
            flat_trailing: vec![],
            grouped_trailing: trailing_comma_ir(ctx.config.output.trailing_comma.clone()),
        },
    ))
}

fn build_fill_parts(items: &[Vec<DocIR>], separator: &[DocIR]) -> Vec<DocIR> {
    let mut parts = Vec::with_capacity(items.len().saturating_mul(2));

    for (index, item) in items.iter().enumerate() {
        parts.push(ir::list(item.clone()));
        if index + 1 < items.len() {
            parts.push(ir::list(separator.to_vec()));
        }
    }

    parts
}

fn build_fill_parts_with_leading_separator(
    items: &[Vec<DocIR>],
    separator: &[DocIR],
) -> Vec<DocIR> {
    let mut parts = Vec::with_capacity(items.len().saturating_mul(2) + 1);
    parts.push(ir::list(Vec::new()));

    for item in items {
        parts.push(ir::list(separator.to_vec()));
        parts.push(ir::list(item.clone()));
    }

    parts
}

fn try_format_flat_binary_chain(
    ctx: &FormatContext,
    plan: &FormatPlan,
    expr: &LuaBinaryExpr,
) -> Option<Vec<DocIR>> {
    let op_token = expr.get_op_token()?;
    let op = op_token.get_op();
    let mut operands = Vec::new();
    collect_binary_chain_operands(&LuaExpr::BinaryExpr(expr.clone()), op, &mut operands);
    if operands.len() < 3 {
        return None;
    }

    let fill_parts =
        build_binary_chain_fill_parts(ctx, plan, &operands, &op_token.syntax().clone(), op);
    let packed = build_binary_chain_packed(ctx, plan, &operands, &op_token.syntax().clone(), op);

    Some(choose_sequence_layout(
        ctx,
        SequenceLayoutCandidates {
            fill: Some(vec![ir::group(vec![ir::indent(vec![ir::fill(
                fill_parts,
            )])])]),
            packed: Some(packed),
            ..Default::default()
        },
        SequenceLayoutPolicy {
            allow_alignment: false,
            allow_fill: true,
            allow_preserve: false,
            prefer_preserve_multiline: false,
            force_break_on_standalone_comments: false,
            prefer_balanced_break_lines: true,
            first_line_prefix_width: source_line_prefix_width(expr.syntax()),
        },
    ))
}

fn collect_binary_chain_operands(expr: &LuaExpr, op: BinaryOperator, out: &mut Vec<LuaExpr>) {
    if let LuaExpr::BinaryExpr(binary) = expr
        && !node_has_direct_comment_child(binary.syntax())
        && binary
            .get_op_token()
            .is_some_and(|token| token.get_op() == op)
        && let Some((left, right)) = binary.get_exprs()
    {
        collect_binary_chain_operands(&left, op, out);
        collect_binary_chain_operands(&right, op, out);
        return;
    }

    out.push(expr.clone());
}

fn build_binary_chain_fill_parts(
    ctx: &FormatContext,
    plan: &FormatPlan,
    operands: &[LuaExpr],
    op_token: &LuaSyntaxToken,
    op: BinaryOperator,
) -> Vec<DocIR> {
    let mut parts = Vec::new();
    let mut previous = &operands[0];
    let mut first_chunk = format_expr(ctx, plan, &operands[0]);

    for (index, operand) in operands.iter().enumerate().skip(1) {
        let (space_before_segment, segment) =
            build_binary_chain_segment(ctx, plan, previous, operand, op_token, op);

        if index == 1 {
            if space_before_segment {
                first_chunk.push(ir::space());
            }
            first_chunk.extend(segment);
            parts.push(ir::list(first_chunk.clone()));
        } else {
            parts.push(ir::list(vec![continuation_break_ir(space_before_segment)]));
            parts.push(ir::list(segment));
        }

        previous = operand;
    }

    if parts.is_empty() {
        parts.push(ir::list(first_chunk));
    }

    parts
}

fn build_binary_chain_packed(
    ctx: &FormatContext,
    plan: &FormatPlan,
    operands: &[LuaExpr],
    op_token: &LuaSyntaxToken,
    op: BinaryOperator,
) -> Vec<DocIR> {
    let mut first_line = format_expr(ctx, plan, &operands[0]);
    let (space_before, segment) =
        build_binary_chain_segment(ctx, plan, &operands[0], &operands[1], op_token, op);
    if space_before {
        first_line.push(ir::space());
    }
    first_line.extend(segment);

    let mut tail = Vec::new();
    let mut previous = &operands[1];
    let mut remaining = Vec::new();
    for operand in operands.iter().skip(2) {
        remaining.push(build_binary_chain_segment(
            ctx, plan, previous, operand, op_token, op,
        ));
        previous = operand;
    }

    for chunk in remaining.chunks(2) {
        let mut line = Vec::new();
        for (index, (space_before_segment, segment)) in chunk.iter().enumerate() {
            if index > 0 && *space_before_segment {
                line.push(ir::space());
            }
            line.extend(segment.clone());
        }
        tail.push(ir::hard_line());
        tail.extend(line);
    }

    vec![ir::group_break(vec![
        ir::list(first_line),
        ir::indent(tail),
    ])]
}

fn build_binary_chain_segment(
    ctx: &FormatContext,
    plan: &FormatPlan,
    _previous: &LuaExpr,
    operand: &LuaExpr,
    op_token: &LuaSyntaxToken,
    op: BinaryOperator,
) -> (bool, Vec<DocIR>) {
    let space_rule = space_around_binary_op(op, ctx.config);
    let mut segment = Vec::new();
    segment.push(ir::source_token(op_token.clone()));
    segment.push(space_rule.to_ir());
    segment.extend(format_expr(ctx, plan, operand));
    (space_rule != SpaceRule::NoSpace, segment)
}

fn continuation_break_ir(flat_space: bool) -> DocIR {
    if flat_space {
        ir::soft_line()
    } else {
        ir::soft_line_or_empty()
    }
}

fn format_index_access_ir(
    ctx: &FormatContext,
    plan: &FormatPlan,
    expr: &LuaIndexExpr,
) -> Vec<DocIR> {
    let mut docs = Vec::new();
    if let Some(index_token) = expr.get_index_token() {
        if index_token.is_dot() {
            docs.push(ir::syntax_token(LuaTokenKind::TkDot));
            docs.extend(format_named_index_key_ir(expr));
        } else if index_token.is_colon() {
            docs.push(ir::syntax_token(LuaTokenKind::TkColon));
            docs.extend(format_named_index_key_ir(expr));
        } else if index_token.is_left_bracket() {
            docs.push(ir::syntax_token(LuaTokenKind::TkLeftBracket));
            if ctx.config.spacing.space_inside_brackets {
                docs.push(ir::space());
            }
            if let Some(key) = expr.get_index_key() {
                match key {
                    LuaIndexKey::Expr(expr) => docs.extend(format_expr(ctx, plan, &expr)),
                    LuaIndexKey::Integer(number) => {
                        docs.push(ir::source_token(number.syntax().clone()));
                    }
                    LuaIndexKey::String(string) => {
                        docs.push(ir::source_token(string.syntax().clone()));
                    }
                    LuaIndexKey::Name(name) => {
                        docs.push(ir::source_token(name.syntax().clone()));
                    }
                    LuaIndexKey::Idx(_) => {}
                }
            }
            if ctx.config.spacing.space_inside_brackets {
                docs.push(ir::space());
            }
            docs.push(ir::syntax_token(LuaTokenKind::TkRightBracket));
        }
    }
    docs
}

fn format_named_index_key_ir(expr: &LuaIndexExpr) -> Vec<DocIR> {
    if let Some(name_token) = expr.get_index_name_token()
        && name_token.kind() == LuaTokenKind::TkName.into()
    {
        return vec![ir::source_token(name_token)];
    }

    match expr.get_index_key() {
        Some(LuaIndexKey::Name(name)) => vec![ir::source_token(name.syntax().clone())],
        Some(LuaIndexKey::String(string)) => vec![ir::source_token(string.syntax().clone())],
        Some(LuaIndexKey::Integer(number)) => vec![ir::source_token(number.syntax().clone())],
        _ => Vec::new(),
    }
}

fn trailing_comma_ir(policy: TrailingComma) -> DocIR {
    match policy {
        TrailingComma::Never => ir::list(vec![]),
        TrailingComma::Multiline => {
            ir::if_break(ir::syntax_token(LuaTokenKind::TkComma), ir::list(vec![]))
        }
        TrailingComma::Always => ir::syntax_token(LuaTokenKind::TkComma),
    }
}

fn expr_sequence_layout_plan(plan: &FormatPlan, syntax: &LuaSyntaxNode) -> ExprSequenceLayoutPlan {
    plan.layout
        .expr_sequences
        .get(&LuaSyntaxId::from_node(syntax))
        .copied()
        .unwrap_or_default()
}

fn token_or_kind_doc(token: Option<&LuaSyntaxToken>, fallback_kind: LuaTokenKind) -> DocIR {
    token
        .map(|token| ir::source_token(token.clone()))
        .unwrap_or_else(|| ir::syntax_token(fallback_kind))
}

fn paren_tokens(node: &LuaSyntaxNode) -> (Option<LuaSyntaxToken>, Option<LuaSyntaxToken>) {
    (
        first_direct_token(node, LuaTokenKind::TkLeftParen),
        last_direct_token(node, LuaTokenKind::TkRightParen),
    )
}

fn brace_tokens(node: &LuaSyntaxNode) -> (Option<LuaSyntaxToken>, Option<LuaSyntaxToken>) {
    (
        first_direct_token(node, LuaTokenKind::TkLeftBrace),
        last_direct_token(node, LuaTokenKind::TkRightBrace),
    )
}

fn first_direct_token(node: &LuaSyntaxNode, kind: LuaTokenKind) -> Option<LuaSyntaxToken> {
    node.children_with_tokens().find_map(|element| {
        let token = element.into_token()?;
        (token.kind().to_token() == kind).then_some(token)
    })
}

fn last_direct_token(node: &LuaSyntaxNode, kind: LuaTokenKind) -> Option<LuaSyntaxToken> {
    let mut result = None;
    for element in node.children_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };
        if token.kind().to_token() == kind {
            result = Some(token);
        }
    }
    result
}

fn token_left_spacing_docs(plan: &FormatPlan, token: Option<&LuaSyntaxToken>) -> Vec<DocIR> {
    let Some(token) = token else {
        return Vec::new();
    };
    spacing_docs_from_expected(plan.spacing.left_expected(LuaSyntaxId::from_token(token)))
}

fn token_right_spacing_docs(plan: &FormatPlan, token: Option<&LuaSyntaxToken>) -> Vec<DocIR> {
    let Some(token) = token else {
        return Vec::new();
    };
    spacing_docs_from_expected(plan.spacing.right_expected(LuaSyntaxId::from_token(token)))
}

fn token_gap_spacing_docs(
    plan: &FormatPlan,
    previous: Option<&LuaSyntaxToken>,
    current: Option<&LuaSyntaxToken>,
) -> Vec<DocIR> {
    spacing_docs_from_expected(plan.spacing.gap_expected(previous, current))
}

fn spacing_docs_from_expected(expected: Option<&TokenSpacingExpected>) -> Vec<DocIR> {
    match expected {
        Some(TokenSpacingExpected::Space(count)) | Some(TokenSpacingExpected::MaxSpace(count)) => {
            (0..*count).map(|_| ir::space()).collect()
        }
        None => Vec::new(),
    }
}

fn grouped_padding_from_pair(
    plan: &FormatPlan,
    open: Option<&LuaSyntaxToken>,
    close: Option<&LuaSyntaxToken>,
) -> DocIR {
    let has_inner_space = !token_right_spacing_docs(plan, open).is_empty()
        || !token_left_spacing_docs(plan, close).is_empty();
    if has_inner_space {
        ir::soft_line()
    } else {
        ir::soft_line_or_empty()
    }
}

fn comma_token_docs(token: Option<&LuaSyntaxToken>) -> Vec<DocIR> {
    vec![token_or_kind_doc(token, LuaTokenKind::TkComma)]
}

fn comma_flat_separator(plan: &FormatPlan, token: Option<&LuaSyntaxToken>) -> Vec<DocIR> {
    let mut docs = comma_token_docs(token);
    docs.extend(token_right_spacing_docs(plan, token));
    docs
}

fn comma_fill_separator(token: Option<&LuaSyntaxToken>) -> Vec<DocIR> {
    let mut docs = comma_token_docs(token);
    docs.push(ir::soft_line());
    docs
}

fn comma_break_separator(token: Option<&LuaSyntaxToken>) -> Vec<DocIR> {
    let mut docs = comma_token_docs(token);
    docs.push(ir::hard_line());
    docs
}

fn trailing_comment_prefix(ctx: &FormatContext) -> Vec<DocIR> {
    trailing_comment_prefix_for_width(ctx, 0, None)
}

fn trailing_comment_prefix_for_width(
    ctx: &FormatContext,
    content_width: usize,
    aligned_content_width: Option<usize>,
) -> Vec<DocIR> {
    let aligned_content_width = aligned_content_width.unwrap_or(content_width);
    let natural_padding = aligned_content_width.saturating_sub(content_width)
        + ctx.config.comments.line_comment_min_spaces_before.max(1);
    let padding = if ctx.config.comments.line_comment_min_column == 0 {
        natural_padding
    } else {
        natural_padding.max(
            ctx.config
                .comments
                .line_comment_min_column
                .saturating_sub(content_width),
        )
    };
    (0..padding).map(|_| ir::space()).collect()
}

fn aligned_trailing_comment_widths<I>(allow_alignment: bool, entries: I) -> Vec<Option<usize>>
where
    I: IntoIterator<Item = (Vec<DocIR>, bool)>,
{
    let entries: Vec<_> = entries.into_iter().collect();
    if !allow_alignment {
        return entries.into_iter().map(|_| None).collect();
    }

    let max_width = entries
        .iter()
        .filter(|(_, has_comment)| *has_comment)
        .map(|(docs, _)| ir::ir_flat_width(docs))
        .max();

    entries
        .into_iter()
        .map(|(_, has_comment)| has_comment.then_some(max_width.unwrap_or(0)))
        .collect()
}

fn aligned_trailing_comment_widths_for_widths<I>(
    allow_alignment: bool,
    entries: I,
) -> Vec<Option<usize>>
where
    I: IntoIterator<Item = (usize, bool)>,
{
    let entries: Vec<_> = entries.into_iter().collect();
    if !allow_alignment {
        return entries.into_iter().map(|_| None).collect();
    }

    let max_width = entries
        .iter()
        .filter(|(_, has_comment)| *has_comment)
        .map(|(width, _)| *width)
        .max();

    entries
        .into_iter()
        .map(|(_, has_comment)| has_comment.then_some(max_width.unwrap_or(0)))
        .collect()
}

fn extract_trailing_comment(
    ctx: &FormatContext,
    plan: &FormatPlan,
    node: &LuaSyntaxNode,
) -> Option<(Vec<DocIR>, TextRange)> {
    for child in node.children() {
        if child.kind() != LuaKind::Syntax(LuaSyntaxKind::Comment) {
            continue;
        }

        let comment = LuaComment::cast(child.clone())?;
        if !has_inline_non_trivia_before(comment.syntax())
            || has_inline_non_trivia_after(comment.syntax())
        {
            continue;
        }
        if child.text().contains_char('\n') {
            return None;
        }

        return Some((
            render::render_comment_with_spacing(ctx, &comment, plan),
            child.text_range(),
        ));
    }

    let mut next = node.next_sibling_or_token();
    for _ in 0..4 {
        let sibling = next.as_ref()?;
        match sibling.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace)
            | LuaKind::Token(LuaTokenKind::TkSemicolon)
            | LuaKind::Token(LuaTokenKind::TkComma) => {}
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                let comment_node = sibling.as_node()?;
                if comment_node.text().contains_char('\n') {
                    return None;
                }
                let comment = LuaComment::cast(comment_node.clone())?;
                return Some((
                    render::render_comment_with_spacing(ctx, &comment, plan),
                    comment_node.text_range(),
                ));
            }
            _ => return None,
        }
        next = sibling.next_sibling_or_token();
    }

    None
}

fn extract_trailing_comment_text(node: &LuaSyntaxNode) -> Option<(Vec<DocIR>, TextRange)> {
    let mut next = node.next_sibling_or_token();
    for _ in 0..4 {
        let sibling = next.as_ref()?;
        match sibling.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace)
            | LuaKind::Token(LuaTokenKind::TkSemicolon)
            | LuaKind::Token(LuaTokenKind::TkComma) => {}
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                let comment_node = sibling.as_node()?;
                if comment_node.text().contains_char('\n') {
                    return None;
                }
                let text = trim_end_owned(comment_node.text().to_string());
                return Some((vec![ir::text(text)], comment_node.text_range()));
            }
            _ => return None,
        }
        next = sibling.next_sibling_or_token();
    }

    None
}

fn trim_end_owned(mut text: String) -> String {
    while matches!(text.chars().last(), Some(' ' | '\t' | '\r' | '\n')) {
        text.pop();
    }
    text
}

fn has_inline_non_trivia_before(node: &LuaSyntaxNode) -> bool {
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

fn has_inline_non_trivia_after(node: &LuaSyntaxNode) -> bool {
    let Some(last_token) = node.last_token() else {
        return false;
    };
    let mut next = last_token.next_token();
    while let Some(token) = next {
        match token.kind().to_token() {
            LuaTokenKind::TkWhitespace => next = token.next_token(),
            LuaTokenKind::TkEndOfLine => return false,
            _ => return true,
        }
    }
    false
}
