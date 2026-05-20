use emmylua_parser::{
    LuaAst, LuaAstNode, LuaChunk, LuaLanguageLevel, LuaParser, LuaSyntaxId, LuaSyntaxKind,
    LuaTokenKind, ParserConfig,
};

use crate::config::LuaFormatConfig;
use crate::formatter::FormatContext;
use crate::formatter::layout::analyze_layout;
use crate::formatter::model::{FormatPlan, LayoutNodePlan, StatementExprListLayoutKind};
use crate::formatter::spacing::analyze_spacing;

fn setup_plan(chunk: &LuaChunk, config: &LuaFormatConfig) -> FormatPlan {
    let mut plan = FormatPlan::from_config(config);
    analyze_spacing(&FormatContext::new(config), chunk, &mut plan);
    analyze_layout(&FormatContext::new(config), chunk, &mut plan);
    plan
}

#[test]
fn test_layout_collects_recursive_node_tree_with_comment_exception() {
    let config = LuaFormatConfig::default();
    let tree = LuaParser::parse(
        "-- hello\nlocal x = 1\n",
        ParserConfig::with_level(LuaLanguageLevel::Lua54),
    );
    let chunk = tree.get_chunk_node();
    let plan = setup_plan(&chunk, &config);

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
    let plan = setup_plan(&chunk, &config);

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
    let inline_plan = setup_plan(&inline_if_chunk, &config);
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
    let plan = setup_plan(&chunk, &config);

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
    assert!(!call_layout.preserve_multiline);
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

#[test]
fn test_layout_prefers_multiline_call_args_from_source_when_enabled() {
    let config = LuaFormatConfig {
        layout: crate::config::LayoutConfig {
            prefer_call_args_layout_from_source: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let tree = LuaParser::parse(
        "local x = call(\n    foo,\n    bar\n)\n",
        ParserConfig::with_level(LuaLanguageLevel::Lua54),
    );
    let chunk = tree.get_chunk_node();
    let plan = setup_plan(&chunk, &config);

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
}

#[test]
fn test_layout_prefers_multiline_keyed_table_from_source_when_enabled() {
    let config = LuaFormatConfig {
        layout: crate::config::LayoutConfig {
            prefer_table_layout_from_source: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let tree = LuaParser::parse(
        "local t = {\n    a = 1,\n    b = 2\n}\n",
        ParserConfig::with_level(LuaLanguageLevel::Lua54),
    );
    let chunk = tree.get_chunk_node();
    let ctx = FormatContext::new(&config);
    let mut plan = FormatPlan::default();
    analyze_layout(&ctx, &chunk, &mut plan);

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
    assert!(table_layout.preserve_multiline);
}
