use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaAstToken, LuaBinaryExpr, LuaCallExpr, LuaClosureExpr,
    LuaComment, LuaExpr, LuaIndexExpr, LuaKind, LuaLiteralExpr, LuaNameExpr, LuaParenExpr,
    LuaSyntaxKind, LuaTableExpr, LuaTableField, LuaUnaryExpr, UnaryOperator,
};
use rowan::TextRange;

use crate::config::ExpandStrategy;
use crate::ir::{self, AlignEntry, DocIR, EqSplit};

use super::FormatContext;
use super::comment::{extract_trailing_comment, format_comment};
use super::spacing::{SpaceRule, space_around_assign, space_around_binary_op};

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
    if let Some(name) = expr.get_name_text() {
        vec![ir::text(name)]
    } else {
        vec![]
    }
}

fn format_literal_expr(_ctx: &FormatContext, expr: &LuaLiteralExpr) -> Vec<DocIR> {
    // 直接使用原始文本
    vec![ir::text(expr.syntax().text().to_string())]
}

/// 二元表达式: a + b, a and b, ...
///
/// 当表达式太长时，在操作符前断行并缩进：
/// ```text
/// very_long_left
///     + right
/// ```
fn format_binary_expr(ctx: &FormatContext, expr: &LuaBinaryExpr) -> Vec<DocIR> {
    if let Some((left, right)) = expr.get_exprs() {
        let left_docs = format_expr(ctx, &left);
        let right_docs = format_expr(ctx, &right);

        if let Some(op_token) = expr.get_op_token() {
            let op_text = op_token.syntax().text().to_string();
            let op = op_token.get_op();
            let space_rule = space_around_binary_op(op, ctx.config);
            let space_ir = space_rule.to_ir();

            // Safety: when the left operand text ends with '.' and the operator
            // is '..', we must force a space before the operator to avoid
            // ambiguity (e.g. `1. ..` must not become `1...`).
            // Only the before-space is forced; the after-space follows the
            // configured space_rule.
            let force_space_before = op == BinaryOperator::OpConcat
                && space_rule == SpaceRule::NoSpace
                && left.syntax().text().to_string().ends_with('.');

            // Before-operator break: soft_line (→space when flat) if space,
            // soft_line_or_empty (→"" when flat) if no space
            let break_ir = if !force_space_before && space_rule == SpaceRule::NoSpace {
                ir::soft_line_or_empty()
            } else {
                ir::soft_line()
            };

            return vec![ir::group(vec![
                ir::list(left_docs),
                ir::indent(vec![
                    break_ir,
                    ir::text(op_text),
                    space_ir,
                    ir::list(right_docs),
                ]),
            ])];
        }
    }

    vec![]
}

/// 一元表达式: -x, not x, #t, ~x
fn format_unary_expr(ctx: &FormatContext, expr: &LuaUnaryExpr) -> Vec<DocIR> {
    let mut docs = Vec::new();

    if let Some(op_token) = expr.get_op_token() {
        let op = op_token.get_op();
        let op_text = op_token.syntax().text().to_string();
        docs.push(ir::text(op_text));

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
    let mut docs = Vec::new();

    // 前缀
    if let Some(prefix) = expr.get_prefix_expr() {
        docs.extend(format_expr(ctx, &prefix));
    }

    // 索引操作符和 key
    docs.extend(format_index_access_ir(ctx, expr));

    docs
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
                emmylua_parser::LuaSingleArgExpr::TableExpr(table) => {
                    docs.push(ir::space());
                    docs.extend(format_table_expr(ctx, &table));
                    return docs;
                }
                emmylua_parser::LuaSingleArgExpr::LiteralExpr(lit) => {
                    docs.push(ir::space());
                    docs.extend(format_literal_expr(ctx, &lit));
                    return docs;
                }
            }
        }

        let args: Vec<_> = args_list.get_args().collect();

        if ctx.config.space_before_call_paren {
            docs.push(ir::space());
        }

        if args.is_empty() {
            docs.push(ir::text("("));
            docs.push(ir::text(")"));
        } else {
            let arg_docs: Vec<Vec<DocIR>> = args.iter().map(|a| format_expr(ctx, a)).collect();
            let trailing = format_trailing_comma_ir(ctx.config.trailing_comma.clone());

            match ctx.config.call_args_expand {
                ExpandStrategy::Always => {
                    let inner = ir::intersperse(arg_docs, vec![ir::text(","), ir::soft_line()]);
                    docs.push(ir::group_break(vec![
                        ir::text("("),
                        ir::indent(vec![ir::hard_line(), ir::list(inner), trailing]),
                        ir::hard_line(),
                        ir::text(")"),
                    ]));
                }
                ExpandStrategy::Never => {
                    let flat_inner = ir::intersperse(arg_docs, vec![ir::text(","), ir::space()]);
                    docs.push(ir::text("("));
                    docs.push(ir::list(flat_inner));
                    docs.push(ir::text(")"));
                }
                ExpandStrategy::Auto => {
                    let inner = ir::intersperse(arg_docs, vec![ir::text(","), ir::soft_line()]);
                    docs.push(ir::group(vec![
                        ir::text("("),
                        ir::indent(vec![ir::soft_line_or_empty(), ir::list(inner), trailing]),
                        ir::soft_line_or_empty(),
                        ir::text(")"),
                    ]));
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
            docs.push(ir::text("."));
            if let Some(key) = expr.get_index_key() {
                docs.push(ir::text(key.get_path_part()));
            }
        } else if index_token.is_colon() {
            docs.push(ir::text(":"));
            if let Some(key) = expr.get_index_key() {
                docs.push(ir::text(key.get_path_part()));
            }
        } else if index_token.is_left_bracket() {
            docs.push(ir::text("["));
            if ctx.config.space_inside_brackets {
                docs.push(ir::space());
            }
            if let Some(key) = expr.get_index_key() {
                match key {
                    emmylua_parser::LuaIndexKey::Expr(e) => {
                        docs.extend(format_expr(ctx, &e));
                    }
                    emmylua_parser::LuaIndexKey::Integer(n) => {
                        docs.push(ir::text(n.syntax().text().to_string()));
                    }
                    emmylua_parser::LuaIndexKey::String(s) => {
                        docs.push(ir::text(s.syntax().text().to_string()));
                    }
                    emmylua_parser::LuaIndexKey::Name(name) => {
                        docs.push(ir::text(name.get_name_text().to_string()));
                    }
                    _ => {}
                }
            }
            if ctx.config.space_inside_brackets {
                docs.push(ir::space());
            }
            docs.push(ir::text("]"));
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

    segments.reverse();

    // 基础表达式
    let base = format_expr(ctx, &current);

    // 构建链内容: indent(soft_line + seg1 + soft_line + seg2 + ...)
    let mut chain_content = Vec::new();
    for seg in &segments {
        chain_content.push(ir::soft_line_or_empty());
        chain_content.extend(seg.access.clone());
        if let Some(args) = &seg.call_args {
            chain_content.extend(args.clone());
        }
    }

    let mut docs = Vec::new();
    docs.extend(base);
    docs.push(ir::group(vec![ir::indent(chain_content)]));

    Some(docs)
}

/// Table literal: {}, { 1, 2, 3 }, { key = value, ... }
fn format_table_expr(ctx: &FormatContext, expr: &LuaTableExpr) -> Vec<DocIR> {
    if expr.is_empty() {
        return vec![ir::text("{}")];
    }

    // Collect all child nodes: fields and standalone comments
    let mut entries: Vec<TableEntry> = Vec::new();
    let mut consumed_comment_ranges: Vec<TextRange> = Vec::new();
    let mut has_standalone_comments = false;

    for child in expr.syntax().children() {
        if let Some(field) = LuaTableField::cast(child.clone()) {
            let fdoc = format_table_field_ir(ctx, &field);
            let eq_split = if ctx.config.align_table_field {
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
            entries.push(TableEntry::StandaloneComment(format_comment(&comment)));
            has_standalone_comments = true;
        }
    }

    // Trailing comma
    let trailing = format_trailing_comma_ir(ctx.config.trailing_comma.clone());

    let space_inside = if ctx.config.space_inside_braces {
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
    let force_expand = has_standalone_comments || has_trailing_comments;

    match ctx.config.table_expand {
        ExpandStrategy::Always => {
            build_table_expanded(entries, trailing, true, ctx.config.align_table_field)
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
            let flat_inner = ir::intersperse(field_docs, vec![ir::text(","), ir::space()]);
            let mut result = vec![ir::text("{")];
            if ctx.config.space_inside_braces {
                result.push(ir::space());
            }
            result.push(ir::list(flat_inner));
            if ctx.config.space_inside_braces {
                result.push(ir::space());
            }
            result.push(ir::text("}"));
            result
        }
        ExpandStrategy::Never => {
            // Never mode but has comments — must expand
            build_table_expanded(entries, trailing, true, ctx.config.align_table_field)
        }
        ExpandStrategy::Auto if force_expand => {
            // Has comments: force expand
            build_table_expanded(entries, trailing, true, ctx.config.align_table_field)
        }
        ExpandStrategy::Auto => {
            if ctx.config.align_table_field
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
                let flat_separator = vec![ir::text(","), ir::soft_line()];
                let flat_inner = ir::intersperse(flat_field_docs, flat_separator);
                let flat_doc = ir::list(vec![
                    ir::text("{"),
                    ir::indent(vec![
                        space_inside.clone(),
                        ir::list(flat_inner),
                        trailing.clone(),
                    ]),
                    space_inside.clone(),
                    ir::text("}"),
                ]);

                // Build break content with alignment for multi-line display
                let break_inner = build_table_expanded_inner(&entries, &trailing, true);
                let break_doc = ir::list(vec![
                    ir::text("{"),
                    ir::indent(break_inner),
                    ir::hard_line(),
                    ir::text("}"),
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
                let separator = vec![ir::text(","), ir::soft_line()];
                let inner = ir::intersperse(field_docs, separator);
                // Auto: single line if fits, otherwise expand
                vec![ir::group(vec![
                    ir::text("{"),
                    ir::indent(vec![space_inside.clone(), ir::list(inner), trailing]),
                    space_inside,
                    ir::text("}"),
                ])]
            }
        }
    }
}

/// Format a single table field IR (without trailing comment)
fn format_table_field_ir(ctx: &FormatContext, field: &LuaTableField) -> Vec<DocIR> {
    let mut fdoc = Vec::new();

    if field.is_assign_field() {
        fdoc.extend(format_table_field_key_ir(ctx, field));
        let assign_space = space_around_assign(ctx.config).to_ir();
        fdoc.push(assign_space.clone());
        fdoc.push(ir::text("="));
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
            emmylua_parser::LuaIndexKey::Name(name) => {
                docs.push(ir::text(name.get_name_text().to_string()));
            }
            emmylua_parser::LuaIndexKey::String(s) => {
                docs.push(ir::text("["));
                docs.push(ir::text(s.syntax().text().to_string()));
                docs.push(ir::text("]"));
            }
            emmylua_parser::LuaIndexKey::Integer(n) => {
                docs.push(ir::text("["));
                docs.push(ir::text(n.syntax().text().to_string()));
                docs.push(ir::text("]"));
            }
            emmylua_parser::LuaIndexKey::Expr(e) => {
                docs.push(ir::text("["));
                docs.extend(format_expr(ctx, e));
                docs.push(ir::text("]"));
            }
            emmylua_parser::LuaIndexKey::Idx(_) => {}
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
    let mut after = vec![ir::text("="), assign_space];
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
    entries: &[TableEntry],
    trailing: &DocIR,
    align_eq: bool,
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
                                    after_with_comma.push(ir::text(","));
                                }
                                align_entries.push(AlignEntry::Aligned {
                                    before: before.clone(),
                                    after: after_with_comma,
                                    trailing: trailing_comment.clone(),
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
                                trailing_comment,
                                ..
                            } => {
                                let is_last = last_field_idx == Some(j);
                                let mut line = doc.clone();
                                if is_last {
                                    line.push(trailing.clone());
                                } else {
                                    line.push(ir::text(","));
                                }
                                align_entries.push(AlignEntry::Line {
                                    content: line,
                                    trailing: trailing_comment.clone(),
                                });
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
                        inner.push(ir::text(","));
                    }
                    if let Some(comment_docs) = trailing_comment {
                        let mut suffix = vec![ir::space()];
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
                        inner.push(ir::text(","));
                    }

                    if let Some(comment_docs) = trailing_comment {
                        let mut suffix = vec![ir::space()];
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
    entries: Vec<TableEntry>,
    trailing: DocIR,
    should_break: bool,
    align_eq: bool,
) -> Vec<DocIR> {
    let inner = build_table_expanded_inner(&entries, &trailing, align_eq);

    if should_break {
        vec![ir::group_break(vec![
            ir::text("{"),
            ir::indent(inner),
            ir::hard_line(),
            ir::text("}"),
        ])]
    } else {
        vec![ir::group(vec![
            ir::text("{"),
            ir::indent(inner),
            ir::hard_line(),
            ir::text("}"),
        ])]
    }
}

/// 匿名函数: function(params) ... end
fn format_closure_expr(ctx: &FormatContext, expr: &LuaClosureExpr) -> Vec<DocIR> {
    let mut docs = vec![ir::text("function")];

    if ctx.config.space_before_func_paren {
        docs.push(ir::space());
    }

    // 参数列表
    docs.push(ir::text("("));
    if let Some(params) = expr.get_params_list() {
        docs.extend(format_params_ir(ctx, &params));
    }
    docs.push(ir::text(")"));

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
    let mut docs = vec![ir::text("(")];
    if ctx.config.space_inside_parens {
        docs.push(ir::space());
    }
    if let Some(inner) = expr.get_expr() {
        docs.extend(format_expr(ctx, &inner));
    }
    if ctx.config.space_inside_parens {
        docs.push(ir::space());
    }
    docs.push(ir::text(")"));
    docs
}

/// 根据 TrailingComma 配置生成尾逗号 IR
fn format_trailing_comma_ir(policy: crate::config::TrailingComma) -> DocIR {
    use crate::config::TrailingComma;
    match policy {
        TrailingComma::Never => ir::list(vec![]),
        TrailingComma::Multiline => ir::if_break(ir::text(","), ir::list(vec![])),
        TrailingComma::Always => ir::text(","),
    }
}

/// 参数条目
struct ParamEntry {
    doc: Vec<DocIR>,
    /// Raw trailing comment docs (NOT wrapped in LineSuffix)
    trailing_comment: Option<Vec<DocIR>>,
}

/// 格式化函数参数列表（支持参数注释）
///
/// 当参数之间有注释时，自动强制展开为多行。
/// 返回括号内的 IR（不含括号本身）。
pub fn format_params_ir(ctx: &FormatContext, params: &emmylua_parser::LuaParamList) -> Vec<DocIR> {
    // 收集参数和每个参数后的行尾注释
    let mut entries: Vec<ParamEntry> = Vec::new();
    let mut consumed_comment_ranges: Vec<TextRange> = Vec::new();

    for p in params.get_params() {
        let doc = if p.is_dots() {
            vec![ir::text("...")]
        } else if let Some(token) = p.get_name_token() {
            vec![ir::text(token.get_name_text().to_string())]
        } else {
            continue;
        };

        let trailing_comment = if let Some((docs, range)) = extract_trailing_comment(p.syntax()) {
            consumed_comment_ranges.push(range);
            Some(docs)
        } else {
            None
        };

        entries.push(ParamEntry {
            doc,
            trailing_comment,
        });
    }

    if entries.is_empty() {
        return vec![];
    }

    let has_comments = entries.iter().any(|e| e.trailing_comment.is_some());

    if has_comments {
        // 有注释：强制多行展开，使用 AlignGroup 对齐注释
        let len = entries.len();
        if ctx.config.align_continuous_line_comment {
            let mut align_entries = Vec::new();
            for (i, entry) in entries.into_iter().enumerate() {
                let mut content = entry.doc;
                if i < len - 1 {
                    content.push(ir::text(","));
                }
                align_entries.push(AlignEntry::Line {
                    content,
                    trailing: entry.trailing_comment,
                });
            }
            vec![ir::group_break(vec![
                ir::indent(vec![ir::hard_line(), ir::align_group(align_entries)]),
                ir::hard_line(),
            ])]
        } else {
            let mut inner = Vec::new();
            for (i, entry) in entries.into_iter().enumerate() {
                inner.push(ir::hard_line());
                inner.extend(entry.doc);
                if i < len - 1 {
                    inner.push(ir::text(","));
                }
                if let Some(comment_docs) = entry.trailing_comment {
                    let mut suffix = vec![ir::space()];
                    suffix.extend(comment_docs);
                    inner.push(ir::line_suffix(suffix));
                }
            }
            vec![ir::group_break(vec![ir::indent(inner), ir::hard_line()])]
        }
    } else {
        let param_docs: Vec<Vec<DocIR>> = entries.into_iter().map(|e| e.doc).collect();
        let inner = ir::intersperse(param_docs.clone(), vec![ir::text(","), ir::soft_line()]);

        match ctx.config.func_params_expand {
            ExpandStrategy::Always => {
                vec![ir::hard_line(), ir::indent(inner), ir::hard_line()]
            }
            ExpandStrategy::Never => ir::intersperse(param_docs, vec![ir::text(","), ir::space()]),
            ExpandStrategy::Auto => {
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
