use emmylua_parser::{
    LuaAstNode, LuaChunk, LuaLanguageLevel, LuaParser, LuaSyntaxId, LuaSyntaxToken, LuaTokenKind,
    ParserConfig,
};

use crate::{
    config::LuaFormatConfig,
    formatter::{
        FormatContext,
        model::{FormatPlan, TokenSpacingExpected},
        spacing::analyze_spacing,
    },
};

fn find_token(chunk: &LuaChunk, kind: LuaTokenKind) -> LuaSyntaxToken {
    chunk
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind().to_token() == kind)
        .unwrap()
}

#[test]
fn test_spacing_assign_defaults_to_single_spaces() {
    let config = LuaFormatConfig::default();
    let tree = LuaParser::parse(
        "local x=1\n",
        ParserConfig::with_level(LuaLanguageLevel::Lua54),
    );
    let chunk = tree.get_chunk_node();
    let mut plan = FormatPlan::from_config(&config);
    analyze_spacing(&FormatContext::new(&config), &chunk, &mut plan);
    let spacing = &plan.spacing;
    let assign = find_token(&chunk, LuaTokenKind::TkAssign);
    let assign_id = LuaSyntaxId::from_token(&assign);

    assert_eq!(
        spacing.left_expected(assign_id),
        Some(&TokenSpacingExpected::Space(1))
    );
    assert_eq!(
        spacing.right_expected(assign_id),
        Some(&TokenSpacingExpected::Space(1))
    );
}

#[test]
fn test_spacing_uses_call_paren_config() {
    let config = LuaFormatConfig {
        spacing: crate::config::SpacingConfig {
            space_before_call_paren: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let tree = LuaParser::parse(
        "foo(a)\n",
        ParserConfig::with_level(LuaLanguageLevel::Lua54),
    );
    let chunk = tree.get_chunk_node();
    let mut plan = FormatPlan::from_config(&config);
    analyze_spacing(&FormatContext::new(&config), &chunk, &mut plan);
    let spacing = &plan.spacing;
    let left_paren = find_token(&chunk, LuaTokenKind::TkLeftParen);
    let paren_id = LuaSyntaxId::from_token(&left_paren);

    assert_eq!(
        spacing.left_expected(paren_id),
        Some(&TokenSpacingExpected::Space(1))
    );
    assert_eq!(
        spacing.right_expected(paren_id),
        Some(&TokenSpacingExpected::Space(0))
    );
}

#[test]
fn test_spacing_respects_paren_expr_inner_space() {
    let config = LuaFormatConfig {
        spacing: crate::config::SpacingConfig {
            space_inside_parens: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let tree = LuaParser::parse(
        "local x = (a)\n",
        ParserConfig::with_level(LuaLanguageLevel::Lua54),
    );
    let chunk = tree.get_chunk_node();
    let mut plan = FormatPlan::from_config(&config);
    analyze_spacing(&FormatContext::new(&config), &chunk, &mut plan);
    let spacing = &plan.spacing;
    let left_paren = find_token(&chunk, LuaTokenKind::TkLeftParen);
    let right_paren = find_token(&chunk, LuaTokenKind::TkRightParen);

    assert_eq!(
        spacing.right_expected(LuaSyntaxId::from_token(&left_paren)),
        Some(&TokenSpacingExpected::Space(1))
    );
    assert_eq!(
        spacing.left_expected(LuaSyntaxId::from_token(&right_paren)),
        Some(&TokenSpacingExpected::Space(1))
    );
}

#[test]
fn test_spacing_respects_math_operator_config() {
    let config = LuaFormatConfig {
        spacing: crate::config::SpacingConfig {
            space_around_math_operator: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let tree = LuaParser::parse(
        "local x = a+b\n",
        ParserConfig::with_level(LuaLanguageLevel::Lua54),
    );
    let chunk = tree.get_chunk_node();
    let mut plan = FormatPlan::from_config(&config);
    analyze_spacing(&FormatContext::new(&config), &chunk, &mut plan);
    let spacing = &plan.spacing;
    let plus = find_token(&chunk, LuaTokenKind::TkPlus);
    let plus_id = LuaSyntaxId::from_token(&plus);

    assert_eq!(
        spacing.left_expected(plus_id),
        Some(&TokenSpacingExpected::Space(0))
    );
    assert_eq!(
        spacing.right_expected(plus_id),
        Some(&TokenSpacingExpected::Space(0))
    );
}

#[test]
fn test_spacing_collects_comment_prefix_replacement() {
    let config = LuaFormatConfig::default();
    let tree = LuaParser::parse(
        "--hello\n",
        ParserConfig::with_level(LuaLanguageLevel::Lua54),
    );
    let chunk = tree.get_chunk_node();
    let mut plan = FormatPlan::from_config(&config);
    analyze_spacing(&FormatContext::new(&config), &chunk, &mut plan);
    let spacing = &plan.spacing;
    let start = find_token(&chunk, LuaTokenKind::TkNormalStart);
    let start_id = LuaSyntaxId::from_token(&start);

    assert_eq!(spacing.token_replace(start_id), Some("-- "));
    assert_eq!(
        spacing.right_expected(start_id),
        Some(&TokenSpacingExpected::Space(0))
    );
}

#[test]
fn test_spacing_does_not_normalize_spaced_long_comment_prefix() {
    let config = LuaFormatConfig::default();
    let tree = LuaParser::parse(
        "-- [[ not a long comment ]]\n",
        ParserConfig::with_level(LuaLanguageLevel::Lua54),
    );
    let chunk = tree.get_chunk_node();
    let mut plan = FormatPlan::from_config(&config);
    analyze_spacing(&FormatContext::new(&config), &chunk, &mut plan);
    let spacing = &plan.spacing;
    let start = find_token(&chunk, LuaTokenKind::TkNormalStart);
    let start_id = LuaSyntaxId::from_token(&start);

    assert_eq!(spacing.token_replace(start_id), None);
    assert_eq!(spacing.right_expected(start_id), None);
}

#[test]
fn test_spacing_collects_doc_prefix_replacement() {
    let config = LuaFormatConfig::default();
    let tree = LuaParser::parse(
        "---@param x string\n",
        ParserConfig::with_level(LuaLanguageLevel::Lua54),
    );
    let chunk = tree.get_chunk_node();
    let mut plan = FormatPlan::from_config(&config);
    analyze_spacing(&FormatContext::new(&config), &chunk, &mut plan);
    let spacing = &plan.spacing;
    let start = find_token(&chunk, LuaTokenKind::TkDocStart);

    assert_eq!(
        spacing.token_replace(LuaSyntaxId::from_token(&start)),
        Some("---@")
    );
}
