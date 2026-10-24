#[cfg(test)]
mod tests {
    use crate::{
        LuaAst, LuaDocDescription, LuaExpr, LuaLocalStat, LuaParser, LuaVarExpr,
        parser::ParserConfig, syntax::traits::LuaAstNode,
    };

    #[allow(unused)]
    fn get_ast_node<N: LuaAstNode>(code: &str) -> N {
        let tree = LuaParser::parse(code, ParserConfig::default());
        let chunk = tree.get_chunk_node();
        let node = chunk.descendants::<N>().next().unwrap();
        node
    }

    #[test]
    fn test_iter_ast() {
        let code = r#"
            local a = 1
            local b = 2
            print(a + b)
        "#;
        let tree = LuaParser::parse(code, ParserConfig::default());

        let chunk = tree.get_chunk_node();
        for node in chunk.descendants::<LuaAst>() {
            println!("{:?}", node);
        }
    }

    #[test]
    fn test_local_stat1() {
        let code = "local a = 123";
        let local_stat = get_ast_node::<LuaLocalStat>(code);
        let mut name_list = local_stat.get_local_name_list();
        let local_name = name_list.next().unwrap();
        assert_eq!(
            format!("{:?}", local_name),
            r#"LuaLocalName { syntax: Syntax(LocalName)@6..7 }"#
        );
        let mut expr_list = local_stat.get_value_exprs();
        let expr = expr_list.next().unwrap();
        println!("{:?}", expr);
        assert_eq!(
            format!("{:?}", expr),
            r#"LiteralExpr(LuaLiteralExpr { syntax: Syntax(LiteralExpr)@10..13 })"#
        );
    }

    #[test]
    fn test_doc_type_on_the_tag_line() {
        let errors = |code: &str| {
            let tree = LuaParser::parse(code, ParserConfig::default());
            tree.get_errors()
                .iter()
                .map(|error| error.message.clone())
                .collect::<Vec<_>>()
        };
        let expected = vec!["expect type on the same line as the tag".to_string()];
        for code in [
            "---@param x\n--- integer\nlocal function f(x) end",
            "---@return\n--- string name\nlocal function f() end",
            "---@field x\n--- integer\nlocal t",
            "---@type\n--- integer\nlocal t",
            "---@alias B\n--- string\nlocal t",
            "local u = t --[[@as\n--- integer]]",
            "---@class D :\n--- A\nlocal t",
            "---@overload\n--- fun(): integer\nlocal function g() end",
            "---@param x\r\n--- integer\r\nlocal function f(x) end",
        ] {
            assert_eq!(errors(code), expected, "{code}");
        }

        // A type that starts on the tag's line can continue onto the next lines, and a `---|`
        // union can start on the next line.
        for code in [
            "---@alias C\n---| \"a\"\n---| \"b\"\nlocal t",
            "---@param y\n---| \"p\"\n---| \"q\"\nlocal function h(y) end",
            "---@type {\n---   a: integer,\n--- }\nlocal o",
            "---@type integer\n---| string\nlocal v",
            "---@return fun(\n---   a: integer\n---): string\nlocal function f() end",
        ] {
            assert!(errors(code).is_empty(), "{code}");
        }

        // A missing type is still reported once.
        assert_eq!(
            errors("---@param x\n---@return string\nlocal function f(x) end"),
            vec!["expect type".to_string()]
        );
    }

    #[test]
    fn test_name_token() {
        let code = "local a<const> = 123";
        let local_stat = get_ast_node::<LuaLocalStat>(code);
        let mut name_list = local_stat.get_local_name_list();
        let local_name1 = name_list.next().unwrap();
        let name = local_name1.get_name_token().unwrap();
        assert_eq!(name.get_name_text(), "a");
        let attrib = local_name1.get_attrib().unwrap();
        assert!(attrib.is_const());
    }

    #[test]
    fn test_iter_all_lua_ast() {
        let code = r#"
            local a = 1
            local b = 2
            local function foo(x)
                return x + 1
            end
            function f()
            end
            local c = foo(a + b)
            if c > 2 then
                print(c)
            elseif c == 2 then
            else
                print("c is not greater than 2")
            end
            for i = 1, 10 do
                print(i)
            end
            for i, v in ipairs({1, 2, 3}) do
                print(i, v)
            end
            while c < 10 do
                c = c + 1
            end
            repeat
                c = c - 1
            until c == 0
            ::ll::
            goto ll
            local a = c["string"]
        "#;
        let tree = LuaParser::parse(code, ParserConfig::default());
        let chunk = tree.get_chunk_node();
        for node in chunk.descendants::<LuaAst>() {
            match node {
                LuaAst::LuaChunk(lua_chunk) => {
                    assert!(lua_chunk.get_block().is_some());
                }
                LuaAst::LuaBlock(lua_block) => {
                    assert!(lua_block.get_stats().next().is_some());
                }
                LuaAst::LuaAssignStat(lua_assign_stat) => {
                    let (var_list, expr_list) = lua_assign_stat.get_var_and_expr_list();
                    assert!(!var_list.is_empty());
                    assert!(!expr_list.is_empty());
                }
                LuaAst::LuaLocalStat(lua_local_stat) => {
                    let mut name_list = lua_local_stat.get_local_name_list();
                    let local_name = name_list.next().unwrap();
                    assert!(local_name.get_name_token().is_some());
                    let mut expr_list = lua_local_stat.get_value_exprs();
                    assert!(expr_list.next().is_some());
                }
                LuaAst::LuaCallExprStat(lua_call_expr_stat) => {
                    assert!(lua_call_expr_stat.get_call_expr().is_some());
                }
                LuaAst::LuaLabelStat(lua_label_stat) => {
                    assert!(lua_label_stat.get_label_name_token().is_some());
                    assert_eq!(
                        lua_label_stat
                            .get_label_name_token()
                            .unwrap()
                            .get_name_text(),
                        "ll"
                    );
                }
                LuaAst::LuaGotoStat(lua_goto_stat) => {
                    assert!(lua_goto_stat.get_label_name_token().is_some());
                    assert_eq!(
                        lua_goto_stat
                            .get_label_name_token()
                            .unwrap()
                            .get_name_text(),
                        "ll"
                    );
                }
                LuaAst::LuaDoStat(lua_do_stat) => {
                    assert!(lua_do_stat.get_block().is_some());
                }
                LuaAst::LuaWhileStat(lua_while_stat) => {
                    assert!(lua_while_stat.get_condition_expr().is_some());
                    assert!(lua_while_stat.get_block().is_some());
                }
                LuaAst::LuaRepeatStat(lua_repeat_stat) => {
                    assert!(lua_repeat_stat.get_block().is_some());
                    assert!(lua_repeat_stat.get_condition_expr().is_some());
                }
                LuaAst::LuaIfStat(lua_if_stat) => {
                    assert!(lua_if_stat.get_condition_expr().is_some());
                    assert!(lua_if_stat.get_block().is_some());
                    assert!(lua_if_stat.get_else_if_clause_list().next().is_some());
                    assert!(lua_if_stat.get_else_clause().is_some());
                }
                LuaAst::LuaForStat(lua_for_stat) => {
                    assert!(lua_for_stat.get_var_name().is_some());
                    assert!(lua_for_stat.get_block().is_some());
                    assert_eq!(lua_for_stat.get_iter_expr().count(), 2);
                }
                LuaAst::LuaForRangeStat(lua_for_range_stat) => {
                    assert_eq!(lua_for_range_stat.get_var_name_list().count(), 2);
                    assert!(lua_for_range_stat.get_block().is_some());
                    assert!(lua_for_range_stat.get_expr_list().next().is_some());
                }
                LuaAst::LuaFuncStat(lua_func_stat) => {
                    assert!(lua_func_stat.get_func_name().is_some());
                    assert!(lua_func_stat.get_closure().is_some());
                }
                LuaAst::LuaLocalFuncStat(lua_local_func_stat) => {
                    assert!(lua_local_func_stat.get_local_name().is_some());
                    assert!(lua_local_func_stat.get_closure().is_some());
                }
                LuaAst::LuaReturnStat(lua_return_stat) => {
                    assert!(lua_return_stat.get_expr_list().next().is_some());
                }
                LuaAst::LuaNameExpr(lua_name_expr) => {
                    assert!(lua_name_expr.get_name_token().is_some());
                }
                LuaAst::LuaIndexExpr(lua_index_expr) => {
                    assert!(lua_index_expr.get_prefix_expr().is_some());
                }
                LuaAst::LuaTableExpr(lua_table_expr) => {
                    assert!(lua_table_expr.is_array());
                }
                _ => {}
            }
        }
    }

    #[test]
    fn test_doc_return_name_is_on_the_tag_line() {
        use crate::LuaDocTagReturn;
        let names = |code: &str| {
            get_ast_node::<LuaDocTagReturn>(code)
                .get_info_list()
                .into_iter()
                .map(|(_, name)| name.map(|name| name.get_name_text().to_string()))
                .collect::<Vec<_>>()
        };
        // A word on the next line starts a description, not the return's name.
        assert_eq!(
            names(
                "---@return string
--- The result.
local function f() end"
            ),
            [None]
        );
        assert_eq!(
            names(
                "---@return string name The result.
local function f() end"
            ),
            [Some("name".to_string())]
        );
        assert_eq!(
            names(
                "---@return integer a, string b Both.
local function f() end"
            ),
            [Some("a".to_string()), Some("b".to_string())]
        );
    }

    #[test]
    fn test_func_stat() {
        let code = r#"
        function f()
        end

        local t = {}
        function t:aaa()
        end
        "#;

        let tree = LuaParser::parse(code, ParserConfig::default());
        let chunk = tree.get_chunk_node();
        for node in chunk.descendants::<LuaAst>() {
            if let LuaAst::LuaFuncStat(func_stat) = node {
                match func_stat.get_func_name().unwrap() {
                    LuaVarExpr::NameExpr(name) => {
                        assert_eq!(name.get_name_token().unwrap().get_name_text(), "f");
                    }
                    LuaVarExpr::IndexExpr(field_exp) => {
                        if let LuaExpr::NameExpr(name) = field_exp.get_prefix_expr().unwrap() {
                            assert_eq!(name.get_name_token().unwrap().get_name_text(), "t");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_comment_description() {
        let code = r#"
            ---hello
            ---world
            ---      hihihi
        "#;
        let description = get_ast_node::<LuaDocDescription>(code);
        let expected = r#"hello
world
      hihihi"#;
        assert_eq!(description.get_description_text(), expected);

        let code2 = r#"
        ---Command-line arguments of Lua Standalone.
        ---
        ---[View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-arg"])
        "#;

        let description2 = get_ast_node::<LuaDocDescription>(code2);
        let expected2 = r#"Command-line arguments of Lua Standalone.

[View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-arg"])"#;
        assert_eq!(description2.get_description_text(), expected2);
    }
}
