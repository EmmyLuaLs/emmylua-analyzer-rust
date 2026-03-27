mod tree;

use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaCallArgList, LuaChunk, LuaComment, LuaExpr,
    LuaForRangeStat, LuaForStat, LuaIfStat, LuaLocalStat, LuaParamList, LuaRepeatStat,
    LuaReturnStat, LuaSyntaxId, LuaTableExpr, LuaWhileStat,
};

use super::FormatContext;
use super::model::{
    ControlHeaderLayoutPlan, ExprSequenceLayoutPlan, RootFormatPlan, StatementExprListLayoutKind,
    StatementExprListLayoutPlan, StatementTriviaLayoutPlan,
};
use super::trivia::{
    has_non_trivia_before_on_same_line_tokenwise, node_has_direct_comment_child,
    source_line_prefix_width,
};

pub fn analyze_root_layout(
    _ctx: &FormatContext,
    chunk: &LuaChunk,
    mut plan: RootFormatPlan,
) -> RootFormatPlan {
    plan.layout.format_block_with_legacy = true;
    plan.layout.root_nodes = tree::collect_root_layout_nodes(chunk);
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
}

fn analyze_for_stat_layout(stat: &LuaForStat, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
    analyze_control_header_layout(stat.syntax(), syntax_id, plan);
    let exprs: Vec<_> = stat.get_iter_expr().collect();
    analyze_control_header_expr_list_layout(syntax_id, &exprs, plan);
}

fn analyze_for_range_stat_layout(stat: &LuaForRangeStat, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
    analyze_control_header_layout(stat.syntax(), syntax_id, plan);
    let exprs: Vec<_> = stat.get_expr_list().collect();
    analyze_control_header_expr_list_layout(syntax_id, &exprs, plan);
}

fn analyze_repeat_stat_layout(stat: &LuaRepeatStat, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
    analyze_control_header_layout(stat.syntax(), syntax_id, plan);
}

fn analyze_if_stat_layout(stat: &LuaIfStat, plan: &mut RootFormatPlan) {
    let syntax_id = LuaSyntaxId::from_node(stat.syntax());
    analyze_control_header_layout(stat.syntax(), syntax_id, plan);

    for clause in stat.get_else_if_clause_list() {
        let clause_id = LuaSyntaxId::from_node(clause.syntax());
        analyze_control_header_layout(clause.syntax(), clause_id, plan);
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
    use crate::formatter_new::model::{LayoutNodePlan, StatementExprListLayoutKind};

    use super::*;

    #[test]
    fn test_layout_collects_recursive_node_tree_with_comment_exception() {
        let config = LuaFormatConfig::default();
        let tree = LuaParser::parse(
            "-- hello\nlocal x = 1\n",
            ParserConfig::with_level(LuaLanguageLevel::Lua54),
        );
        let chunk = tree.get_chunk_node();
        let spacing_plan = crate::formatter_new::spacing::analyze_root_spacing(
            &crate::formatter_new::FormatContext::new(&config),
            &chunk,
        );
        let plan = analyze_root_layout(
            &crate::formatter_new::FormatContext::new(&config),
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
        let ctx = crate::formatter_new::FormatContext::new(&config);
        let spacing_plan = crate::formatter_new::spacing::analyze_root_spacing(&ctx, &chunk);
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
    }

    #[test]
    fn test_layout_collects_expr_sequence_metadata() {
        let config = LuaFormatConfig::default();
        let tree = LuaParser::parse(
            "local function foo(\n    a,\n    b\n)\n    return call(\n        foo,\n        bar\n    ), {\n        x = 1,\n        y = 2,\n    }\nend\n",
            ParserConfig::with_level(LuaLanguageLevel::Lua54),
        );
        let chunk = tree.get_chunk_node();
        let ctx = crate::formatter_new::FormatContext::new(&config);
        let spacing_plan = crate::formatter_new::spacing::analyze_root_spacing(&ctx, &chunk);
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
