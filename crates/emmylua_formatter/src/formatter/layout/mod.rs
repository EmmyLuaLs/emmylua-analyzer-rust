mod tree;
#[cfg(test)]
mod test;

use super::FormatContext;
use super::model::{
    ControlHeaderLayoutPlan, ExprSequenceLayoutPlan, RootFormatPlan, StatementExprListLayoutKind,
    StatementExprListLayoutPlan, StatementTriviaLayoutPlan,
};
use super::trivia::{
    has_non_trivia_before_on_same_line_tokenwise, node_has_direct_comment_child,
    source_line_prefix_width,
};
use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaCallArgList, LuaChunk, LuaComment, LuaDoStat, LuaExpr,
    LuaForRangeStat, LuaForStat, LuaFuncStat, LuaIfStat, LuaLocalFuncStat, LuaLocalStat,
    LuaParamList, LuaRepeatStat, LuaReturnStat, LuaSyntaxId, LuaSyntaxNode, LuaSyntaxToken,
    LuaTableExpr, LuaTokenKind, LuaWhileStat,
};

pub fn analyze_root_layout(
    _ctx: &FormatContext,
    chunk: &LuaChunk,
    mut plan: RootFormatPlan,
) -> RootFormatPlan {
    plan.layout.format_block_with_legacy = true;
    plan.layout.root_nodes =
        tree::collect_root_layout_nodes(chunk, &mut plan.layout.format_disabled);
    analyze_node_layouts(chunk, &mut plan);
    plan
}

fn analyze_node_layouts(chunk: &LuaChunk, plan: &mut RootFormatPlan) {
    for node in chunk.descendants::<LuaAst>() {
        match node {
            LuaAst::LuaLocalStat(stat) => {
                analyze_local_stat_layout(&stat, plan);
            }
            LuaAst::LuaAssignStat(stat) => {
                analyze_assign_stat_layout(&stat, plan);
            }
            LuaAst::LuaReturnStat(stat) => {
                analyze_return_stat_layout(&stat, plan);
            }
            LuaAst::LuaWhileStat(stat) => {
                analyze_while_stat_layout(&stat, plan);
            }
            LuaAst::LuaForStat(stat) => {
                analyze_for_stat_layout(&stat, plan);
            }
            LuaAst::LuaForRangeStat(stat) => {
                analyze_for_range_stat_layout(&stat, plan);
            }
            LuaAst::LuaRepeatStat(stat) => {
                analyze_repeat_stat_layout(&stat, plan);
            }
            LuaAst::LuaIfStat(stat) => {
                analyze_if_stat_layout(&stat, plan);
            }
            LuaAst::LuaFuncStat(stat) => {
                analyze_func_stat_layout(&stat, plan);
            }
            LuaAst::LuaLocalFuncStat(stat) => {
                analyze_local_func_stat_layout(&stat, plan);
            }
            LuaAst::LuaDoStat(stat) => {
                analyze_do_stat_layout(&stat, plan);
            }
            LuaAst::LuaParamList(param) => {
                analyze_param_list_layout(&param, plan);
            }
            LuaAst::LuaCallArgList(args) => {
                analyze_call_arg_list_layout(&args, plan);
            }
            LuaAst::LuaTableExpr(table) => {
                analyze_table_expr_layout(&table, plan);
            }
            _ => {}
        }
    }
}

fn analyze_local_stat_layout(stat: &LuaLocalStat, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
    analyze_statement_trivia_layout(stat.syntax(), syntax_id, plan);
    let exprs: Vec<_> = stat.get_value_exprs().collect();
    analyze_statement_expr_list_layout(syntax_id, &exprs, plan);
}

fn analyze_assign_stat_layout(stat: &LuaAssignStat, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
    analyze_statement_trivia_layout(stat.syntax(), syntax_id, plan);
    let (_, exprs) = stat.get_var_and_expr_list();
    analyze_statement_expr_list_layout(syntax_id, &exprs, plan);
}

fn analyze_return_stat_layout(stat: &LuaReturnStat, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
    analyze_statement_trivia_layout(stat.syntax(), syntax_id, plan);
    let exprs: Vec<_> = stat.get_expr_list().collect();
    analyze_statement_expr_list_layout(syntax_id, &exprs, plan);
}

fn analyze_while_stat_layout(stat: &LuaWhileStat, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
    analyze_control_header_layout(stat.syntax(), syntax_id, plan);
    analyze_boundary_comments_after_token(
        stat.syntax(),
        syntax_id,
        LuaTokenKind::TkDo,
        first_direct_token(stat.syntax(), LuaTokenKind::TkDo).as_ref(),
        plan,
    );
    if let Some(block) = stat.get_block() {
        analyze_boundary_comments_in_block(block.syntax(), syntax_id, LuaTokenKind::TkDo, plan);
    }
}

fn analyze_for_stat_layout(stat: &LuaForStat, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
    analyze_control_header_layout(stat.syntax(), syntax_id, plan);
    let exprs: Vec<_> = stat.get_iter_expr().collect();
    analyze_control_header_expr_list_layout(syntax_id, &exprs, plan);
    analyze_boundary_comments_after_token(
        stat.syntax(),
        syntax_id,
        LuaTokenKind::TkDo,
        first_direct_token(stat.syntax(), LuaTokenKind::TkDo).as_ref(),
        plan,
    );
    if let Some(block) = stat.get_block() {
        analyze_boundary_comments_in_block(block.syntax(), syntax_id, LuaTokenKind::TkDo, plan);
    }
}

fn analyze_for_range_stat_layout(stat: &LuaForRangeStat, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
    analyze_control_header_layout(stat.syntax(), syntax_id, plan);
    let exprs: Vec<_> = stat.get_expr_list().collect();
    analyze_control_header_expr_list_layout(syntax_id, &exprs, plan);
    analyze_boundary_comments_after_token(
        stat.syntax(),
        syntax_id,
        LuaTokenKind::TkIn,
        first_direct_token(stat.syntax(), LuaTokenKind::TkIn).as_ref(),
        plan,
    );
    analyze_boundary_comments_after_token(
        stat.syntax(),
        syntax_id,
        LuaTokenKind::TkDo,
        first_direct_token(stat.syntax(), LuaTokenKind::TkDo).as_ref(),
        plan,
    );
    if let Some(block) = stat.get_block() {
        analyze_boundary_comments_in_block(block.syntax(), syntax_id, LuaTokenKind::TkDo, plan);
    }
}

fn analyze_repeat_stat_layout(stat: &LuaRepeatStat, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
    analyze_control_header_layout(stat.syntax(), syntax_id, plan);
}

fn analyze_if_stat_layout(stat: &LuaIfStat, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
    analyze_control_header_layout(stat.syntax(), syntax_id, plan);
    analyze_boundary_comments_after_token(
        stat.syntax(),
        syntax_id,
        LuaTokenKind::TkThen,
        first_direct_token(stat.syntax(), LuaTokenKind::TkThen).as_ref(),
        plan,
    );

    for clause in stat.get_else_if_clause_list() {
        let clause_id = LuaSyntaxId::from_node(clause.syntax());
        analyze_control_header_layout(clause.syntax(), clause_id, plan);
        analyze_boundary_comments_after_token(
            clause.syntax(),
            clause_id,
            LuaTokenKind::TkThen,
            first_direct_token(clause.syntax(), LuaTokenKind::TkThen).as_ref(),
            plan,
        );
        if let Some(block) = clause.get_block() {
            analyze_boundary_comments_in_block(
                block.syntax(),
                clause_id,
                LuaTokenKind::TkThen,
                plan,
            );
        }
    }

    if let Some(clause) = stat.get_else_clause() {
        let clause_id = LuaSyntaxId::from_node(clause.syntax());
        analyze_boundary_comments_after_token(
            clause.syntax(),
            clause_id,
            LuaTokenKind::TkElse,
            first_direct_token(clause.syntax(), LuaTokenKind::TkElse).as_ref(),
            plan,
        );
        if let Some(block) = clause.get_block() {
            analyze_boundary_comments_in_block(
                block.syntax(),
                clause_id,
                LuaTokenKind::TkElse,
                plan,
            );
        }
    }

    if let Some(block) = stat.get_block() {
        analyze_boundary_comments_in_block(block.syntax(), syntax_id, LuaTokenKind::TkThen, plan);
    }
}

fn analyze_func_stat_layout(stat: &LuaFuncStat, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
    if let Some(closure) = stat.get_closure()
        && let Some(params) = closure.get_params_list()
    {
        analyze_boundary_comments_after_token(
            stat.syntax(),
            syntax_id,
            LuaTokenKind::TkRightParen,
            first_direct_token(params.syntax(), LuaTokenKind::TkRightParen).as_ref(),
            plan,
        );

        analyze_boundary_comments_after_token(
            closure.syntax(),
            syntax_id,
            LuaTokenKind::TkRightParen,
            first_direct_token(params.syntax(), LuaTokenKind::TkRightParen).as_ref(),
            plan,
        );

        if let Some(block) = closure.get_block() {
            analyze_boundary_comments_in_block(
                block.syntax(),
                syntax_id,
                LuaTokenKind::TkRightParen,
                plan,
            );
        }
    }
}

fn analyze_local_func_stat_layout(stat: &LuaLocalFuncStat, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
    if let Some(closure) = stat.get_closure()
        && let Some(params) = closure.get_params_list()
    {
        analyze_boundary_comments_after_token(
            stat.syntax(),
            syntax_id,
            LuaTokenKind::TkRightParen,
            first_direct_token(params.syntax(), LuaTokenKind::TkRightParen).as_ref(),
            plan,
        );

        analyze_boundary_comments_after_token(
            closure.syntax(),
            syntax_id,
            LuaTokenKind::TkRightParen,
            first_direct_token(params.syntax(), LuaTokenKind::TkRightParen).as_ref(),
            plan,
        );

        if let Some(block) = closure.get_block() {
            analyze_boundary_comments_in_block(
                block.syntax(),
                syntax_id,
                LuaTokenKind::TkRightParen,
                plan,
            );
        }
    }
}

fn analyze_do_stat_layout(stat: &LuaDoStat, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
    analyze_boundary_comments_after_token(
        stat.syntax(),
        syntax_id,
        LuaTokenKind::TkDo,
        first_direct_token(stat.syntax(), LuaTokenKind::TkDo).as_ref(),
        plan,
    );
    if let Some(block) = stat.get_block() {
        analyze_boundary_comments_in_block(block.syntax(), syntax_id, LuaTokenKind::TkDo, plan);
    }
}

fn analyze_param_list_layout(params: &LuaParamList, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(params.syntax());
    let first_line_prefix_width = params
        .get_params()
        .next()
        .map(|param| source_line_prefix_width(param.syntax()))
        .unwrap_or(0);

    plan.layout.expr_sequences.insert(
        syntax_id,
        ExprSequenceLayoutPlan {
            first_line_prefix_width,
            preserve_multiline: false,
        },
    );
}

fn analyze_call_arg_list_layout(args: &LuaCallArgList, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(args.syntax());
    let first_line_prefix_width = args
        .get_args()
        .next()
        .map(|arg| source_line_prefix_width(arg.syntax()))
        .unwrap_or(0);

    plan.layout.expr_sequences.insert(
        syntax_id,
        ExprSequenceLayoutPlan {
            first_line_prefix_width,
            preserve_multiline: args.syntax().text().contains_char('\n'),
        },
    );
}

fn analyze_table_expr_layout(table: &LuaTableExpr, plan: &mut RootFormatPlan) {
    if table.is_empty() {
        return;
    }

    let syntax_id = LuaSyntaxId::from_node(table.syntax());
    let first_line_prefix_width = table
        .get_fields()
        .next()
        .map(|field| source_line_prefix_width(field.syntax()))
        .unwrap_or(0);

    plan.layout.expr_sequences.insert(
        syntax_id,
        ExprSequenceLayoutPlan {
            first_line_prefix_width,
            preserve_multiline: false,
        },
    );
}

fn analyze_statement_trivia_layout(
    node: &emmylua_parser::LuaSyntaxNode,
    syntax_id: LuaSyntaxId,
    plan: &mut RootFormatPlan,
) {
    if !node_has_direct_comment_child(node) {
        return;
    }

    let has_inline_comment = node
        .children()
        .filter_map(LuaComment::cast)
        .any(|comment| has_non_trivia_before_on_same_line_tokenwise(comment.syntax()));

    plan.layout
        .statement_trivia
        .insert(syntax_id, StatementTriviaLayoutPlan { has_inline_comment });
}

fn analyze_control_header_layout(
    node: &emmylua_parser::LuaSyntaxNode,
    syntax_id: LuaSyntaxId,
    plan: &mut RootFormatPlan,
) {
    if !node_has_direct_comment_child(node) {
        return;
    }

    let has_inline_comment = node
        .children()
        .filter_map(LuaComment::cast)
        .any(|comment| has_non_trivia_before_on_same_line_tokenwise(comment.syntax()));

    plan.layout
        .control_headers
        .insert(syntax_id, ControlHeaderLayoutPlan { has_inline_comment });
}

fn analyze_boundary_comments_after_token(
    node: &LuaSyntaxNode,
    owner_syntax_id: LuaSyntaxId,
    anchor_kind: LuaTokenKind,
    anchor_token: Option<&LuaSyntaxToken>,
    plan: &mut RootFormatPlan,
) {
    let Some(anchor_token) = anchor_token else {
        return;
    };

    let anchor_end = anchor_token.text_range().end();
    let comment_ids: Vec<_> = node
        .children()
        .filter(|child| {
            child.kind() == emmylua_parser::LuaKind::Syntax(emmylua_parser::LuaSyntaxKind::Comment)
        })
        .filter(|child| child.text_range().start() >= anchor_end)
        .map(|child| LuaSyntaxId::from_node(&child))
        .collect();

    record_boundary_comment_ids(owner_syntax_id, anchor_kind, None, comment_ids, plan);
}

fn analyze_boundary_comments_in_block(
    block: &LuaSyntaxNode,
    owner_syntax_id: LuaSyntaxId,
    anchor_kind: LuaTokenKind,
    plan: &mut RootFormatPlan,
) {
    let mut comment_ids = Vec::new();
    for child in block.children() {
        match child.kind() {
            emmylua_parser::LuaKind::Syntax(emmylua_parser::LuaSyntaxKind::Comment) => {
                comment_ids.push(LuaSyntaxId::from_node(&child));
            }
            _ => break,
        }
    }

    record_boundary_comment_ids(
        owner_syntax_id,
        anchor_kind,
        Some(LuaSyntaxId::from_node(block)),
        comment_ids,
        plan,
    );
}

fn record_boundary_comment_ids(
    owner_syntax_id: LuaSyntaxId,
    anchor_kind: LuaTokenKind,
    block_syntax_id: Option<LuaSyntaxId>,
    comment_ids: Vec<LuaSyntaxId>,
    plan: &mut RootFormatPlan,
) {
    if comment_ids.is_empty() {
        return;
    }

    let boundary_entry = plan
        .layout
        .boundary_comments
        .entry(owner_syntax_id)
        .or_default()
        .entry(anchor_kind)
        .or_default();
    for comment_id in &comment_ids {
        if !boundary_entry.comment_ids.contains(comment_id) {
            boundary_entry.comment_ids.push(*comment_id);
        }
    }

    if let Some(block_syntax_id) = block_syntax_id {
        let excluded_entry = plan
            .layout
            .block_excluded_comments
            .entry(block_syntax_id)
            .or_default();
        for comment_id in comment_ids {
            if !excluded_entry.contains(&comment_id) {
                excluded_entry.push(comment_id);
            }
        }
    }
}

fn first_direct_token(node: &LuaSyntaxNode, kind: LuaTokenKind) -> Option<LuaSyntaxToken> {
    node.children_with_tokens().find_map(|element| {
        let token = element.into_token()?;
        (token.kind() == kind.into()).then_some(token)
    })
}

fn analyze_statement_expr_list_layout(
    syntax_id: LuaSyntaxId,
    exprs: &[LuaExpr],
    plan: &mut RootFormatPlan,
) {
    if exprs.is_empty() {
        return;
    }

    let first_line_prefix_width = exprs
        .first()
        .map(|expr| source_line_prefix_width(expr.syntax()))
        .unwrap_or(0);
    let kind = if should_preserve_first_multiline_statement_value(exprs) {
        StatementExprListLayoutKind::PreserveFirstMultiline
    } else {
        StatementExprListLayoutKind::Sequence
    };

    plan.layout.statement_expr_lists.insert(
        syntax_id,
        build_expr_list_layout_plan(
            kind,
            first_line_prefix_width,
            should_attach_single_value_head(exprs),
            exprs.len() > 2,
        ),
    );
}

fn analyze_control_header_expr_list_layout(
    syntax_id: LuaSyntaxId,
    exprs: &[LuaExpr],
    plan: &mut RootFormatPlan,
) {
    if exprs.is_empty() {
        return;
    }

    let first_line_prefix_width = exprs
        .first()
        .map(|expr| source_line_prefix_width(expr.syntax()))
        .unwrap_or(0);
    let kind = if should_preserve_first_multiline_statement_value(exprs) {
        StatementExprListLayoutKind::PreserveFirstMultiline
    } else {
        StatementExprListLayoutKind::Sequence
    };

    plan.layout.control_header_expr_lists.insert(
        syntax_id,
        build_expr_list_layout_plan(kind, first_line_prefix_width, false, exprs.len() > 2),
    );
}

fn build_expr_list_layout_plan(
    kind: StatementExprListLayoutKind,
    first_line_prefix_width: usize,
    attach_single_value_head: bool,
    allow_packed: bool,
) -> StatementExprListLayoutPlan {
    StatementExprListLayoutPlan {
        kind,
        first_line_prefix_width,
        attach_single_value_head,
        allow_fill: true,
        allow_packed,
        allow_one_per_line: true,
        prefer_balanced_break_lines: true,
    }
}

fn should_preserve_first_multiline_statement_value(exprs: &[LuaExpr]) -> bool {
    exprs.len() > 1
        && exprs.first().is_some_and(|expr| {
            is_block_like_expr(expr) && expr.syntax().text().contains_char('\n')
        })
}

fn is_block_like_expr(expr: &LuaExpr) -> bool {
    matches!(expr, LuaExpr::ClosureExpr(_) | LuaExpr::TableExpr(_))
}

fn should_attach_single_value_head(exprs: &[LuaExpr]) -> bool {
    exprs.len() == 1
        && exprs.first().is_some_and(|expr| {
            is_block_like_expr(expr) || node_has_direct_comment_child(expr.syntax())
        })
}
