#[cfg(test)]
mod test {
    use crate::{
        LuaAstNode, LuaComment, LuaCommentFormatDirective, LuaDocTagClass, LuaKind, LuaParser,
        LuaTokenKind, ParserConfig,
    };

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
                (
                    LuaKind::Token(LuaTokenKind::TkNormalStart),
                    "--".to_string()
                ),
            ]
        );
    }

    #[test]
    fn test_comment_format_directive_only_recognizes_fmt_on_off() {
        let tree = LuaParser::parse(
            "-- fmt: off\nlocal a = 1\n-- fmt: on\n",
            ParserConfig::default(),
        );
        let root = tree.get_chunk_node();
        let mut comments = root.descendants::<LuaComment>();

        assert_eq!(
            comments.next().unwrap().get_format_directive(),
            Some(LuaCommentFormatDirective::FormatOff)
        );
        assert_eq!(
            comments.next().unwrap().get_format_directive(),
            Some(LuaCommentFormatDirective::FormatOn)
        );
    }

    #[test]
    fn test_doc_comment_is_not_treated_as_format_directive() {
        let tree = LuaParser::parse("--- fmt: off\nlocal a = 1\n", ParserConfig::default());
        let root = tree.get_chunk_node();
        let comment = root.descendants::<LuaComment>().next().unwrap();

        assert_eq!(comment.get_format_directive(), None);
    }

    #[test]
    fn test_doc_generic_default_type_accessor() {
        let tree = LuaParser::parse(
            "---@class A<T = number>\n---@class B<T extends number = string>\n",
            ParserConfig::default(),
        );
        let root = tree.get_chunk_node();
        let mut classes = root.descendants::<LuaDocTagClass>();

        let class_a = classes.next().unwrap();
        let param_a = class_a
            .get_generic_decl()
            .unwrap()
            .get_generic_decl()
            .next()
            .unwrap();
        assert!(param_a.get_constraint_type().is_none());
        assert_eq!(
            param_a
                .get_default_type()
                .unwrap()
                .syntax()
                .text()
                .to_string(),
            "number"
        );

        let class_b = classes.next().unwrap();
        let param_b = class_b
            .get_generic_decl()
            .unwrap()
            .get_generic_decl()
            .next()
            .unwrap();
        assert_eq!(
            param_b
                .get_constraint_type()
                .unwrap()
                .syntax()
                .text()
                .to_string(),
            "number"
        );
        assert_eq!(
            param_b
                .get_default_type()
                .unwrap()
                .syntax()
                .text()
                .to_string(),
            "string"
        );
    }
}
