use std::collections::HashMap;

use crate::formatter::model::{StatementExprListLayoutKind, StatementExprListLayoutPlan};
use crate::ir::{self, AlignEntry, DocIR};
use emmylua_parser::*;
use rowan::{TextRange, TextSize};

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
    if let Some(disabled) = render_format_disabled_layout_node(root, node, plan) {
        return disabled;
    }

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

fn render_format_disabled_layout_node(
    root: &LuaSyntaxNode,
    node: &LayoutNodePlan,
    plan: &RootFormatPlan,
) -> Option<Vec<DocIR>> {
    let syntax_id = match node {
        LayoutNodePlan::Comment(comment) => comment.syntax_id,
        LayoutNodePlan::Syntax(syntax) => syntax.syntax_id,
    };

    if !plan.layout.format_disabled.contains(&syntax_id) {
        return None;
    }

    let syntax = find_node_by_id(root, syntax_id)?;
    Some(vec![ir::source_node_trimmed(syntax)])
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
    plan: &RootFormatPlan,
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
    if layout_node_is_format_disabled(&nodes[start], plan) {
        return None;
    }

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
        if layout_node_is_format_disabled(node, plan) {
            break;
        }
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

fn layout_node_is_format_disabled(node: &LayoutNodePlan, plan: &RootFormatPlan) -> bool {
    let syntax_id = match node {
        LayoutNodePlan::Comment(comment) => comment.syntax_id,
        LayoutNodePlan::Syntax(syntax) => syntax.syntax_id,
    };

    plan.layout.format_disabled.contains(&syntax_id)
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

pub(super) fn append_trailing_comment_suffix(
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

        if has_inline_non_trivia_before(&child) && !has_non_trivia_after_in_node(&child) {
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

fn has_non_trivia_after_in_node(node: &LuaSyntaxNode) -> bool {
    let mut next = node.next_sibling_or_token();
    while let Some(element) = next {
        match element.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine) => {
                next = element.next_sibling_or_token();
            }
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                next = element.next_sibling_or_token();
            }
            _ => return true,
        }
    }

    false
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
    let lines = if is_pure_doc_comment_block(&raw) {
        normalize_doc_comment_block(
            ctx,
            comment,
            &raw,
            &prefix_replacements,
            normalized_lines.as_slice(),
        )
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

fn is_pure_doc_comment_block(raw: &str) -> bool {
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .all(|line| line.trim_start().starts_with("---"))
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
    _normalized_line: Option<&str>,
) -> String {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("--") || trimmed.starts_with("---") {
        return trimmed.to_string();
    }
    let body_with_gap = &trimmed[2..];
    let prefix = prefix_override.map(str::to_string).unwrap_or_else(|| {
        if ctx.config.comments.space_after_comment_dash {
            "-- ".to_string()
        } else {
            "--".to_string()
        }
    });
    let body = body_with_gap.trim_start();
    if prefix.trim_end() == "--"
        && body_with_gap
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        && body.starts_with('[')
    {
        return format!("-- {body}");
    }
    if body.is_empty() {
        prefix.trim_end().to_string()
    } else {
        format!("{prefix}{body}")
    }
}

#[derive(Clone)]
enum DocBlockLine {
    Description(DocDescriptionLine),
    Tag(DocTagLine),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DocLinePrefixKind {
    Start,
    Continue,
    ContinueOr,
    Unknown,
}

struct DocBlockLineInput<'a> {
    raw_line: &'a str,
    normalized_line: Option<&'a str>,
    structured_tag: Option<StructuredDocTagColumns>,
    prefix_kind: DocLinePrefixKind,
}

#[derive(Clone)]
enum DocDescriptionKind {
    Plain,
    ContinueOr(String),
}

#[derive(Clone)]
struct DocDescriptionLine {
    kind: DocDescriptionKind,
    content: String,
    preserve_spacing: bool,
    gap_after_dash: Option<String>,
    columns: Vec<String>,
    align_key: Option<String>,
}

#[derive(Clone)]
struct DocTagLine {
    tag: String,
    raw_rest: String,
    columns: Vec<String>,
    align_key: Option<String>,
    preserve_body_spacing: bool,
    gap_after_dash: Option<String>,
}

#[derive(Clone)]
struct StructuredDocTagColumns {
    tag: String,
    head_columns: Vec<String>,
    description: Option<String>,
    use_normalized_head_as_single_column: bool,
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
    comment: &LuaComment,
    raw: &str,
    prefix_replacements: &[Option<String>],
    normalized_lines: &[Option<String>],
) -> Vec<String> {
    let line_inputs = collect_doc_block_line_inputs(ctx, comment, raw, normalized_lines);
    let mut parsed = Vec::with_capacity(line_inputs.len());

    for line_input in &line_inputs {
        parsed.push(parse_doc_block_line(ctx, line_input));
    }

    let parsed = annotate_multiline_alias_continue_lines(ctx, parsed);

    let mut widths: HashMap<String, Vec<usize>> = HashMap::new();
    for line in &parsed {
        let (align_key, columns) = match line {
            DocBlockLine::Tag(tag) => (tag.align_key.as_ref(), &tag.columns),
            DocBlockLine::Description(line)
                if matches!(line.kind, DocDescriptionKind::ContinueOr(_)) =>
            {
                (line.align_key.as_ref(), &line.columns)
            }
            DocBlockLine::Description(_) => (None, &Vec::new()),
        };
        let Some(key) = align_key else {
            continue;
        };
        let entry = widths
            .entry(key.clone())
            .or_insert_with(|| vec![0; columns.len().saturating_sub(1)]);
        if entry.len() < columns.len().saturating_sub(1) {
            entry.resize(columns.len().saturating_sub(1), 0);
        }
        for (index, column) in columns
            .iter()
            .take(columns.len().saturating_sub(1))
            .enumerate()
        {
            entry[index] = entry[index].max(column.len());
        }
    }

    parsed
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            format_doc_block_line(
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

fn collect_doc_block_line_inputs<'a>(
    ctx: &FormatContext,
    comment: &'a LuaComment,
    raw: &'a str,
    normalized_lines: &'a [Option<String>],
) -> Vec<DocBlockLineInput<'a>> {
    let raw_lines: Vec<&str> = raw.lines().collect();
    let structured_tags_by_line = collect_structured_doc_tag_columns_by_line(ctx, comment, raw);
    let prefix_kinds = collect_doc_line_prefix_kinds(comment, raw_lines.len());

    raw_lines
        .into_iter()
        .enumerate()
        .map(|(index, raw_line)| DocBlockLineInput {
            raw_line,
            normalized_line: normalized_lines.get(index).and_then(|line| line.as_deref()),
            structured_tag: structured_tags_by_line.get(index).cloned().flatten(),
            prefix_kind: prefix_kinds
                .get(index)
                .copied()
                .unwrap_or(DocLinePrefixKind::Unknown),
        })
        .collect()
}

fn parse_doc_block_line(ctx: &FormatContext, line_input: &DocBlockLineInput) -> DocBlockLine {
    let raw_suffix = strip_doc_line_prefix(line_input.raw_line, line_input.prefix_kind);
    let trimmed = raw_suffix.trim_start();
    let gap_after_dash = preserved_dash_gap(raw_suffix);
    let normalized_suffix = strip_doc_line_prefix(
        line_input.normalized_line.unwrap_or(line_input.raw_line),
        line_input.prefix_kind,
    );
    let normalized_trimmed = normalized_suffix.trim_start();

    if is_continue_or_doc_line(line_input.prefix_kind, trimmed) {
        let marker = doc_continue_marker(trimmed);
        return DocBlockLine::Description(DocDescriptionLine {
            kind: DocDescriptionKind::ContinueOr(marker.to_string()),
            content: normalized_continue_line_content(normalized_trimmed).to_string(),
            preserve_spacing: false,
            gap_after_dash,
            columns: Vec::new(),
            align_key: None,
        });
    }

    let tag_rest = if line_input.prefix_kind == DocLinePrefixKind::Start {
        Some(trimmed.strip_prefix('@').unwrap_or(trimmed))
    } else {
        trimmed.strip_prefix('@')
    };

    if let Some(rest) = tag_rest {
        let normalized_rest = if line_input.prefix_kind == DocLinePrefixKind::Start {
            normalized_trimmed
                .strip_prefix('@')
                .unwrap_or(normalized_trimmed)
        } else {
            normalized_trimmed
                .strip_prefix('@')
                .unwrap_or(rest)
                .trim_start()
        };
        return DocBlockLine::Tag(parse_doc_tag_line(
            ctx,
            normalized_rest.trim_start(),
            rest.trim_start(),
            gap_after_dash,
            line_input.structured_tag.as_ref(),
        ));
    }

    let preserve_spacing = gap_after_dash.is_some();
    let content = if preserve_spacing {
        raw_suffix.to_string()
    } else {
        strip_single_comment_gap(raw_suffix).to_string()
    };
    DocBlockLine::Description(DocDescriptionLine {
        kind: DocDescriptionKind::Plain,
        content,
        preserve_spacing,
        gap_after_dash,
        columns: Vec::new(),
        align_key: None,
    })
}

fn parse_doc_tag_line(
    ctx: &FormatContext,
    rest: &str,
    raw_rest_source: &str,
    gap_after_dash: Option<String>,
    structured_tag: Option<&StructuredDocTagColumns>,
) -> DocTagLine {
    let mut parts = rest.split_whitespace();
    let tag = parts.next().unwrap_or_default().to_string();
    let normalized_rest = rest
        .strip_prefix(tag.as_str())
        .unwrap_or("")
        .trim_start()
        .to_string();
    let raw_rest = raw_rest_source
        .strip_prefix(tag.as_str())
        .unwrap_or("")
        .trim_start()
        .to_string();
    let structured_tag = structured_tag.filter(|structured| structured.tag == tag);
    let (normalized_head, raw_description) = if structured_tag.is_some() {
        (normalized_rest.clone(), None)
    } else {
        split_doc_tag_description(&normalized_rest, &raw_rest)
    };
    let structured_description = structured_tag
        .and_then(|structured| structured.description.clone())
        .and_then(first_structured_description_line);
    let mut columns = structured_tag
        .map(|structured| {
            if structured.use_normalized_head_as_single_column {
                structured_single_head_columns(&normalized_head, structured_description.as_deref())
            } else {
                structured_columns_from_normalized_head(
                    &normalized_head,
                    &structured.head_columns,
                    structured_description.as_deref(),
                )
            }
        })
        .unwrap_or_else(|| match tag.as_str() {
            "param" => parse_param_columns(&normalized_head),
            "field" => parse_field_columns(&normalized_head),
            "return" => parse_return_columns(&normalized_head),
            "class" => split_columns(&normalized_head, &[1]),
            "alias" => parse_alias_columns(&normalized_head),
            "generic" => parse_generic_columns(&normalized_head),
            "type" | "overload" => vec![normalized_head.clone()],
            _ => vec![collapse_spaces(&normalized_head)],
        });
    if let Some(description) = raw_description.or(structured_description) {
        columns.push(description);
    }
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
        gap_after_dash,
    }
}

fn format_doc_block_line(
    ctx: &FormatContext,
    line: DocBlockLine,
    widths: &HashMap<String, Vec<usize>>,
    prefix_override: Option<&str>,
) -> String {
    match line {
        DocBlockLine::Description(line) => match line.kind {
            DocDescriptionKind::Plain => {
                if line.preserve_spacing {
                    format!("---{}", line.content)
                } else {
                    let prefix = prefix_override.map(str::to_string).unwrap_or_else(|| {
                        if ctx.config.emmy_doc.space_after_description_dash {
                            "--- ".to_string()
                        } else {
                            "---".to_string()
                        }
                    });
                    if line.content.is_empty() {
                        prefix.trim_end().to_string()
                    } else {
                        format!("{prefix}{}", line.content)
                    }
                }
            }
            DocDescriptionKind::ContinueOr(marker) => {
                let prefix = if let Some(gap_after_dash) = line.gap_after_dash.as_deref() {
                    format!("---{gap_after_dash}{marker}")
                } else {
                    prefix_override
                        .map(str::to_string)
                        .unwrap_or_else(|| normalized_doc_continue_marker_prefix(ctx, &marker))
                };
                if let Some(key) = &line.align_key {
                    let Some((first, rest)) = line.columns.split_first() else {
                        return prefix;
                    };
                    let mut rendered = prefix;
                    rendered.push(' ');
                    rendered.push_str(first);
                    for (index, column) in rest.iter().enumerate() {
                        let source_index = index;
                        let padding = widths
                            .get(key)
                            .and_then(|widths| widths.get(source_index))
                            .map(|width| width.saturating_sub(line.columns[source_index].len()) + 1)
                            .unwrap_or(1);
                        rendered.extend(std::iter::repeat_n(' ', padding));
                        rendered.push_str(column);
                    }
                    return rendered;
                }
                if line.content.is_empty() {
                    prefix
                } else {
                    let separator = if prefix.ends_with(' ') { "" } else { " " };
                    format!("{prefix}{separator}{}", line.content)
                }
            }
        },
        DocBlockLine::Tag(tag) => {
            let prefix = if let Some(gap_after_dash) = tag.gap_after_dash.as_deref() {
                format!("---{gap_after_dash}@{}", tag.tag)
            } else if let Some(prefix) = prefix_override {
                format_doc_tag_prefix_override(prefix, &tag.tag)
            } else if ctx.config.emmy_doc.space_between_tag_columns {
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
                            width.saturating_sub(tag.columns[source_index].len()) + 1
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

fn annotate_multiline_alias_continue_lines(
    ctx: &FormatContext,
    parsed: Vec<DocBlockLine>,
) -> Vec<DocBlockLine> {
    let mut in_alias_block = false;

    parsed
        .into_iter()
        .map(|line| match line {
            DocBlockLine::Tag(tag) => {
                in_alias_block = tag.tag == "alias";
                DocBlockLine::Tag(tag)
            }
            DocBlockLine::Description(mut line) => {
                if in_alias_block
                    && matches!(line.kind, DocDescriptionKind::ContinueOr(_))
                    && ctx
                        .config
                        .should_align_emmy_doc_multiline_alias_descriptions()
                {
                    let columns = parse_multiline_alias_continue_columns(&line.content);
                    if columns.len() > 1 {
                        line.align_key = Some("alias_multiline_description".to_string());
                        line.columns = columns;
                    }
                }

                if matches!(line.kind, DocDescriptionKind::Plain) {
                    in_alias_block = false;
                }

                DocBlockLine::Description(line)
            }
        })
        .collect()
}

fn collect_doc_line_prefix_kinds(
    comment: &LuaComment,
    raw_line_count: usize,
) -> Vec<DocLinePrefixKind> {
    let mut prefix_kinds = vec![DocLinePrefixKind::Unknown; raw_line_count];
    let mut current_line = 0usize;
    let mut saw_non_whitespace = false;

    for element in comment.syntax().descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };

        match token.kind().to_token() {
            LuaTokenKind::TkEndOfLine => {
                current_line = current_line.saturating_add(1);
                saw_non_whitespace = false;
            }
            LuaTokenKind::TkWhitespace => {}
            kind if !saw_non_whitespace => {
                if let Some(prefix_kind) = doc_line_prefix_kind_from_token(kind)
                    && let Some(slot) = prefix_kinds.get_mut(current_line)
                {
                    *slot = prefix_kind;
                }
                saw_non_whitespace = true;
            }
            _ => {
                saw_non_whitespace = true;
            }
        }
    }

    prefix_kinds
}

fn strip_doc_line_prefix(line: &str, prefix_kind: DocLinePrefixKind) -> &str {
    let trimmed = line.trim_start();
    match prefix_kind {
        DocLinePrefixKind::Start => trimmed
            .strip_prefix("---@")
            .or_else(|| trimmed.strip_prefix("---"))
            .unwrap_or(trimmed),
        DocLinePrefixKind::Continue | DocLinePrefixKind::Unknown => {
            trimmed.strip_prefix("---").unwrap_or(trimmed)
        }
        DocLinePrefixKind::ContinueOr => trimmed
            .strip_prefix("---|+")
            .or_else(|| trimmed.strip_prefix("---|>"))
            .or_else(|| trimmed.strip_prefix("---|"))
            .or_else(|| trimmed.strip_prefix("---"))
            .unwrap_or(trimmed),
    }
}

fn is_continue_or_doc_line(prefix_kind: DocLinePrefixKind, trimmed_content: &str) -> bool {
    prefix_kind == DocLinePrefixKind::ContinueOr || trimmed_content.starts_with('|')
}

fn doc_line_prefix_kind_from_token(token_kind: LuaTokenKind) -> Option<DocLinePrefixKind> {
    match token_kind {
        LuaTokenKind::TkDocStart | LuaTokenKind::TkDocLongStart => Some(DocLinePrefixKind::Start),
        LuaTokenKind::TkDocContinue => Some(DocLinePrefixKind::Continue),
        LuaTokenKind::TkDocContinueOr => Some(DocLinePrefixKind::ContinueOr),
        _ => None,
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
        if let Some((name, ty)) = split_attached_field_name_and_type(tokens[1]) {
            let mut columns = vec![format!("{} {}", tokens[0], name), ty.to_string()];
            if tokens.len() >= 3 {
                columns.push(tokens[2..].join(" "));
            }
            return columns;
        }
        if tokens
            .get(2)
            .is_some_and(|token| !looks_like_field_type_start(token))
        {
            return vec![
                format!("{} {}", tokens[0], tokens[1]),
                tokens[2..].join(" "),
            ];
        }
        let mut columns = vec![format!("{} {}", tokens[0], tokens[1])];
        if tokens.len() >= 3 {
            columns.push(tokens[2].to_string());
        }
        if tokens.len() >= 4 {
            columns.push(tokens[3..].join(" "));
        }
        columns
    } else {
        if let Some((name, ty)) = split_attached_field_name_and_type(tokens[0]) {
            let mut columns = vec![name.to_string(), ty.to_string()];
            if tokens.len() >= 2 {
                columns.push(tokens[1..].join(" "));
            }
            return columns;
        }
        if tokens.len() >= 2 && !looks_like_field_type_start(tokens[1]) {
            vec![tokens[0].to_string(), tokens[1..].join(" ")]
        } else {
            split_columns(input, &[1, 1])
        }
    }
}

fn split_attached_field_name_and_type(token: &str) -> Option<(&str, &str)> {
    let split_index = token.find('(')?;
    if split_index == 0 {
        return None;
    }

    let (name, ty) = token.split_at(split_index);
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.'))
    {
        return None;
    }

    Some((name, ty))
}

fn parse_param_columns(input: &str) -> Vec<String> {
    let tokens: Vec<_> = input.split_whitespace().collect();
    if tokens.is_empty() {
        return Vec::new();
    }

    if tokens.len() >= 3 && matches!(tokens[1], "sync" | "async") && tokens[2].starts_with("fun") {
        return vec![tokens[0].to_string(), tokens[1..].join(" ")];
    }

    split_columns(input, &[1, 1])
}

fn parse_return_columns(input: &str) -> Vec<String> {
    parse_return_columns_without_raw_description(input)
}

fn parse_return_columns_without_raw_description(input: &str) -> Vec<String> {
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

fn parse_multiline_alias_continue_columns(input: &str) -> Vec<String> {
    let Some(hash_index) = input.find(" #") else {
        return vec![input.trim().to_string()];
    };

    let value = input[..hash_index].trim();
    let description = input[hash_index + 2..].trim_start();
    if value.is_empty() || description.is_empty() {
        return vec![input.trim().to_string()];
    }

    vec![value.to_string(), format!("# {description}")]
}

fn looks_like_field_type_start(token: &str) -> bool {
    if token.is_empty() || token.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        return false;
    }

    token.starts_with("fun(")
        || token.starts_with('[')
        || token.starts_with('"')
        || token.contains('|')
        || token.contains('?')
        || token.contains('<')
        || token.contains('{')
        || matches!(
            token,
            "any"
                | "boolean"
                | "function"
                | "global"
                | "integer"
                | "lightuserdata"
                | "nil"
                | "number"
                | "self"
                | "string"
                | "table"
                | "thread"
                | "unknown"
                | "userdata"
                | "void"
        )
}

fn split_doc_tag_description(normalized_input: &str, raw_input: &str) -> (String, Option<String>) {
    let normalized_head = normalized_input
        .find('#')
        .map(|index| normalized_input[..index].trim_end().to_string())
        .unwrap_or_else(|| normalized_input.to_string());

    let raw_description = raw_input.find('#').and_then(|index| {
        let description = raw_input[index..].trim_start();
        (!description.is_empty()).then(|| description.to_string())
    });

    (normalized_head, raw_description)
}

fn doc_continue_marker(text: &str) -> &str {
    if text.starts_with("|+") {
        "|+"
    } else if text.starts_with("|>") {
        "|>"
    } else {
        "|"
    }
}

fn strip_doc_continue_marker(text: &str) -> Option<&str> {
    text.strip_prefix("|+")
        .or_else(|| text.strip_prefix("|>"))
        .or_else(|| text.strip_prefix('|'))
}

fn normalized_continue_line_content(text: &str) -> &str {
    strip_doc_continue_marker(text).unwrap_or(text).trim_start()
}

fn format_doc_tag_prefix_override(prefix: &str, tag: &str) -> String {
    let tag = tag.strip_prefix('@').unwrap_or(tag);
    if prefix.contains('@') {
        format!("{prefix}{tag}")
    } else {
        format!("{prefix}@{tag}")
    }
}

fn normalized_doc_continue_marker_prefix(ctx: &FormatContext, marker: &str) -> String {
    if ctx.config.emmy_doc.space_after_description_dash {
        format!("--- {marker}")
    } else {
        format!("---{marker}")
    }
}

fn strip_single_comment_gap(text_after_dash: &str) -> &str {
    text_after_dash
        .strip_prefix(' ')
        .or_else(|| text_after_dash.strip_prefix('\t'))
        .unwrap_or(text_after_dash)
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

fn collect_structured_doc_tag_columns_by_line(
    ctx: &FormatContext,
    comment: &LuaComment,
    raw: &str,
) -> Vec<Option<StructuredDocTagColumns>> {
    let raw_line_count = raw.lines().count();
    let mut structured_tags = vec![None; raw_line_count];
    let line_start_offsets = collect_line_start_offsets(raw);
    let comment_start = comment.syntax().text_range().start();

    for child in comment.syntax().children() {
        let Some(tag) = LuaDocTag::cast(child.clone()) else {
            continue;
        };
        let relative_start =
            u32::from(child.text_range().start()).saturating_sub(u32::from(comment_start)) as usize;
        let line_index = line_index_for_offset(&line_start_offsets, relative_start);
        let Some(columns) = structured_doc_tag_columns_from_ast(ctx, &tag) else {
            continue;
        };
        if let Some(slot) = structured_tags.get_mut(line_index) {
            *slot = Some(columns);
        }
    }

    structured_tags
}

fn collect_line_start_offsets(raw: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in raw.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

fn line_index_for_offset(line_start_offsets: &[usize], offset: usize) -> usize {
    match line_start_offsets.binary_search(&offset) {
        Ok(index) => index,
        Err(index) => index.saturating_sub(1),
    }
}

fn structured_doc_tag_columns_from_ast(
    ctx: &FormatContext,
    tag: &LuaDocTag,
) -> Option<StructuredDocTagColumns> {
    match tag {
        LuaDocTag::Class(tag) => Some(StructuredDocTagColumns {
            tag: "class".to_string(),
            head_columns: Vec::new(),
            description: tag.get_description().map(|it| it.get_description_text()),
            use_normalized_head_as_single_column: true,
        }),
        LuaDocTag::Alias(tag) => Some(StructuredDocTagColumns {
            tag: "alias".to_string(),
            head_columns: Vec::new(),
            description: tag.get_description().map(|it| it.get_description_text()),
            use_normalized_head_as_single_column: true,
        }),
        LuaDocTag::Generic(tag) => Some(StructuredDocTagColumns {
            tag: "generic".to_string(),
            head_columns: Vec::new(),
            description: tag.get_description().map(|it| it.get_description_text()),
            use_normalized_head_as_single_column: true,
        }),
        LuaDocTag::Type(tag) => {
            let head_columns = structured_type_columns(ctx, tag);
            Some(StructuredDocTagColumns {
                tag: "type".to_string(),
                use_normalized_head_as_single_column: head_columns.is_empty(),
                head_columns,
                description: tag.get_description().map(|it| it.get_description_text()),
            })
        }
        LuaDocTag::Overload(tag) => Some(StructuredDocTagColumns {
            tag: "overload".to_string(),
            head_columns: Vec::new(),
            description: tag.get_description().map(|it| it.get_description_text()),
            use_normalized_head_as_single_column: true,
        }),
        LuaDocTag::Param(tag) => Some(StructuredDocTagColumns {
            tag: "param".to_string(),
            head_columns: structured_param_columns(tag),
            description: tag.get_description().map(|it| it.get_description_text()),
            use_normalized_head_as_single_column: false,
        }),
        LuaDocTag::Field(tag) => Some(StructuredDocTagColumns {
            tag: "field".to_string(),
            head_columns: structured_field_columns(tag),
            description: tag.get_description().map(|it| it.get_description_text()),
            use_normalized_head_as_single_column: false,
        }),
        LuaDocTag::Return(tag) => Some(StructuredDocTagColumns {
            tag: "return".to_string(),
            head_columns: structured_return_columns(tag),
            description: tag.get_description().map(|it| it.get_description_text()),
            use_normalized_head_as_single_column: false,
        }),
        _ => None,
    }
}

fn structured_type_columns(ctx: &FormatContext, tag: &LuaDocTagType) -> Vec<String> {
    let mut types = tag.get_type_list();
    let Some(first_type) = types.next() else {
        return Vec::new();
    };

    if types.next().is_some() {
        return Vec::new();
    }

    match first_type {
        LuaDocType::Object(object) => vec![format_doc_object_type_inline(ctx, &object)],
        _ => Vec::new(),
    }
}

fn format_doc_object_type_inline(ctx: &FormatContext, object: &LuaDocObjectType) -> String {
    let fields = object
        .get_fields()
        .map(|field| field.syntax().text().to_string().trim().to_string())
        .collect::<Vec<_>>();

    if fields.is_empty() {
        return "{}".to_string();
    }

    if ctx.config.spacing.space_inside_braces {
        format!("{{ {} }}", fields.join(", "))
    } else {
        format!("{{{}}}", fields.join(", "))
    }
}

fn structured_single_head_columns(
    normalized_head: &str,
    structured_description: Option<&str>,
) -> Vec<String> {
    let head = structured_description
        .and_then(|description| strip_structured_description_suffix(normalized_head, description))
        .unwrap_or_else(|| normalized_head.trim().to_string());

    if head.is_empty() {
        Vec::new()
    } else {
        vec![head]
    }
}

fn strip_structured_description_suffix(normalized_head: &str, description: &str) -> Option<String> {
    let trimmed_head = normalized_head.trim_end();
    let trimmed_description = description.trim();
    if trimmed_description.is_empty() {
        return Some(trimmed_head.to_string());
    }

    trimmed_head
        .strip_suffix(trimmed_description)
        .map(str::trim_end)
        .map(str::to_string)
}

fn structured_columns_from_normalized_head(
    normalized_head: &str,
    structured_head_columns: &[String],
    structured_description: Option<&str>,
) -> Vec<String> {
    let normalized_head = structured_description
        .and_then(|description| strip_structured_description_suffix(normalized_head, description))
        .unwrap_or_else(|| normalized_head.trim().to_string());

    match structured_head_columns.len() {
        0 => Vec::new(),
        1 => structured_head_columns.to_vec(),
        2 => {
            let raw_first = structured_head_columns[0].trim();
            let normalized_first = collapse_spaces(raw_first);
            let first_variants = structured_first_column_variants(raw_first, &normalized_first);
            let rest = first_variants
                .iter()
                .find_map(|candidate| normalized_head.strip_prefix(candidate.as_str()))
                .map(str::trim_start)
                .unwrap_or(normalized_head.as_str());

            if rest.is_empty() {
                vec![normalized_first]
            } else {
                vec![normalized_first, rest.to_string()]
            }
        }
        _ => structured_head_columns.to_vec(),
    }
}

fn structured_first_column_variants(raw_first: &str, normalized_first: &str) -> Vec<String> {
    let mut variants = vec![raw_first.to_string()];
    if normalized_first != raw_first {
        variants.push(normalized_first.to_string());
    }

    if let Some(base) = normalized_first.strip_suffix('?') {
        variants.push(format!("{base} ?"));
    }

    variants.sort();
    variants.dedup();
    variants
}

fn structured_param_columns(tag: &LuaDocTagParam) -> Vec<String> {
    let node = tag.syntax();
    let Some(content_start) = structured_tag_content_start(node) else {
        return Vec::new();
    };

    let head_token = tag
        .get_name_token()
        .map(|token| token.syntax().clone())
        .or_else(|| {
            node.children_with_tokens()
                .filter_map(|element| element.into_token())
                .find(|token| token.kind() == LuaTokenKind::TkDots.into())
        });

    let Some(head_token) = head_token else {
        return vec![
            slice_node_text(node, content_start, node.text_range().end())
                .trim()
                .to_string(),
        ];
    };

    let mut name = head_token.text().to_string();
    if tag.is_nullable() {
        name.push('?');
    }

    let type_text = if let Some(type_node) = tag.get_type() {
        let type_start = adjust_attached_type_start(node, type_node.syntax().text_range().start());
        slice_same_line_node_text(node, type_start, node.text_range().end())
            .trim()
            .to_string()
    } else {
        slice_node_text(node, head_token.text_range().end(), node.text_range().end())
            .trim()
            .trim_start_matches('?')
            .trim()
            .to_string()
    };

    if type_text.is_empty() {
        vec![name]
    } else {
        vec![name, type_text]
    }
}

fn structured_field_columns(tag: &LuaDocTagField) -> Vec<String> {
    let node = tag.syntax();
    let Some(content_start) = structured_tag_content_start(node) else {
        return Vec::new();
    };
    let Some(type_node) = tag.get_type() else {
        return vec![
            slice_node_text(node, content_start, node.text_range().end())
                .trim()
                .to_string(),
        ];
    };

    let type_start = adjust_attached_type_start(node, type_node.syntax().text_range().start());

    let key = slice_node_text(node, content_start, type_start)
        .trim()
        .to_string();
    let type_text = slice_same_line_node_text(node, type_start, node.text_range().end())
        .trim()
        .to_string();

    if type_text.is_empty() {
        vec![key]
    } else {
        vec![key, type_text]
    }
}

fn structured_return_columns(tag: &LuaDocTagReturn) -> Vec<String> {
    let node = tag.syntax();
    let Some(content_start) = structured_tag_content_start(node) else {
        return Vec::new();
    };

    let head = slice_node_text(node, content_start, node.text_range().end())
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();

    if head.is_empty() {
        Vec::new()
    } else {
        vec![head]
    }
}

fn structured_tag_content_start(node: &LuaSyntaxNode) -> Option<TextSize> {
    node.first_token().map(|token| token.text_range().end())
}

fn slice_same_line_node_text(node: &LuaSyntaxNode, start: TextSize, end: TextSize) -> String {
    slice_node_text(node, start, end)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string()
}

fn first_structured_description_line(text: String) -> Option<String> {
    text.lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
}

fn adjust_attached_type_start(node: &LuaSyntaxNode, type_start: TextSize) -> TextSize {
    let base = u32::from(node.text_range().start());
    let relative = u32::from(type_start).saturating_sub(base) as usize;
    if relative == 0 {
        return type_start;
    }

    let text = node.text().to_string();
    if text.as_bytes().get(relative.saturating_sub(1)) == Some(&b'(') {
        TextSize::from((u32::from(type_start)).saturating_sub(1))
    } else {
        type_start
    }
}

fn slice_node_text(node: &LuaSyntaxNode, start: TextSize, end: TextSize) -> String {
    let base = u32::from(node.text_range().start());
    let start = u32::from(start).saturating_sub(base) as usize;
    let end = u32::from(end).saturating_sub(base) as usize;
    let text = node.text().to_string();
    text.get(start..end).unwrap_or("").to_string()
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
            | LuaTokenKind::TkTagVersion
    )
}

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn preserved_dash_gap(text_after_dash: &str) -> Option<String> {
    let gap_len = text_after_dash
        .chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .count();
    if gap_len > 1 {
        Some(text_after_dash[..gap_len].to_string())
    } else {
        None
    }
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
