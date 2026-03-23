use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaAstToken, LuaBinaryExpr, LuaCallExpr, LuaClosureExpr,
    LuaComment, LuaExpr, LuaIndexExpr, LuaIndexKey, LuaKind, LuaLiteralExpr, LuaLiteralToken,
    LuaNameExpr, LuaParenExpr, LuaSingleArgExpr, LuaStringToken, LuaSyntaxKind, LuaSyntaxNode,
    LuaTableExpr, LuaTableField, LuaTokenKind, LuaUnaryExpr, UnaryOperator,
};
use rowan::TextRange;

use crate::config::{ExpandStrategy, QuoteStyle, SingleArgCallParens};
use crate::ir::{self, AlignEntry, DocIR, EqSplit, ir_flat_width, ir_has_forced_line_break};

use super::FormatContext;
use super::comment::{extract_trailing_comment, format_comment, trailing_comment_prefix};
use super::sequence::{
    DelimitedSequenceLayout, SequenceEntry, SequenceLayoutCandidates, SequenceLayoutPolicy,
    build_delimited_sequence_break_candidate, build_delimited_sequence_default_break_candidate,
    build_delimited_sequence_flat_candidate, choose_sequence_layout, format_delimited_sequence,
    render_sequence, sequence_ends_with_comment, sequence_has_comment,
    sequence_starts_with_comment,
};
use super::spacing::{SpaceRule, space_around_assign, space_around_binary_op};
use super::tokens::{comma_soft_line_sep, comma_space_sep, tok};
use super::trivia::{
    node_has_direct_comment_child, node_has_direct_same_line_inline_comment,
    source_line_prefix_width, trailing_gap_requests_alignment,
};

struct BinaryExprSplit {
    lhs_entries: Vec<SequenceEntry>,
    op_text: Option<DocIR>,
    rhs_entries: Vec<SequenceEntry>,
}

enum IndexStandaloneSuffix {
    Dot(Vec<SequenceEntry>),
    Colon(Vec<SequenceEntry>),
    Bracket(Vec<SequenceEntry>),
}

struct IndexStandaloneLayout {
    before_suffix_comments: Vec<SequenceEntry>,
    suffix: Option<IndexStandaloneSuffix>,
}

pub fn format_expr(ctx: &FormatContext, expr: &LuaExpr) -> Vec<DocIR> {
    match expr {
        LuaExpr::NameExpr(e) => format_name_expr(ctx, e),
        LuaExpr::LiteralExpr(e) => format_literal_expr(ctx, e),
        LuaExpr::BinaryExpr(e) => format_binary_expr(ctx, e),
        LuaExpr::UnaryExpr(e) => format_unary_expr(ctx, e),
        LuaExpr::CallExpr(e) => format_call_expr(ctx, e),
        LuaExpr::IndexExpr(e) => format_index_expr(ctx, e),
        LuaExpr::TableExpr(e) => format_table_expr(ctx, e),
        LuaExpr::ClosureExpr(e) => format_closure_expr(ctx, e),
        LuaExpr::ParenExpr(e) => format_paren_expr(ctx, e),
    }
}

fn format_table_expr_with_forced_expand(
    ctx: &FormatContext,
    expr: &LuaTableExpr,
    force_expand_from_context: bool,
) -> Vec<DocIR> {
    if expr.is_empty() {
        return vec![
            tok(LuaTokenKind::TkLeftBrace),
            tok(LuaTokenKind::TkRightBrace),
        ];
    }

    let mut entries: Vec<TableEntry> = Vec::new();
    let mut consumed_comment_ranges: Vec<TextRange> = Vec::new();
    let mut has_standalone_comments = false;

    for child in expr.syntax().children() {
        if let Some(field) = LuaTableField::cast(child.clone()) {
            let fdoc = format_table_field_ir(ctx, &field);
            let force_expand = field
                .get_value_expr()
                .as_ref()
                .is_some_and(should_preserve_multiline_table_field_value);
            let eq_split = if ctx.config.align.table_field {
                format_table_field_eq_split(ctx, &field)
            } else {
                None
            };
            let align_hint = field_requests_alignment(&field);
            let (trailing_comment, comment_align_hint) =
                if let Some((docs, range)) = extract_trailing_comment(ctx.config, field.syntax()) {
                    consumed_comment_ranges.push(range);
                    (
                        Some(docs),
                        trailing_comment_requests_alignment(
                            field.syntax(),
                            range,
                            ctx.config.comments.line_comment_min_spaces_before.max(1),
                        ),
                    )
                } else {
                    (None, false)
                };
            entries.push(TableEntry::Field {
                doc: fdoc,
                eq_split,
                force_expand,
                align_hint,
                comment_align_hint,
                trailing_comment,
            });
        } else if child.kind() == LuaKind::Syntax(LuaSyntaxKind::Comment) {
            if consumed_comment_ranges
                .iter()
                .any(|r| *r == child.text_range())
            {
                continue;
            }
            let comment = LuaComment::cast(child).unwrap();
            entries.push(TableEntry::StandaloneComment(format_comment(
                ctx.config, &comment,
            )));
            has_standalone_comments = true;
        }
    }

    let trailing = format_trailing_comma_ir(ctx.config.trailing_table_comma());

    let space_inside = if ctx.config.spacing.space_inside_braces {
        ir::soft_line()
    } else {
        ir::soft_line_or_empty()
    };

    let has_trailing_comments = entries.iter().any(|e| {
        matches!(
            e,
            TableEntry::Field {
                trailing_comment: Some(_),
                ..
            }
        )
    });

    let has_multiline_field_docs = entries.iter().any(|entry| match entry {
        TableEntry::Field {
            doc, force_expand, ..
        } => *force_expand || ir_has_forced_line_break(doc),
        TableEntry::StandaloneComment(_) => false,
    });

    let force_expand = force_expand_from_context
        || has_standalone_comments
        || has_trailing_comments
        || has_multiline_field_docs;

    match ctx.config.layout.table_expand {
        ExpandStrategy::Always => format_table_multiline_candidates(
            ctx,
            entries,
            trailing,
            ctx.config.align.table_field,
            true,
            has_standalone_comments,
            source_line_prefix_width(expr.syntax()),
        ),
        ExpandStrategy::Never if !force_expand => {
            format_delimited_sequence(DelimitedSequenceLayout {
                open: tok(LuaTokenKind::TkLeftBrace),
                close: tok(LuaTokenKind::TkRightBrace),
                items: entries
                    .into_iter()
                    .filter_map(|e| match e {
                        TableEntry::Field { doc, .. } => Some(doc),
                        TableEntry::StandaloneComment(_) => None,
                    })
                    .collect(),
                strategy: ExpandStrategy::Never,
                preserve_multiline: false,
                flat_separator: comma_space_sep(),
                fill_separator: comma_soft_line_sep(),
                break_separator: comma_soft_line_sep(),
                flat_open_padding: if ctx.config.spacing.space_inside_braces {
                    vec![ir::space()]
                } else {
                    vec![]
                },
                flat_close_padding: if ctx.config.spacing.space_inside_braces {
                    vec![ir::space()]
                } else {
                    vec![]
                },
                grouped_padding: space_inside.clone(),
                flat_trailing: vec![],
                grouped_trailing: trailing.clone(),
                custom_break_contents: None,
                prefer_custom_break_in_auto: false,
            })
        }
        ExpandStrategy::Never => format_table_multiline_candidates(
            ctx,
            entries,
            trailing,
            ctx.config.align.table_field,
            true,
            has_standalone_comments,
            source_line_prefix_width(expr.syntax()),
        ),
        ExpandStrategy::Auto if force_expand => format_table_multiline_candidates(
            ctx,
            entries,
            trailing,
            ctx.config.align.table_field,
            true,
            has_standalone_comments,
            source_line_prefix_width(expr.syntax()),
        ),
        ExpandStrategy::Auto => {
            let flat_field_docs: Vec<Vec<DocIR>> = entries
                .iter()
                .filter_map(|e| match e {
                    TableEntry::Field { doc, .. } => Some(doc.clone()),
                    TableEntry::StandaloneComment(_) => None,
                })
                .collect();
            let layout = DelimitedSequenceLayout {
                open: tok(LuaTokenKind::TkLeftBrace),
                close: tok(LuaTokenKind::TkRightBrace),
                items: flat_field_docs,
                strategy: ExpandStrategy::Auto,
                preserve_multiline: false,
                flat_separator: comma_space_sep(),
                fill_separator: comma_soft_line_sep(),
                break_separator: comma_soft_line_sep(),
                flat_open_padding: if ctx.config.spacing.space_inside_braces {
                    vec![ir::space()]
                } else {
                    vec![]
                },
                flat_close_padding: if ctx.config.spacing.space_inside_braces {
                    vec![ir::space()]
                } else {
                    vec![]
                },
                grouped_padding: space_inside,
                flat_trailing: vec![],
                grouped_trailing: trailing.clone(),
                custom_break_contents: None,
                prefer_custom_break_in_auto: false,
            };
            let has_assign_fields = entries.iter().any(|e| {
                matches!(
                    e,
                    TableEntry::Field {
                        eq_split: Some(_),
                        ..
                    }
                )
            });
            let has_assign_alignment = ctx.config.align.table_field && has_assign_fields;

            if has_assign_fields {
                let aligned = has_assign_alignment.then(|| {
                    build_delimited_sequence_break_candidate(
                        layout.open.clone(),
                        layout.close.clone(),
                        build_table_expanded_inner(
                            ctx,
                            &entries,
                            &trailing,
                            true,
                            ctx.config.should_align_table_line_comments(),
                        ),
                    )
                });

                choose_sequence_layout(
                    ctx,
                    SequenceLayoutCandidates {
                        flat: Some(build_delimited_sequence_flat_candidate(&layout)),
                        aligned,
                        one_per_line: Some(build_delimited_sequence_default_break_candidate(
                            &layout,
                        )),
                        ..Default::default()
                    },
                    SequenceLayoutPolicy {
                        allow_alignment: has_assign_alignment,
                        allow_fill: false,
                        allow_preserve: false,
                        prefer_preserve_multiline: false,
                        force_break_on_standalone_comments: false,
                        prefer_balanced_break_lines: false,
                        first_line_prefix_width: source_line_prefix_width(expr.syntax()),
                    },
                )
            } else {
                format_delimited_sequence(layout)
            }
        }
    }
}

fn format_name_expr(_ctx: &FormatContext, expr: &LuaNameExpr) -> Vec<DocIR> {
    if let Some(token) = expr.get_name_token() {
        vec![ir::source_token(token.syntax().clone())]
    } else {
        vec![]
    }
}

fn format_literal_expr(ctx: &FormatContext, expr: &LuaLiteralExpr) -> Vec<DocIR> {
    if let Some(LuaLiteralToken::String(token)) = expr.get_literal() {
        return format_string_literal(ctx, &token);
    }

    vec![ir::source_node(expr.syntax().clone())]
}

fn format_string_literal(ctx: &FormatContext, token: &LuaStringToken) -> Vec<DocIR> {
    let text = token.syntax().text().to_string();
    let Some(original_quote) = text.chars().next() else {
        return vec![ir::source_token(token.syntax().clone())];
    };

    if token.syntax().kind() == LuaTokenKind::TkLongString.into()
        || !matches!(original_quote, '\'' | '"')
    {
        return vec![ir::source_token(token.syntax().clone())];
    }

    let preferred_quote = match ctx.config.output.quote_style {
        QuoteStyle::Preserve => return vec![ir::source_token(token.syntax().clone())],
        QuoteStyle::Double => '"',
        QuoteStyle::Single => '\'',
    };

    if preferred_quote == original_quote {
        return vec![ir::source_token(token.syntax().clone())];
    }

    let raw_body = &text[1..text.len() - 1];
    if raw_short_string_contains_unescaped_quote(raw_body, preferred_quote) {
        return vec![ir::source_token(token.syntax().clone())];
    }

    vec![ir::text(rewrite_short_string_quotes(
        raw_body,
        original_quote,
        preferred_quote,
    ))]
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

/// 二元表达式: a + b, a and b, ...
///
/// 当表达式太长时，在操作符前断行并缩进：
/// ```text
/// very_long_left
///     + right
/// ```
fn format_binary_expr(ctx: &FormatContext, expr: &LuaBinaryExpr) -> Vec<DocIR> {
    if node_has_direct_comment_child(expr.syntax()) {
        return format_binary_expr_with_standalone_comments(ctx, expr);
    }

    if let Some(flattened) = try_format_flat_binary_chain(ctx, expr) {
        return flattened;
    }

    if let Some((left, right)) = expr.get_exprs() {
        let left_docs = format_expr(ctx, &left);
        let right_docs = format_expr(ctx, &right);

        if let Some(op_token) = expr.get_op_token() {
            let op = op_token.get_op();
            let space_rule = space_around_binary_op(op, ctx.config);
            let space_ir = space_rule.to_ir();
            // Safety: when the left operand text ends with '.' and the operator
            // is '..', we must force a space before the operator to avoid
            // ambiguity (e.g. `1. ..` must not become `1...`).
            // Only the before-space is forced; the after-space follows the
            // configured space_rule.
            let mut force_space_before = false;
            if op == BinaryOperator::OpConcat
                && space_rule == SpaceRule::NoSpace
                && let Some(last_token) = left.syntax().last_token()
                && last_token.kind() == LuaTokenKind::TkFloat.into()
            {
                force_space_before = true;
            }

            if ir_has_forced_line_break(&left_docs)
                && should_attach_short_binary_tail(op, &right, &right_docs)
            {
                let mut docs = left_docs;
                if force_space_before {
                    docs.push(ir::space());
                } else {
                    docs.push(space_rule.to_ir());
                }
                docs.push(ir::source_token(op_token.syntax().clone()));
                docs.push(space_ir);
                docs.extend(right_docs);
                return docs;
            }

            // Before-operator break: soft_line (→space when flat) if space,
            // soft_line_or_empty (→"" when flat) if no space
            let break_ir =
                continuation_break_ir(force_space_before || space_rule != SpaceRule::NoSpace);

            return vec![ir::group(vec![
                ir::list(left_docs),
                ir::indent(vec![
                    break_ir,
                    ir::source_token(op_token.syntax().clone()),
                    space_ir,
                    ir::list(right_docs),
                ]),
            ])];
        }
    }

    vec![]
}

fn should_attach_short_binary_tail(
    op: BinaryOperator,
    right: &LuaExpr,
    right_docs: &[DocIR],
) -> bool {
    if ir_has_forced_line_break(right_docs) {
        return false;
    }

    match op {
        BinaryOperator::OpEq
        | BinaryOperator::OpNe
        | BinaryOperator::OpLt
        | BinaryOperator::OpLe
        | BinaryOperator::OpGt
        | BinaryOperator::OpGe => {
            ir_flat_width(right_docs) <= 16
                && matches!(
                    right,
                    LuaExpr::LiteralExpr(_) | LuaExpr::NameExpr(_) | LuaExpr::ParenExpr(_)
                )
        }
        BinaryOperator::OpAnd | BinaryOperator::OpOr => {
            ir_flat_width(right_docs) <= 24
                && matches!(
                    right,
                    LuaExpr::LiteralExpr(_)
                        | LuaExpr::NameExpr(_)
                        | LuaExpr::ParenExpr(_)
                        | LuaExpr::IndexExpr(_)
                        | LuaExpr::CallExpr(_)
                )
        }
        _ => false,
    }
}

fn format_binary_expr_with_standalone_comments(
    ctx: &FormatContext,
    expr: &LuaBinaryExpr,
) -> Vec<DocIR> {
    let BinaryExprSplit {
        lhs_entries,
        op_text,
        rhs_entries,
    } = collect_binary_expr_entries(ctx, expr);
    let mut docs = Vec::new();

    render_sequence(&mut docs, &lhs_entries, false);

    let Some(op_text) = op_text else {
        return docs;
    };

    let op = expr.get_op_token().map(|token| token.get_op());
    let space_rule = op
        .map(|op| space_around_binary_op(op, ctx.config))
        .unwrap_or(SpaceRule::Space);
    let after_op_ir = space_rule.to_ir();

    let force_space_before = matches!(op, Some(BinaryOperator::OpConcat))
        && space_rule == SpaceRule::NoSpace
        && expr
            .get_left_expr()
            .as_ref()
            .is_some_and(expr_end_with_float);

    if sequence_has_comment(&lhs_entries) {
        if !sequence_ends_with_comment(&lhs_entries) {
            docs.push(ir::hard_line());
        }
    } else if force_space_before {
        docs.push(ir::space());
    } else {
        docs.push(space_rule.to_ir());
    }

    docs.push(op_text);

    if !rhs_entries.is_empty() {
        if sequence_starts_with_comment(&rhs_entries) {
            docs.push(ir::hard_line());
            render_sequence(&mut docs, &rhs_entries, true);
        } else {
            docs.push(after_op_ir);
            render_sequence(&mut docs, &rhs_entries, false);
        }
    }

    docs
}

fn collect_binary_expr_entries(ctx: &FormatContext, expr: &LuaBinaryExpr) -> BinaryExprSplit {
    let mut lhs_entries = Vec::new();
    let mut rhs_entries = Vec::new();
    let mut op_text = None;
    let op_range = expr.get_op_token().map(|token| token.syntax().text_range());
    let mut meet_op = false;

    for child in expr.syntax().children_with_tokens() {
        if let Some(token) = child.as_token()
            && Some(token.text_range()) == op_range
        {
            meet_op = true;
            op_text = Some(ir::source_token(token.clone()));
            continue;
        }

        match child.kind() {
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                if let Some(node) = child.as_node()
                    && let Some(comment) = LuaComment::cast(node.clone())
                {
                    let entry = SequenceEntry::Comment(format_comment(ctx.config, &comment));
                    if meet_op {
                        rhs_entries.push(entry);
                    } else {
                        lhs_entries.push(entry);
                    }
                }
            }
            _ => {
                if let Some(node) = child.as_node()
                    && let Some(inner_expr) = LuaExpr::cast(node.clone())
                {
                    let entry = SequenceEntry::Item(format_expr(ctx, &inner_expr));
                    if meet_op {
                        rhs_entries.push(entry);
                    } else {
                        lhs_entries.push(entry);
                    }
                }
            }
        }
    }

    BinaryExprSplit {
        lhs_entries,
        op_text,
        rhs_entries,
    }
}

fn try_format_flat_binary_chain(ctx: &FormatContext, expr: &LuaBinaryExpr) -> Option<Vec<DocIR>> {
    let op_token = expr.get_op_token()?;
    let op = op_token.get_op();
    let mut operands = Vec::new();
    collect_binary_chain_operands(&LuaExpr::BinaryExpr(expr.clone()), op, &mut operands);
    if operands.len() < 3 {
        return None;
    }

    let fill_parts = build_binary_chain_fill_parts(ctx, &operands, op_token.syntax().clone(), op);
    let packed = build_binary_chain_packed(ctx, &operands, op_token.syntax().clone(), op);
    let one_per_line =
        build_binary_chain_one_per_line(ctx, &operands, op_token.syntax().clone(), op);

    Some(choose_sequence_layout(
        ctx,
        SequenceLayoutCandidates {
            fill: Some(vec![ir::group(vec![ir::indent(vec![ir::fill(
                fill_parts,
            )])])]),
            packed: Some(packed),
            one_per_line: Some(one_per_line),
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

fn build_binary_chain_segment(
    ctx: &FormatContext,
    previous: &LuaExpr,
    operand: &LuaExpr,
    op_token: &emmylua_parser::LuaSyntaxToken,
    op: BinaryOperator,
) -> (bool, Vec<DocIR>) {
    let space_rule = space_around_binary_op(op, ctx.config);
    let space_ir = space_rule.to_ir();
    let force_space_before = op == BinaryOperator::OpConcat
        && space_rule == SpaceRule::NoSpace
        && expr_end_with_float(previous);
    let mut segment = Vec::new();
    segment.push(ir::source_token(op_token.clone()));
    segment.push(space_ir);
    segment.extend(format_expr(ctx, operand));

    (
        force_space_before || space_rule != SpaceRule::NoSpace,
        segment,
    )
}

fn build_binary_chain_fill_parts(
    ctx: &FormatContext,
    operands: &[LuaExpr],
    op_token: emmylua_parser::LuaSyntaxToken,
    op: BinaryOperator,
) -> Vec<DocIR> {
    let mut fill_parts = Vec::new();
    let mut previous = &operands[0];
    let first_operand = format_expr(ctx, &operands[0]);
    let mut first_chunk = first_operand;

    for (index, operand) in operands.iter().skip(1).enumerate() {
        let (space_before_segment, segment) =
            build_binary_chain_segment(ctx, previous, operand, &op_token, op);
        let break_ir = continuation_break_ir(space_before_segment);

        if index == 0 {
            if space_before_segment {
                first_chunk.push(ir::space());
            }
            first_chunk.extend(segment);
            fill_parts.push(ir::list(first_chunk.clone()));
        } else {
            fill_parts.push(break_ir);
            fill_parts.push(ir::list(segment));
        }

        previous = operand;
    }

    fill_parts
}

fn build_binary_chain_packed(
    ctx: &FormatContext,
    operands: &[LuaExpr],
    op_token: emmylua_parser::LuaSyntaxToken,
    op: BinaryOperator,
) -> Vec<DocIR> {
    let mut docs = Vec::new();
    let mut previous = &operands[0];
    let mut first_line = format_expr(ctx, &operands[0]);
    let mut tail_segments = Vec::new();

    for (index, operand) in operands.iter().skip(1).enumerate() {
        let (space_before_segment, segment) =
            build_binary_chain_segment(ctx, previous, operand, &op_token, op);
        if index == 0 {
            if space_before_segment {
                first_line.push(ir::space());
            }
            first_line.extend(segment);
        } else {
            tail_segments.push((space_before_segment, segment));
        }
        previous = operand;
    }

    docs.push(ir::list(first_line));

    for chunk in tail_segments.chunks(2) {
        let mut line = Vec::new();
        for (index, (space_before_segment, segment)) in chunk.iter().enumerate() {
            if index > 0 && *space_before_segment {
                line.push(ir::space());
            }
            line.extend(segment.clone());
        }

        docs.push(ir::hard_line());
        docs.push(ir::list(line));
    }

    vec![ir::group_break(vec![ir::indent(docs)])]
}

fn build_binary_chain_one_per_line(
    ctx: &FormatContext,
    operands: &[LuaExpr],
    op_token: emmylua_parser::LuaSyntaxToken,
    op: BinaryOperator,
) -> Vec<DocIR> {
    let mut docs = format_expr(ctx, &operands[0]);
    let mut previous = &operands[0];

    for operand in operands.iter().skip(1) {
        let (space_before_segment, segment) =
            build_binary_chain_segment(ctx, previous, operand, &op_token, op);
        let break_ir = continuation_break_ir(space_before_segment);
        docs.push(break_ir);
        docs.extend(segment);
        previous = operand;
    }

    vec![ir::group_break(vec![ir::indent(docs)])]
}

fn collect_binary_chain_operands(expr: &LuaExpr, op: BinaryOperator, operands: &mut Vec<LuaExpr>) {
    if let LuaExpr::BinaryExpr(binary) = expr
        && let Some(op_token) = binary.get_op_token()
        && op_token.get_op() == op
        && let Some((left, right)) = binary.get_exprs()
    {
        collect_binary_chain_operands(&left, op, operands);
        collect_binary_chain_operands(&right, op, operands);
        return;
    }

    operands.push(expr.clone());
}

fn expr_end_with_float(expr: &LuaExpr) -> bool {
    let Some(last_token) = expr.syntax().last_token() else {
        return false;
    };

    last_token.kind() == LuaTokenKind::TkFloat.into()
}

/// 一元表达式: -x, not x, #t, ~x
fn format_unary_expr(ctx: &FormatContext, expr: &LuaUnaryExpr) -> Vec<DocIR> {
    let mut docs = Vec::new();

    if let Some(op_token) = expr.get_op_token() {
        let op = op_token.get_op();
        docs.push(ir::source_token(op_token.syntax().clone()));

        // `not` 和 `-`（作为关键字的）后面需要空格，`#` 和 `~` 不需要
        match op {
            UnaryOperator::OpNot => docs.push(ir::space()),
            UnaryOperator::OpUnm | UnaryOperator::OpLen | UnaryOperator::OpBNot => {}
            UnaryOperator::OpNop => {}
        }
    }

    if let Some(inner) = expr.get_expr() {
        docs.extend(format_expr(ctx, &inner));
    }

    docs
}

/// 函数调用: f(a, b), obj:m(a), f "hello", f { ... }
fn format_call_expr(ctx: &FormatContext, expr: &LuaCallExpr) -> Vec<DocIR> {
    if should_preserve_raw_call_expr(expr) {
        return vec![ir::source_node_trimmed(expr.syntax().clone())];
    }

    // 尝试方法链格式化
    if let Some(chain) = try_format_chain(ctx, expr) {
        return chain;
    }

    let mut docs = Vec::new();

    // 前缀（函数名/表达式）
    if let Some(prefix) = expr.get_prefix_expr() {
        docs.extend(format_expr(ctx, &prefix));
    }

    // 参数列表
    docs.extend(format_call_args_ir(ctx, expr));

    docs
}

/// 索引表达式: t.x, t:m, t[k]
fn format_index_expr(ctx: &FormatContext, expr: &LuaIndexExpr) -> Vec<DocIR> {
    if node_has_direct_comment_child(expr.syntax()) {
        return format_index_expr_with_standalone_comments(ctx, expr);
    }

    let mut docs = Vec::new();

    // 前缀
    if let Some(prefix) = expr.get_prefix_expr() {
        docs.extend(format_expr(ctx, &prefix));
    }

    // 索引操作符和 key
    docs.extend(format_index_access_ir(ctx, expr));

    docs
}

fn format_index_expr_with_standalone_comments(
    ctx: &FormatContext,
    expr: &LuaIndexExpr,
) -> Vec<DocIR> {
    let mut docs = Vec::new();

    if let Some(prefix) = expr.get_prefix_expr() {
        docs.extend(format_expr(ctx, &prefix));
    }

    let IndexStandaloneLayout {
        before_suffix_comments,
        suffix,
    } = collect_index_standalone_layout(ctx, expr);

    if sequence_has_comment(&before_suffix_comments) {
        docs.push(ir::hard_line());
        render_sequence(&mut docs, &before_suffix_comments, true);
    }

    match suffix {
        Some(IndexStandaloneSuffix::Dot(entries)) => {
            docs.push(tok(LuaTokenKind::TkDot));
            if sequence_starts_with_comment(&entries) {
                docs.push(ir::hard_line());
                render_sequence(&mut docs, &entries, true);
            } else {
                render_sequence(&mut docs, &entries, false);
            }
        }
        Some(IndexStandaloneSuffix::Colon(entries)) => {
            docs.push(tok(LuaTokenKind::TkColon));
            if sequence_starts_with_comment(&entries) {
                docs.push(ir::hard_line());
                render_sequence(&mut docs, &entries, true);
            } else {
                render_sequence(&mut docs, &entries, false);
            }
        }
        Some(IndexStandaloneSuffix::Bracket(entries)) => {
            docs.push(tok(LuaTokenKind::TkLeftBracket));
            if sequence_has_comment(&entries) {
                docs.push(ir::hard_line());
                render_sequence(&mut docs, &entries, true);
                docs.push(ir::hard_line());
            } else {
                if ctx.config.spacing.space_inside_brackets {
                    docs.push(ir::space());
                }
                render_sequence(&mut docs, &entries, false);
                if ctx.config.spacing.space_inside_brackets {
                    docs.push(ir::space());
                }
            }
            docs.push(tok(LuaTokenKind::TkRightBracket));
        }
        None => docs.extend(format_index_access_ir(ctx, expr)),
    }

    docs
}

fn collect_index_standalone_layout(
    ctx: &FormatContext,
    expr: &LuaIndexExpr,
) -> IndexStandaloneLayout {
    let mut before_suffix_comments = Vec::new();
    let mut suffix_entries = Vec::new();
    let index_range = expr
        .get_index_token()
        .map(|token| token.syntax().text_range());
    let mut meet_prefix = false;
    let mut suffix_kind = None;

    for child in expr.syntax().children_with_tokens() {
        if let Some(token) = child.as_token()
            && Some(token.text_range()) == index_range
        {
            suffix_kind = Some(match token.kind().into() {
                LuaTokenKind::TkDot => LuaTokenKind::TkDot,
                LuaTokenKind::TkColon => LuaTokenKind::TkColon,
                LuaTokenKind::TkLeftBracket => LuaTokenKind::TkLeftBracket,
                _ => LuaTokenKind::None,
            });
            meet_prefix = true;
            continue;
        }

        match child.kind() {
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                if let Some(node) = child.as_node()
                    && let Some(comment) = LuaComment::cast(node.clone())
                {
                    let entry = SequenceEntry::Comment(format_comment(ctx.config, &comment));
                    if meet_prefix {
                        suffix_entries.push(entry);
                    } else {
                        before_suffix_comments.push(entry);
                    }
                }
            }
            _ => {
                if let Some(node) = child.as_node() {
                    if !meet_prefix && LuaExpr::cast(node.clone()).is_some() {
                        meet_prefix = false;
                        continue;
                    }

                    if meet_prefix && let Some(inner_expr) = LuaExpr::cast(node.clone()) {
                        suffix_entries.push(SequenceEntry::Item(format_expr(ctx, &inner_expr)));
                    }
                } else if let Some(token) = child.as_token()
                    && meet_prefix
                {
                    match token.kind().into() {
                        LuaTokenKind::TkName => suffix_entries
                            .push(SequenceEntry::Item(vec![ir::source_token(token.clone())])),
                        LuaTokenKind::TkRightBracket => {}
                        _ => {}
                    }
                }
            }
        }
    }

    let suffix = match suffix_kind {
        Some(LuaTokenKind::TkDot) => Some(IndexStandaloneSuffix::Dot(suffix_entries)),
        Some(LuaTokenKind::TkColon) => Some(IndexStandaloneSuffix::Colon(suffix_entries)),
        Some(LuaTokenKind::TkLeftBracket) => Some(IndexStandaloneSuffix::Bracket(suffix_entries)),
        _ => None,
    };

    IndexStandaloneLayout {
        before_suffix_comments,
        suffix,
    }
}

/// 格式化调用参数部分（不含前缀），如 `(a, b)` 或单参数简写 ` "str"` / ` { ... }`
fn format_call_args_ir(ctx: &FormatContext, expr: &LuaCallExpr) -> Vec<DocIR> {
    format_call_args_ir_with_options(ctx, expr, false)
}

fn format_call_args_ir_with_options(
    ctx: &FormatContext,
    expr: &LuaCallExpr,
    preserve_chain_attached_table_source: bool,
) -> Vec<DocIR> {
    let mut docs = Vec::new();

    if let Some(args_list) = expr.get_args_list() {
        let args: Vec<_> = args_list.get_args().collect();
        if let Some(single_arg_docs) = format_single_arg_call_without_parens(ctx, &args_list, &args)
        {
            docs.push(ir::space());
            docs.extend(single_arg_docs);
            return docs;
        }

        if ctx.config.spacing.space_before_call_paren {
            docs.push(ir::space());
        }

        if args.is_empty() {
            docs.push(tok(LuaTokenKind::TkLeftParen));
            docs.push(tok(LuaTokenKind::TkRightParen));
        } else {
            let arg_entries = collect_call_arg_entries(ctx, &args_list);
            let has_comments = arg_entries.iter().any(|entry| match entry {
                CallArgEntry::Arg {
                    trailing_comment, ..
                } => trailing_comment.is_some(),
                CallArgEntry::StandaloneComment(_) => true,
            });
            let has_standalone_comments = arg_entries
                .iter()
                .any(|entry| matches!(entry, CallArgEntry::StandaloneComment(_)));
            let align_comments = ctx.config.should_align_call_arg_line_comments()
                && !has_standalone_comments
                && call_arg_group_requests_alignment(&arg_entries);
            let trailing = format_trailing_comma_ir(ctx.config.output.trailing_comma.clone());

            match ctx.config.layout.call_args_expand {
                ExpandStrategy::Always => {
                    let inner = if has_comments {
                        build_multiline_call_arg_entries(ctx, arg_entries, align_comments)
                    } else {
                        let arg_docs: Vec<Vec<DocIR>> =
                            args.iter().map(|a| format_expr(ctx, a)).collect();
                        docs.extend(format_delimited_sequence(DelimitedSequenceLayout {
                            open: tok(LuaTokenKind::TkLeftParen),
                            close: tok(LuaTokenKind::TkRightParen),
                            items: arg_docs,
                            strategy: ExpandStrategy::Always,
                            preserve_multiline: false,
                            flat_separator: comma_space_sep(),
                            fill_separator: comma_soft_line_sep(),
                            break_separator: comma_soft_line_sep(),
                            flat_open_padding: vec![],
                            flat_close_padding: vec![],
                            grouped_padding: ir::soft_line_or_empty(),
                            flat_trailing: vec![],
                            grouped_trailing: trailing,
                            custom_break_contents: None,
                            prefer_custom_break_in_auto: false,
                        }));
                        return docs;
                    };
                    docs.push(ir::group_break(vec![
                        tok(LuaTokenKind::TkLeftParen),
                        ir::indent(vec![ir::hard_line(), ir::list(inner), trailing]),
                        ir::hard_line(),
                        tok(LuaTokenKind::TkRightParen),
                    ]));
                }
                ExpandStrategy::Never => {
                    if has_comments {
                        let inner =
                            build_multiline_call_arg_entries(ctx, arg_entries, align_comments);
                        docs.push(ir::group_break(vec![
                            tok(LuaTokenKind::TkLeftParen),
                            ir::indent(vec![ir::hard_line(), ir::list(inner), trailing]),
                            ir::hard_line(),
                            tok(LuaTokenKind::TkRightParen),
                        ]));
                    } else {
                        let arg_docs: Vec<Vec<DocIR>> =
                            args.iter().map(|a| format_expr(ctx, a)).collect();
                        docs.extend(format_delimited_sequence(DelimitedSequenceLayout {
                            open: tok(LuaTokenKind::TkLeftParen),
                            close: tok(LuaTokenKind::TkRightParen),
                            items: arg_docs,
                            strategy: ExpandStrategy::Never,
                            preserve_multiline: false,
                            flat_separator: comma_space_sep(),
                            fill_separator: comma_soft_line_sep(),
                            break_separator: comma_soft_line_sep(),
                            flat_open_padding: vec![],
                            flat_close_padding: vec![],
                            grouped_padding: ir::soft_line_or_empty(),
                            flat_trailing: vec![],
                            grouped_trailing: trailing,
                            custom_break_contents: None,
                            prefer_custom_break_in_auto: false,
                        }));
                    }
                }
                ExpandStrategy::Auto => {
                    if has_comments {
                        docs.extend(format_call_args_multiline_candidates(
                            ctx,
                            arg_entries,
                            trailing,
                            align_comments,
                            has_standalone_comments,
                            source_line_prefix_width(args_list.syntax()),
                        ));
                    } else {
                        let attach_first_arg = should_attach_first_call_arg(&args);
                        let preserve_multiline_args = args_list.syntax().text().contains_char('\n');
                        let arg_docs: Vec<Vec<DocIR>> = args
                            .iter()
                            .enumerate()
                            .map(|(index, arg)| {
                                format_call_arg_value_ir(
                                    ctx,
                                    arg,
                                    attach_first_arg,
                                    preserve_multiline_args,
                                    index,
                                    preserve_chain_attached_table_source,
                                )
                            })
                            .collect();
                        if attach_first_arg {
                            docs.extend(format_call_args_with_attached_first_arg(
                                arg_docs,
                                trailing,
                                preserve_multiline_args,
                            ));
                        } else if arg_docs.iter().any(|doc| ir_has_forced_line_break(doc)) {
                            let multiline_entries = arg_docs
                                .into_iter()
                                .enumerate()
                                .map(|(index, doc)| CallArgEntry::Arg {
                                    doc,
                                    trailing_comment: None,
                                    align_hint: false,
                                    has_following_arg: index + 1 < args.len(),
                                })
                                .collect();
                            docs.extend(format_call_args_multiline_candidates(
                                ctx,
                                multiline_entries,
                                trailing,
                                false,
                                false,
                                source_line_prefix_width(args_list.syntax()),
                            ));
                        } else {
                            docs.extend(format_delimited_sequence(DelimitedSequenceLayout {
                                open: tok(LuaTokenKind::TkLeftParen),
                                close: tok(LuaTokenKind::TkRightParen),
                                items: arg_docs,
                                strategy: ExpandStrategy::Auto,
                                preserve_multiline: false,
                                flat_separator: comma_space_sep(),
                                fill_separator: comma_soft_line_sep(),
                                break_separator: comma_soft_line_sep(),
                                flat_open_padding: vec![],
                                flat_close_padding: vec![],
                                grouped_padding: ir::soft_line_or_empty(),
                                flat_trailing: vec![],
                                grouped_trailing: trailing,
                                custom_break_contents: None,
                                prefer_custom_break_in_auto: false,
                            }));
                        }
                    }
                }
            }
        }
    }

    docs
}

fn should_attach_first_call_arg(args: &[LuaExpr]) -> bool {
    matches!(
        args.first(),
        Some(LuaExpr::TableExpr(_) | LuaExpr::ClosureExpr(_))
    )
}

fn format_call_arg_value_ir(
    ctx: &FormatContext,
    arg: &LuaExpr,
    attach_first_arg: bool,
    preserve_multiline_args: bool,
    index: usize,
    preserve_chain_attached_table_source: bool,
) -> Vec<DocIR> {
    if preserve_multiline_args && arg.syntax().text().contains_char('\n') {
        if let LuaExpr::TableExpr(table) = arg {
            if preserve_chain_attached_table_source && attach_first_arg && index == 0 {
                return format_preserved_multiline_attached_table_arg(ctx, table);
            }

            return format_table_expr_with_forced_expand(ctx, table, true);
        }

        if attach_first_arg && index == 0 {
            return format_expr(ctx, arg);
        }
    }

    format_expr(ctx, arg)
}

fn format_preserved_multiline_attached_table_arg(
    ctx: &FormatContext,
    table: &LuaTableExpr,
) -> Vec<DocIR> {
    let text = table.syntax().text().to_string();
    let normalized = normalize_multiline_table_trailing_separator(
        text.trim_end_matches(['\r', '\n', ' ', '\t']),
        ctx.config.trailing_table_comma(),
    );

    vec![ir::text(normalized)]
}

fn normalize_multiline_table_trailing_separator(
    source: &str,
    policy: crate::config::TrailingComma,
) -> String {
    let mut normalized = source.to_string();
    let close_index = normalized.rfind('}');
    let Some(close_index) = close_index else {
        return normalized;
    };

    let before_close = &normalized[..close_index];
    let content_end = before_close.trim_end_matches(['\r', '\n', ' ', '\t']).len();
    if content_end == 0 {
        return normalized;
    }

    let has_trailing_comma = normalized[..content_end].ends_with(',');
    match policy {
        crate::config::TrailingComma::Never => {
            if has_trailing_comma {
                normalized.remove(content_end - 1);
            }
        }
        crate::config::TrailingComma::Always | crate::config::TrailingComma::Multiline => {
            if !has_trailing_comma {
                normalized.insert(content_end, ',');
            }
        }
    }

    normalized
}

fn format_call_args_with_attached_first_arg(
    arg_docs: Vec<Vec<DocIR>>,
    trailing: DocIR,
    preserve_multiline: bool,
) -> Vec<DocIR> {
    let layout = DelimitedSequenceLayout {
        open: tok(LuaTokenKind::TkLeftParen),
        close: tok(LuaTokenKind::TkRightParen),
        items: arg_docs.clone(),
        strategy: ExpandStrategy::Auto,
        preserve_multiline: false,
        flat_separator: comma_space_sep(),
        fill_separator: comma_soft_line_sep(),
        break_separator: comma_soft_line_sep(),
        flat_open_padding: vec![],
        flat_close_padding: vec![],
        grouped_padding: ir::soft_line_or_empty(),
        flat_trailing: vec![],
        grouped_trailing: trailing.clone(),
        custom_break_contents: None,
        prefer_custom_break_in_auto: false,
    };

    let flat_docs = build_delimited_sequence_flat_candidate(&layout);
    let break_docs = build_call_args_attached_first_break_doc(arg_docs, trailing);

    if preserve_multiline {
        break_docs
    } else {
        let gid = ir::next_group_id();
        vec![ir::group_with_id(
            vec![ir::if_break_with_group(
                ir::list(break_docs),
                ir::list(flat_docs),
                gid,
            )],
            gid,
        )]
    }
}

fn build_call_args_attached_first_break_doc(
    arg_docs: Vec<Vec<DocIR>>,
    trailing: DocIR,
) -> Vec<DocIR> {
    if arg_docs.is_empty() {
        return vec![];
    }

    let mut docs = vec![tok(LuaTokenKind::TkLeftParen)];
    docs.extend(arg_docs[0].clone());

    if arg_docs.len() == 1 {
        docs.push(trailing);
        docs.push(tok(LuaTokenKind::TkRightParen));
        return vec![ir::group_break(docs)];
    } else {
        docs.push(tok(LuaTokenKind::TkComma));
        let mut rest = Vec::new();
        for (index, item_docs) in arg_docs.iter().enumerate().skip(1) {
            rest.push(ir::hard_line());
            rest.extend(item_docs.clone());
            if index + 1 < arg_docs.len() {
                rest.push(tok(LuaTokenKind::TkComma));
            }
        }
        rest.push(trailing);
        docs.push(ir::indent(rest));
    }

    docs.push(ir::hard_line());
    docs.push(tok(LuaTokenKind::TkRightParen));

    vec![ir::group_break(docs)]
}

/// 格式化索引访问部分（不含前缀），如 `.x`、`:m`、`[k]`
fn format_index_access_ir(ctx: &FormatContext, expr: &LuaIndexExpr) -> Vec<DocIR> {
    let mut docs = Vec::new();

    if let Some(index_token) = expr.get_index_token() {
        if index_token.is_dot() {
            docs.push(tok(LuaTokenKind::TkDot));
            if let Some(key) = expr.get_index_key() {
                docs.push(ir::text(key.get_path_part()));
            }
        } else if index_token.is_colon() {
            docs.push(tok(LuaTokenKind::TkColon));
            if let Some(key) = expr.get_index_key() {
                docs.push(ir::text(key.get_path_part()));
            }
        } else if index_token.is_left_bracket() {
            docs.push(tok(LuaTokenKind::TkLeftBracket));
            if ctx.config.spacing.space_inside_brackets {
                docs.push(ir::space());
            }
            if let Some(key) = expr.get_index_key() {
                match key {
                    LuaIndexKey::Expr(e) => {
                        docs.extend(format_expr(ctx, &e));
                    }
                    LuaIndexKey::Integer(n) => {
                        docs.push(ir::source_token(n.syntax().clone()));
                    }
                    LuaIndexKey::String(s) => {
                        docs.push(ir::source_token(s.syntax().clone()));
                    }
                    LuaIndexKey::Name(name) => {
                        docs.push(ir::source_token(name.syntax().clone()));
                    }
                    _ => {}
                }
            }
            if ctx.config.spacing.space_inside_brackets {
                docs.push(ir::space());
            }
            docs.push(tok(LuaTokenKind::TkRightBracket));
        }
    }

    docs
}

/// 尝试将方法链格式化为缩进形式
///
/// 对于 `a:b():c():d()` 这样的链式调用，扁平化为：
/// - 单行放得下: `a:b():c():d()`
/// - 超宽时展开:
/// ```text
///   a
///       :b()
///       :c()
///       :d()
/// ```
///
/// 仅在链长度 >= 2 段时触发（base + 2+ 段）。
fn try_format_chain(ctx: &FormatContext, expr: &LuaCallExpr) -> Option<Vec<DocIR>> {
    // 收集链段（从外向内遍历，最后翻转）
    struct ChainSegment {
        access: Vec<DocIR>,
        call_args: Option<Vec<DocIR>>,
    }

    let mut segments: Vec<ChainSegment> = Vec::new();
    let mut current: LuaExpr = expr.clone().into();

    loop {
        match &current {
            LuaExpr::CallExpr(call) => {
                let args = format_call_args_ir_with_options(ctx, call, true);
                if let Some(prefix) = call.get_prefix_expr()
                    && let LuaExpr::IndexExpr(idx) = &prefix
                {
                    let access = format_index_access_ir(ctx, idx);
                    segments.push(ChainSegment {
                        access,
                        call_args: Some(args),
                    });
                    if let Some(idx_prefix) = idx.get_prefix_expr() {
                        current = idx_prefix;
                        continue;
                    }
                }
                break;
            }
            LuaExpr::IndexExpr(idx) => {
                let access = format_index_access_ir(ctx, idx);
                segments.push(ChainSegment {
                    access,
                    call_args: None,
                });
                if let Some(idx_prefix) = idx.get_prefix_expr() {
                    current = idx_prefix;
                    continue;
                }
                break;
            }
            _ => break,
        }
    }

    // 至少 2 段才使用链式格式化
    if segments.len() < 2 {
        return None;
    }

    segments.reverse();

    // 基础表达式
    let base = format_expr(ctx, &current);

    let mut fill_parts = Vec::new();
    for (index, seg) in segments.iter().enumerate() {
        let mut segment = Vec::new();
        segment.extend(seg.access.clone());
        if let Some(args) = &seg.call_args {
            segment.extend(args.clone());
        }
        fill_parts.push(ir::list(segment));
        if index + 1 < segments.len() {
            fill_parts.push(ir::soft_line_or_empty());
        }
    }

    let mut docs = Vec::new();
    docs.extend(base);
    docs.push(ir::group(vec![ir::indent(vec![
        ir::soft_line_or_empty(),
        ir::fill(fill_parts),
    ])]));

    Some(docs)
}

/// Table literal: {}, { 1, 2, 3 }, { key = value, ... }
fn format_table_expr(ctx: &FormatContext, expr: &LuaTableExpr) -> Vec<DocIR> {
    format_table_expr_with_forced_expand(ctx, expr, false)
}

fn format_table_multiline_candidates(
    ctx: &FormatContext,
    entries: Vec<TableEntry>,
    trailing: DocIR,
    align_eq: bool,
    should_break: bool,
    has_standalone_comments: bool,
    first_line_prefix_width: usize,
) -> Vec<DocIR> {
    let align_comments = ctx.config.should_align_table_line_comments();
    let aligned = align_eq.then(|| {
        wrap_multiline_table_docs(build_table_expanded_inner(
            ctx,
            &entries,
            &trailing,
            true,
            align_comments,
        ))
    });
    let one_per_line = Some(wrap_multiline_table_docs(build_table_expanded_inner(
        ctx, &entries, &trailing, false, false,
    )));

    if should_break {
        choose_sequence_layout(
            ctx,
            SequenceLayoutCandidates {
                aligned,
                one_per_line,
                ..Default::default()
            },
            SequenceLayoutPolicy {
                allow_alignment: align_eq,
                allow_fill: false,
                allow_preserve: false,
                prefer_preserve_multiline: true,
                force_break_on_standalone_comments: has_standalone_comments,
                prefer_balanced_break_lines: false,
                first_line_prefix_width,
            },
        )
    } else {
        aligned.or(one_per_line).unwrap_or_default()
    }
}

fn continuation_break_ir(flat_space: bool) -> DocIR {
    if flat_space {
        ir::soft_line()
    } else {
        ir::soft_line_or_empty()
    }
}

/// Format a single table field IR (without trailing comment)
fn format_table_field_ir(ctx: &FormatContext, field: &LuaTableField) -> Vec<DocIR> {
    let mut fdoc = Vec::new();

    if field.is_assign_field() {
        fdoc.extend(format_table_field_key_ir(ctx, field));
        let assign_space = space_around_assign(ctx.config).to_ir();
        fdoc.push(assign_space.clone());
        fdoc.push(tok(LuaTokenKind::TkAssign));
        fdoc.push(assign_space);

        if let Some(value) = field.get_value_expr() {
            fdoc.extend(format_table_field_value_ir(ctx, &value));
        }
    } else {
        // value only
        if let Some(value) = field.get_value_expr() {
            fdoc.extend(format_table_field_value_ir(ctx, &value));
        }
    }

    fdoc
}

fn format_table_field_value_ir(ctx: &FormatContext, value: &LuaExpr) -> Vec<DocIR> {
    if let LuaExpr::TableExpr(table) = value
        && should_preserve_multiline_table_field_table_value(value)
    {
        format_table_expr_with_forced_expand(ctx, table, true)
    } else {
        format_expr(ctx, value)
    }
}

/// Format the key part of a table field
fn format_table_field_key_ir(ctx: &FormatContext, field: &LuaTableField) -> Vec<DocIR> {
    let mut docs = Vec::new();
    if let Some(key) = field.get_field_key() {
        match &key {
            LuaIndexKey::Name(name) => {
                docs.push(ir::source_token(name.syntax().clone()));
            }
            LuaIndexKey::String(s) => {
                docs.push(tok(LuaTokenKind::TkLeftBracket));
                docs.push(ir::source_token(s.syntax().clone()));
                docs.push(tok(LuaTokenKind::TkRightBracket));
            }
            LuaIndexKey::Integer(n) => {
                docs.push(tok(LuaTokenKind::TkLeftBracket));
                docs.push(ir::source_token(n.syntax().clone()));
                docs.push(tok(LuaTokenKind::TkRightBracket));
            }
            LuaIndexKey::Expr(e) => {
                docs.push(tok(LuaTokenKind::TkLeftBracket));
                docs.extend(format_expr(ctx, e));
                docs.push(tok(LuaTokenKind::TkRightBracket));
            }
            LuaIndexKey::Idx(_) => {}
        }
    }
    docs
}

/// Split a table field at `=` for alignment.
/// Returns (key_docs, value_docs) where value_docs starts with "=".
fn format_table_field_eq_split(ctx: &FormatContext, field: &LuaTableField) -> Option<EqSplit> {
    if !field.is_assign_field() {
        return None;
    }

    if field
        .get_value_expr()
        .as_ref()
        .is_some_and(should_preserve_multiline_table_field_value)
    {
        return None;
    }

    let before = format_table_field_key_ir(ctx, field);
    if before.is_empty() {
        return None;
    }

    let assign_space = space_around_assign(ctx.config).to_ir();
    let mut after = vec![tok(LuaTokenKind::TkAssign), assign_space];
    if let Some(value) = field.get_value_expr() {
        after.extend(format_expr(ctx, &value));
    }

    Some((before, after))
}

fn should_preserve_multiline_table_field_value(expr: &LuaExpr) -> bool {
    matches!(expr, LuaExpr::ClosureExpr(_) | LuaExpr::TableExpr(_))
        && expr.syntax().text().contains_char('\n')
}

fn should_preserve_multiline_table_field_table_value(expr: &LuaExpr) -> bool {
    matches!(expr, LuaExpr::TableExpr(_)) && expr.syntax().text().contains_char('\n')
}

/// Table entry: field or standalone comment
enum TableEntry {
    Field {
        doc: Vec<DocIR>,
        /// Split at `=` for alignment: (key_docs, eq_value_docs)
        eq_split: Option<EqSplit>,
        /// The field value should keep its multiline source shape, so the outer
        /// table must not stay in a flat candidate.
        force_expand: bool,
        /// Whether the original source shows an intent to align this field's value.
        align_hint: bool,
        /// Whether the original source shows an intent to align this field's trailing comment.
        comment_align_hint: bool,
        /// Raw trailing comment docs (NOT wrapped in LineSuffix)
        trailing_comment: Option<Vec<DocIR>>,
    },
    StandaloneComment(Vec<DocIR>),
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
    entries.iter().any(|entry| {
        matches!(
            entry,
            TableEntry::Field {
                align_hint: true,
                ..
            }
        )
    })
}

fn table_comment_group_requests_alignment(entries: &[TableEntry]) -> bool {
    entries.iter().any(|entry| {
        matches!(
            entry,
            TableEntry::Field {
                trailing_comment: Some(_),
                comment_align_hint: true,
                ..
            }
        )
    })
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

fn trailing_comment_suffix_with_padding(comment_docs: &[DocIR], padding: usize) -> DocIR {
    let mut suffix = Vec::new();
    suffix.extend((0..padding).map(|_| ir::space()));
    suffix.extend(comment_docs.iter().cloned());
    ir::line_suffix(suffix)
}

fn aligned_table_comment_widths(
    entries: &[TableEntry],
    group_start: usize,
    group_end: usize,
    last_field_idx: Option<usize>,
    trailing: &DocIR,
    max_before: usize,
) -> Vec<Option<usize>> {
    let mut widths = vec![None; group_end - group_start];
    let mut subgroup_start = group_start;

    while subgroup_start < group_end {
        while subgroup_start < group_end
            && !matches!(
                &entries[subgroup_start],
                TableEntry::Field {
                    trailing_comment: Some(_),
                    ..
                }
            )
        {
            subgroup_start += 1;
        }

        if subgroup_start >= group_end {
            break;
        }

        let mut subgroup_end = subgroup_start + 1;
        while subgroup_end < group_end
            && matches!(
                &entries[subgroup_end],
                TableEntry::Field {
                    trailing_comment: Some(_),
                    ..
                }
            )
        {
            subgroup_end += 1;
        }

        if table_comment_group_requests_alignment(&entries[subgroup_start..subgroup_end]) {
            let mut max_content_width = 0;

            for (index, entry) in entries
                .iter()
                .enumerate()
                .take(subgroup_end)
                .skip(subgroup_start)
            {
                if let TableEntry::Field {
                    eq_split: Some((_, after)),
                    ..
                } = entry
                {
                    let mut after_with_separator = after.clone();
                    if last_field_idx == Some(index) {
                        after_with_separator.push(trailing.clone());
                    } else {
                        after_with_separator.push(tok(LuaTokenKind::TkComma));
                    }

                    max_content_width = max_content_width
                        .max(max_before + 1 + ir::ir_flat_width(&after_with_separator));
                }
            }

            for index in subgroup_start..subgroup_end {
                widths[index - group_start] = Some(max_content_width);
            }
        }

        subgroup_start = subgroup_end;
    }

    widths
}

/// Build inner content (entries between { and }) for an expanded table.
/// When `align_eq` is true and there are consecutive `key = value` fields,
/// they are wrapped in an AlignGroup so the Printer aligns their `=` signs.
fn build_table_expanded_inner(
    ctx: &FormatContext,
    entries: &[TableEntry],
    trailing: &DocIR,
    align_eq: bool,
    align_comments: bool,
) -> Vec<DocIR> {
    let mut inner = Vec::new();

    let last_field_idx = entries
        .iter()
        .rposition(|e| matches!(e, TableEntry::Field { .. }));

    if align_eq {
        let len = entries.len();
        let mut i = 0;
        while i < len {
            if let TableEntry::Field {
                eq_split: Some(_), ..
            } = &entries[i]
            {
                let group_start = i;
                let mut group_end = i + 1;
                while group_end < len {
                    match &entries[group_end] {
                        TableEntry::Field {
                            eq_split: Some(_), ..
                        } => {
                            group_end += 1;
                        }
                        _ => break,
                    }
                }

                if group_end - group_start >= 2
                    && table_group_requests_alignment(&entries[group_start..group_end])
                {
                    inner.push(ir::hard_line());
                    let max_before = entries[group_start..group_end]
                        .iter()
                        .filter_map(|entry| match entry {
                            TableEntry::Field {
                                eq_split: Some((before, _)),
                                ..
                            } => Some(ir::ir_flat_width(before)),
                            _ => None,
                        })
                        .max()
                        .unwrap_or(0);
                    let comment_widths = if align_comments {
                        aligned_table_comment_widths(
                            entries,
                            group_start,
                            group_end,
                            last_field_idx,
                            trailing,
                            max_before,
                        )
                    } else {
                        vec![None; group_end - group_start]
                    };
                    let mut align_entries = Vec::new();
                    for (j, entry) in entries.iter().enumerate().take(group_end).skip(group_start) {
                        match entry {
                            TableEntry::Field {
                                eq_split: Some((before, after)),
                                align_hint: _,
                                comment_align_hint: _,
                                trailing_comment,
                                ..
                            } => {
                                let is_last = last_field_idx == Some(j);
                                let mut after_with_comma = after.clone();
                                if is_last {
                                    after_with_comma.push(trailing.clone());
                                } else {
                                    after_with_comma.push(tok(LuaTokenKind::TkComma));
                                }
                                if let Some(comment_docs) = trailing_comment {
                                    if let Some(aligned_content_width) =
                                        comment_widths[j - group_start]
                                    {
                                        let content_width =
                                            max_before + 1 + ir::ir_flat_width(&after_with_comma);
                                        let padding = trailing_comment_padding_for_config(
                                            ctx,
                                            content_width,
                                            aligned_content_width,
                                        );
                                        after_with_comma.push(
                                            trailing_comment_suffix_with_padding(
                                                comment_docs,
                                                padding,
                                            ),
                                        );
                                    } else {
                                        let mut suffix = trailing_comment_prefix(ctx.config);
                                        suffix.extend(comment_docs.clone());
                                        after_with_comma.push(ir::line_suffix(suffix));
                                    }
                                }
                                align_entries.push(AlignEntry::Aligned {
                                    before: before.clone(),
                                    after: after_with_comma,
                                    trailing: None,
                                });
                            }
                            TableEntry::StandaloneComment(comment_docs) => {
                                align_entries.push(AlignEntry::Line {
                                    content: comment_docs.clone(),
                                    trailing: None,
                                });
                            }
                            TableEntry::Field {
                                doc,
                                align_hint: _,
                                comment_align_hint: _,
                                trailing_comment,
                                ..
                            } => {
                                let is_last = last_field_idx == Some(j);
                                let mut line = doc.clone();
                                if is_last {
                                    line.push(trailing.clone());
                                } else {
                                    line.push(tok(LuaTokenKind::TkComma));
                                }
                                if align_comments {
                                    align_entries.push(AlignEntry::Line {
                                        content: line,
                                        trailing: trailing_comment.clone(),
                                    });
                                } else {
                                    if let Some(comment_docs) = trailing_comment {
                                        let mut suffix = trailing_comment_prefix(ctx.config);
                                        suffix.extend(comment_docs.clone());
                                        line.push(ir::line_suffix(suffix));
                                    }
                                    align_entries.push(AlignEntry::Line {
                                        content: line,
                                        trailing: None,
                                    });
                                }
                            }
                        }
                    }
                    inner.push(ir::align_group(align_entries));
                    i = group_end;
                    continue;
                }
            }

            match &entries[i] {
                TableEntry::Field {
                    doc,
                    align_hint: _,
                    comment_align_hint: _,
                    trailing_comment,
                    ..
                } => {
                    inner.push(ir::hard_line());
                    inner.extend(doc.clone());
                    let is_last = last_field_idx == Some(i);
                    if is_last {
                        inner.push(trailing.clone());
                    } else {
                        inner.push(tok(LuaTokenKind::TkComma));
                    }
                    if let Some(comment_docs) = trailing_comment {
                        let mut suffix = trailing_comment_prefix(ctx.config);
                        suffix.extend(comment_docs.clone());
                        inner.push(ir::line_suffix(suffix));
                    }
                }
                TableEntry::StandaloneComment(comment_docs) => {
                    inner.push(ir::hard_line());
                    inner.extend(comment_docs.clone());
                }
            }
            i += 1;
        }
    } else {
        for (i, entry) in entries.iter().enumerate() {
            match entry {
                TableEntry::Field {
                    doc,
                    align_hint: _,
                    comment_align_hint: _,
                    trailing_comment,
                    ..
                } => {
                    inner.push(ir::hard_line());
                    inner.extend(doc.clone());

                    let is_last_field = last_field_idx == Some(i);
                    if is_last_field {
                        inner.push(trailing.clone());
                    } else {
                        inner.push(tok(LuaTokenKind::TkComma));
                    }

                    if let Some(comment_docs) = trailing_comment {
                        let mut suffix = trailing_comment_prefix(ctx.config);
                        suffix.extend(comment_docs.clone());
                        inner.push(ir::line_suffix(suffix));
                    }
                }
                TableEntry::StandaloneComment(comment_docs) => {
                    inner.push(ir::hard_line());
                    inner.extend(comment_docs.clone());
                }
            }
        }
    }

    inner
}

/// 匿名函数: function(params) ... end
fn format_closure_expr(ctx: &FormatContext, expr: &LuaClosureExpr) -> Vec<DocIR> {
    if should_preserve_raw_closure_expr(expr) {
        return vec![ir::source_node_trimmed(expr.syntax().clone())];
    }

    let mut docs = vec![tok(LuaTokenKind::TkFunction)];

    if ctx.config.spacing.space_before_func_paren {
        docs.push(ir::space());
    }

    // 参数列表
    if let Some(params) = expr.get_params_list() {
        docs.extend(format_param_list_ir(ctx, &params));
    } else {
        docs.push(tok(LuaTokenKind::TkLeftParen));
        docs.push(tok(LuaTokenKind::TkRightParen));
    }

    // body
    super::format_body_end_with_parent(
        ctx,
        expr.get_block().as_ref(),
        Some(expr.syntax()),
        &mut docs,
    );

    docs
}

/// 括号表达式: (expr)
fn format_paren_expr(ctx: &FormatContext, expr: &LuaParenExpr) -> Vec<DocIR> {
    if node_has_direct_comment_child(expr.syntax()) {
        return format_paren_expr_with_standalone_comments(ctx, expr);
    }

    let mut docs = vec![tok(LuaTokenKind::TkLeftParen)];
    if ctx.config.spacing.space_inside_parens {
        docs.push(ir::space());
    }
    if let Some(inner) = expr.get_expr() {
        docs.extend(format_expr(ctx, &inner));
    }
    if ctx.config.spacing.space_inside_parens {
        docs.push(ir::space());
    }
    docs.push(tok(LuaTokenKind::TkRightParen));
    docs
}

fn format_paren_expr_with_standalone_comments(
    ctx: &FormatContext,
    expr: &LuaParenExpr,
) -> Vec<DocIR> {
    let entries = collect_paren_expr_entries(ctx, expr);
    let mut docs = vec![tok(LuaTokenKind::TkLeftParen)];

    if sequence_has_comment(&entries) {
        docs.push(ir::hard_line());
        render_sequence(&mut docs, &entries, true);
        docs.push(ir::hard_line());
    } else {
        if ctx.config.spacing.space_inside_parens {
            docs.push(ir::space());
        }
        render_sequence(&mut docs, &entries, false);
        if ctx.config.spacing.space_inside_parens {
            docs.push(ir::space());
        }
    }

    docs.push(tok(LuaTokenKind::TkRightParen));
    docs
}

fn collect_paren_expr_entries(ctx: &FormatContext, expr: &LuaParenExpr) -> Vec<SequenceEntry> {
    let mut entries = Vec::new();

    for child in expr.syntax().children_with_tokens() {
        match child.kind() {
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                if let Some(node) = child.as_node()
                    && let Some(comment) = LuaComment::cast(node.clone())
                {
                    entries.push(SequenceEntry::Comment(format_comment(ctx.config, &comment)));
                }
            }
            _ => {
                if let Some(node) = child.as_node()
                    && let Some(inner_expr) = LuaExpr::cast(node.clone())
                {
                    entries.push(SequenceEntry::Item(format_expr(ctx, &inner_expr)));
                }
            }
        }
    }

    entries
}

/// 根据 TrailingComma 配置生成尾逗号 IR
fn format_trailing_comma_ir(policy: crate::config::TrailingComma) -> DocIR {
    use crate::config::TrailingComma;
    match policy {
        TrailingComma::Never => ir::list(vec![]),
        TrailingComma::Multiline => ir::if_break(tok(LuaTokenKind::TkComma), ir::list(vec![])),
        TrailingComma::Always => tok(LuaTokenKind::TkComma),
    }
}

fn format_single_arg_call_without_parens(
    ctx: &FormatContext,
    args_list: &emmylua_parser::LuaCallArgList,
    args: &[LuaExpr],
) -> Option<Vec<DocIR>> {
    let single_arg = match ctx.config.output.single_arg_call_parens {
        SingleArgCallParens::Always => None,
        SingleArgCallParens::Preserve => args_list
            .is_single_arg_no_parens()
            .then(|| args_list.get_single_arg_expr())
            .flatten(),
        SingleArgCallParens::Omit => args_list
            .get_single_arg_expr()
            .or_else(|| single_arg_expr_from_args(args)),
    }?;

    Some(match single_arg {
        LuaSingleArgExpr::TableExpr(table) => format_table_expr(ctx, &table),
        LuaSingleArgExpr::LiteralExpr(lit) => format_literal_expr(ctx, &lit),
    })
}

fn single_arg_expr_from_args(args: &[LuaExpr]) -> Option<LuaSingleArgExpr> {
    if args.len() != 1 {
        return None;
    }

    match &args[0] {
        LuaExpr::TableExpr(table) => Some(LuaSingleArgExpr::TableExpr(table.clone())),
        LuaExpr::LiteralExpr(lit)
            if matches!(lit.get_literal(), Some(LuaLiteralToken::String(_))) =>
        {
            Some(LuaSingleArgExpr::LiteralExpr(lit.clone()))
        }
        _ => None,
    }
}

fn should_preserve_raw_call_expr(expr: &LuaCallExpr) -> bool {
    if node_has_direct_same_line_inline_comment(expr.syntax()) {
        return true;
    }

    expr.get_args_list()
        .map(|args| {
            node_has_direct_same_line_inline_comment(args.syntax())
                && !args.syntax().text().to_string().starts_with("(\n")
                && !args.syntax().text().to_string().starts_with("(\r\n")
        })
        .unwrap_or(false)
}

fn should_preserve_raw_closure_expr(expr: &LuaClosureExpr) -> bool {
    if node_has_direct_same_line_inline_comment(expr.syntax()) {
        return true;
    }

    expr.get_params_list()
        .map(|params| node_has_direct_same_line_inline_comment(params.syntax()))
        .unwrap_or(false)
}

enum CallArgEntry {
    Arg {
        doc: Vec<DocIR>,
        trailing_comment: Option<Vec<DocIR>>,
        align_hint: bool,
        has_following_arg: bool,
    },
    StandaloneComment(Vec<DocIR>),
}

impl Clone for CallArgEntry {
    fn clone(&self) -> Self {
        match self {
            Self::Arg {
                doc,
                trailing_comment,
                align_hint,
                has_following_arg,
            } => Self::Arg {
                doc: doc.clone(),
                trailing_comment: trailing_comment.clone(),
                align_hint: *align_hint,
                has_following_arg: *has_following_arg,
            },
            Self::StandaloneComment(comment_docs) => Self::StandaloneComment(comment_docs.clone()),
        }
    }
}

fn wrap_multiline_call_arg_docs(inner: Vec<DocIR>, trailing: DocIR) -> Vec<DocIR> {
    vec![ir::group_break(vec![
        tok(LuaTokenKind::TkLeftParen),
        ir::indent(vec![ir::hard_line(), ir::list(inner), trailing]),
        ir::hard_line(),
        tok(LuaTokenKind::TkRightParen),
    ])]
}

fn format_call_args_multiline_candidates(
    ctx: &FormatContext,
    entries: Vec<CallArgEntry>,
    trailing: DocIR,
    align_comments: bool,
    has_standalone_comments: bool,
    first_line_prefix_width: usize,
) -> Vec<DocIR> {
    let aligned = align_comments.then(|| {
        wrap_multiline_call_arg_docs(
            build_multiline_call_arg_entries(ctx, entries.clone(), true),
            trailing.clone(),
        )
    });
    let one_per_line = Some(wrap_multiline_call_arg_docs(
        build_multiline_call_arg_entries(ctx, entries, false),
        trailing,
    ));

    choose_sequence_layout(
        ctx,
        SequenceLayoutCandidates {
            aligned,
            one_per_line,
            ..Default::default()
        },
        SequenceLayoutPolicy {
            allow_alignment: align_comments,
            allow_fill: false,
            allow_preserve: false,
            prefer_preserve_multiline: true,
            force_break_on_standalone_comments: has_standalone_comments,
            prefer_balanced_break_lines: false,
            first_line_prefix_width,
        },
    )
}

fn trailing_comment_requests_alignment(
    node: &LuaSyntaxNode,
    comment_range: TextRange,
    required_min_gap: usize,
) -> bool {
    trailing_gap_requests_alignment(node, comment_range, required_min_gap)
}

fn call_arg_group_requests_alignment(entries: &[CallArgEntry]) -> bool {
    entries.iter().any(|entry| {
        matches!(
            entry,
            CallArgEntry::Arg {
                trailing_comment: Some(_),
                align_hint: true,
                ..
            }
        )
    })
}

fn collect_call_arg_entries(
    ctx: &FormatContext,
    args_list: &emmylua_parser::LuaCallArgList,
) -> Vec<CallArgEntry> {
    let args: Vec<_> = args_list.get_args().collect();
    let mut entries = Vec::new();
    let mut consumed_comment_ranges: Vec<TextRange> = Vec::new();
    let mut arg_index = 0usize;

    for child in args_list.syntax().children() {
        if let Some(arg) = LuaExpr::cast(child.clone()) {
            let (trailing_comment, align_hint) =
                if let Some((docs, range)) = extract_trailing_comment(ctx.config, arg.syntax()) {
                    consumed_comment_ranges.push(range);
                    (
                        Some(docs),
                        trailing_comment_requests_alignment(
                            arg.syntax(),
                            range,
                            ctx.config.comments.line_comment_min_spaces_before.max(1),
                        ),
                    )
                } else {
                    (None, false)
                };

            let has_following_arg = arg_index + 1 < args.len();
            arg_index += 1;
            entries.push(CallArgEntry::Arg {
                doc: format_expr(ctx, &arg),
                trailing_comment,
                align_hint,
                has_following_arg,
            });
        } else if child.kind() == LuaKind::Syntax(LuaSyntaxKind::Comment)
            && let Some(comment) = LuaComment::cast(child)
        {
            if consumed_comment_ranges
                .iter()
                .any(|range| *range == comment.syntax().text_range())
            {
                continue;
            }
            entries.push(CallArgEntry::StandaloneComment(format_comment(
                ctx.config, &comment,
            )));
        }
    }

    entries
}

fn build_multiline_call_arg_entries(
    ctx: &FormatContext,
    entries: Vec<CallArgEntry>,
    align_comments: bool,
) -> Vec<DocIR> {
    if align_comments {
        let mut align_entries = Vec::new();

        for entry in entries {
            match entry {
                CallArgEntry::Arg {
                    mut doc,
                    trailing_comment,
                    align_hint: _,
                    has_following_arg,
                } => {
                    if has_following_arg {
                        doc.push(tok(LuaTokenKind::TkComma));
                    }
                    align_entries.push(AlignEntry::Line {
                        content: doc,
                        trailing: trailing_comment,
                    });
                }
                CallArgEntry::StandaloneComment(comment_docs) => {
                    align_entries.push(AlignEntry::Line {
                        content: comment_docs,
                        trailing: None,
                    });
                }
            }
        }

        return vec![ir::align_group(align_entries)];
    }

    let mut inner = Vec::new();

    for (index, entry) in entries.into_iter().enumerate() {
        if index > 0 {
            inner.push(ir::hard_line());
        }

        match entry {
            CallArgEntry::Arg {
                doc,
                trailing_comment,
                align_hint: _,
                has_following_arg,
            } => {
                inner.extend(doc);
                if has_following_arg {
                    inner.push(tok(LuaTokenKind::TkComma));
                }
                if let Some(comment_docs) = trailing_comment {
                    let mut suffix = trailing_comment_prefix(ctx.config);
                    suffix.extend(comment_docs);
                    inner.push(ir::line_suffix(suffix));
                }
            }
            CallArgEntry::StandaloneComment(comment_docs) => {
                inner.extend(comment_docs);
            }
        }
    }

    inner
}

/// 格式化函数参数列表（支持参数注释）
///
/// 当参数之间有注释时，自动强制展开为多行。
pub fn format_param_list_ir(
    ctx: &FormatContext,
    params: &emmylua_parser::LuaParamList,
) -> Vec<DocIR> {
    let entries = collect_param_entries(ctx, params);
    if entries.is_empty() {
        return vec![
            tok(LuaTokenKind::TkLeftParen),
            tok(LuaTokenKind::TkRightParen),
        ];
    }

    let has_comments = entries.iter().any(|entry| match entry {
        ParamEntry::Param {
            trailing_comment, ..
        } => trailing_comment.is_some(),
        ParamEntry::StandaloneComment(_) => true,
    });

    if has_comments {
        let has_standalone_comments = entries
            .iter()
            .any(|entry| matches!(entry, ParamEntry::StandaloneComment(_)));
        let align_comments = ctx.config.should_align_param_line_comments()
            && !has_standalone_comments
            && param_group_requests_alignment(&entries);

        format_param_multiline_candidates(
            ctx,
            entries,
            format_trailing_comma_ir(ctx.config.output.trailing_comma.clone()),
            align_comments,
            has_standalone_comments,
            source_line_prefix_width(params.syntax()),
        )
    } else {
        let param_docs: Vec<Vec<DocIR>> = entries
            .into_iter()
            .filter_map(|entry| match entry {
                ParamEntry::Param { doc, .. } => Some(doc),
                ParamEntry::StandaloneComment(_) => None,
            })
            .collect();
        format_delimited_sequence(DelimitedSequenceLayout {
            open: tok(LuaTokenKind::TkLeftParen),
            close: tok(LuaTokenKind::TkRightParen),
            items: param_docs,
            strategy: ctx.config.layout.func_params_expand.clone(),
            preserve_multiline: false,
            flat_separator: comma_space_sep(),
            fill_separator: comma_soft_line_sep(),
            break_separator: comma_soft_line_sep(),
            flat_open_padding: vec![],
            flat_close_padding: vec![],
            grouped_padding: ir::soft_line_or_empty(),
            flat_trailing: vec![],
            grouped_trailing: format_trailing_comma_ir(ctx.config.output.trailing_comma.clone()),
            custom_break_contents: None,
            prefer_custom_break_in_auto: false,
        })
    }
}

enum ParamEntry {
    Param {
        doc: Vec<DocIR>,
        trailing_comment: Option<Vec<DocIR>>,
        align_hint: bool,
        has_following_param: bool,
    },
    StandaloneComment(Vec<DocIR>),
}

impl Clone for ParamEntry {
    fn clone(&self) -> Self {
        match self {
            Self::Param {
                doc,
                trailing_comment,
                align_hint,
                has_following_param,
            } => Self::Param {
                doc: doc.clone(),
                trailing_comment: trailing_comment.clone(),
                align_hint: *align_hint,
                has_following_param: *has_following_param,
            },
            Self::StandaloneComment(comment_docs) => Self::StandaloneComment(comment_docs.clone()),
        }
    }
}

fn wrap_multiline_param_docs(inner: Vec<DocIR>, trailing: DocIR) -> Vec<DocIR> {
    vec![ir::group_break(vec![
        tok(LuaTokenKind::TkLeftParen),
        ir::indent(vec![ir::hard_line(), ir::list(inner), trailing]),
        ir::hard_line(),
        tok(LuaTokenKind::TkRightParen),
    ])]
}

fn format_param_multiline_candidates(
    ctx: &FormatContext,
    entries: Vec<ParamEntry>,
    trailing: DocIR,
    align_comments: bool,
    has_standalone_comments: bool,
    first_line_prefix_width: usize,
) -> Vec<DocIR> {
    let aligned = align_comments.then(|| {
        let mut align_entries = Vec::new();
        for entry in entries.clone() {
            if let ParamEntry::Param {
                mut doc,
                trailing_comment,
                align_hint: _,
                has_following_param,
            } = entry
            {
                if has_following_param {
                    doc.push(tok(LuaTokenKind::TkComma));
                }
                align_entries.push(AlignEntry::Line {
                    content: doc,
                    trailing: trailing_comment,
                });
            }
        }

        wrap_multiline_param_docs(vec![ir::align_group(align_entries)], trailing.clone())
    });
    let one_per_line = Some(wrap_multiline_param_docs(
        build_multiline_param_entries(ctx, entries),
        trailing,
    ));

    choose_sequence_layout(
        ctx,
        SequenceLayoutCandidates {
            aligned,
            one_per_line,
            ..Default::default()
        },
        SequenceLayoutPolicy {
            allow_alignment: align_comments,
            allow_fill: false,
            allow_preserve: false,
            prefer_preserve_multiline: true,
            force_break_on_standalone_comments: has_standalone_comments,
            prefer_balanced_break_lines: false,
            first_line_prefix_width,
        },
    )
}

fn wrap_multiline_table_docs(inner: Vec<DocIR>) -> Vec<DocIR> {
    vec![ir::group_break(vec![
        tok(LuaTokenKind::TkLeftBrace),
        ir::indent(inner),
        ir::hard_line(),
        tok(LuaTokenKind::TkRightBrace),
    ])]
}

fn param_group_requests_alignment(entries: &[ParamEntry]) -> bool {
    entries.iter().any(|entry| {
        matches!(
            entry,
            ParamEntry::Param {
                trailing_comment: Some(_),
                align_hint: true,
                ..
            }
        )
    })
}

fn collect_param_entries(
    ctx: &FormatContext,
    params: &emmylua_parser::LuaParamList,
) -> Vec<ParamEntry> {
    let param_nodes: Vec<_> = params.get_params().collect();
    let mut entries = Vec::new();
    let mut consumed_comment_ranges: Vec<TextRange> = Vec::new();
    let mut param_index = 0usize;

    for child in params.syntax().children() {
        if let Some(param) = emmylua_parser::LuaParamName::cast(child.clone()) {
            let doc = if param.is_dots() {
                vec![ir::text("...")]
            } else if let Some(token) = param.get_name_token() {
                vec![ir::source_token(token.syntax().clone())]
            } else {
                continue;
            };

            let (trailing_comment, align_hint) =
                if let Some((docs, range)) = extract_trailing_comment(ctx.config, param.syntax()) {
                    consumed_comment_ranges.push(range);
                    (
                        Some(docs),
                        trailing_comment_requests_alignment(
                            param.syntax(),
                            range,
                            ctx.config.comments.line_comment_min_spaces_before.max(1),
                        ),
                    )
                } else {
                    (None, false)
                };

            let has_following_param = param_index + 1 < param_nodes.len();
            param_index += 1;
            entries.push(ParamEntry::Param {
                doc,
                trailing_comment,
                align_hint,
                has_following_param,
            });
        } else if child.kind() == LuaKind::Syntax(LuaSyntaxKind::Comment)
            && let Some(comment) = LuaComment::cast(child)
        {
            if consumed_comment_ranges
                .iter()
                .any(|range| *range == comment.syntax().text_range())
            {
                continue;
            }
            entries.push(ParamEntry::StandaloneComment(format_comment(
                ctx.config, &comment,
            )));
        }
    }

    entries
}

fn build_multiline_param_entries(ctx: &FormatContext, entries: Vec<ParamEntry>) -> Vec<DocIR> {
    let mut inner = Vec::new();

    for (index, entry) in entries.into_iter().enumerate() {
        if index > 0 {
            inner.push(ir::hard_line());
        }

        match entry {
            ParamEntry::Param {
                doc,
                trailing_comment,
                align_hint: _,
                has_following_param,
            } => {
                inner.extend(doc);
                if has_following_param {
                    inner.push(tok(LuaTokenKind::TkComma));
                }
                if let Some(comment_docs) = trailing_comment {
                    let mut suffix = trailing_comment_prefix(ctx.config);
                    suffix.extend(comment_docs);
                    inner.push(ir::line_suffix(suffix));
                }
            }
            ParamEntry::StandaloneComment(comment_docs) => {
                inner.extend(comment_docs);
            }
        }
    }

    inner
}
