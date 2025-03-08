#[cfg(test)]
mod tests {
    use crate::{
        parser::ParserConfig, syntax::traits::LuaAstNode, LuaAst, LuaDocDescription, LuaExpr,
        LuaLocalStat, LuaParser, LuaVarExpr,
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
                    assert!(var_list.len() > 0);
                    assert!(expr_list.len() > 0);
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
            match node {
                LuaAst::LuaFuncStat(func_stat) => match func_stat.get_func_name().unwrap() {
                    LuaVarExpr::NameExpr(name) => {
                        assert_eq!(name.get_name_token().unwrap().get_name_text(), "f");
                    }
                    LuaVarExpr::IndexExpr(field_exp) => {
                        match field_exp.get_prefix_expr().unwrap() {
                            LuaExpr::NameExpr(name) => {
                                assert_eq!(name.get_name_token().unwrap().get_name_text(), "t");
                            }
                            _ => {}
                        }
                    }
                },
                _ => {}
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
