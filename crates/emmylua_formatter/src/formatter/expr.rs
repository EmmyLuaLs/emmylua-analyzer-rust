use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaCallArgList, LuaCallExpr, LuaClosureExpr, LuaComment, LuaExpr,
    LuaIndexKey, LuaKind, LuaLiteralToken, LuaParamList, LuaSingleArgExpr, LuaSyntaxId,
    LuaSyntaxKind, LuaSyntaxNode, LuaSyntaxToken, LuaTableExpr, LuaTableField, LuaTokenKind,
};
use rowan::TextRange;

use crate::config::{ExpandStrategy, SingleArgCallParens, TrailingComma};
use crate::ir::{self, DocIR};

use super::FormatContext;
use super::model::{ExprSequenceLayoutPlan, RootFormatPlan, TokenSpacingExpected};
use super::sequence::{DelimitedSequenceLayout, format_delimited_sequence};
use super::trivia::{has_non_trivia_before_on_same_line_tokenwise, node_has_direct_comment_child};

pub fn format_expr(ctx: &FormatContext, plan: &RootFormatPlan, expr: &LuaExpr) -> Vec<DocIR> {
    match expr {
        LuaExpr::CallExpr(expr) => format_call_expr(ctx, plan, expr),
        LuaExpr::TableExpr(expr) => format_table_expr(ctx, plan, expr),
        LuaExpr::ClosureExpr(expr) => format_closure_expr(ctx, plan, expr),
        _ => vec![ir::source_node_trimmed(expr.syntax().clone())],
    }
}

pub fn format_param_list_ir(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    params: &LuaParamList,
) -> Vec<DocIR> {
    let collected = collect_param_entries(params);

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
    let layout_plan = expr_sequence_layout_plan(plan, params.syntax());

    format_delimited_sequence(
        ctx,
        DelimitedSequenceLayout {
            open: token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftParen),
            close: token_or_kind_doc(close.as_ref(), LuaTokenKind::TkRightParen),
            items: param_docs,
            strategy: ctx.config.layout.func_params_expand.clone(),
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

#[derive(Default)]
struct CollectedParamEntries {
    entries: Vec<ParamEntry>,
    comments_after_open: Vec<Vec<DocIR>>,
    comments_before_close: Vec<Vec<DocIR>>,
    has_comments: bool,
    consumed_comment_ranges: Vec<TextRange>,
}

struct ParamEntry {
    leading_comments: Vec<Vec<DocIR>>,
    doc: Vec<DocIR>,
    trailing_comment: Option<Vec<DocIR>>,
}

fn collect_param_entries(params: &LuaParamList) -> CollectedParamEntries {
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
                collected.comments_after_open.push(docs);
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
        let mut inner = Vec::new();

        for comment_docs in collected.comments_after_open {
            inner.push(ir::hard_line());
            inner.extend(comment_docs);
        }

        let entry_count = collected.entries.len();
        for (index, entry) in collected.entries.into_iter().enumerate() {
            inner.push(ir::hard_line());
            for comment_docs in entry.leading_comments {
                inner.extend(comment_docs);
                inner.push(ir::hard_line());
            }
            inner.extend(entry.doc);
            if index + 1 < entry_count {
                inner.extend(comma_token_docs(comma.as_ref()));
            } else {
                inner.push(trailing.clone());
            }
            if let Some(comment_docs) = entry.trailing_comment {
                let mut suffix = trailing_comment_prefix(ctx);
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
    let collected = collect_call_arg_entries(ctx, plan, args_list);

    if collected.has_comments {
        return format_call_arg_list_with_comments(ctx, plan, args_list, collected);
    }

    let arg_docs: Vec<Vec<DocIR>> = collected
        .entries
        .into_iter()
        .map(|entry| entry.doc)
        .collect();
    let (open, close) = paren_tokens(args_list.syntax());
    let comma = first_direct_token(args_list.syntax(), LuaTokenKind::TkComma);
    let layout_plan = expr_sequence_layout_plan(plan, args_list.syntax());

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

#[derive(Default)]
struct CollectedCallArgEntries {
    entries: Vec<CallArgEntry>,
    comments_after_open: Vec<Vec<DocIR>>,
    comments_before_close: Vec<Vec<DocIR>>,
    has_comments: bool,
    consumed_comment_ranges: Vec<TextRange>,
}

struct CallArgEntry {
    leading_comments: Vec<Vec<DocIR>>,
    doc: Vec<DocIR>,
    trailing_comment: Option<Vec<DocIR>>,
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
                collected.comments_after_open.push(docs);
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
            if let Some((_, range)) = &trailing_comment {
                collected.consumed_comment_ranges.push(*range);
            }
            collected.entries.push(CallArgEntry {
                leading_comments: std::mem::take(&mut pending_comments),
                doc: format_expr(ctx, plan, &arg),
                trailing_comment: trailing_comment.map(|(docs, _)| docs),
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
        let mut inner = Vec::new();

        for comment_docs in collected.comments_after_open {
            inner.push(ir::hard_line());
            inner.extend(comment_docs);
        }

        let entry_count = collected.entries.len();
        for (index, entry) in collected.entries.into_iter().enumerate() {
            inner.push(ir::hard_line());
            for comment_docs in entry.leading_comments {
                inner.extend(comment_docs);
                inner.push(ir::hard_line());
            }
            inner.extend(entry.doc);
            if index + 1 < entry_count {
                inner.extend(comma_token_docs(comma.as_ref()));
            } else {
                inner.push(trailing.clone());
            }
            if let Some(comment_docs) = entry.trailing_comment {
                let mut suffix = trailing_comment_prefix(ctx);
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

    if collected.has_comments {
        return format_table_with_comments(ctx, expr, collected);
    }

    let field_docs: Vec<Vec<DocIR>> = collected
        .entries
        .into_iter()
        .map(|entry| entry.doc)
        .collect();
    let (open, close) = brace_tokens(expr.syntax());
    let comma = first_direct_token(expr.syntax(), LuaTokenKind::TkComma);
    let layout_plan = expr_sequence_layout_plan(plan, expr.syntax());

    format_delimited_sequence(
        ctx,
        DelimitedSequenceLayout {
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
        },
    )
}

#[derive(Default)]
struct CollectedTableEntries {
    entries: Vec<TableEntry>,
    comments_after_open: Vec<Vec<DocIR>>,
    comments_before_close: Vec<Vec<DocIR>>,
    has_comments: bool,
    consumed_comment_ranges: Vec<TextRange>,
}

struct TableEntry {
    leading_comments: Vec<Vec<DocIR>>,
    doc: Vec<DocIR>,
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
                collected.comments_after_open.push(docs);
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
            collected.entries.push(TableEntry {
                leading_comments: std::mem::take(&mut pending_comments),
                doc: format_table_field_ir(ctx, plan, &field),
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
    let comma = first_direct_token(expr.syntax(), LuaTokenKind::TkComma);
    let mut docs = vec![token_or_kind_doc(open.as_ref(), LuaTokenKind::TkLeftBrace)];

    if !collected.comments_after_open.is_empty() || !collected.entries.is_empty() {
        let mut inner = Vec::new();

        for comment_docs in collected.comments_after_open {
            inner.push(ir::hard_line());
            inner.extend(comment_docs);
        }

        let entry_count = collected.entries.len();
        for (index, entry) in collected.entries.into_iter().enumerate() {
            inner.push(ir::hard_line());
            for comment_docs in entry.leading_comments {
                inner.extend(comment_docs);
                inner.push(ir::hard_line());
            }
            inner.extend(entry.doc);
            if index + 1 < entry_count
                || !matches!(ctx.config.trailing_table_comma(), TrailingComma::Never)
            {
                inner.extend(comma_token_docs(comma.as_ref()));
            }
            if let Some(comment_docs) = entry.trailing_comment {
                let mut suffix = trailing_comment_prefix(ctx);
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
    let gap = ctx.config.comments.line_comment_min_spaces_before.max(1);
    (0..gap).map(|_| ir::space()).collect()
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
