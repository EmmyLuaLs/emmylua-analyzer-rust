use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaAstToken, LuaChunk, LuaComment, LuaExpr, LuaForRangeStat,
    LuaForStat, LuaIfStat, LuaKind, LuaLocalName, LuaLocalStat, LuaRepeatStat, LuaReturnStat,
    LuaStat, LuaSyntaxId, LuaSyntaxKind, LuaSyntaxNode, LuaSyntaxToken, LuaTokenKind, LuaVarExpr,
    LuaWhileStat,
};

use crate::formatter::model::StatementExprListLayoutKind;
use crate::ir::{self, DocIR};

use super::FormatContext;
use super::expr;
use super::model::{LayoutNodePlan, RootFormatPlan, SyntaxNodeLayoutPlan, TokenSpacingExpected};
use super::sequence::{
    SequenceComment, SequenceEntry, SequenceLayoutCandidates, SequenceLayoutPolicy,
    choose_sequence_layout, render_sequence, sequence_ends_with_comment, sequence_has_comment,
    sequence_starts_with_inline_comment,
};
use super::trivia::{
    count_blank_lines_before, has_non_trivia_before_on_same_line_tokenwise,
    node_has_direct_comment_child,
};

pub fn render_root(ctx: &FormatContext, chunk: &LuaChunk, plan: &RootFormatPlan) -> Vec<DocIR> {
    let mut docs = Vec::new();

    if plan.spacing.has_shebang
        && let Some(first_token) = chunk.syntax().first_token()
    {
        docs.push(ir::text(first_token.text().to_string()));
        docs.push(DocIR::HardLine);
    }

    if !plan.layout.root_nodes.is_empty() {
        docs.extend(render_layout_nodes(
            ctx,
            chunk.syntax(),
            &plan.layout.root_nodes,
            plan,
            false,
        ));
    }

    if plan.line_breaks.insert_final_newline {
        docs.push(DocIR::HardLine);
    }

    docs
}

fn render_layout_nodes(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    nodes: &[LayoutNodePlan],
    plan: &RootFormatPlan,
    inside_block: bool,
) -> Vec<DocIR> {
    let mut docs = Vec::new();

    for (index, node) in nodes.iter().enumerate() {
        if inside_block && index > 0 {
            let blank_lines = count_blank_lines_before_layout_node(root, node)
                .min(ctx.config.layout.max_blank_lines);
            docs.push(ir::hard_line());
            for _ in 0..blank_lines {
                docs.push(ir::hard_line());
            }
        }

        docs.extend(render_layout_node(ctx, root, node, plan));
    }

    docs
}

fn render_layout_node(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    node: &LayoutNodePlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
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
                render_layout_nodes(ctx, root, &syntax_plan.children, plan, true)
            }
            LuaSyntaxKind::LocalStat => {
                render_local_stat_new(ctx, root, syntax_plan.syntax_id, plan)
            }
            LuaSyntaxKind::AssignStat => {
                render_assign_stat_new(ctx, root, syntax_plan.syntax_id, plan)
            }
            LuaSyntaxKind::ReturnStat => {
                render_return_stat_new(ctx, root, syntax_plan.syntax_id, plan)
            }
            LuaSyntaxKind::WhileStat => render_while_stat_new(ctx, root, syntax_plan, plan),
            LuaSyntaxKind::ForStat => render_for_stat_new(ctx, root, syntax_plan, plan),
            LuaSyntaxKind::ForRangeStat => render_for_range_stat_new(ctx, root, syntax_plan, plan),
            LuaSyntaxKind::RepeatStat => render_repeat_stat_new(ctx, root, syntax_plan, plan),
            LuaSyntaxKind::IfStat => render_if_stat_new(ctx, root, syntax_plan, plan),
            _ => render_unmigrated_syntax_leaf(root, syntax_plan.syntax_id),
        },
    }
}

struct StatementAssignSplit {
    lhs_entries: Vec<SequenceEntry>,
    assign_op: Option<LuaSyntaxToken>,
    rhs_entries: Vec<SequenceEntry>,
}

fn render_local_stat_new(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_id: LuaSyntaxId,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_id) else {
        return Vec::new();
    };
    let Some(stat) = LuaLocalStat::cast(node) else {
        return Vec::new();
    };

    if node_has_direct_comment_child(stat.syntax()) {
        return format_local_stat_trivia_aware_new(ctx, plan, &stat);
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
        docs.extend(format_local_name_ir_new(local_name));
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
                format_statement_value_expr_new(
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

        docs.extend(render_statement_exprs_new(
            ctx,
            plan,
            expr_list_plan,
            assign_token.as_ref(),
            comma_token.as_ref(),
            expr_docs,
        ));
    }

    docs
}

fn render_assign_stat_new(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_id: LuaSyntaxId,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_id) else {
        return Vec::new();
    };
    let Some(stat) = LuaAssignStat::cast(node) else {
        return Vec::new();
    };

    if node_has_direct_comment_child(stat.syntax()) {
        return format_assign_stat_trivia_aware_new(ctx, plan, &stat);
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
        .map(|var| render_expr_new(ctx, plan, &var.clone().into()))
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
            format_statement_value_expr_new(
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

    docs.extend(render_statement_exprs_new(
        ctx,
        plan,
        expr_list_plan,
        assign_token.as_ref(),
        comma_token.as_ref(),
        expr_docs,
    ));

    docs
}

fn render_return_stat_new(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_id: LuaSyntaxId,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_id) else {
        return Vec::new();
    };
    let Some(stat) = LuaReturnStat::cast(node) else {
        return Vec::new();
    };

    if node_has_direct_comment_child(stat.syntax()) {
        return format_return_stat_trivia_aware_new(ctx, plan, &stat);
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
                format_statement_value_expr_new(
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

        docs.extend(render_statement_exprs_new(
            ctx,
            plan,
            expr_list_plan,
            return_token.as_ref(),
            comma_token.as_ref(),
            expr_docs,
        ));
    }

    docs
}

fn render_while_stat_new(
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

    let while_token = first_direct_token(stat.syntax(), LuaTokenKind::TkWhile);
    let do_token = first_direct_token(stat.syntax(), LuaTokenKind::TkDo);
    let mut docs = vec![token_or_kind_doc(
        while_token.as_ref(),
        LuaTokenKind::TkWhile,
    )];

    if node_has_direct_comment_child(stat.syntax()) {
        let entries = collect_while_stat_entries_new(ctx, plan, &stat);
        if sequence_has_comment(&entries) {
            docs.extend(token_right_spacing_docs(plan, while_token.as_ref()));
            render_sequence(&mut docs, &entries, false);
            if !sequence_ends_with_comment(&entries) {
                docs.push(ir::hard_line());
            }
            docs.push(token_or_kind_doc(do_token.as_ref(), LuaTokenKind::TkDo));
        } else {
            docs.extend(token_right_spacing_docs(plan, while_token.as_ref()));
            render_sequence(&mut docs, &entries, false);
            docs.extend(token_left_spacing_docs(plan, do_token.as_ref()));
            docs.push(token_or_kind_doc(do_token.as_ref(), LuaTokenKind::TkDo));
        }
    } else {
        docs.extend(token_right_spacing_docs(plan, while_token.as_ref()));
        if let Some(cond) = stat.get_condition_expr() {
            docs.extend(render_expr_new(ctx, plan, &cond));
        }
        docs.extend(token_left_spacing_docs(plan, do_token.as_ref()));
        docs.push(token_or_kind_doc(do_token.as_ref(), LuaTokenKind::TkDo));
    }

    docs.extend(render_control_body_end_new(
        ctx,
        root,
        syntax_plan,
        plan,
        LuaTokenKind::TkEnd,
    ));
    docs
}

fn render_for_stat_new(
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

    let for_token = first_direct_token(stat.syntax(), LuaTokenKind::TkFor);
    let assign_token = first_direct_token(stat.syntax(), LuaTokenKind::TkAssign);
    let comma_token = first_direct_token(stat.syntax(), LuaTokenKind::TkComma);
    let do_token = first_direct_token(stat.syntax(), LuaTokenKind::TkDo);
    let mut docs = vec![token_or_kind_doc(for_token.as_ref(), LuaTokenKind::TkFor)];

    if node_has_direct_comment_child(stat.syntax()) {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    } else {
        docs.extend(token_right_spacing_docs(plan, for_token.as_ref()));
        if let Some(var_name) = stat.get_var_name() {
            docs.push(ir::source_token(var_name.syntax().clone()));
        }
        docs.extend(token_left_spacing_docs(plan, assign_token.as_ref()));
        docs.push(token_or_kind_doc(
            assign_token.as_ref(),
            LuaTokenKind::TkAssign,
        ));

        let iter_exprs: Vec<_> = stat.get_iter_expr().collect();
        let expr_list_plan = plan
            .layout
            .control_header_expr_lists
            .get(&syntax_plan.syntax_id)
            .copied()
            .expect("missing for header expr-list layout plan");
        let expr_docs: Vec<Vec<DocIR>> = iter_exprs
            .iter()
            .enumerate()
            .map(|(index, expr)| {
                format_statement_value_expr_new(
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
        docs.extend(render_header_exprs_new(
            ctx,
            plan,
            expr_list_plan,
            assign_token.as_ref(),
            comma_token.as_ref(),
            expr_docs,
        ));
        docs.extend(token_left_spacing_docs(plan, do_token.as_ref()));
        docs.push(token_or_kind_doc(do_token.as_ref(), LuaTokenKind::TkDo));
    }

    docs.extend(render_control_body_end_new(
        ctx,
        root,
        syntax_plan,
        plan,
        LuaTokenKind::TkEnd,
    ));
    docs
}

fn render_for_range_stat_new(
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

    let for_token = first_direct_token(stat.syntax(), LuaTokenKind::TkFor);
    let in_token = first_direct_token(stat.syntax(), LuaTokenKind::TkIn);
    let comma_token = first_direct_token(stat.syntax(), LuaTokenKind::TkComma);
    let do_token = first_direct_token(stat.syntax(), LuaTokenKind::TkDo);
    let mut docs = vec![token_or_kind_doc(for_token.as_ref(), LuaTokenKind::TkFor)];

    if node_has_direct_comment_child(stat.syntax()) {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    } else {
        docs.extend(token_right_spacing_docs(plan, for_token.as_ref()));
        let var_names: Vec<_> = stat.get_var_name_list().collect();
        for (index, name) in var_names.iter().enumerate() {
            if index > 0 {
                docs.extend(comma_flat_separator(plan, comma_token.as_ref()));
            }
            docs.push(ir::source_token(name.syntax().clone()));
        }
        docs.extend(token_left_spacing_docs(plan, in_token.as_ref()));
        docs.push(token_or_kind_doc(in_token.as_ref(), LuaTokenKind::TkIn));

        let exprs: Vec<_> = stat.get_expr_list().collect();
        let expr_list_plan = plan
            .layout
            .control_header_expr_lists
            .get(&syntax_plan.syntax_id)
            .copied()
            .expect("missing for-range header expr-list layout plan");
        let expr_docs: Vec<Vec<DocIR>> = exprs
            .iter()
            .enumerate()
            .map(|(index, expr)| {
                format_statement_value_expr_new(
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
        docs.extend(render_header_exprs_new(
            ctx,
            plan,
            expr_list_plan,
            in_token.as_ref(),
            comma_token.as_ref(),
            expr_docs,
        ));
        docs.extend(token_left_spacing_docs(plan, do_token.as_ref()));
        docs.push(token_or_kind_doc(do_token.as_ref(), LuaTokenKind::TkDo));
    }

    docs.extend(render_control_body_end_new(
        ctx,
        root,
        syntax_plan,
        plan,
        LuaTokenKind::TkEnd,
    ));
    docs
}

fn render_repeat_stat_new(
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

    let repeat_token = first_direct_token(stat.syntax(), LuaTokenKind::TkRepeat);
    let until_token = first_direct_token(stat.syntax(), LuaTokenKind::TkUntil);
    let has_inline_comment = plan
        .layout
        .control_headers
        .get(&syntax_plan.syntax_id)
        .is_some_and(|layout| layout.has_inline_comment);
    let mut docs = vec![token_or_kind_doc(
        repeat_token.as_ref(),
        LuaTokenKind::TkRepeat,
    )];

    docs.extend(render_control_body_new(ctx, root, syntax_plan, plan));
    docs.push(token_or_kind_doc(
        until_token.as_ref(),
        LuaTokenKind::TkUntil,
    ));

    if node_has_direct_comment_child(stat.syntax()) {
        let entries = collect_repeat_stat_entries_new(ctx, plan, &stat);
        let tail = render_trivia_aware_sequence_tail_new(
            plan,
            token_right_spacing_docs(plan, until_token.as_ref()),
            &entries,
        );
        if has_inline_comment {
            docs.push(ir::indent(tail));
        } else {
            docs.extend(tail);
        }
    } else if let Some(cond) = stat.get_condition_expr() {
        docs.extend(token_right_spacing_docs(plan, until_token.as_ref()));
        docs.extend(render_expr_new(ctx, plan, &cond));
    }

    docs
}

fn render_if_stat_new(
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

    if let Some(preserved) = try_preserve_single_line_if_body_new(ctx, &stat) {
        return preserved;
    }

    if should_preserve_raw_if_stat_new(&stat) {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    }

    let if_token = first_direct_token(stat.syntax(), LuaTokenKind::TkIf);
    let then_token = first_direct_token(stat.syntax(), LuaTokenKind::TkThen);
    let mut docs = vec![token_or_kind_doc(if_token.as_ref(), LuaTokenKind::TkIf)];
    docs.extend(token_right_spacing_docs(plan, if_token.as_ref()));
    if let Some(cond) = stat.get_condition_expr() {
        docs.extend(render_expr_new(ctx, plan, &cond));
    }
    docs.extend(token_left_spacing_docs(plan, then_token.as_ref()));
    docs.push(token_or_kind_doc(then_token.as_ref(), LuaTokenKind::TkThen));
    docs.extend(render_block_from_parent_plan_new(
        ctx,
        root,
        syntax_plan,
        plan,
    ));

    let else_if_plans: Vec<_> = syntax_plan
        .children
        .iter()
        .filter_map(|child| match child {
            LayoutNodePlan::Syntax(plan) if plan.kind == LuaSyntaxKind::ElseIfClauseStat => {
                Some(plan)
            }
            _ => None,
        })
        .collect();
    for (clause, clause_plan) in stat.get_else_if_clause_list().zip(else_if_plans) {
        let else_if_token = first_direct_token(clause.syntax(), LuaTokenKind::TkElseIf);
        let then_token = first_direct_token(clause.syntax(), LuaTokenKind::TkThen);
        docs.push(token_or_kind_doc(
            else_if_token.as_ref(),
            LuaTokenKind::TkElseIf,
        ));
        docs.extend(token_right_spacing_docs(plan, else_if_token.as_ref()));
        if let Some(cond) = clause.get_condition_expr() {
            docs.extend(render_expr_new(ctx, plan, &cond));
        }
        docs.extend(token_left_spacing_docs(plan, then_token.as_ref()));
        docs.push(token_or_kind_doc(then_token.as_ref(), LuaTokenKind::TkThen));
        docs.extend(render_block_from_parent_plan_new(
            ctx,
            root,
            clause_plan,
            plan,
        ));
    }

    if let Some(else_clause) = stat.get_else_clause() {
        let else_token = first_direct_token(else_clause.syntax(), LuaTokenKind::TkElse);
        docs.push(token_or_kind_doc(else_token.as_ref(), LuaTokenKind::TkElse));
        if let Some(else_plan) =
            find_direct_child_plan_by_kind(syntax_plan, LuaSyntaxKind::ElseClauseStat)
        {
            docs.extend(render_block_from_parent_plan_new(
                ctx, root, else_plan, plan,
            ));
        } else {
            docs.push(ir::hard_line());
        }
    }

    docs.push(ir::syntax_token(LuaTokenKind::TkEnd));
    docs
}

fn format_local_stat_trivia_aware_new(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    stat: &LuaLocalStat,
) -> Vec<DocIR> {
    let StatementAssignSplit {
        lhs_entries,
        assign_op,
        rhs_entries,
    } = collect_local_stat_entries_new(ctx, plan, stat);
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
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
        docs.push(ir::indent(render_trivia_aware_split_sequence_tail_new(
            plan,
            token_right_spacing_docs(plan, local_token.as_ref()),
            &lhs_entries,
            assign_op.as_ref(),
            &rhs_entries,
        )));
        return docs;
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

    docs
}

fn format_assign_stat_trivia_aware_new(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    stat: &LuaAssignStat,
) -> Vec<DocIR> {
    let StatementAssignSplit {
        lhs_entries,
        assign_op,
        rhs_entries,
    } = collect_assign_stat_entries_new(ctx, plan, stat);
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
    let has_inline_comment = plan
        .layout
        .statement_trivia
        .get(&syntax_id)
        .is_some_and(|layout| layout.has_inline_comment);

    if has_inline_comment {
        return vec![ir::indent(render_trivia_aware_split_sequence_tail_new(
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

    docs
}

fn format_return_stat_trivia_aware_new(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    stat: &LuaReturnStat,
) -> Vec<DocIR> {
    let entries = collect_return_stat_entries_new(ctx, plan, stat);
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
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
        docs.push(ir::indent(render_trivia_aware_sequence_tail_new(
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

    docs
}

fn collect_local_stat_entries_new(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
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
                    let entry = SequenceEntry::Item(format_local_name_ir_new(&local_name));
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
                    let entry = SequenceEntry::Item(render_expr_new(ctx, plan, &expr));
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

fn collect_assign_stat_entries_new(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
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
                            lhs_entries.push(SequenceEntry::Item(render_expr_new(
                                ctx,
                                plan,
                                &var.into(),
                            )));
                        }
                    } else if let Some(expr) = LuaExpr::cast(node.clone()) {
                        rhs_entries.push(SequenceEntry::Item(render_expr_new(ctx, plan, &expr)));
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

fn collect_return_stat_entries_new(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
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
                    entries.push(SequenceEntry::Item(render_expr_new(ctx, plan, &expr)));
                }
            }
        }
    }
    entries
}

fn collect_while_stat_entries_new(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    stat: &LuaWhileStat,
) -> Vec<SequenceEntry> {
    let mut entries = Vec::new();
    for child in stat.syntax().children_with_tokens() {
        match child.kind() {
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                if let Some(node) = child.as_node()
                    && let Some(comment) = LuaComment::cast(node.clone())
                {
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
                    entries.push(SequenceEntry::Item(render_expr_new(ctx, plan, &expr)));
                }
            }
        }
    }
    entries
}

fn collect_repeat_stat_entries_new(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    stat: &LuaRepeatStat,
) -> Vec<SequenceEntry> {
    let mut entries = Vec::new();
    for child in stat.syntax().children_with_tokens() {
        match child.kind() {
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                if let Some(node) = child.as_node()
                    && let Some(comment) = LuaComment::cast(node.clone())
                {
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
                    entries.push(SequenceEntry::Item(render_expr_new(ctx, plan, &expr)));
                }
            }
        }
    }
    entries
}

fn format_local_name_ir_new(local_name: &LuaLocalName) -> Vec<DocIR> {
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
    plan: &RootFormatPlan,
    expr_list_plan: super::model::StatementExprListLayoutPlan,
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

    let fill_parts =
        build_statement_expr_fill_parts_new(comma_token, leading_docs.clone(), expr_docs.clone());
    let packed = expr_list_plan.allow_packed.then(|| {
        build_statement_expr_packed_new(plan, comma_token, leading_docs.clone(), expr_docs.clone())
    });
    let one_per_line = expr_list_plan
        .allow_one_per_line
        .then(|| build_statement_expr_one_per_line_new(comma_token, leading_docs, expr_docs));

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

fn format_statement_expr_list_with_attached_first_multiline_new(
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

fn render_statement_exprs_new(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    expr_list_plan: super::model::StatementExprListLayoutPlan,
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
        format_statement_expr_list_with_attached_first_multiline_new(
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

fn render_header_exprs_new(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    expr_list_plan: super::model::StatementExprListLayoutPlan,
    leading_token: Option<&LuaSyntaxToken>,
    comma_token: Option<&LuaSyntaxToken>,
    expr_docs: Vec<Vec<DocIR>>,
) -> Vec<DocIR> {
    let leading_docs = token_right_spacing_docs(plan, leading_token);
    if matches!(
        expr_list_plan.kind,
        StatementExprListLayoutKind::PreserveFirstMultiline
    ) {
        format_statement_expr_list_with_attached_first_multiline_new(
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

fn build_statement_expr_fill_parts_new(
    comma_token: Option<&LuaSyntaxToken>,
    leading_docs: Vec<DocIR>,
    expr_docs: Vec<Vec<DocIR>>,
) -> Vec<DocIR> {
    let mut parts = Vec::with_capacity(expr_docs.len().saturating_mul(2));
    let mut expr_docs = expr_docs.into_iter();
    let mut first_chunk = leading_docs;
    first_chunk.extend(expr_docs.next().unwrap_or_default());
    parts.push(ir::list(first_chunk));
    for expr_doc in expr_docs {
        parts.push(ir::list(comma_fill_separator(comma_token)));
        parts.push(ir::list(expr_doc));
    }
    parts
}

fn build_statement_expr_one_per_line_new(
    comma_token: Option<&LuaSyntaxToken>,
    leading_docs: Vec<DocIR>,
    expr_docs: Vec<Vec<DocIR>>,
) -> Vec<DocIR> {
    let mut docs = Vec::new();
    let mut expr_docs = expr_docs.into_iter();
    let mut first_chunk = leading_docs;
    first_chunk.extend(expr_docs.next().unwrap_or_default());
    docs.push(ir::list(first_chunk));
    for expr_doc in expr_docs {
        docs.push(ir::list(comma_token_docs(comma_token)));
        docs.push(ir::hard_line());
        docs.push(ir::list(expr_doc));
    }
    vec![ir::group_break(vec![ir::indent(docs)])]
}

fn build_statement_expr_packed_new(
    plan: &RootFormatPlan,
    comma_token: Option<&LuaSyntaxToken>,
    leading_docs: Vec<DocIR>,
    expr_docs: Vec<Vec<DocIR>>,
) -> Vec<DocIR> {
    let mut docs = Vec::new();
    let mut expr_docs = expr_docs.into_iter().peekable();
    let mut first_chunk = leading_docs;
    first_chunk.extend(expr_docs.next().unwrap_or_default());
    if expr_docs.peek().is_some() {
        first_chunk.extend(comma_token_docs(comma_token));
    }
    docs.push(ir::list(first_chunk));
    let mut remaining = Vec::new();
    while let Some(expr_doc) = expr_docs.next() {
        let has_more = expr_docs.peek().is_some();
        remaining.push((expr_doc, has_more));
    }
    for chunk in remaining.chunks(2) {
        let mut line = Vec::new();
        for (index, (expr_doc, has_more)) in chunk.iter().enumerate() {
            if index > 0 {
                line.extend(token_right_spacing_docs(plan, comma_token));
            }
            line.extend(expr_doc.clone());
            if *has_more {
                line.extend(comma_token_docs(comma_token));
            }
        }
        docs.push(ir::hard_line());
        docs.push(ir::list(line));
    }
    vec![ir::group_break(vec![ir::indent(docs)])]
}

fn is_block_like_expr_new(expr: &LuaExpr) -> bool {
    matches!(expr, LuaExpr::ClosureExpr(_) | LuaExpr::TableExpr(_))
}

fn try_preserve_single_line_if_body_new(
    ctx: &FormatContext,
    stat: &LuaIfStat,
) -> Option<Vec<DocIR>> {
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

    if !is_simple_single_line_if_body_new(&only_stat) {
        return None;
    }

    Some(vec![ir::source_node(stat.syntax().clone())])
}

fn is_simple_single_line_if_body_new(stat: &LuaStat) -> bool {
    match stat {
        LuaStat::ReturnStat(_)
        | LuaStat::BreakStat(_)
        | LuaStat::GotoStat(_)
        | LuaStat::CallExprStat(_) => true,
        LuaStat::LocalStat(local) => {
            let exprs: Vec<_> = local.get_value_exprs().collect();
            exprs.len() <= 1 && exprs.iter().all(|expr| !is_block_like_expr_new(expr))
        }
        LuaStat::AssignStat(assign) => {
            let (_, exprs) = assign.get_var_and_expr_list();
            exprs.len() <= 1 && exprs.iter().all(|expr| !is_block_like_expr_new(expr))
        }
        _ => false,
    }
}

fn should_preserve_raw_if_stat_new(stat: &LuaIfStat) -> bool {
    if node_has_direct_comment_child(stat.syntax()) {
        return true;
    }

    if stat
        .get_else_if_clause_list()
        .clone()
        .any(|clause| node_has_direct_comment_child(clause.syntax()))
    {
        return true;
    }

    if stat
        .get_else_clause()
        .is_some_and(|clause| node_has_direct_comment_child(clause.syntax()))
    {
        return true;
    }

    stat.get_else_if_clause_list().next().is_some()
        && syntax_has_descendant_comment_new(stat.syntax())
}

fn syntax_has_descendant_comment_new(syntax: &LuaSyntaxNode) -> bool {
    syntax
        .descendants()
        .any(|node| node.kind() == LuaKind::Syntax(LuaSyntaxKind::Comment))
}

fn format_statement_value_expr_new(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    expr: &LuaExpr,
    preserve_first_multiline: bool,
) -> Vec<DocIR> {
    if preserve_first_multiline {
        vec![ir::source_node_trimmed(expr.syntax().clone())]
    } else {
        render_expr_new(ctx, plan, expr)
    }
}

fn render_unmigrated_syntax_leaf(root: &LuaSyntaxNode, syntax_id: LuaSyntaxId) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_id) else {
        return Vec::new();
    };

    vec![ir::source_node_trimmed(node)]
}

fn render_control_body_end_new(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
    end_kind: LuaTokenKind,
) -> Vec<DocIR> {
    let mut docs = render_control_body_new(ctx, root, syntax_plan, plan);
    docs.push(ir::syntax_token(end_kind));
    docs
}

fn render_control_body_new(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let block_children = syntax_plan.children.iter().find_map(|child| match child {
        LayoutNodePlan::Syntax(block) if block.kind == LuaSyntaxKind::Block => {
            Some(block.children.as_slice())
        }
        _ => None,
    });

    render_block_children_new(ctx, root, block_children, plan)
}

fn render_block_from_parent_plan_new(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let block_children = syntax_plan.children.iter().find_map(|child| match child {
        LayoutNodePlan::Syntax(block) if block.kind == LuaSyntaxKind::Block => {
            Some(block.children.as_slice())
        }
        _ => None,
    });

    render_block_children_new(ctx, root, block_children, plan)
}

fn render_block_children_new(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    block_children: Option<&[LayoutNodePlan]>,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let mut docs = Vec::new();

    if let Some(children) = block_children {
        if !children.is_empty() {
            let mut body = vec![ir::hard_line()];
            body.extend(render_layout_nodes(ctx, root, children, plan, true));
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

fn render_expr_new(_ctx: &FormatContext, plan: &RootFormatPlan, expr: &LuaExpr) -> Vec<DocIR> {
    expr::format_expr(_ctx, plan, expr)
}

fn find_direct_child_plan_by_kind(
    syntax_plan: &SyntaxNodeLayoutPlan,
    kind: LuaSyntaxKind,
) -> Option<&SyntaxNodeLayoutPlan> {
    syntax_plan.children.iter().find_map(|child| match child {
        LayoutNodePlan::Syntax(plan) if plan.kind == kind => Some(plan),
        _ => None,
    })
}

fn token_or_kind_doc(token: Option<&LuaSyntaxToken>, fallback_kind: LuaTokenKind) -> DocIR {
    token
        .map(|token| ir::source_token(token.clone()))
        .unwrap_or_else(|| ir::syntax_token(fallback_kind))
}

fn first_direct_token(node: &LuaSyntaxNode, kind: LuaTokenKind) -> Option<LuaSyntaxToken> {
    node.children_with_tokens().find_map(|element| {
        let token = element.into_token()?;
        (token.kind().to_token() == kind).then_some(token)
    })
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

fn separator_entry_from_token(
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

fn render_trivia_aware_sequence_tail_new(
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

fn render_trivia_aware_split_sequence_tail_new(
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

fn render_comment_with_spacing(
    _ctx: &FormatContext,
    comment: &LuaComment,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    if should_preserve_comment_raw(comment) {
        return vec![ir::source_node_trimmed(comment.syntax().clone())];
    }

    let lines = collect_comment_render_lines(comment, plan);
    lines
        .into_iter()
        .enumerate()
        .flat_map(|(index, line)| {
            let mut docs = Vec::new();
            if index > 0 {
                docs.push(ir::hard_line());
            }
            if !line.is_empty() {
                docs.push(ir::text(line));
            }
            docs
        })
        .collect()
}

#[derive(Default)]
struct RenderCommentLine {
    tokens: Vec<(LuaSyntaxId, String)>,
    gaps: Vec<String>,
}

fn collect_comment_render_lines(comment: &LuaComment, plan: &RootFormatPlan) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = RenderCommentLine::default();
    let mut pending_gap = String::new();
    let mut ended_with_newline = false;

    for element in comment.syntax().descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };

        match token.kind().to_token() {
            LuaTokenKind::TkWhitespace => pending_gap.push_str(token.text()),
            LuaTokenKind::TkEndOfLine => {
                apply_comment_spacing_line(plan, &mut current);
                lines.push(render_comment_line(current));
                current = RenderCommentLine::default();
                pending_gap.clear();
                ended_with_newline = true;
            }
            _ => {
                let syntax_id = LuaSyntaxId::from_token(&token);
                if !current.tokens.is_empty() {
                    current.gaps.push(std::mem::take(&mut pending_gap));
                } else {
                    pending_gap.clear();
                }
                let text = plan
                    .spacing
                    .token_replace(syntax_id)
                    .map(str::to_string)
                    .unwrap_or_else(|| token.text().to_string());
                current.tokens.push((syntax_id, text));
                ended_with_newline = false;
            }
        }
    }

    if !current.tokens.is_empty() || ended_with_newline {
        apply_comment_spacing_line(plan, &mut current);
        lines.push(render_comment_line(current));
    }

    lines
}

fn apply_comment_spacing_line(plan: &RootFormatPlan, line: &mut RenderCommentLine) {
    for index in 0..line.gaps.len() {
        let prev_id = line.tokens[index].0;
        let token_id = line.tokens[index + 1].0;
        line.gaps[index] = resolve_comment_gap(plan, Some(prev_id), token_id, &line.gaps[index]);
    }
}

fn resolve_comment_gap(
    plan: &RootFormatPlan,
    prev_token_id: Option<LuaSyntaxId>,
    token_id: LuaSyntaxId,
    gap: &str,
) -> String {
    let mut exact_space = None;
    let mut max_space = None;

    if let Some(prev_token_id) = prev_token_id
        && let Some(expected) = plan.spacing.right_expected(prev_token_id)
    {
        match expected {
            TokenSpacingExpected::Space(count) => exact_space = Some(*count),
            TokenSpacingExpected::MaxSpace(count) => max_space = Some(*count),
        }
    }

    if let Some(expected) = plan.spacing.left_expected(token_id) {
        match expected {
            TokenSpacingExpected::Space(count) => {
                exact_space = Some(exact_space.map_or(*count, |current| current.max(*count)));
            }
            TokenSpacingExpected::MaxSpace(count) => {
                max_space = Some(max_space.map_or(*count, |current| current.min(*count)));
            }
        }
    }

    if let Some(exact_space) = exact_space {
        return " ".repeat(exact_space);
    }
    if let Some(max_space) = max_space {
        let original_space_count = gap.chars().take_while(|ch| *ch == ' ').count();
        return " ".repeat(original_space_count.min(max_space));
    }

    gap.to_string()
}

fn render_comment_line(line: RenderCommentLine) -> String {
    let mut tokens = line.tokens.into_iter();
    let Some((_, first_text)) = tokens.next() else {
        return String::new();
    };

    let mut rendered = first_text;
    for (gap, (_, token_text)) in line.gaps.into_iter().zip(tokens) {
        rendered.push_str(&gap);
        rendered.push_str(&token_text);
    }
    rendered
}

fn should_preserve_comment_raw(comment: &LuaComment) -> bool {
    let Some(first_token) = comment.syntax().first_token() else {
        return false;
    };

    matches!(
        first_token.kind().to_token(),
        LuaTokenKind::TkLongCommentStart | LuaTokenKind::TkDocLongStart
    ) || dash_prefix_len(first_token.text()) > 3
}

fn dash_prefix_len(prefix_text: &str) -> usize {
    prefix_text.bytes().take_while(|byte| *byte == b'-').count()
}

fn count_blank_lines_before_layout_node(root: &LuaSyntaxNode, node: &LayoutNodePlan) -> usize {
    let syntax_id = match node {
        LayoutNodePlan::Comment(comment) => comment.syntax_id,
        LayoutNodePlan::Syntax(syntax) => syntax.syntax_id,
    };
    let Some(node) = find_node_by_id(root, syntax_id) else {
        return 0;
    };

    count_blank_lines_before(&node)
}

fn find_node_by_id(root: &LuaSyntaxNode, syntax_id: LuaSyntaxId) -> Option<LuaSyntaxNode> {
    if LuaSyntaxId::from_node(root) == syntax_id {
        return Some(root.clone());
    }

    root.descendants()
        .find(|node| LuaSyntaxId::from_node(node) == syntax_id)
}
