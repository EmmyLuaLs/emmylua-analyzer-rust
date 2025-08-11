use crate::util::sort_result;
use crate::{DescItem, LuaDescParser};
use emmylua_parser::{
    LuaAstNode, LuaDocDescription, LuaKind, LuaParser, LuaSyntaxKind, ParserConfig,
};
use googletest::prelude::*;

pub fn test(code: &str, mut parser: Box<dyn LuaDescParser>, expected: &str) -> Result<()> {
    let tree = LuaParser::parse(code, ParserConfig::default());
    let Some(desc) = tree
        .get_red_root()
        .descendants()
        .filter(|node| matches!(node.kind(), LuaKind::Syntax(LuaSyntaxKind::DocDescription)))
        .next()
    else {
        return fail!("No desc found in {:?}", tree.get_red_root());
    };
    let ranges = parser.parse(code, LuaDocDescription::cast(desc).unwrap());
    let result = format_result(code, ranges);

    let result_trimmed = result.trim();
    let expected_trimmed = expected.trim();

    expect_eq!(result_trimmed, expected_trimmed);

    Ok(())
}

#[allow(unused)]
pub fn print_result(code: &str, mut parser: Box<dyn LuaDescParser>) {
    let tree = LuaParser::parse(code, ParserConfig::default());
    let Some(desc) = tree
        .get_red_root()
        .descendants()
        .filter(|node| matches!(node.kind(), LuaKind::Syntax(LuaSyntaxKind::DocDescription)))
        .next()
    else {
        panic!("No desc found in {:?}", tree.get_red_root());
    };
    let ranges = parser.parse(code, LuaDocDescription::cast(desc).unwrap());
    let result = format_result(code, ranges);
    println!("{}", result);
}

pub fn format_result(text: &str, mut items: Vec<DescItem>) -> String {
    sort_result(&mut items);

    let mut pos = 0;
    let mut cur_items: Vec<DescItem> = Vec::new();
    let mut res = String::new();

    fn pop_cur_items(
        text: &str,
        cur_items: &mut Vec<DescItem>,
        pos: &mut usize,
        end: usize,
        res: &mut String,
    ) {
        while let Some(cur_item) = cur_items.last() {
            let cur_end: usize = cur_item.range.end().into();
            if cur_end <= end {
                *res += &text[*pos..cur_end];
                *pos = cur_end;
                *res += &format!("</{:?}>", cur_item.kind);
                cur_items.pop();
            } else {
                break;
            }
        }

        *res += &text[*pos..end];
        *pos = end;
    }

    for next_item in items {
        pop_cur_items(
            text,
            &mut cur_items,
            &mut pos,
            next_item.range.start().into(),
            &mut res,
        );
        res += &text[pos..next_item.range.start().into()];
        pos = next_item.range.start().into();
        res += &format!("<{:?}>", next_item.kind);
        cur_items.push(next_item);
    }

    pop_cur_items(text, &mut cur_items, &mut pos, text.len(), &mut res);
    res += &text[pos..];

    res
}
