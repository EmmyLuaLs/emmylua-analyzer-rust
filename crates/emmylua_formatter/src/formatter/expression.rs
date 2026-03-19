use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaAstToken, LuaBinaryExpr, LuaCallExpr, LuaClosureExpr,
    LuaComment, LuaExpr, LuaIndexExpr, LuaIndexKey, LuaKind, LuaLiteralExpr, LuaNameExpr,
    LuaParenExpr, LuaSingleArgExpr, LuaSyntaxKind, LuaTableExpr, LuaTableField, LuaTokenKind,
    LuaUnaryExpr, UnaryOperator,
};
use rowan::TextRange;

use crate::config::ExpandStrategy;
use crate::ir::{self, AlignEntry, DocIR, EqSplit};

use super::FormatContext;
use super::comment::{extract_trailing_comment, format_comment, trailing_comment_prefix};
use super::sequence::{
    SequenceEntry, render_sequence, sequence_ends_with_comment, sequence_has_comment,
    sequence_starts_with_comment,
};
use super::spacing::{SpaceRule, space_around_assign, space_around_binary_op};
use super::tokens::{comma_soft_line_sep, comma_space_sep, tok};
use super::trivia::{node_has_direct_comment_child, node_has_direct_same_line_inline_comment};

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

fn format_name_expr(_ctx: &FormatContext, expr: &LuaNameExpr) -> Vec<DocIR> {
    if let Some(token) = expr.get_name_token() {
        vec![ir::source_token(token.syntax().clone())]
    } else {
        vec![]
    }
}

fn format_literal_expr(_ctx: &FormatContext, expr: &LuaLiteralExpr) -> Vec<DocIR> {
    vec![ir::source_node(expr.syntax().clone())]
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
            let preserve_multiline_layout = expr.syntax().text().contains_char('\n');

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

            // Before-operator break: soft_line (→space when flat) if space,
            // soft_line_or_empty (→"" when flat) if no space
            let break_ir = continuation_break_ir(
                preserve_multiline_layout,
                force_space_before || space_rule != SpaceRule::NoSpace,
            );

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

    let space_rule = space_around_binary_op(op, ctx.config);
    let space_ir = space_rule.to_ir();
    let preserve_multiline_layout = expr.syntax().text().contains_char('\n');

    let mut docs = format_expr(ctx, &operands[0]);
    let mut previous = &operands[0];
    for operand in operands.iter().skip(1) {
        let force_space_before = op == BinaryOperator::OpConcat
            && space_rule == SpaceRule::NoSpace
            && expr_end_with_float(previous);
        let break_ir = continuation_break_ir(
            preserve_multiline_layout,
            force_space_before || space_rule != SpaceRule::NoSpace,
        );
        let mut segment = Vec::new();
        segment.push(break_ir);
        segment.push(ir::source_token(op_token.syntax().clone()));
        segment.push(space_ir.clone());
        segment.extend(format_expr(ctx, operand));

        if preserve_multiline_layout {
            docs.push(ir::indent(segment));
        } else {
            docs.push(ir::group(vec![ir::indent(segment)]));
        }

        previous = operand;
    }

    Some(docs)
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
    let mut docs = Vec::new();

    if let Some(args_list) = expr.get_args_list() {
        // 单参数简写
        if args_list.is_single_arg_no_parens()
            && let Some(single_arg) = args_list.get_single_arg_expr()
        {
            match single_arg {
                LuaSingleArgExpr::TableExpr(table) => {
                    docs.push(ir::space());
                    docs.extend(format_table_expr(ctx, &table));
                    return docs;
                }
                LuaSingleArgExpr::LiteralExpr(lit) => {
                    docs.push(ir::space());
                    docs.extend(format_literal_expr(ctx, &lit));
                    return docs;
                }
            }
        }

        let args: Vec<_> = args_list.get_args().collect();
        let preserve_multiline_layout = args_list.syntax().text().contains_char('\n');

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
            let trailing = format_trailing_comma_ir(ctx.config.output.trailing_comma.clone());

            match ctx.config.layout.call_args_expand {
                ExpandStrategy::Always => {
                    let inner = if has_comments {
                        build_multiline_call_arg_entries(ctx, arg_entries)
                    } else {
                        let arg_docs: Vec<Vec<DocIR>> =
                            args.iter().map(|a| format_expr(ctx, a)).collect();
                        vec![ir::list(ir::intersperse(arg_docs, comma_soft_line_sep()))]
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
                        let inner = build_multiline_call_arg_entries(ctx, arg_entries);
                        docs.push(ir::group_break(vec![
                            tok(LuaTokenKind::TkLeftParen),
                            ir::indent(vec![ir::hard_line(), ir::list(inner), trailing]),
                            ir::hard_line(),
                            tok(LuaTokenKind::TkRightParen),
                        ]));
                    } else {
                        let arg_docs: Vec<Vec<DocIR>> =
                            args.iter().map(|a| format_expr(ctx, a)).collect();
                        let flat_inner = ir::intersperse(arg_docs, comma_space_sep());
                        docs.push(tok(LuaTokenKind::TkLeftParen));
                        docs.push(ir::list(flat_inner));
                        docs.push(tok(LuaTokenKind::TkRightParen));
                    }
                }
                ExpandStrategy::Auto => {
                    if has_comments || preserve_multiline_layout {
                        let inner = if has_comments {
                            build_multiline_call_arg_entries(ctx, arg_entries)
                        } else {
                            let arg_docs: Vec<Vec<DocIR>> =
                                args.iter().map(|a| format_expr(ctx, a)).collect();
                            vec![ir::list(ir::intersperse(arg_docs, comma_soft_line_sep()))]
                        };
                        docs.push(ir::group_break(vec![
                            tok(LuaTokenKind::TkLeftParen),
                            ir::indent(vec![ir::hard_line(), ir::list(inner), trailing]),
                            ir::hard_line(),
                            tok(LuaTokenKind::TkRightParen),
                        ]));
                    } else {
                        let arg_docs: Vec<Vec<DocIR>> =
                            args.iter().map(|a| format_expr(ctx, a)).collect();
                        let inner = ir::intersperse(arg_docs, comma_soft_line_sep());
                        docs.push(ir::group(vec![
                            tok(LuaTokenKind::TkLeftParen),
                            ir::indent(vec![ir::soft_line_or_empty(), ir::list(inner), trailing]),
                            ir::soft_line_or_empty(),
                            tok(LuaTokenKind::TkRightParen),
                        ]));
                    }
                }
            }
        }
    }

    docs
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
                let args = format_call_args_ir(ctx, call);
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

    let preserve_multiline_layout = expr.syntax().text().contains_char('\n');

    segments.reverse();

    // 基础表达式
    let base = format_expr(ctx, &current);

    // 构建链内容: indent(soft_line + seg1 + soft_line + seg2 + ...)
    let mut chain_content = Vec::new();
    for seg in &segments {
        chain_content.push(continuation_break_ir(preserve_multiline_layout, false));
        chain_content.extend(seg.access.clone());
        if let Some(args) = &seg.call_args {
            chain_content.extend(args.clone());
        }
    }

    let mut docs = Vec::new();
    docs.extend(base);
    if preserve_multiline_layout {
        docs.push(ir::group_break(vec![ir::indent(chain_content)]));
    } else {
        docs.push(ir::group(vec![ir::indent(chain_content)]));
    }

    Some(docs)
}

/// Table literal: {}, { 1, 2, 3 }, { key = value, ... }
fn format_table_expr(ctx: &FormatContext, expr: &LuaTableExpr) -> Vec<DocIR> {
    if expr.is_empty() {
        return vec![
            tok(LuaTokenKind::TkLeftBrace),
            tok(LuaTokenKind::TkRightBrace),
        ];
    }

    // Collect all child nodes: fields and standalone comments
    let mut entries: Vec<TableEntry> = Vec::new();
    let mut consumed_comment_ranges: Vec<TextRange> = Vec::new();
    let mut has_standalone_comments = false;

    for child in expr.syntax().children() {
        if let Some(field) = LuaTableField::cast(child.clone()) {
            let fdoc = format_table_field_ir(ctx, &field);
            let eq_split = if ctx.config.align.table_field {
                format_table_field_eq_split(ctx, &field)
            } else {
                None
            };
            let trailing_comment =
                if let Some((docs, range)) = extract_trailing_comment(field.syntax()) {
                    consumed_comment_ranges.push(range);
                    Some(docs)
                } else {
                    None
                };
            entries.push(TableEntry::Field {
                doc: fdoc,
                eq_split,
                trailing_comment,
            });
        } else if child.kind() == LuaKind::Syntax(LuaSyntaxKind::Comment) {
            // Check if already consumed as trailing comment
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

    // Trailing comma
    let trailing = format_trailing_comma_ir(ctx.config.output.trailing_comma.clone());

    let space_inside = if ctx.config.spacing.space_inside_braces {
        ir::soft_line()
    } else {
        ir::soft_line_or_empty()
    };

    // Whether any field has a trailing comment
    let has_trailing_comments = entries.iter().any(|e| {
        matches!(
            e,
            TableEntry::Field {
                trailing_comment: Some(_),
                ..
            }
        )
    });

    // Standalone or trailing comments force expansion
    let preserve_multiline_layout = expr.syntax().text().contains_char('\n');
    let force_expand = has_standalone_comments || has_trailing_comments;

    match ctx.config.layout.table_expand {
        ExpandStrategy::Always => {
            build_table_expanded(ctx, entries, trailing, true, ctx.config.align.table_field)
        }
        ExpandStrategy::Never if !force_expand => {
            // Force single line (valid when no comments)
            let field_docs: Vec<Vec<DocIR>> = entries
                .into_iter()
                .filter_map(|e| match e {
                    TableEntry::Field { doc, .. } => Some(doc),
                    TableEntry::StandaloneComment(_) => None,
                })
                .collect();
            let flat_inner = ir::intersperse(field_docs, comma_space_sep());
            let mut result = vec![tok(LuaTokenKind::TkLeftBrace)];
            if ctx.config.spacing.space_inside_braces {
                result.push(ir::space());
            }
            result.push(ir::list(flat_inner));
            if ctx.config.spacing.space_inside_braces {
                result.push(ir::space());
            }
            result.push(tok(LuaTokenKind::TkRightBrace));
            result
        }
        ExpandStrategy::Never => {
            // Never mode but has comments — must expand
            build_table_expanded(ctx, entries, trailing, true, ctx.config.align.table_field)
        }
        ExpandStrategy::Auto if force_expand || preserve_multiline_layout => {
            // Has comments: force expand
            build_table_expanded(ctx, entries, trailing, true, ctx.config.align.table_field)
        }
        ExpandStrategy::Auto => {
            if ctx.config.align.table_field
                && entries.iter().any(|e| {
                    matches!(
                        e,
                        TableEntry::Field {
                            eq_split: Some(_),
                            ..
                        }
                    )
                })
            {
                // Build flat content for single-line display
                let flat_field_docs: Vec<Vec<DocIR>> = entries
                    .iter()
                    .filter_map(|e| match e {
                        TableEntry::Field { doc, .. } => Some(doc.clone()),
                        TableEntry::StandaloneComment(_) => None,
                    })
                    .collect();
                let flat_separator = comma_soft_line_sep();
                let flat_inner = ir::intersperse(flat_field_docs, flat_separator);
                let flat_doc = ir::list(vec![
                    tok(LuaTokenKind::TkLeftBrace),
                    ir::indent(vec![
                        space_inside.clone(),
                        ir::list(flat_inner),
                        trailing.clone(),
                    ]),
                    space_inside.clone(),
                    tok(LuaTokenKind::TkRightBrace),
                ]);

                // Build break content with alignment for multi-line display
                let break_inner = build_table_expanded_inner(
                    ctx,
                    &entries,
                    &trailing,
                    true,
                    ctx.config.should_align_table_line_comments(),
                );
                let break_doc = ir::list(vec![
                    tok(LuaTokenKind::TkLeftBrace),
                    ir::indent(break_inner),
                    ir::hard_line(),
                    tok(LuaTokenKind::TkRightBrace),
                ]);

                let gid = ir::next_group_id();
                vec![ir::group_with_id(
                    vec![ir::if_break_with_group(break_doc, flat_doc, gid)],
                    gid,
                )]
            } else {
                let field_docs: Vec<Vec<DocIR>> = entries
                    .into_iter()
                    .filter_map(|e| match e {
                        TableEntry::Field { doc, .. } => Some(doc),
                        TableEntry::StandaloneComment(_) => None,
                    })
                    .collect();
                let separator = comma_soft_line_sep();
                let inner = ir::intersperse(field_docs, separator);
                // Auto: single line if fits, otherwise expand
                vec![ir::group(vec![
                    tok(LuaTokenKind::TkLeftBrace),
                    ir::indent(vec![space_inside.clone(), ir::list(inner), trailing]),
                    space_inside,
                    tok(LuaTokenKind::TkRightBrace),
                ])]
            }
        }
    }
}

fn continuation_break_ir(preserve_multiline_layout: bool, flat_space: bool) -> DocIR {
    if preserve_multiline_layout {
        ir::hard_line()
    } else if flat_space {
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
            fdoc.extend(format_expr(ctx, &value));
        }
    } else {
        // value only
        if let Some(value) = field.get_value_expr() {
            fdoc.extend(format_expr(ctx, &value));
        }
    }

    fdoc
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

/// Table entry: field or standalone comment
enum TableEntry {
    Field {
        doc: Vec<DocIR>,
        /// Split at `=` for alignment: (key_docs, eq_value_docs)
        eq_split: Option<EqSplit>,
        /// Raw trailing comment docs (NOT wrapped in LineSuffix)
        trailing_comment: Option<Vec<DocIR>>,
    },
    StandaloneComment(Vec<DocIR>),
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
                        TableEntry::StandaloneComment(_) => {
                            group_end += 1;
                        }
                        _ => break,
                    }
                }

                if group_end - group_start >= 2 {
                    inner.push(ir::hard_line());
                    let mut align_entries = Vec::new();
                    for (j, entry) in entries.iter().enumerate().take(group_end).skip(group_start) {
                        match entry {
                            TableEntry::Field {
                                eq_split: Some((before, after)),
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
                                if align_comments {
                                    align_entries.push(AlignEntry::Aligned {
                                        before: before.clone(),
                                        after: after_with_comma,
                                        trailing: trailing_comment.clone(),
                                    });
                                } else {
                                    if let Some(comment_docs) = trailing_comment {
                                        let mut suffix = trailing_comment_prefix(ctx.config);
                                        suffix.extend(comment_docs.clone());
                                        after_with_comma.push(ir::line_suffix(suffix));
                                    }
                                    align_entries.push(AlignEntry::Aligned {
                                        before: before.clone(),
                                        after: after_with_comma,
                                        trailing: None,
                                    });
                                }
                            }
                            TableEntry::StandaloneComment(comment_docs) => {
                                align_entries.push(AlignEntry::Line {
                                    content: comment_docs.clone(),
                                    trailing: None,
                                });
                            }
                            TableEntry::Field {
                                doc,
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

/// Build expanded table (one field per line), wrapped in a Group.
fn build_table_expanded(
    ctx: &FormatContext,
    entries: Vec<TableEntry>,
    trailing: DocIR,
    should_break: bool,
    align_eq: bool,
) -> Vec<DocIR> {
    let inner = build_table_expanded_inner(
        ctx,
        &entries,
        &trailing,
        align_eq,
        ctx.config.should_align_table_line_comments(),
    );

    if should_break {
        vec![ir::group_break(vec![
            tok(LuaTokenKind::TkLeftBrace),
            ir::indent(inner),
            ir::hard_line(),
            tok(LuaTokenKind::TkRightBrace),
        ])]
    } else {
        vec![ir::group(vec![
            tok(LuaTokenKind::TkLeftBrace),
            ir::indent(inner),
            ir::hard_line(),
            tok(LuaTokenKind::TkRightBrace),
        ])]
    }
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
    docs.push(tok(LuaTokenKind::TkLeftParen));
    if let Some(params) = expr.get_params_list() {
        docs.extend(format_params_ir(ctx, &params));
    }
    docs.push(tok(LuaTokenKind::TkRightParen));

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

fn should_preserve_raw_call_expr(expr: &LuaCallExpr) -> bool {
    if node_has_direct_same_line_inline_comment(expr.syntax()) {
        return true;
    }

    expr.get_args_list()
        .map(|args| node_has_direct_same_line_inline_comment(args.syntax()))
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
        has_following_arg: bool,
    },
    StandaloneComment(Vec<DocIR>),
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
            let trailing_comment =
                if let Some((docs, range)) = extract_trailing_comment(arg.syntax()) {
                    consumed_comment_ranges.push(range);
                    Some(docs)
                } else {
                    None
                };

            let has_following_arg = arg_index + 1 < args.len();
            arg_index += 1;
            entries.push(CallArgEntry::Arg {
                doc: format_expr(ctx, &arg),
                trailing_comment,
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

fn build_multiline_call_arg_entries(ctx: &FormatContext, entries: Vec<CallArgEntry>) -> Vec<DocIR> {
    let mut inner = Vec::new();

    for (index, entry) in entries.into_iter().enumerate() {
        if index > 0 {
            inner.push(ir::hard_line());
        }

        match entry {
            CallArgEntry::Arg {
                doc,
                trailing_comment,
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
/// 返回括号内的 IR（不含括号本身）。
pub fn format_params_ir(ctx: &FormatContext, params: &emmylua_parser::LuaParamList) -> Vec<DocIR> {
    let entries = collect_param_entries(ctx, params);
    let preserve_multiline_layout = params.syntax().text().contains_char('\n');

    if entries.is_empty() {
        return vec![];
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

        if ctx.config.should_align_param_line_comments() && !has_standalone_comments {
            let mut align_entries = Vec::new();
            for entry in entries {
                if let ParamEntry::Param {
                    mut doc,
                    trailing_comment,
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
            vec![ir::group_break(vec![
                ir::indent(vec![ir::hard_line(), ir::align_group(align_entries)]),
                ir::hard_line(),
            ])]
        } else {
            let inner = build_multiline_param_entries(ctx, entries);
            vec![ir::group_break(vec![
                ir::indent(vec![ir::hard_line(), ir::list(inner)]),
                ir::hard_line(),
            ])]
        }
    } else {
        let param_docs: Vec<Vec<DocIR>> = entries
            .into_iter()
            .filter_map(|entry| match entry {
                ParamEntry::Param { doc, .. } => Some(doc),
                ParamEntry::StandaloneComment(_) => None,
            })
            .collect();
        let inner = ir::intersperse(param_docs.clone(), comma_soft_line_sep());

        match ctx.config.layout.func_params_expand {
            ExpandStrategy::Always => {
                vec![ir::hard_line(), ir::indent(inner), ir::hard_line()]
            }
            ExpandStrategy::Never => ir::intersperse(param_docs, comma_space_sep()),
            ExpandStrategy::Auto => {
                if preserve_multiline_layout {
                    vec![ir::group_break(vec![
                        ir::indent(vec![ir::hard_line(), ir::list(inner)]),
                        ir::hard_line(),
                    ])]
                } else {
                    vec![ir::group(
                        [
                            vec![ir::soft_line_or_empty()],
                            vec![ir::indent(inner)],
                            vec![ir::soft_line_or_empty()],
                        ]
                        .concat(),
                    )]
                }
            }
        }
    }
}

enum ParamEntry {
    Param {
        doc: Vec<DocIR>,
        trailing_comment: Option<Vec<DocIR>>,
        has_following_param: bool,
    },
    StandaloneComment(Vec<DocIR>),
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

            let trailing_comment =
                if let Some((docs, range)) = extract_trailing_comment(param.syntax()) {
                    consumed_comment_ranges.push(range);
                    Some(docs)
                } else {
                    None
                };

            let has_following_param = param_index + 1 < param_nodes.len();
            param_index += 1;
            entries.push(ParamEntry::Param {
                doc,
                trailing_comment,
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
