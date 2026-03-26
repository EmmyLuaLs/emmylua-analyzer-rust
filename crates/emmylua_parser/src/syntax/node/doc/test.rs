#[cfg(test)]
mod test {
    use crate::{LuaAstNode, LuaComment, LuaKind, LuaParser, LuaTokenKind, ParserConfig};

    #[allow(unused)]
    fn print_ast(lua_code: &str) {
        let tree = LuaParser::parse(lua_code, ParserConfig::default());
        println!("{:#?}", tree.get_red_root());
    }

    #[test]
    fn test_comment() {
        let code = r#"
        -- 1 comment
        local t = 123 -- 2 comment

        local c = {
            aa = 1123, -- 3 comment
            bb = 123, --[[4 comment]]
            -- 5 comment
            qi = 123,
        }
        "#;

        let tree = LuaParser::parse(code, ParserConfig::default());
        let root = tree.get_chunk_node();
        let mut comment_iter = root.descendants::<LuaComment>();
        let comment_1 = comment_iter.next().unwrap();
        assert_eq!(
            comment_1.get_description().unwrap().get_description_text(),
            "1 comment"
        );
        assert_eq!(
            comment_1.get_owner().unwrap().syntax().text(),
            "local t = 123"
        );

        let comment_2 = comment_iter.next().unwrap();
        assert_eq!(
            comment_2.get_description().unwrap().get_description_text(),
            "2 comment"
        );
        assert_eq!(
            comment_2.get_owner().unwrap().syntax().text(),
            "local t = 123"
        );

        let comment_3 = comment_iter.next().unwrap();
        assert_eq!(
            comment_3.get_description().unwrap().get_description_text(),
            "3 comment"
        );
        assert_eq!(comment_3.get_owner().unwrap().syntax().text(), "aa = 1123");

        let comment_4 = comment_iter.next().unwrap();
        assert_eq!(
            comment_4.get_description().unwrap().get_description_text(),
            "4 comment"
        );
        assert_eq!(comment_4.get_owner().unwrap().syntax().text(), "bb = 123");

        let comment_5 = comment_iter.next().unwrap();
        assert_eq!(
            comment_5.get_description().unwrap().get_description_text(),
            "5 comment"
        );
        assert_eq!(comment_5.get_owner().unwrap().syntax().text(), "qi = 123");
    }

    #[test]
    fn test_description() {
        let code = r#"
--- yeysysf
---@class Test
--- oooo
---@class Test2
---
---hhhh
---@field a string

        "#;

        print_ast(code);
    }

    #[test]
    fn test_doc_type_with_inline_comment_marker_has_second_prefix_on_same_line() {
        let code = "---@type string --1\nlocal s\n";

        let tree = LuaParser::parse(code, ParserConfig::default());
        let root = tree.get_chunk_node();
        let comment = root.descendants::<LuaComment>().next().unwrap();

        let prefix_tokens: Vec<_> = comment
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| {
                let token = element.into_token()?;
                matches!(
                    token.kind(),
                    LuaKind::Token(
                        LuaTokenKind::TkDocStart
                            | LuaTokenKind::TkDocLongStart
                            | LuaTokenKind::TkDocContinue
                            | LuaTokenKind::TkDocContinueOr
                            | LuaTokenKind::TkNormalStart
                    )
                )
                .then_some((token.kind(), token.text().to_string()))
            })
            .collect();

        assert_eq!(
            prefix_tokens,
            vec![
                (LuaKind::Token(LuaTokenKind::TkDocStart), "---@".to_string()),
                (LuaKind::Token(LuaTokenKind::TkNormalStart), "--".to_string()),
            ]
        );
    }
}
