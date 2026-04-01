use std::collections::HashMap;

use crate::formatter::model::{StatementExprListLayoutKind, StatementExprListLayoutPlan};
use crate::ir::{self, AlignEntry, DocIR};
use emmylua_parser::*;
use rowan::TextRange;

use super::FormatContext;
use crate::formatter::model::{
    LayoutNodePlan, RootFormatPlan, SyntaxNodeLayoutPlan, TokenSpacingExpected,
};
use crate::formatter::sequence::*;
use crate::formatter::trivia::*;

mod control;
mod helpers;

use self::control::{
    render_do_stat, render_for_range_stat, render_for_stat, render_func_stat, render_if_stat,
    render_local_func_stat, render_repeat_stat, render_while_stat,
};
use self::helpers::*;

pub fn render_root(ctx: &FormatContext, chunk: &LuaChunk, plan: &RootFormatPlan) -> Vec<DocIR> {
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
    expr: &emmylua_parser::LuaClosureExpr,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let root = expr
        .syntax()
        .ancestors()
        .last()
        .unwrap_or_else(|| expr.syntax().clone());
    let closure_id = LuaSyntaxId::from_node(expr.syntax());
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
                render_aligned_block_layout_nodes(ctx, root, &syntax_plan.children, plan)
            }
            LuaSyntaxKind::LocalStat => render_local_stat(ctx, root, syntax_plan.syntax_id, plan),
            LuaSyntaxKind::AssignStat => render_assign_stat(ctx, root, syntax_plan.syntax_id, plan),
            LuaSyntaxKind::ReturnStat => render_return_stat(ctx, root, syntax_plan.syntax_id, plan),
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
            _ => render_unmigrated_syntax_leaf(root, syntax_plan.syntax_id),
        },
    }
}

struct StatementAssignSplit {
    lhs_entries: Vec<SequenceEntry>,
    assign_op: Option<LuaSyntaxToken>,
    rhs_entries: Vec<SequenceEntry>,
}

type DocPair = (Vec<DocIR>, Vec<DocIR>);
type RenderedTrailingComment = (Vec<DocIR>, TextRange, bool);

fn render_local_stat(
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

    append_trailing_comment_suffix(ctx, plan, &mut docs, stat.syntax());

    docs
}

fn render_assign_stat(
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

    append_trailing_comment_suffix(ctx, plan, &mut docs, stat.syntax());

    docs
}

fn render_return_stat(
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

    append_trailing_comment_suffix(ctx, plan, &mut docs, stat.syntax());

    docs
}

fn render_call_expr_stat(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_id: LuaSyntaxId,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let Some(node) = find_node_by_id(root, syntax_id) else {
        return Vec::new();
    };
    let Some(stat) = LuaCallExprStat::cast(node) else {
        return Vec::new();
    };

    let mut docs = stat
        .get_call_expr()
        .map(|expr| render_expr(ctx, plan, &expr.into()))
        .unwrap_or_default();
    append_trailing_comment_suffix(ctx, plan, &mut docs, stat.syntax());
    docs
}

fn format_local_stat_trivia_aware(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    stat: &LuaLocalStat,
) -> Vec<DocIR> {
    let StatementAssignSplit {
        lhs_entries,
        assign_op,
        rhs_entries,
    } = collect_local_stat_entries(ctx, plan, stat);
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

    append_trailing_comment_suffix(ctx, plan, &mut docs, stat.syntax());

    docs
}

fn format_assign_stat_trivia_aware(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    stat: &LuaAssignStat,
) -> Vec<DocIR> {
    let StatementAssignSplit {
        lhs_entries,
        assign_op,
        rhs_entries,
    } = collect_assign_stat_entries(ctx, plan, stat);
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
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

    append_trailing_comment_suffix(ctx, plan, &mut docs, stat.syntax());

    docs
}

fn format_return_stat_trivia_aware(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    stat: &LuaReturnStat,
) -> Vec<DocIR> {
    let entries = collect_return_stat_entries(ctx, plan, stat);
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

    append_trailing_comment_suffix(ctx, plan, &mut docs, stat.syntax());

    docs
}

fn collect_local_stat_entries(
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

fn has_direct_comment_before_token(syntax: &LuaSyntaxNode, token: Option<&LuaSyntaxToken>) -> bool {
    let Some(token) = token else {
        return false;
    };

    let token_start = token.text_range().start();
    syntax.children_with_tokens().any(|child| {
        child.kind() == LuaKind::Syntax(LuaSyntaxKind::Comment)
            && child.text_range().start() < token_start
    })
}

fn render_header_exprs_with_leading_docs(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
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
        build_statement_expr_fill_parts(comma_token, leading_docs.clone(), expr_docs.clone());
    let packed = expr_list_plan.allow_packed.then(|| {
        build_statement_expr_packed(plan, comma_token, leading_docs.clone(), expr_docs.clone())
    });
    let one_per_line = expr_list_plan
        .allow_one_per_line
        .then(|| build_statement_expr_one_per_line(comma_token, leading_docs, expr_docs));

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

fn build_statement_expr_one_per_line(
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

fn build_statement_expr_packed(
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

fn format_statement_value_expr(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    expr: &LuaExpr,
    preserve_first_multiline: bool,
) -> Vec<DocIR> {
    if preserve_first_multiline {
        vec![ir::source_node_trimmed(expr.syntax().clone())]
    } else {
        render_expr(ctx, plan, expr)
    }
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
    plan: &RootFormatPlan,
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

fn render_direct_body_comment(
    comment: LuaComment,
    ctx: &FormatContext,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    vec![
        ir::indent({
            let mut docs = vec![ir::hard_line()];
            docs.extend(render_comment_with_spacing(ctx, &comment, plan));
            docs
        }),
        ir::hard_line(),
    ]
}

fn comment_is_inline_after_anchor(
    root: &LuaSyntaxNode,
    anchor_token: Option<&LuaSyntaxToken>,
    comment: &LuaSyntaxNode,
) -> bool {
    let Some(anchor_token) = anchor_token else {
        return false;
    };

    let start: usize = anchor_token.text_range().end().into();
    let end: usize = comment.text_range().start().into();
    if end < start {
        return false;
    }

    let text = root.text().to_string();
    !text[start..end].chars().any(|ch| matches!(ch, '\n' | '\r'))
}

fn render_block_children(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    block_children: Option<&[LayoutNodePlan]>,
    plan: &RootFormatPlan,
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
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    let mut docs = Vec::new();
    let mut index = 0usize;

    while index < nodes.len() {
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

fn try_render_aligned_statement_group(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    nodes: &[LayoutNodePlan],
    start: usize,
    plan: &RootFormatPlan,
) -> Option<(Vec<DocIR>, usize)> {
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
    plan: &RootFormatPlan,
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

fn render_statement_align_split(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
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

fn render_statement_line_content(
    ctx: &FormatContext,
    root: &LuaSyntaxNode,
    syntax_plan: &SyntaxNodeLayoutPlan,
    plan: &RootFormatPlan,
) -> Option<Vec<DocIR>> {
    let (before, after) = render_statement_align_split(ctx, root, syntax_plan, plan)?;
    let mut docs = before;
    docs.push(ir::space());
    docs.extend(after);
    Some(docs)
}

fn render_local_stat_align_split(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
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
    plan: &RootFormatPlan,
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

fn extract_trailing_comment_rendered(
    ctx: &FormatContext,
    syntax_plan: &SyntaxNodeLayoutPlan,
    node: &LuaSyntaxNode,
    plan: &RootFormatPlan,
) -> Option<RenderedTrailingComment> {
    let comment = find_inline_trailing_comment_node(node)?;
    if comment.text().contains_char('\n') {
        return None;
    }
    let comment = LuaComment::cast(comment.clone())?;
    let docs = render_comment_with_spacing(ctx, &comment, plan);
    let align_hint = matches!(
        syntax_plan.kind,
        LuaSyntaxKind::LocalStat | LuaSyntaxKind::AssignStat
    ) && trailing_gap_requests_alignment(
        node,
        comment.syntax().text_range(),
        ctx.config.comments.line_comment_min_spaces_before.max(1),
    );
    Some((docs, comment.syntax().text_range(), align_hint))
}

fn append_trailing_comment_suffix(
    ctx: &FormatContext,
    plan: &RootFormatPlan,
    docs: &mut Vec<DocIR>,
    node: &LuaSyntaxNode,
) {
    let Some(comment_node) = find_inline_trailing_comment_node(node) else {
        return;
    };
    let Some(comment) = LuaComment::cast(comment_node) else {
        return;
    };

    let content_width = crate::ir::ir_flat_width(docs);
    let padding = if ctx.config.comments.line_comment_min_column == 0 {
        ctx.config.comments.line_comment_min_spaces_before.max(1)
    } else {
        ctx.config
            .comments
            .line_comment_min_spaces_before
            .max(1)
            .max(
                ctx.config
                    .comments
                    .line_comment_min_column
                    .saturating_sub(content_width),
            )
    };
    let mut suffix = (0..padding).map(|_| ir::space()).collect::<Vec<_>>();
    suffix.extend(render_comment_with_spacing(ctx, &comment, plan));
    docs.push(ir::line_suffix(suffix));
}

fn find_inline_trailing_comment_node(node: &LuaSyntaxNode) -> Option<LuaSyntaxNode> {
    for child in node.children() {
        if child.kind() != LuaKind::Syntax(LuaSyntaxKind::Comment) {
            continue;
        }

        if has_inline_non_trivia_before(&child) && !has_inline_non_trivia_after(&child) {
            return Some(child);
        }
    }

    let mut next = node.next_sibling_or_token();
    for _ in 0..4 {
        let sibling = next.as_ref()?;
        match sibling.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace)
            | LuaKind::Token(LuaTokenKind::TkSemicolon)
            | LuaKind::Token(LuaTokenKind::TkComma) => {}
            LuaKind::Syntax(LuaSyntaxKind::Comment) => return sibling.as_node().cloned(),
            _ => return None,
        }
        next = sibling.next_sibling_or_token();
    }

    None
}

fn has_inline_non_trivia_before(node: &LuaSyntaxNode) -> bool {
    let mut previous = node.prev_sibling_or_token();
    while let Some(element) = previous {
        match element.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace) => {
                previous = element.prev_sibling_or_token()
            }
            LuaKind::Token(LuaTokenKind::TkEndOfLine) => return false,
            LuaKind::Syntax(LuaSyntaxKind::Comment) => previous = element.prev_sibling_or_token(),
            _ => return true,
        }
    }
    false
}

fn has_inline_non_trivia_after(node: &LuaSyntaxNode) -> bool {
    let mut next = node.next_sibling_or_token();
    while let Some(element) = next {
        match element.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace) => next = element.next_sibling_or_token(),
            LuaKind::Token(LuaTokenKind::TkEndOfLine) => return false,
            LuaKind::Syntax(LuaSyntaxKind::Comment) => next = element.next_sibling_or_token(),
            _ => return true,
        }
    }
    false
}

fn render_comment_with_spacing(
    ctx: &FormatContext,
    comment: &LuaComment,
    plan: &RootFormatPlan,
) -> Vec<DocIR> {
    if should_preserve_comment_raw(comment) || should_preserve_doc_comment_block_raw(comment) {
        return vec![ir::source_node_trimmed(comment.syntax().clone())];
    }

    let raw = trim_end_comment_text(comment.syntax().text().to_string());
    let prefix_replacements = collect_comment_line_prefix_replacements(comment, plan);
    let normalized_lines = collect_comment_line_spacing_normalized_texts(comment, plan);
    let lines = if raw.starts_with("---") {
        normalize_doc_comment_block(ctx, &raw, &prefix_replacements, normalized_lines.as_slice())
    } else {
        normalize_normal_comment_block(ctx, &raw, &prefix_replacements, normalized_lines.as_slice())
    };
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

fn trim_end_comment_text(mut text: String) -> String {
    while matches!(text.chars().last(), Some(' ' | '\t' | '\r' | '\n')) {
        text.pop();
    }
    text
}

fn collect_comment_line_prefix_replacements(
    comment: &LuaComment,
    plan: &RootFormatPlan,
) -> Vec<Option<String>> {
    let mut line_prefixes = Vec::new();
    let mut current_prefix = None;
    let mut saw_token_on_line = false;

    for element in comment.syntax().descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };

        match token.kind().to_token() {
            LuaTokenKind::TkWhitespace => {}
            LuaTokenKind::TkEndOfLine => {
                line_prefixes.push(current_prefix.take());
                saw_token_on_line = false;
            }
            _ => {
                if !saw_token_on_line {
                    current_prefix = comment_prefix_replacement_for_token(plan, &token);
                    saw_token_on_line = true;
                }
            }
        }
    }

    if saw_token_on_line || current_prefix.is_some() {
        line_prefixes.push(current_prefix);
    }

    line_prefixes
}

fn comment_prefix_replacement_for_token(
    plan: &RootFormatPlan,
    token: &LuaSyntaxToken,
) -> Option<String> {
    match token.kind().to_token() {
        LuaTokenKind::TkNormalStart
        | LuaTokenKind::TkDocStart
        | LuaTokenKind::TkDocContinue
        | LuaTokenKind::TkDocContinueOr => Some(
            plan.spacing
                .token_replace(LuaSyntaxId::from_token(token))
                .unwrap_or(token.text())
                .to_string(),
        ),
        _ => None,
    }
}

fn normalize_normal_comment_block(
    ctx: &FormatContext,
    raw: &str,
    prefix_replacements: &[Option<String>],
    normalized_lines: &[Option<String>],
) -> Vec<String> {
    let lines: Vec<_> = raw.lines().collect();
    if lines.len() <= 1 {
        return vec![normalize_single_normal_comment_line(
            ctx,
            raw,
            prefix_replacements
                .first()
                .and_then(|prefix| prefix.as_deref()),
            normalized_lines.first().and_then(|line| line.as_deref()),
        )];
    }
    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let trimmed = line.trim_start();
            if trimmed.is_empty() {
                String::new()
            } else {
                normalize_single_normal_comment_line(
                    ctx,
                    trimmed,
                    prefix_replacements
                        .get(index)
                        .and_then(|prefix| prefix.as_deref()),
                    normalized_lines.get(index).and_then(|line| line.as_deref()),
                )
            }
        })
        .collect()
}

fn normalize_single_normal_comment_line(
    ctx: &FormatContext,
    line: &str,
    prefix_override: Option<&str>,
    normalized_line: Option<&str>,
) -> String {
    if !line.starts_with("--") || line.starts_with("---") {
        return line.to_string();
    }
    if let Some(normalized_line) = normalized_line {
        return normalized_line.to_string();
    }
    let prefix = prefix_override.map(str::to_string).unwrap_or_else(|| {
        if ctx.config.comments.space_after_comment_dash {
            "-- ".to_string()
        } else {
            "--".to_string()
        }
    });
    let body = line[2..].trim_start();
    if body.is_empty() {
        prefix.trim_end().to_string()
    } else {
        format!("{prefix}{body}")
    }
}

#[derive(Clone)]
enum DocLineKind {
    Description {
        content: String,
        preserve_spacing: bool,
    },
    ContinueOr {
        content: String,
    },
    Tag(DocTagLine),
}

#[derive(Clone)]
struct DocTagLine {
    tag: String,
    raw_rest: String,
    columns: Vec<String>,
    align_key: Option<String>,
    preserve_body_spacing: bool,
}

fn should_preserve_doc_comment_block_raw(comment: &LuaComment) -> bool {
    let raw = comment.syntax().text().to_string();
    raw.lines().any(|line| {
        let trimmed = line.trim_start();
        (trimmed.starts_with("---@type") || trimmed.starts_with("--- @type"))
            && trimmed.contains(" --")
    })
}

fn normalize_doc_comment_block(
    ctx: &FormatContext,
    raw: &str,
    prefix_replacements: &[Option<String>],
    normalized_lines: &[Option<String>],
) -> Vec<String> {
    let raw_lines: Vec<&str> = raw.lines().collect();
    let parsed: Vec<DocLineKind> = raw_lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            parse_doc_comment_line(
                ctx,
                line,
                normalized_lines.get(index).and_then(|line| line.as_deref()),
                index == 0,
                raw_lines.len() == 1,
            )
        })
        .collect();

    let mut widths: HashMap<String, Vec<usize>> = HashMap::new();
    for line in &parsed {
        let DocLineKind::Tag(tag) = line else {
            continue;
        };
        let Some(key) = &tag.align_key else {
            continue;
        };
        let entry = widths
            .entry(key.clone())
            .or_insert_with(|| vec![0; tag.columns.len().saturating_sub(1)]);
        if entry.len() < tag.columns.len().saturating_sub(1) {
            entry.resize(tag.columns.len().saturating_sub(1), 0);
        }
        for (index, column) in tag
            .columns
            .iter()
            .take(tag.columns.len().saturating_sub(1))
            .enumerate()
        {
            entry[index] = entry[index].max(column.len());
        }
    }

    parsed
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            format_doc_comment_line(
                ctx,
                line,
                &widths,
                prefix_replacements
                    .get(index)
                    .and_then(|prefix| prefix.as_deref()),
            )
        })
        .collect()
}

fn parse_doc_comment_line(
    ctx: &FormatContext,
    line: &str,
    normalized_line: Option<&str>,
    is_first_line: bool,
    single_line_block: bool,
) -> DocLineKind {
    let suffix = line.strip_prefix("---").unwrap_or(line);
    let trimmed = suffix.trim_start();
    let normalized = normalized_line.unwrap_or(line);
    let normalized_suffix = normalized.strip_prefix("---").unwrap_or(normalized);
    let normalized_trimmed = normalized_suffix.trim_start();

    if let Some(rest) = trimmed.strip_prefix('@') {
        let normalized_rest = normalized_trimmed
            .strip_prefix('@')
            .unwrap_or(rest)
            .trim_start();
        return DocLineKind::Tag(parse_doc_tag_line(ctx, normalized_rest, rest.trim_start()));
    }
    if let Some(rest) = trimmed.strip_prefix('|') {
        return DocLineKind::ContinueOr {
            content: normalized_trimmed
                .strip_prefix('|')
                .unwrap_or(rest)
                .trim_start()
                .to_string(),
        };
    }

    let preserve_spacing = !single_line_block && !is_first_line;
    let content = if preserve_spacing {
        suffix.to_string()
    } else {
        collapse_spaces(trimmed)
    };
    DocLineKind::Description {
        content,
        preserve_spacing,
    }
}

fn parse_doc_tag_line(ctx: &FormatContext, rest: &str, raw_rest_source: &str) -> DocTagLine {
    let mut parts = rest.split_whitespace();
    let tag = parts.next().unwrap_or_default().to_string();
    let normalized_rest = rest[tag.len()..].trim_start().to_string();
    let raw_rest = raw_rest_source[tag.len()..].trim_start().to_string();
    let mut columns = match tag.as_str() {
        "param" => split_columns(&normalized_rest, &[1, 1]),
        "field" => parse_field_columns(&normalized_rest),
        "return" => parse_return_columns(&normalized_rest),
        "class" => split_columns(&normalized_rest, &[1]),
        "alias" => parse_alias_columns(&normalized_rest),
        "generic" => parse_generic_columns(&normalized_rest),
        "type" | "overload" => vec![normalized_rest.clone()],
        _ => vec![collapse_spaces(&normalized_rest)],
    };
    columns.retain(|column| !column.is_empty());

    let align_key = match tag.as_str() {
        "class" | "alias" | "field" | "generic"
            if ctx.config.should_align_emmy_doc_declaration_tags() =>
        {
            Some(tag.clone())
        }
        "param" | "return" if ctx.config.should_align_emmy_doc_reference_tags() => {
            Some(tag.clone())
        }
        _ => None,
    };

    let preserve_body_spacing = tag == "alias" && !ctx.config.emmy_doc.align_tag_columns;

    DocTagLine {
        tag,
        raw_rest,
        columns,
        align_key,
        preserve_body_spacing,
    }
}

fn format_doc_comment_line(
    ctx: &FormatContext,
    line: DocLineKind,
    widths: &HashMap<String, Vec<usize>>,
    prefix_override: Option<&str>,
) -> String {
    match line {
        DocLineKind::Description {
            content,
            preserve_spacing,
        } => {
            if preserve_spacing {
                format!("---{content}")
            } else {
                let prefix = prefix_override.map(str::to_string).unwrap_or_else(|| {
                    if ctx.config.emmy_doc.space_after_description_dash {
                        "--- ".to_string()
                    } else {
                        "---".to_string()
                    }
                });
                if content.is_empty() {
                    prefix.trim_end().to_string()
                } else {
                    format!("{prefix}{content}")
                }
            }
        }
        DocLineKind::ContinueOr { content } => {
            let prefix = prefix_override.map(str::to_string).unwrap_or_else(|| {
                if ctx.config.emmy_doc.space_after_description_dash {
                    "--- |".to_string()
                } else {
                    "---|".to_string()
                }
            });
            if content.is_empty() {
                prefix
            } else {
                let separator = if prefix.ends_with(' ') { "" } else { " " };
                format!("{prefix}{separator}{content}")
            }
        }
        DocLineKind::Tag(tag) => {
            let prefix = if let Some(prefix) = prefix_override {
                format!("{prefix}{}", tag.tag)
            } else if ctx.config.emmy_doc.space_after_description_dash {
                format!("--- @{}", tag.tag)
            } else {
                format!("---@{}", tag.tag)
            };
            if tag.preserve_body_spacing {
                return if tag.raw_rest.is_empty() {
                    prefix
                } else {
                    format!("{prefix} {}", tag.raw_rest)
                };
            }
            let Some(key) = &tag.align_key else {
                return if tag.columns.is_empty() {
                    prefix
                } else {
                    format!("{prefix} {}", tag.columns.join(" "))
                };
            };
            let target_widths = widths.get(key);
            let mut rendered = prefix;
            if let Some((first, rest)) = tag.columns.split_first() {
                rendered.push(' ');
                rendered.push_str(first);
                for (index, column) in rest.iter().enumerate() {
                    let source_index = index;
                    let padding = target_widths
                        .and_then(|widths: &Vec<usize>| widths.get(source_index))
                        .map(|width: &usize| {
                            width.saturating_sub(tag.columns[source_index].len())
                                + ctx.config.emmy_doc.tag_spacing
                        })
                        .unwrap_or(1);
                    rendered.extend(std::iter::repeat_n(' ', padding));
                    rendered.push_str(column);
                }
            }
            rendered
        }
    }
}

fn split_columns(input: &str, head_sizes: &[usize]) -> Vec<String> {
    let tokens: Vec<_> = input.split_whitespace().collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut columns = Vec::new();
    let mut index = 0;
    for head_size in head_sizes {
        if index >= tokens.len() {
            break;
        }
        let end = (index + *head_size).min(tokens.len());
        columns.push(tokens[index..end].join(" "));
        index = end;
    }
    if index < tokens.len() {
        columns.push(tokens[index..].join(" "));
    }
    columns
}

fn parse_field_columns(input: &str) -> Vec<String> {
    let tokens: Vec<_> = input.split_whitespace().collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    let visibility = matches!(
        tokens.first().copied(),
        Some("public" | "private" | "protected")
    );
    if visibility && tokens.len() >= 2 {
        let mut columns = vec![format!("{} {}", tokens[0], tokens[1])];
        if tokens.len() >= 3 {
            columns.push(tokens[2].to_string());
        }
        if tokens.len() >= 4 {
            columns.push(tokens[3..].join(" "));
        }
        columns
    } else {
        split_columns(input, &[1, 1])
    }
}

fn parse_return_columns(input: &str) -> Vec<String> {
    let tokens: Vec<_> = input.split_whitespace().collect();
    match tokens.len() {
        0 => Vec::new(),
        1 => vec![tokens[0].to_string()],
        2 => vec![tokens.join(" ")],
        _ => vec![
            tokens[..tokens.len() - 1].join(" "),
            tokens[tokens.len() - 1].to_string(),
        ],
    }
}

fn parse_alias_columns(input: &str) -> Vec<String> {
    let tokens: Vec<_> = input.split_whitespace().collect();
    match tokens.len() {
        0 => Vec::new(),
        1 => vec![tokens[0].to_string()],
        2 => vec![tokens.join(" ")],
        _ => vec![tokens[..2].join(" "), tokens[2..].join(" ")],
    }
}

fn parse_generic_columns(input: &str) -> Vec<String> {
    let tokens: Vec<_> = input.split_whitespace().collect();
    match tokens.len() {
        0 => Vec::new(),
        1 => vec![tokens[0].to_string()],
        2 => vec![tokens[0].to_string(), tokens[1].to_string()],
        _ => vec![
            tokens[..tokens.len() - 2].join(" "),
            tokens[tokens.len() - 2..].join(" "),
        ],
    }
}

fn collect_comment_line_spacing_normalized_texts(
    comment: &LuaComment,
    plan: &RootFormatPlan,
) -> Vec<Option<String>> {
    let mut lines = Vec::new();
    let mut current_line = Vec::new();

    for element in comment.syntax().descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };

        match token.kind().to_token() {
            LuaTokenKind::TkEndOfLine => {
                lines.push(normalize_comment_line_with_spacing(&current_line, plan));
                current_line.clear();
            }
            _ => current_line.push(token),
        }
    }

    if !current_line.is_empty() {
        lines.push(normalize_comment_line_with_spacing(&current_line, plan));
    }

    lines
}

fn normalize_comment_line_with_spacing(
    tokens: &[LuaSyntaxToken],
    plan: &RootFormatPlan,
) -> Option<String> {
    let mut out = String::new();
    let mut previous_token: Option<&LuaSyntaxToken> = None;
    let mut saw_whitespace = false;

    for token in tokens {
        if token.kind().to_token() == LuaTokenKind::TkWhitespace {
            saw_whitespace = !out.is_empty();
            continue;
        }

        if !out.is_empty() {
            let spacing =
                comment_spacing_between_tokens(plan, previous_token, token, saw_whitespace);
            out.extend(std::iter::repeat_n(' ', spacing));
        }

        out.push_str(comment_token_text(plan, token));
        previous_token = Some(token);
        saw_whitespace = false;
    }

    (!out.is_empty()).then_some(out)
}

fn comment_spacing_between_tokens(
    plan: &RootFormatPlan,
    previous_token: Option<&LuaSyntaxToken>,
    current_token: &LuaSyntaxToken,
    had_source_whitespace: bool,
) -> usize {
    if had_source_whitespace && previous_token.is_some_and(is_doc_tag_keyword_token) {
        return 1;
    }

    if had_source_whitespace
        && current_token.kind().to_token() == LuaTokenKind::TkLeftBracket
        && previous_token.is_some_and(|token| {
            matches!(
                token.kind().to_token(),
                LuaTokenKind::TkDocVisibility | LuaTokenKind::TkTagVisibility
            )
        })
    {
        return 1;
    }

    let current_id = LuaSyntaxId::from_token(current_token);
    if let Some(expected) = plan.spacing.left_expected(current_id) {
        return resolve_comment_spacing_expected(expected, had_source_whitespace);
    }

    if let Some(previous_token) = previous_token {
        let previous_id = LuaSyntaxId::from_token(previous_token);
        if let Some(expected) = plan.spacing.right_expected(previous_id) {
            return resolve_comment_spacing_expected(expected, had_source_whitespace);
        }
    }

    usize::from(had_source_whitespace)
}

fn resolve_comment_spacing_expected(
    expected: &TokenSpacingExpected,
    had_source_whitespace: bool,
) -> usize {
    match expected {
        TokenSpacingExpected::Space(count) => *count,
        TokenSpacingExpected::MaxSpace(count) => {
            if had_source_whitespace {
                (*count).min(1)
            } else {
                0
            }
        }
    }
}

fn comment_token_text<'a>(plan: &'a RootFormatPlan, token: &'a LuaSyntaxToken) -> &'a str {
    plan.spacing
        .token_replace(LuaSyntaxId::from_token(token))
        .unwrap_or(token.text())
}

fn is_doc_tag_keyword_token(token: &LuaSyntaxToken) -> bool {
    matches!(
        token.kind().to_token(),
        LuaTokenKind::TkTagClass
            | LuaTokenKind::TkTagAlias
            | LuaTokenKind::TkTagField
            | LuaTokenKind::TkTagType
            | LuaTokenKind::TkTagParam
            | LuaTokenKind::TkTagReturn
            | LuaTokenKind::TkTagGeneric
            | LuaTokenKind::TkTagOverload
    )
}

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn should_preserve_comment_raw(comment: &LuaComment) -> bool {
    if comment.syntax().text().to_string().starts_with("----") {
        return true;
    }
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

#[cfg(test)]
mod tests {
    use emmylua_parser::{LuaAstNode, LuaComment, LuaLanguageLevel, LuaParser, ParserConfig};

    use crate::{config::LuaFormatConfig, printer::Printer};

    use super::*;

    fn parse_comment(input: &str) -> LuaComment {
        let tree = LuaParser::parse(input, ParserConfig::with_level(LuaLanguageLevel::Lua54));
        tree.get_chunk_node()
            .syntax()
            .descendants()
            .find_map(LuaComment::cast)
            .unwrap()
    }

    #[test]
    fn test_render_comment_with_spacing_uses_normal_prefix_replacement() {
        let config = LuaFormatConfig::default();
        let ctx = FormatContext::new(&config);
        let comment = parse_comment("--hello\n");
        let mut plan = RootFormatPlan::from_config(&config);
        let start = comment.syntax().first_token().unwrap();
        plan.spacing
            .add_token_replace(LuaSyntaxId::from_token(&start), "--  ".to_string());

        let docs = render_comment_with_spacing(&ctx, &comment, &plan);
        let rendered = Printer::new(&config).print(&docs);

        assert_eq!(rendered, "--  hello");
    }

    #[test]
    fn test_render_comment_with_spacing_uses_doc_prefix_replacement() {
        let config = LuaFormatConfig::default();
        let ctx = FormatContext::new(&config);
        let comment = parse_comment("---@param x string\n");
        let mut plan = RootFormatPlan::from_config(&config);
        let start = comment.syntax().first_token().unwrap();
        plan.spacing
            .add_token_replace(LuaSyntaxId::from_token(&start), "---  @".to_string());

        let docs = render_comment_with_spacing(&ctx, &comment, &plan);
        let rendered = Printer::new(&config).print(&docs);

        assert_eq!(rendered, "---  @param x string");
    }
}
