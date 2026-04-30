use emmylua_parser::{
    LuaAstNode, LuaComment, LuaLanguageLevel, LuaParser, LuaSyntaxId, ParserConfig,
};

use crate::{
    config::LuaFormatConfig,
    formatter::{FormatContext, model::RootFormatPlan, render::render_comment_with_spacing},
    printer::Printer,
};

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
