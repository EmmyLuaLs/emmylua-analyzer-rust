mod tree;

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

#[cfg(test)]
mod tests {
    use emmylua_parser::{LuaAstNode, LuaLanguageLevel, LuaParser, LuaSyntaxKind, ParserConfig};

    use crate::config::LuaFormatConfig;
    use crate::formatter::model::{LayoutNodePlan, StatementExprListLayoutKind};

    use super::*;

    #[test]
    fn test_layout_collects_recursive_node_tree_with_comment_exception() {
        let config = LuaFormatConfig::default();
        let tree = LuaParser::parse(
            "-- hello\nlocal x = 1\n",
            ParserConfig::with_level(LuaLanguageLevel::Lua54),
        );
        let chunk = tree.get_chunk_node();
        let spacing_plan = crate::formatter::spacing::analyze_root_spacing(
            &crate::formatter::FormatContext::new(&config),
            &chunk,
        );
        let plan = analyze_root_layout(
            &crate::formatter::FormatContext::new(&config),
            &chunk,
            spacing_plan,
        );

        assert_eq!(plan.layout.root_nodes.len(), 1);
        let LayoutNodePlan::Syntax(block) = &plan.layout.root_nodes[0] else {
            panic!("expected block syntax node");
        };
        assert_eq!(block.kind, LuaSyntaxKind::Block);
        assert_eq!(block.children.len(), 2);
        assert!(matches!(block.children[0], LayoutNodePlan::Comment(_)));
        assert!(matches!(block.children[1], LayoutNodePlan::Syntax(_)));

        let LayoutNodePlan::Comment(comment) = &block.children[0] else {
            panic!("expected comment child");
        };
        assert_eq!(comment.syntax_id.get_kind(), LuaSyntaxKind::Comment);
    }

    #[test]
    fn test_layout_collects_statement_trivia_and_expr_list_metadata() {
        let config = LuaFormatConfig::default();
        let tree = LuaParser::parse(
            "local a, -- lhs\n    b = {\n        1,\n        2,\n    }, c\nreturn -- head\n    foo, bar\nreturn\n    -- standalone\n    baz\n",
            ParserConfig::with_level(LuaLanguageLevel::Lua54),
        );
        let chunk = tree.get_chunk_node();
        let ctx = crate::formatter::FormatContext::new(&config);
        let spacing_plan = crate::formatter::spacing::analyze_root_spacing(&ctx, &chunk);
        let plan = analyze_root_layout(&ctx, &chunk, spacing_plan);

        let local_stat = chunk
            .syntax()
            .descendants()
            .find_map(emmylua_parser::LuaLocalStat::cast)
            .expect("expected local stat");
        let local_layout = plan
            .layout
            .statement_trivia
            .get(&LuaSyntaxId::from_node(local_stat.syntax()))
            .expect("expected local trivia layout");
        assert!(local_layout.has_inline_comment);

        let local_expr_layout = plan
            .layout
            .statement_expr_lists
            .get(&LuaSyntaxId::from_node(local_stat.syntax()))
            .expect("expected local expr layout");
        assert_eq!(
            local_expr_layout.kind,
            StatementExprListLayoutKind::PreserveFirstMultiline
        );
        assert!(!local_expr_layout.attach_single_value_head);
        assert!(local_expr_layout.allow_fill);
        assert!(!local_expr_layout.allow_packed);
        assert!(local_expr_layout.allow_one_per_line);

        let return_stats: Vec<_> = chunk
            .syntax()
            .descendants()
            .filter_map(emmylua_parser::LuaReturnStat::cast)
            .collect();
        assert_eq!(return_stats.len(), 2);

        let inline_return_layout = plan
            .layout
            .statement_trivia
            .get(&LuaSyntaxId::from_node(return_stats[0].syntax()))
            .expect("expected inline return trivia layout");
        assert!(inline_return_layout.has_inline_comment);

        let standalone_return_layout = plan
            .layout
            .statement_trivia
            .get(&LuaSyntaxId::from_node(return_stats[1].syntax()))
            .expect("expected standalone return trivia layout");
        assert!(!standalone_return_layout.has_inline_comment);

        let while_stat = chunk
            .syntax()
            .descendants()
            .find_map(emmylua_parser::LuaWhileStat::cast);
        assert!(while_stat.is_none());

        let inline_if_tree = LuaParser::parse(
            "if ok then -- note\n    print(1)\nelseif retry then -- retry note\n    print(2)\nelse -- fallback note\n    print(3)\nend\n",
            ParserConfig::with_level(LuaLanguageLevel::Lua54),
        );
        let inline_if_chunk = inline_if_tree.get_chunk_node();
        let inline_ctx = crate::formatter::FormatContext::new(&config);
        let inline_spacing =
            crate::formatter::spacing::analyze_root_spacing(&inline_ctx, &inline_if_chunk);
        let inline_plan = analyze_root_layout(&inline_ctx, &inline_if_chunk, inline_spacing);
        let if_stat = inline_if_chunk
            .syntax()
            .descendants()
            .find_map(emmylua_parser::LuaIfStat::cast)
            .expect("expected if stat");
        let if_boundary = inline_plan
            .layout
            .boundary_comments
            .get(&LuaSyntaxId::from_node(if_stat.syntax()))
            .expect("expected if boundary comment layout");
        assert_eq!(
            if_boundary
                .get(&LuaTokenKind::TkThen)
                .unwrap()
                .comment_ids
                .len(),
            1
        );

        let else_if_clause = if_stat
            .get_else_if_clause_list()
            .next()
            .expect("expected elseif clause");
        let else_if_boundary = inline_plan
            .layout
            .boundary_comments
            .get(&LuaSyntaxId::from_node(else_if_clause.syntax()))
            .expect("expected elseif boundary comment layout");
        assert_eq!(
            else_if_boundary
                .get(&LuaTokenKind::TkThen)
                .unwrap()
                .comment_ids
                .len(),
            1
        );

        let else_clause = if_stat.get_else_clause().expect("expected else clause");
        let else_boundary = inline_plan
            .layout
            .boundary_comments
            .get(&LuaSyntaxId::from_node(else_clause.syntax()))
            .expect("expected else boundary comment layout");
        assert_eq!(
            else_boundary
                .get(&LuaTokenKind::TkElse)
                .unwrap()
                .comment_ids
                .len(),
            1
        );
    }

    #[test]
    fn test_layout_collects_expr_sequence_metadata() {
        let config = LuaFormatConfig::default();
        let tree = LuaParser::parse(
            "local function foo(\n    a,\n    b\n)\n    return call(\n        foo,\n        bar\n    ), {\n        x = 1,\n        y = 2,\n    }\nend\n",
            ParserConfig::with_level(LuaLanguageLevel::Lua54),
        );
        let chunk = tree.get_chunk_node();
        let ctx = crate::formatter::FormatContext::new(&config);
        let spacing_plan = crate::formatter::spacing::analyze_root_spacing(&ctx, &chunk);
        let plan = analyze_root_layout(&ctx, &chunk, spacing_plan);

        let param_list = chunk
            .descendants::<LuaAst>()
            .find_map(|node| match node {
                LuaAst::LuaParamList(node) => Some(node),
                _ => None,
            })
            .expect("expected param list");
        let param_layout = plan
            .layout
            .expr_sequences
            .get(&LuaSyntaxId::from_node(param_list.syntax()))
            .expect("expected param layout");
        assert!(!param_layout.preserve_multiline);
        assert!(param_layout.first_line_prefix_width > 0);

        let call_args = chunk
            .descendants::<LuaAst>()
            .find_map(|node| match node {
                LuaAst::LuaCallArgList(node) => Some(node),
                _ => None,
            })
            .expect("expected call arg list");
        let call_layout = plan
            .layout
            .expr_sequences
            .get(&LuaSyntaxId::from_node(call_args.syntax()))
            .expect("expected call arg layout");
        assert!(call_layout.preserve_multiline);
        assert!(call_layout.first_line_prefix_width > 0);

        let table_expr = chunk
            .descendants::<LuaAst>()
            .find_map(|node| match node {
                LuaAst::LuaTableExpr(node) => Some(node),
                _ => None,
            })
            .expect("expected table expr");
        let table_layout = plan
            .layout
            .expr_sequences
            .get(&LuaSyntaxId::from_node(table_expr.syntax()))
            .expect("expected table layout");
        assert!(!table_layout.preserve_multiline);
        assert!(table_layout.first_line_prefix_width > 0);
    }
}
