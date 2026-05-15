use super::comments::{
    append_trailing_statement_suffix, has_inline_non_trivia_after,
    has_inline_non_trivia_before,
};
use super::*;

struct StatementAssignSplit {
    lhs_entries: Vec<SequenceEntry>,
    assign_op: Option<LuaSyntaxToken>,
    rhs_entries: Vec<SequenceEntry>,
}

type DocPair = (Vec<DocIR>, Vec<DocIR>);

pub(crate) fn render_local_stat(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_id: LuaSyntaxId,
    plan: &FormatPlan,
) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_id) else {
        return Vec::new();
    };
    let Some(stat) = LuaLocalStat::cast(node) else {
        return Vec::new();
    };

    if node_has_direct_comment_child(stat.syntax()) {
        return format_local_stat_trivia_aware(ctx, plan, &stat);
    }

    let local_token = first_direct_token(stat.syntax(), LuaTokenKind::TkLocal);
    let comma_token = first_direct_token(stat.syntax(), LuaTokenKind::TkComma);
    let assign_token = first_direct_token(stat.syntax(), LuaTokenKind::TkAssign);
    let mut docs = vec![token_or_kind_doc(
        local_token.as_ref(),
        LuaTokenKind::TkLocal,
    )];
    docs.extend(token_right_spacing_docs(plan, local_token.as_ref()));
    let local_names: Vec<_> = stat.get_local_name_list().collect();
    for (index, local_name) in local_names.iter().enumerate() {
        if index > 0 {
            docs.extend(comma_flat_separator(plan, comma_token.as_ref()));
        }
        docs.extend(format_local_name_ir(local_name));
    }

    let exprs: Vec<_> = stat.get_value_exprs().collect();
    if !exprs.is_empty() {
        let expr_list_plan = plan
            .layout
            .statement_expr_lists
            .get(&syntax_id)
            .copied()
            .expect("missing local statement expr-list layout plan");
        docs.extend(token_left_spacing_docs(plan, assign_token.as_ref()));
        docs.push(token_or_kind_doc(
            assign_token.as_ref(),
            LuaTokenKind::TkAssign,
        ));

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
                    index + 1 == exprs.len(),
                )
            })
            .collect();

        docs.extend(render_statement_exprs(
            ctx,
            plan,
            expr_list_plan,
            assign_token.as_ref(),
            comma_token.as_ref(),
            expr_docs,
        ));
    }

    append_trailing_statement_suffix(ctx, plan, &mut docs, stat.syntax());

    docs
}

pub(crate) fn render_assign_stat(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_id: LuaSyntaxId,
    plan: &FormatPlan,
) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_id) else {
        return Vec::new();
    };
    let Some(stat) = LuaAssignStat::cast(node) else {
        return Vec::new();
    };

    if node_has_direct_comment_child(stat.syntax()) {
        return format_assign_stat_trivia_aware(ctx, plan, &stat);
    }

    let mut docs = Vec::new();
    let (vars, exprs) = stat.get_var_and_expr_list();
    let expr_list_plan = plan
        .layout
        .statement_expr_lists
        .get(&syntax_id)
        .copied()
        .expect("missing assign statement expr-list layout plan");
    let comma_token = first_direct_token(stat.syntax(), LuaTokenKind::TkComma);
    let assign_token = stat.get_assign_op().map(|op| op.syntax().clone());
    let var_docs: Vec<Vec<DocIR>> = vars
        .iter()
        .map(|var| render_expr(ctx, plan, &var.clone().into()))
        .collect();
    docs.extend(ir::intersperse(
        var_docs,
        comma_flat_separator(plan, comma_token.as_ref()),
    ));

    if let Some(op) = stat.get_assign_op() {
        docs.extend(token_left_spacing_docs(plan, assign_token.as_ref()));
        docs.push(ir::source_token(op.syntax().clone()));
    }

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
                index + 1 == exprs.len(),
            )
        })
        .collect();

    docs.extend(render_statement_exprs(
        ctx,
        plan,
        expr_list_plan,
        assign_token.as_ref(),
        comma_token.as_ref(),
        expr_docs,
    ));

    append_trailing_statement_suffix(ctx, plan, &mut docs, stat.syntax());

    docs
}

pub(crate) fn render_return_stat(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_id: LuaSyntaxId,
    plan: &FormatPlan,
) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_id) else {
        return Vec::new();
    };
    let Some(stat) = LuaReturnStat::cast(node) else {
        return Vec::new();
    };

    if node_has_direct_comment_child(stat.syntax()) {
        return format_return_stat_trivia_aware(ctx, plan, &stat);
    }

    let return_token = first_direct_token(stat.syntax(), LuaTokenKind::TkReturn);
    let comma_token = first_direct_token(stat.syntax(), LuaTokenKind::TkComma);
    let mut docs = vec![token_or_kind_doc(
        return_token.as_ref(),
        LuaTokenKind::TkReturn,
    )];

    let exprs: Vec<_> = stat.get_expr_list().collect();
    if !exprs.is_empty() {
        let expr_list_plan = plan
            .layout
            .statement_expr_lists
            .get(&syntax_id)
            .copied()
            .expect("missing return statement expr-list layout plan");
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
                    index + 1 == exprs.len(),
                )
            })
            .collect();

        docs.extend(render_statement_exprs(
            ctx,
            plan,
            expr_list_plan,
            return_token.as_ref(),
            comma_token.as_ref(),
            expr_docs,
        ));
    }

    append_trailing_statement_suffix(ctx, plan, &mut docs, stat.syntax());

    docs
}

pub(crate) fn render_call_expr_stat(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_id: LuaSyntaxId,
    plan: &FormatPlan,
) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_id) else {
        return Vec::new();
    };
    let Some(stat) = LuaCallExprStat::cast(node) else {
        return Vec::new();
    };

    let mut docs = stat
        .get_call_expr()
        .map(|expr| {
            render_expr_with_options(
                ctx,
                plan,
                &expr.into(),
                ExprFormatOptions {
                    prefer_chain_break: ctx.config.layout.prefer_chain_break_on_statement_tail,
                },
            )
        })
        .unwrap_or_default();
    append_trailing_statement_suffix(ctx, plan, &mut docs, stat.syntax());
    docs
}

pub(crate) fn render_empty_stat(root: &LuaSyntaxNode, syntax_id: LuaSyntaxId) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_id) else {
        return Vec::new();
    };

    vec![ir::source_node_trimmed(node)]
}

fn format_local_stat_trivia_aware(
    ctx: &FormatContext,
    plan: &FormatPlan,
    stat: &LuaLocalStat,
) -> Vec<DocIR> {
    let StatementAssignSplit {
        lhs_entries,
        assign_op,
        rhs_entries,
    } = collect_local_stat_entries(ctx, plan, stat);
    let syntax_id = stat.get_syntax_id();
    let local_token = first_direct_token(stat.syntax(), LuaTokenKind::TkLocal);
    let mut docs = vec![token_or_kind_doc(
        local_token.as_ref(),
        LuaTokenKind::TkLocal,
    )];
    let has_inline_comment = plan
        .layout
        .statement_trivia
        .get(&syntax_id)
        .is_some_and(|layout| layout.has_inline_comment);

    if has_inline_comment {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    }

    if !lhs_entries.is_empty() {
        docs.extend(token_right_spacing_docs(plan, local_token.as_ref()));
        render_sequence(&mut docs, &lhs_entries, false);
    }

    if let Some(assign_op) = assign_op {
        if sequence_has_comment(&lhs_entries) {
            if !sequence_ends_with_comment(&lhs_entries) {
                docs.push(ir::hard_line());
            }
            docs.push(ir::source_token(assign_op.clone()));
        } else {
            docs.extend(token_left_spacing_docs(plan, Some(&assign_op)));
            docs.push(ir::source_token(assign_op.clone()));
        }

        if !rhs_entries.is_empty() {
            if sequence_has_comment(&rhs_entries) {
                docs.push(ir::hard_line());
                render_sequence(&mut docs, &rhs_entries, true);
            } else {
                docs.extend(token_right_spacing_docs(plan, Some(&assign_op)));
                render_sequence(&mut docs, &rhs_entries, false);
            }
        }
    }

    append_trailing_statement_suffix(ctx, plan, &mut docs, stat.syntax());

    docs
}

fn format_assign_stat_trivia_aware(
    ctx: &FormatContext,
    plan: &FormatPlan,
    stat: &LuaAssignStat,
) -> Vec<DocIR> {
    let StatementAssignSplit {
        lhs_entries,
        assign_op,
        rhs_entries,
    } = collect_assign_stat_entries(ctx, plan, stat);
    let syntax_id = stat.get_syntax_id();
    let has_inline_comment = plan
        .layout
        .statement_trivia
        .get(&syntax_id)
        .is_some_and(|layout| layout.has_inline_comment);

    if has_inline_comment {
        return vec![ir::indent(render_trivia_aware_split_sequence_tail(
            plan,
            Vec::new(),
            &lhs_entries,
            assign_op.as_ref(),
            &rhs_entries,
        ))];
    }
    let mut docs = Vec::new();
    render_sequence(&mut docs, &lhs_entries, false);

    if let Some(assign_op) = assign_op {
        if sequence_has_comment(&lhs_entries) {
            if !sequence_ends_with_comment(&lhs_entries) {
                docs.push(ir::hard_line());
            }
            docs.push(ir::source_token(assign_op.clone()));
        } else {
            docs.extend(token_left_spacing_docs(plan, Some(&assign_op)));
            docs.push(ir::source_token(assign_op.clone()));
        }

        if !rhs_entries.is_empty() {
            if sequence_has_comment(&rhs_entries) {
                docs.push(ir::hard_line());
                render_sequence(&mut docs, &rhs_entries, true);
            } else {
                docs.extend(token_right_spacing_docs(plan, Some(&assign_op)));
                render_sequence(&mut docs, &rhs_entries, false);
            }
        }
    }

    append_trailing_statement_suffix(ctx, plan, &mut docs, stat.syntax());

    docs
}

fn format_return_stat_trivia_aware(
    ctx: &FormatContext,
    plan: &FormatPlan,
    stat: &LuaReturnStat,
) -> Vec<DocIR> {
    let entries = collect_return_stat_entries(ctx, plan, stat);
    let syntax_id = stat.get_syntax_id();
    let return_token = first_direct_token(stat.syntax(), LuaTokenKind::TkReturn);
    let mut docs = vec![token_or_kind_doc(
        return_token.as_ref(),
        LuaTokenKind::TkReturn,
    )];
    let has_inline_comment = plan
        .layout
        .statement_trivia
        .get(&syntax_id)
        .is_some_and(|layout| layout.has_inline_comment);
    if entries.is_empty() {
        return docs;
    }

    if has_inline_comment {
        docs.push(ir::indent(render_trivia_aware_sequence_tail(
            plan,
            token_right_spacing_docs(plan, return_token.as_ref()),
            &entries,
        )));
        return docs;
    }

    if sequence_has_comment(&entries) {
        docs.push(ir::hard_line());
        render_sequence(&mut docs, &entries, true);
    } else {
        docs.extend(token_right_spacing_docs(plan, return_token.as_ref()));
        render_sequence(&mut docs, &entries, false);
    }

    append_trailing_statement_suffix(ctx, plan, &mut docs, stat.syntax());

    docs
}

fn collect_local_stat_entries(
    ctx: &FormatContext,
    plan: &FormatPlan,
    stat: &LuaLocalStat,
) -> StatementAssignSplit {
    let mut lhs_entries = Vec::new();
    let mut rhs_entries = Vec::new();
    let mut assign_op = None;
    let mut meet_assign = false;

    for child in stat.syntax().children_with_tokens() {
        match child.kind() {
            LuaKind::Token(token_kind) if token_kind.is_assign_op() => {
                meet_assign = true;
                assign_op = child.as_token().cloned();
            }
            LuaKind::Token(LuaTokenKind::TkComma) => {
                let entry = separator_entry_from_token(plan, child.as_token());
                if meet_assign {
                    rhs_entries.push(entry);
                } else {
                    lhs_entries.push(entry);
                }
            }
            LuaKind::Syntax(LuaSyntaxKind::LocalName) => {
                if let Some(node) = child.as_node()
                    && let Some(local_name) = LuaLocalName::cast(node.clone())
                {
                    let entry = SequenceEntry::Item(format_local_name_ir(&local_name));
                    if meet_assign {
                        rhs_entries.push(entry);
                    } else {
                        lhs_entries.push(entry);
                    }
                }
            }
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                if let Some(node) = child.as_node()
                    && let Some(comment) = LuaComment::cast(node.clone())
                {
                    if has_inline_non_trivia_before(comment.syntax())
                        && !has_inline_non_trivia_after(comment.syntax())
                    {
                        continue;
                    }
                    let entry = SequenceEntry::Comment(SequenceComment {
                        docs: vec![ir::source_node_trimmed(comment.syntax().clone())],
                        inline_after_previous: has_non_trivia_before_on_same_line_tokenwise(
                            comment.syntax(),
                        ),
                    });
                    if meet_assign {
                        rhs_entries.push(entry);
                    } else {
                        lhs_entries.push(entry);
                    }
                }
            }
            _ => {
                if let Some(node) = child.as_node()
                    && let Some(expr) = LuaExpr::cast(node.clone())
                {
                    let entry = SequenceEntry::Item(render_expr(ctx, plan, &expr));
                    if meet_assign {
                        rhs_entries.push(entry);
                    } else {
                        lhs_entries.push(entry);
                    }
                }
            }
        }
    }

    StatementAssignSplit {
        lhs_entries,
        assign_op,
        rhs_entries,
    }
}

fn collect_assign_stat_entries(
    ctx: &FormatContext,
    plan: &FormatPlan,
    stat: &LuaAssignStat,
) -> StatementAssignSplit {
    let mut lhs_entries = Vec::new();
    let mut rhs_entries = Vec::new();
    let mut assign_op = None;
    let mut meet_assign = false;

    for child in stat.syntax().children_with_tokens() {
        match child.kind() {
            LuaKind::Token(token_kind) if token_kind.is_assign_op() => {
                meet_assign = true;
                assign_op = child.as_token().cloned();
            }
            LuaKind::Token(LuaTokenKind::TkComma) => {
                let entry = separator_entry_from_token(plan, child.as_token());
                if meet_assign {
                    rhs_entries.push(entry);
                } else {
                    lhs_entries.push(entry);
                }
            }
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                if let Some(node) = child.as_node()
                    && let Some(comment) = LuaComment::cast(node.clone())
                {
                    if has_inline_non_trivia_before(comment.syntax())
                        && !has_inline_non_trivia_after(comment.syntax())
                    {
                        continue;
                    }
                    let entry = SequenceEntry::Comment(SequenceComment {
                        docs: vec![ir::source_node_trimmed(comment.syntax().clone())],
                        inline_after_previous: has_non_trivia_before_on_same_line_tokenwise(
                            comment.syntax(),
                        ),
                    });
                    if meet_assign {
                        rhs_entries.push(entry);
                    } else {
                        lhs_entries.push(entry);
                    }
                }
            }
            _ => {
                if let Some(node) = child.as_node() {
                    if !meet_assign {
                        if let Some(var) = LuaVarExpr::cast(node.clone()) {
                            lhs_entries.push(SequenceEntry::Item(render_expr(
                                ctx,
                                plan,
                                &var.into(),
                            )));
                        }
                    } else if let Some(expr) = LuaExpr::cast(node.clone()) {
                        rhs_entries.push(SequenceEntry::Item(render_expr(ctx, plan, &expr)));
                    }
                }
            }
        }
    }

    StatementAssignSplit {
        lhs_entries,
        assign_op,
        rhs_entries,
    }
}

fn collect_return_stat_entries(
    ctx: &FormatContext,
    plan: &FormatPlan,
    stat: &LuaReturnStat,
) -> Vec<SequenceEntry> {
    let mut entries = Vec::new();
    for child in stat.syntax().children_with_tokens() {
        match child.kind() {
            LuaKind::Token(LuaTokenKind::TkComma) => {
                entries.push(separator_entry_from_token(plan, child.as_token()));
            }
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                if let Some(node) = child.as_node()
                    && let Some(comment) = LuaComment::cast(node.clone())
                {
                    if has_inline_non_trivia_before(comment.syntax())
                        && !has_inline_non_trivia_after(comment.syntax())
                    {
                        continue;
                    }
                    entries.push(SequenceEntry::Comment(SequenceComment {
                        docs: vec![ir::source_node_trimmed(comment.syntax().clone())],
                        inline_after_previous: has_non_trivia_before_on_same_line_tokenwise(
                            comment.syntax(),
                        ),
                    }));
                }
            }
            _ => {
                if let Some(node) = child.as_node()
                    && let Some(expr) = LuaExpr::cast(node.clone())
                {
                    entries.push(SequenceEntry::Item(render_expr(ctx, plan, &expr)));
                }
            }
        }
    }
    entries
}

pub(crate) fn has_direct_comment_before_token(
    syntax: &LuaSyntaxNode,
    token: Option<&LuaSyntaxToken>,
) -> bool {
    let Some(token) = token else {
        return false;
    };

    let token_start = token.text_range().start();
    syntax.children_with_tokens().any(|child| {
        child.kind() == LuaKind::Syntax(LuaSyntaxKind::Comment)
            && child.text_range().start() < token_start
    })
}

pub(crate) fn render_header_exprs_with_leading_docs(
    ctx: &FormatContext,
    plan: &FormatPlan,
    expr_list_plan: StatementExprListLayoutPlan,
    leading_docs: Vec<DocIR>,
    comma_token: Option<&LuaSyntaxToken>,
    expr_docs: Vec<Vec<DocIR>>,
) -> Vec<DocIR> {
    let attach_first_multiline = expr_docs
        .first()
        .is_some_and(|docs| crate::ir::ir_has_forced_line_break(docs))
        || matches!(
            expr_list_plan.kind,
            StatementExprListLayoutKind::PreserveFirstMultiline
        );
    if attach_first_multiline {
        format_statement_expr_list_with_attached_first_multiline(
            comma_token,
            leading_docs,
            expr_docs,
        )
    } else {
        format_statement_expr_list(
            ctx,
            plan,
            expr_list_plan,
            comma_token,
            leading_docs,
            expr_docs,
        )
    }
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

fn format_statement_expr_list(
    ctx: &FormatContext,
    plan: &FormatPlan,
    expr_list_plan: StatementExprListLayoutPlan,
    comma_token: Option<&LuaSyntaxToken>,
    leading_docs: Vec<DocIR>,
    expr_docs: Vec<Vec<DocIR>>,
) -> Vec<DocIR> {
    if expr_docs.is_empty() {
        return Vec::new();
    }
    if expr_docs.len() == 1 {
        let mut docs = leading_docs;
        docs.extend(expr_docs.into_iter().next().unwrap_or_default());
        return docs;
    }

    let fill_parts = build_statement_expr_fill_parts(comma_token, leading_docs.clone(), &expr_docs);
    let packed = expr_list_plan
        .allow_packed
        .then(|| build_statement_expr_packed(plan, comma_token, leading_docs.clone(), &expr_docs));
    let one_per_line = expr_list_plan
        .allow_one_per_line
        .then(|| build_statement_expr_one_per_line(comma_token, leading_docs, &expr_docs));

    choose_sequence_layout(
        ctx,
        SequenceLayoutCandidates {
            fill: Some(vec![ir::group(vec![ir::indent(vec![ir::fill(
                fill_parts,
            )])])]),
            packed,
            one_per_line,
            ..Default::default()
        },
        SequenceLayoutPolicy {
            allow_alignment: false,
            allow_fill: expr_list_plan.allow_fill,
            allow_preserve: false,
            prefer_preserve_multiline: false,
            force_break_on_standalone_comments: false,
            prefer_balanced_break_lines: expr_list_plan.prefer_balanced_break_lines,
            first_line_prefix_width: expr_list_plan.first_line_prefix_width,
        },
    )
}

fn format_statement_expr_list_with_attached_first_multiline(
    comma_token: Option<&LuaSyntaxToken>,
    leading_docs: Vec<DocIR>,
    expr_docs: Vec<Vec<DocIR>>,
) -> Vec<DocIR> {
    if expr_docs.is_empty() {
        return Vec::new();
    }
    let mut docs = leading_docs;
    let mut iter = expr_docs.into_iter();
    let first_expr = iter.next().unwrap_or_default();
    docs.extend(first_expr);
    let remaining: Vec<Vec<DocIR>> = iter.collect();
    if remaining.is_empty() {
        return docs;
    }
    docs.extend(comma_token_docs(comma_token));
    let mut tail = Vec::new();
    let remaining_len = remaining.len();
    for (index, expr_doc) in remaining.into_iter().enumerate() {
        tail.push(ir::hard_line());
        tail.extend(expr_doc);
        if index + 1 < remaining_len {
            tail.extend(comma_token_docs(comma_token));
        }
    }
    docs.push(ir::indent(tail));
    docs
}

fn render_statement_exprs(
    ctx: &FormatContext,
    plan: &FormatPlan,
    expr_list_plan: StatementExprListLayoutPlan,
    leading_token: Option<&LuaSyntaxToken>,
    comma_token: Option<&LuaSyntaxToken>,
    expr_docs: Vec<Vec<DocIR>>,
) -> Vec<DocIR> {
    if expr_list_plan.attach_single_value_head {
        let mut docs = token_right_spacing_docs(plan, leading_token);
        docs.push(ir::list(expr_docs.into_iter().next().unwrap_or_default()));
        return docs;
    }

    let leading_docs = token_right_spacing_docs(plan, leading_token);
    if matches!(
        expr_list_plan.kind,
        StatementExprListLayoutKind::PreserveFirstMultiline
    ) {
        format_statement_expr_list_with_attached_first_multiline(
            comma_token,
            leading_docs,
            expr_docs,
        )
    } else {
        format_statement_expr_list(
            ctx,
            plan,
            expr_list_plan,
            comma_token,
            leading_docs,
            expr_docs,
        )
    }
}

fn build_statement_expr_fill_parts(
    comma_token: Option<&LuaSyntaxToken>,
    leading_docs: Vec<DocIR>,
    expr_docs: &[Vec<DocIR>],
) -> Vec<DocIR> {
    let mut parts = Vec::with_capacity(expr_docs.len().saturating_mul(2));
    let mut first_chunk = leading_docs;
    let Some((first_expr, remaining)) = expr_docs.split_first() else {
        return parts;
    };
    first_chunk.extend(first_expr.clone());
    parts.push(ir::list(first_chunk));
    for expr_doc in remaining {
        parts.push(ir::list(comma_fill_separator(comma_token)));
        parts.push(ir::list(expr_doc.clone()));
    }
    parts
}

fn build_statement_expr_one_per_line(
    comma_token: Option<&LuaSyntaxToken>,
    leading_docs: Vec<DocIR>,
    expr_docs: &[Vec<DocIR>],
) -> Vec<DocIR> {
    let mut docs = Vec::new();
    let mut first_chunk = leading_docs;
    let Some((first_expr, remaining)) = expr_docs.split_first() else {
        return vec![ir::group_break(vec![ir::indent(docs)])];
    };
    first_chunk.extend(first_expr.clone());
    docs.push(ir::list(first_chunk));
    for expr_doc in remaining {
        docs.push(ir::list(comma_token_docs(comma_token)));
        docs.push(ir::hard_line());
        docs.push(ir::list(expr_doc.clone()));
    }
    vec![ir::group_break(vec![ir::indent(docs)])]
}

fn build_statement_expr_packed(
    plan: &FormatPlan,
    comma_token: Option<&LuaSyntaxToken>,
    leading_docs: Vec<DocIR>,
    expr_docs: &[Vec<DocIR>],
) -> Vec<DocIR> {
    let mut docs = Vec::new();
    let mut first_chunk = leading_docs;
    let Some((first_expr, remaining)) = expr_docs.split_first() else {
        return vec![ir::group_break(vec![ir::indent(docs)])];
    };
    first_chunk.extend(first_expr.clone());
    if !remaining.is_empty() {
        first_chunk.extend(comma_token_docs(comma_token));
    }
    docs.push(ir::list(first_chunk));

    for (chunk_index, chunk) in remaining.chunks(2).enumerate() {
        let mut line = Vec::new();
        let chunk_start = chunk_index * 2;
        for (index, expr_doc) in chunk.iter().enumerate() {
            if index > 0 {
                line.extend(token_right_spacing_docs(plan, comma_token));
            }
            line.extend(expr_doc.clone());
            let absolute_index = chunk_start + index;
            let has_more = absolute_index + 1 < remaining.len();
            if has_more {
                line.extend(comma_token_docs(comma_token));
            }
        }
        docs.push(ir::hard_line());
        docs.push(ir::list(line));
    }
    vec![ir::group_break(vec![ir::indent(docs)])]
}

pub(crate) fn format_statement_value_expr(
    ctx: &FormatContext,
    plan: &FormatPlan,
    expr: &LuaExpr,
    preserve_first_multiline: bool,
    is_statement_tail: bool,
) -> Vec<DocIR> {
    if preserve_first_multiline {
        vec![ir::source_node_trimmed(expr.syntax().clone())]
    } else {
        render_expr_with_options(
            ctx,
            plan,
            expr,
            ExprFormatOptions {
                prefer_chain_break: is_statement_tail
                    && ctx.config.layout.prefer_chain_break_on_statement_tail,
            },
        )
    }
}

pub(crate) fn render_statement_align_split(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &FormatPlan,
) -> Option<DocPair> {
    match syntax_plan.kind {
        LuaSyntaxKind::LocalStat => {
            let node = find_node_by_id(root, syntax_plan.syntax_id)?;
            let stat = LuaLocalStat::cast(node)?;
            render_local_stat_align_split(ctx, plan, syntax_plan.syntax_id, &stat)
        }
        LuaSyntaxKind::AssignStat => {
            let node = find_node_by_id(root, syntax_plan.syntax_id)?;
            let stat = LuaAssignStat::cast(node)?;
            render_assign_stat_align_split(ctx, plan, syntax_plan.syntax_id, &stat)
        }
        _ => None,
    }
}

pub(crate) fn render_statement_line_content(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &FormatPlan,
) -> Option<Vec<DocIR>> {
    let (before, after) = render_statement_align_split(ctx, root, syntax_plan, plan)?;
    let mut docs = before;
    docs.push(ir::space());
    docs.extend(after);
    Some(docs)
}

fn render_local_stat_align_split(
    ctx: &FormatContext,
    plan: &FormatPlan,
    syntax_id: LuaSyntaxId,
    stat: &LuaLocalStat,
) -> Option<DocPair> {
    let exprs: Vec<_> = stat.get_value_exprs().collect();
    if exprs.is_empty() {
        return None;
    }

    let expr_list_plan = plan.layout.statement_expr_lists.get(&syntax_id).copied()?;
    let local_token = first_direct_token(stat.syntax(), LuaTokenKind::TkLocal);
    let comma_token = first_direct_token(stat.syntax(), LuaTokenKind::TkComma);
    let assign_token = first_direct_token(stat.syntax(), LuaTokenKind::TkAssign);

    let mut before = vec![token_or_kind_doc(
        local_token.as_ref(),
        LuaTokenKind::TkLocal,
    )];
    before.extend(token_right_spacing_docs(plan, local_token.as_ref()));
    let local_names: Vec<_> = stat.get_local_name_list().collect();
    for (index, local_name) in local_names.iter().enumerate() {
        if index > 0 {
            before.extend(comma_flat_separator(plan, comma_token.as_ref()));
        }
        before.extend(format_local_name_ir(local_name));
    }

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
                index + 1 == exprs.len(),
            )
        })
        .collect();

    let mut after = vec![token_or_kind_doc(
        assign_token.as_ref(),
        LuaTokenKind::TkAssign,
    )];
    after.extend(render_statement_exprs(
        ctx,
        plan,
        expr_list_plan,
        assign_token.as_ref(),
        comma_token.as_ref(),
        expr_docs,
    ));

    Some((before, after))
}

fn render_assign_stat_align_split(
    ctx: &FormatContext,
    plan: &FormatPlan,
    syntax_id: LuaSyntaxId,
    stat: &LuaAssignStat,
) -> Option<DocPair> {
    let (vars, exprs) = stat.get_var_and_expr_list();
    if exprs.is_empty() {
        return None;
    }

    let expr_list_plan = plan.layout.statement_expr_lists.get(&syntax_id).copied()?;
    let comma_token = first_direct_token(stat.syntax(), LuaTokenKind::TkComma);
    let assign_token = first_direct_token(stat.syntax(), LuaTokenKind::TkAssign);

    let mut before = Vec::new();
    for (index, var) in vars.iter().enumerate() {
        if index > 0 {
            before.extend(comma_flat_separator(plan, comma_token.as_ref()));
        }
        before.extend(render_expr(ctx, plan, &var.clone().into()));
    }

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
                index + 1 == exprs.len(),
            )
        })
        .collect();

    let mut after = vec![token_or_kind_doc(
        assign_token.as_ref(),
        LuaTokenKind::TkAssign,
    )];
    after.extend(render_statement_exprs(
        ctx,
        plan,
        expr_list_plan,
        assign_token.as_ref(),
        comma_token.as_ref(),
        expr_docs,
    ));

    Some((before, after))
}
