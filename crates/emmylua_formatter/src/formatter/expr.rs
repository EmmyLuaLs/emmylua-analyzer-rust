use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaAstToken, LuaBinaryExpr, LuaCallArgList, LuaCallExpr,
    LuaClosureExpr, LuaComment, LuaExpr, LuaIndexExpr, LuaIndexKey, LuaKind, LuaLiteralExpr,
    LuaLiteralToken, LuaNameExpr, LuaParamList, LuaParenExpr, LuaSingleArgExpr, LuaSyntaxId,
    LuaSyntaxKind, LuaSyntaxNode, LuaSyntaxToken, LuaTableExpr, LuaTableField, LuaTokenKind,
    LuaUnaryExpr, UnaryOperator,
};
use rowan::TextRange;

use crate::config::{ExpandStrategy, QuoteStyle, SingleArgCallParens, TrailingComma};
use crate::ir::{self, AlignEntry, DocIR};

use super::FormatContext;
use super::model::{ExprSequenceLayoutPlan, RootFormatPlan, TokenSpacingExpected};
use super::sequence::{
    DelimitedSequenceLayout, SequenceLayoutCandidates, SequenceLayoutPolicy,
    choose_sequence_layout, format_delimited_sequence,
};
use super::spacing::{SpaceRule, space_around_binary_op};
use super::trivia::{
    has_non_trivia_before_on_same_line_tokenwise, node_has_direct_comment_child,
    source_line_prefix_width, trailing_gap_requests_alignment,
};

pub fn format_expr(ctx: &FormatContext, plan: &RootFormatPlan, expr: &LuaExpr) -> Vec<DocIR> {
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
    }
}

fn format_name_expr(expr: &LuaNameExpr) -> Vec<DocIR> {
    expr.get_name_token()
        .map(|token| vec![ir::source_token(token.syntax().clone())])
        .unwrap_or_default()
}

type EqSplitDocs = (Vec<DocIR>, Vec<DocIR>);

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

fn format_binary_expr(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    expr: &LuaBinaryExpr,
) -> Vec<DocIR> {
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

    if crate::ir::ir_has_forced_line_break(&left_docs)
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
    if crate::ir::ir_has_forced_line_break(right_docs) {
        return false;
    }

    match op {
        BinaryOperator::OpAnd | BinaryOperator::OpOr => {
            crate::ir::ir_flat_width(right_docs) <= 24
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
            crate::ir::ir_flat_width(right_docs) <= 16
                && matches!(
                    right,
                    LuaExpr::LiteralExpr(_) | LuaExpr::NameExpr(_) | LuaExpr::ParenExpr(_)
                )
        }
        _ => false,
    }
}

fn format_unary_expr(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    expr: &LuaUnaryExpr,
) -> Vec<DocIR> {
    let mut docs = Vec::new();
    if let Some(op_token) = expr.get_op_token() {
        docs.push(ir::source_token(op_token.syntax().clone()));
        if matches!(op_token.get_op(), UnaryOperator::OpNot) {
            docs.push(ir::space());
        }
    }
    if let Some(inner) = expr.get_expr() {
        docs.extend(format_expr(ctx, plan, &inner));
    }
    docs
}

fn format_paren_expr(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    expr: &LuaParenExpr,
) -> Vec<DocIR> {
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

fn format_index_expr(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    expr: &LuaIndexExpr,
) -> Vec<DocIR> {
    if node_has_direct_comment_child(expr.syntax()) {
        return vec![ir::source_node_trimmed(expr.syntax().clone())];
    }

    let mut docs = expr
        .get_prefix_expr()
        .map(|prefix| format_expr(ctx, plan, &prefix))
        .unwrap_or_default();
    docs.extend(format_index_access_ir(ctx, plan, expr));
    docs
}

pub fn format_param_list_ir(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
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
    _plan: &RootFormatPlan,
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
                    crate::ir::ir_flat_width(&line_content),
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

fn format_call_expr(ctx: &FormatContext, plan: &RootFormatPlan, expr: &LuaCallExpr) -> Vec<DocIR> {
    if node_has_direct_comment_child(expr.syntax()) {
        return vec![ir::source_node_trimmed(expr.syntax().clone())];
    }

    let mut docs = expr
        .get_prefix_expr()
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
    docs.extend(format_call_arg_list(ctx, plan, &args_list));
    docs
}

fn format_call_arg_list(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    args_list: &LuaCallArgList,
) -> Vec<DocIR> {
    let args: Vec<_> = args_list.get_args().collect();
    let collected = collect_call_arg_entries(ctx, plan, args_list);

    if collected.has_comments {
        return format_call_arg_list_with_comments(ctx, plan, args_list, collected);
    }

    let preserve_multiline_args = args_list.syntax().text().contains_char('\n');
    let attach_first_arg = preserve_multiline_args && should_attach_first_call_arg(&args);
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
    let (open, close) = paren_tokens(args_list.syntax());
    let comma = first_direct_token(args_list.syntax(), LuaTokenKind::TkComma);
    let layout_plan = expr_sequence_layout_plan(plan, args_list.syntax());

    if attach_first_arg {
        return format_call_args_with_attached_first_arg(
            arg_docs,
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

fn should_attach_first_call_arg(args: &[LuaExpr]) -> bool {
    matches!(
        args.first(),
        Some(LuaExpr::TableExpr(_) | LuaExpr::ClosureExpr(_))
    )
}

fn format_call_arg_value_ir(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    arg: &LuaExpr,
    attach_first_arg: bool,
    preserve_multiline_args: bool,
    index: usize,
) -> Vec<DocIR> {
    if preserve_multiline_args && arg.syntax().text().contains_char('\n') {
        if let LuaExpr::TableExpr(table) = arg
            && attach_first_arg
            && index == 0
        {
            return format_multiline_table_expr(ctx, plan, table);
        }

        if attach_first_arg && index == 0 {
            return format_expr(ctx, plan, arg);
        }
    }

    format_expr(ctx, plan, arg)
}

fn format_call_args_with_attached_first_arg(
    arg_docs: Vec<Vec<DocIR>>,
    close: DocIR,
    comma: Option<&LuaSyntaxToken>,
) -> Vec<DocIR> {
    if arg_docs.is_empty() {
        return vec![ir::syntax_token(LuaTokenKind::TkLeftParen), close];
    }

    let mut docs = vec![ir::syntax_token(LuaTokenKind::TkLeftParen)];
    docs.extend(arg_docs[0].clone());

    if arg_docs.len() == 1 {
        docs.push(close);
        return docs;
    }

    docs.extend(comma_token_docs(comma));
    let mut rest = Vec::new();
    for (index, item_docs) in arg_docs.iter().enumerate().skip(1) {
        rest.push(ir::hard_line());
        rest.extend(item_docs.clone());
        if index + 1 < arg_docs.len() {
            rest.extend(comma_token_docs(comma));
        }
    }
    docs.push(ir::indent(rest));
    docs.push(ir::hard_line());
    docs.push(close);
    vec![ir::group_break(docs)]
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
    plan: &RootFormatPlan,
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
                });
            } else {
                pending_comments.push(docs);
            }
            continue;
        }

        if let Some(arg) = LuaExpr::cast(child) {
            let trailing_comment = extract_trailing_comment(ctx, arg.syntax());
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
    plan: &RootFormatPlan,
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
                    crate::ir::ir_flat_width(&line_content),
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
    plan: &RootFormatPlan,
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

fn format_table_expr(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    expr: &LuaTableExpr,
) -> Vec<DocIR> {
    if expr.is_empty() {
        let (open, close) = brace_tokens(expr.syntax());
        return vec![
            token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftBrace),
            token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightBrace),
        ];
    }

    let collected = collect_table_entries(ctx, plan, expr);
    let has_assign_fields = collected
        .entries
        .iter()
        .any(|entry| entry.eq_split.is_some());
    let has_assign_alignment = ctx.config.align.table_field
        && has_assign_fields
        && table_group_requests_alignment(&collected.entries);

    if collected.has_comments {
        return format_table_with_comments(ctx, expr, collected);
    }

    let field_docs: Vec<Vec<DocIR>> = collected
        .entries
        .iter()
        .map(|entry| entry.doc.clone())
        .collect();
    let (open, close) = brace_tokens(expr.syntax());
    let comma = first_direct_token(expr.syntax(), LuaTokenKind::TkComma);
    let layout_plan = expr_sequence_layout_plan(plan, expr.syntax());

    if has_assign_alignment {
        let layout = DelimitedSequenceLayout {
            open: token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftBrace),
            close: token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightBrace),
            items: field_docs.clone(),
            strategy: if expr.is_empty() {
                ExpandStrategy::Never
            } else {
                ctx.config.layout.table_expand.clone()
            },
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

        return match ctx.config.layout.table_expand {
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
                let mut flat_layout = layout;
                flat_layout.strategy = ExpandStrategy::Never;
                let flat_docs = format_delimited_sequence(ctx, flat_layout);
                if crate::ir::ir_flat_width(&flat_docs) + source_line_prefix_width(expr.syntax())
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
        strategy: if expr.is_empty() {
            ExpandStrategy::Never
        } else {
            ctx.config.layout.table_expand.clone()
        },
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

    if has_assign_fields && matches!(ctx.config.layout.table_expand, ExpandStrategy::Auto) {
        let mut flat_layout = layout.clone();
        flat_layout.strategy = ExpandStrategy::Never;
        let flat_docs = format_delimited_sequence(ctx, flat_layout);
        if crate::ir::ir_flat_width(&flat_docs) + source_line_prefix_width(expr.syntax())
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

fn format_multiline_table_expr(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    expr: &LuaTableExpr,
) -> Vec<DocIR> {
    let collected = collect_table_entries(ctx, plan, expr);

    if collected.has_comments
        || (ctx.config.align.table_field && table_group_requests_alignment(&collected.entries))
    {
        return format_table_with_comments(ctx, expr, collected);
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
    comments_before_close: Vec<Vec<DocIR>>,
    has_comments: bool,
    consumed_comment_ranges: Vec<TextRange>,
}

struct TableEntry {
    leading_comments: Vec<Vec<DocIR>>,
    doc: Vec<DocIR>,
    eq_split: Option<EqSplitDocs>,
    align_hint: bool,
    comment_align_hint: bool,
    trailing_comment: Option<Vec<DocIR>>,
}

fn collect_table_entries(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    expr: &LuaTableExpr,
) -> CollectedTableEntries {
    let mut collected = CollectedTableEntries::default();
    let mut pending_comments: Vec<Vec<DocIR>> = Vec::new();
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
            let docs = vec![ir::source_node_trimmed(comment.syntax().clone())];
            collected.has_comments = true;
            if !seen_field {
                collected.comments_after_open.push(DelimitedComment {
                    docs,
                    same_line_after_open: has_non_trivia_before_on_same_line_tokenwise(
                        comment.syntax(),
                    ),
                });
            } else {
                pending_comments.push(docs);
            }
            continue;
        }

        if let Some(field) = LuaTableField::cast(child) {
            let trailing_comment = extract_trailing_comment(ctx, field.syntax());
            if trailing_comment.is_some() {
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
                leading_comments: std::mem::take(&mut pending_comments),
                doc: format_table_field_ir(ctx, plan, &field),
                eq_split: if ctx.config.align.table_field {
                    format_table_field_eq_split(ctx, plan, &field)
                } else {
                    None
                },
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

fn format_table_with_comments(
    ctx: &FormatContext,
    expr: &LuaTableExpr,
    collected: CollectedTableEntries,
) -> Vec<DocIR> {
    let (open, close) = brace_tokens(expr.syntax());
    let mut docs = vec![token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftBrace)];
    let trailing = trailing_comma_ir(ctx.config.trailing_table_comma());
    let should_align_eq = ctx.config.align.table_field
        && collected
            .entries
            .iter()
            .any(|entry| entry.eq_split.is_some())
        && table_group_requests_alignment(&collected.entries);

    if !collected.comments_after_open.is_empty() || !collected.entries.is_empty() {
        let mut inner = Vec::new();

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

        inner.extend(build_table_expanded_inner(
            ctx,
            &collected.entries,
            &trailing,
            should_align_eq,
            ctx.config.should_align_table_line_comments(),
        ));

        for comment_docs in collected.comments_before_close {
            inner.push(ir::hard_line());
            inner.extend(comment_docs);
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
    plan: &RootFormatPlan,
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
        docs.extend(format_table_field_value_ir(ctx, plan, &value));
    }

    docs
}

fn format_table_field_key_ir(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
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
    plan: &RootFormatPlan,
    value: &LuaExpr,
) -> Vec<DocIR> {
    if let LuaExpr::TableExpr(table) = value
        && value.syntax().text().contains_char('\n')
    {
        return format_table_expr(ctx, plan, table);
    }

    format_expr(ctx, plan, value)
}

fn format_table_field_eq_split(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
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
        after.extend(format_table_field_value_ir(ctx, plan, &value));
    }
    Some((before, after))
}

fn field_requests_alignment(field: &LuaTableField) -> bool {
    if !field.is_assign_field() {
        return false;
    }

    let Some(value) = field.get_value_expr() else {
        return false;
    };
    let Some(assign_token) = field.syntax().children_with_tokens().find_map(|element| {
        let token = element.into_token()?;
        (token.kind() == LuaTokenKind::TkAssign.into()).then_some(token)
    }) else {
        return false;
    };

    let field_start = field.syntax().text_range().start();
    let gap_start = usize::from(assign_token.text_range().end() - field_start);
    let gap_end = usize::from(value.syntax().text_range().start() - field_start);
    if gap_end <= gap_start {
        return false;
    }

    let text = field.syntax().text().to_string();
    let Some(gap) = text.get(gap_start..gap_end) else {
        return false;
    };

    !gap.contains(['\n', '\r']) && gap.chars().filter(|ch| matches!(ch, ' ' | '\t')).count() > 1
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

    if align_eq {
        let mut index = 0usize;
        while index < entries.len() {
            if entries[index].eq_split.is_some() {
                let group_start = index;
                let mut group_end = index + 1;
                while group_end < entries.len()
                    && entries[group_end].eq_split.is_some()
                    && entries[group_end].leading_comments.is_empty()
                {
                    group_end += 1;
                }

                if group_end - group_start >= 2
                    && table_group_requests_alignment(&entries[group_start..group_end])
                {
                    for comment_docs in &entries[group_start].leading_comments {
                        inner.push(ir::hard_line());
                        inner.extend(comment_docs.clone());
                    }
                    inner.push(ir::hard_line());

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
            );
            index += 1;
        }

        return inner;
    }

    for (index, entry) in entries.iter().enumerate() {
        push_table_entry_line(ctx, &mut inner, entry, index, last_field_idx, trailing);
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
) {
    inner.push(ir::hard_line());
    for comment_docs in &entry.leading_comments {
        inner.extend(comment_docs.clone());
        inner.push(ir::hard_line());
    }
    inner.extend(entry.doc.clone());
    if last_field_idx == Some(index) {
        inner.push(trailing.clone());
    } else {
        inner.push(ir::syntax_token(LuaTokenKind::TkComma));
    }
    if let Some(comment_docs) = &entry.trailing_comment {
        let mut suffix = trailing_comment_prefix(ctx);
        suffix.extend(comment_docs.clone());
        inner.push(ir::line_suffix(suffix));
    }
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
                    Some(crate::ir::ir_flat_width(&content))
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
                        crate::ir::ir_flat_width(&content),
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
    plan: &RootFormatPlan,
    expr: &LuaClosureExpr,
) -> Vec<DocIR> {
    let shell_plan = collect_closure_shell_plan(ctx, plan, expr);
    render_closure_shell(ctx, plan, expr, shell_plan)
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
    plan: &RootFormatPlan,
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
    root_plan: &RootFormatPlan,
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

    let block_lines = expr
        .get_block()
        .map(|block| {
            block
                .syntax()
                .text()
                .to_string()
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if !body_comment_lines.is_empty() || !block_lines.is_empty() {
        let mut block_docs = vec![ir::hard_line()];
        for comment_docs in body_comment_lines {
            block_docs.extend(comment_docs);
            block_docs.push(ir::hard_line());
        }
        for (index, line) in block_lines.into_iter().enumerate() {
            if index > 0 {
                block_docs.push(ir::hard_line());
            }
            block_docs.push(ir::text(line));
        }
        docs.push(ir::indent(block_docs));
        docs.push(ir::hard_line());
    } else if saw_same_line_body_comment {
        docs.push(ir::hard_line());
    }

    if !saw_same_line_body_comment && expr.get_block().is_none() {
        docs.push(ir::space());
    }

    docs.push(ir::syntax_token(LuaTokenKind::TkEnd));
    docs
}

fn try_format_flat_binary_chain(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
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
    plan: &RootFormatPlan,
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
    plan: &RootFormatPlan,
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
    plan: &RootFormatPlan,
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
    plan: &RootFormatPlan,
    expr: &LuaIndexExpr,
) -> Vec<DocIR> {
    let mut docs = Vec::new();
    if let Some(index_token) = expr.get_index_token() {
        if index_token.is_dot() {
            docs.push(ir::syntax_token(LuaTokenKind::TkDot));
            if let Some(name_token) = expr.get_index_name_token() {
                docs.push(ir::source_token(name_token));
            }
        } else if index_token.is_colon() {
            docs.push(ir::syntax_token(LuaTokenKind::TkColon));
            if let Some(name_token) = expr.get_index_name_token() {
                docs.push(ir::source_token(name_token));
            }
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

fn trailing_comma_ir(policy: TrailingComma) -> DocIR {
    match policy {
        TrailingComma::Never => ir::list(vec![]),
        TrailingComma::Multiline => {
            ir::if_break(ir::syntax_token(LuaTokenKind::TkComma), ir::list(vec![]))
        }
        TrailingComma::Always => ir::syntax_token(LuaTokenKind::TkComma),
    }
}

fn expr_sequence_layout_plan(
    plan: &RootFormatPlan,
    syntax: &LuaSyntaxNode,
) -> ExprSequenceLayoutPlan {
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

fn token_left_spacing_docs(plan: &RootFormatPlan, token: Option<&LuaSyntaxToken>) -> Vec<DocIR> {
    let Some(token) = token else {
        return Vec::new();
    };
    spacing_docs_from_expected(plan.spacing.left_expected(LuaSyntaxId::from_token(token)))
}

fn token_right_spacing_docs(plan: &RootFormatPlan, token: Option<&LuaSyntaxToken>) -> Vec<DocIR> {
    let Some(token) = token else {
        return Vec::new();
    };
    spacing_docs_from_expected(plan.spacing.right_expected(LuaSyntaxId::from_token(token)))
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
    plan: &RootFormatPlan,
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

fn comma_flat_separator(plan: &RootFormatPlan, token: Option<&LuaSyntaxToken>) -> Vec<DocIR> {
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
        .map(|(docs, _)| crate::ir::ir_flat_width(docs))
        .max();

    entries
        .into_iter()
        .map(|(_, has_comment)| has_comment.then_some(max_width.unwrap_or(0)))
        .collect()
}

fn extract_trailing_comment(
    ctx: &FormatContext,
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

        let text = trim_end_owned(child.text().to_string());
        return Some((
            normalize_single_line_comment_text(ctx, &text),
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
                let text = trim_end_owned(comment_node.text().to_string());
                return Some((
                    normalize_single_line_comment_text(ctx, &text),
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

fn normalize_single_line_comment_text(ctx: &FormatContext, text: &str) -> Vec<DocIR> {
    if text.starts_with("---") || !text.starts_with("--") {
        return vec![ir::text(text.to_string())];
    }

    let body = text[2..].trim_start();
    let prefix = if ctx.config.comments.space_after_comment_dash {
        if body.is_empty() {
            "--".to_string()
        } else {
            "-- ".to_string()
        }
    } else {
        "--".to_string()
    };

    vec![ir::text(format!("{prefix}{body}"))]
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
