#[cfg(test)]
mod tests {
    use emmylua_parser::LuaLanguageLevel;

    use crate::{
        SourceText, assert_format, config::LuaFormatConfig, reformat_lua_code,
        reformat_lua_code_new,
    };

    // ========== shebang ==========

    #[test]
    fn test_shebang_preserved() {
        assert_format!(
            "#!/usr/bin/lua\nlocal a=1\n",
            "#!/usr/bin/lua\nlocal a = 1\n"
        );
    }

    #[test]
    fn test_shebang_env() {
        assert_format!(
            "#!/usr/bin/env lua\nprint(1)\n",
            "#!/usr/bin/env lua\nprint(1)\n"
        );
    }

    #[test]
    fn test_shebang_with_code() {
        assert_format!(
            "#!/usr/bin/lua\nlocal x=1\nlocal y=2\n",
            "#!/usr/bin/lua\nlocal x = 1\nlocal y = 2\n"
        );
    }

    #[test]
    fn test_no_shebang() {
        // Ensure normal code without shebang still works
        assert_format!("local a = 1\n", "local a = 1\n");
    }

    // ========== long string preservation ==========

    #[test]
    fn test_long_string_preserves_trailing_spaces() {
        // Long string content including trailing spaces must be preserved exactly
        assert_format!(
            "local s = [[  hello   \n  world   \n]]\n",
            "local s = [[  hello   \n  world   \n]]\n"
        );
    }

    // ========== idempotency ==========

    #[test]
    fn test_idempotency_basic() {
        let config = LuaFormatConfig::default();
        let input = r#"
local a   =   1
local bbb   =   2
if true
then
return   a  +  bbb
end
"#
        .trim_start_matches('\n');

        let first = crate::reformat_lua_code(
            &SourceText {
                text: input,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );
        let second = crate::reformat_lua_code(
            &SourceText {
                text: &first,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );
        assert_eq!(
            first, second,
            "Formatter is not idempotent!\nFirst pass:\n{first}\nSecond pass:\n{second}"
        );
    }

    #[test]
    fn test_idempotency_table() {
        let config = LuaFormatConfig::default();
        let input = r#"
local t = {
    a = 1,
    bbb = 2,
    cc = 3,
}
"#
        .trim_start_matches('\n');

        let first = reformat_lua_code(
            &SourceText {
                text: input,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );
        let second = reformat_lua_code(
            &SourceText {
                text: &first,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );
        assert_eq!(
            first, second,
            "Formatter is not idempotent for tables!\nFirst pass:\n{first}\nSecond pass:\n{second}"
        );
    }

    #[test]
    fn test_idempotency_complex() {
        let config = LuaFormatConfig::default();
        let input = r#"
local function foo(a, b, c)
    local x = a + b * c
    if x > 10 then
        return {
            result = x,
            name = "test",
            flag = true,
        }
    end

    for i = 1, 10 do
        print(i)
    end

    local t = { 1, 2, 3 }
    return t
end
"#
        .trim_start_matches('\n');

        let first = crate::reformat_lua_code(
            &SourceText {
                text: input,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );
        let second = crate::reformat_lua_code(
            &SourceText {
                text: &first,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );
        assert_eq!(
            first, second,
            "Formatter is not idempotent for complex code!\nFirst pass:\n{first}\nSecond pass:\n{second}"
        );
    }

    #[test]
    fn test_idempotency_alignment() {
        let config = LuaFormatConfig::default();
        let input = r#"
local a = 1 -- comment a
local bbb = 2 -- comment b
local cc = 3 -- comment c
"#
        .trim_start_matches('\n');

        let first = crate::reformat_lua_code(
            &SourceText {
                text: input,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );
        let second = crate::reformat_lua_code(
            &SourceText {
                text: &first,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );
        assert_eq!(
            first, second,
            "Formatter is not idempotent for aligned code!\nFirst pass:\n{first}\nSecond pass:\n{second}"
        );
    }

    #[test]
    fn test_idempotency_method_chain() {
        let config = LuaFormatConfig {
            layout: crate::config::LayoutConfig {
                max_line_width: 40,
                ..Default::default()
            },
            ..Default::default()
        };
        let input = "local x = obj:method1():method2():method3()\n";

        let first = crate::reformat_lua_code(
            &SourceText {
                text: input,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );
        let second = crate::reformat_lua_code(
            &SourceText {
                text: &first,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );
        assert_eq!(
            first, second,
            "Formatter is not idempotent for method chains!\nFirst pass:\n{first}\nSecond pass:\n{second}"
        );
    }

    #[test]
    fn test_idempotency_shebang() {
        let config = LuaFormatConfig::default();
        let input = "#!/usr/bin/lua\nlocal a   =   1\n";

        let first = crate::reformat_lua_code(
            &SourceText {
                text: input,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );
        let second = crate::reformat_lua_code(
            &SourceText {
                text: &first,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );
        assert_eq!(
            first, second,
            "Formatter is not idempotent with shebang!\nFirst pass:\n{first}\nSecond pass:\n{second}"
        );
    }

    #[test]
    fn test_new_formatter_root_pipeline_smoke() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "local value = 1\n",
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code_new(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_comment_and_block_path() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "--hello\nlocal value=1\n",
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code_new(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_local_assign_return_statements() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "local a,b=foo,bar\na,b=foo(),bar()\nreturn foo, bar, baz\n",
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code_new(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_statement_spacing_config_parity() {
        let mut config = LuaFormatConfig::default();
        config.spacing.space_around_assign_operator = false;

        let source = SourceText {
            text: "local a, b = foo, bar\nx, y = 1, 2\nreturn a, y\n",
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code_new(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_trivia_aware_statement_sequences() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "local a, -- lhs\n    b = -- eq\n    foo, -- rhs\n    bar\na, -- lhs\n    b = -- eq\n    foo, -- rhs\n    bar\nreturn -- head\n    foo, -- rhs\n    bar\n",
            level: LuaLanguageLevel::default(),
        };

        let first = reformat_lua_code_new(&source, &config);
        let second = reformat_lua_code_new(
            &SourceText {
                text: &first,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );

        assert_eq!(first, second);
    }

    #[test]
    fn test_new_formatter_trivia_aware_statement_spacing_config_parity() {
        let mut config = LuaFormatConfig::default();
        config.spacing.space_around_assign_operator = false;

        let source = SourceText {
            text: "local a, -- lhs\n    b = -- eq\n    foo, -- rhs\n    bar\nreturn -- head\n    foo, -- rhs\n    bar\n",
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code_new(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_call_and_table_sequences() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "local result=foo(1,2,3)\nlocal tbl={1,2,3}\n",
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code_new(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_while_statements() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "while foo(a, b) do\n    local x = 1\n    return x\nend\n",
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code_new(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_for_statements() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "for i = foo(), bar, baz do\n    local x = i\nend\nfor k, v in pairs(tbl), next(tbl) do\n    return v\nend\n",
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code_new(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_repeat_statements() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "repeat\n    local x = foo()\nuntil bar(x)\n",
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code_new(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_if_statements() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "if ok then return value end\nif foo(a, b) then\n    local x = 1\nelseif bar then\n    return baz\nelse\n    return qux\nend\n",
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code_new(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_trivia_aware_while_header_parity() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "while foo -- cond\ndo\n    return bar\nend\n",
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code_new(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_trivia_aware_for_header_parity() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "for i, -- lhs\n    j = -- eq\n    foo, -- rhs\n    bar do\n    return i\nend\nfor k, -- lhs\n    v in -- in\n    pairs(tbl), -- rhs\n    next(tbl) do\n    return v\nend\n",
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code_new(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_trivia_aware_repeat_header_parity() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "repeat\n    return foo\nuntil -- cond\n    bar(baz)\n",
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code_new(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_trivia_aware_if_parity() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "if foo -- cond\nthen\n    return a\nelseif bar -- cond\nthen\n    return b\nelse\n    return c\nend\n",
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code_new(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_basic_call_arg_shapes() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "local result = foo(a, {1,2}, bar(b, c))\n",
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code_new(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_call_arg_comment_attachment_idempotent() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "local result = foo(\n    -- first\n    a, -- trailing a\n    b,\n    -- last\n)\n",
            level: LuaLanguageLevel::default(),
        };

        let first = reformat_lua_code_new(&source, &config);
        let second = reformat_lua_code_new(
            &SourceText {
                text: &first,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );

        assert_eq!(first, second);
    }

    #[test]
    fn test_new_formatter_closure_params_idempotent() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "local fn = function(a,b,c)\nreturn a\nend\n",
            level: LuaLanguageLevel::default(),
        };

        let first = reformat_lua_code_new(&source, &config);
        let second = reformat_lua_code_new(
            &SourceText {
                text: &first,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );

        assert_eq!(first, second);
    }

    #[test]
    fn test_new_formatter_param_comment_attachment_idempotent() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "local fn = function(\n    -- first\n    a, -- trailing a\n    b,\n    -- tail\n)\nreturn a\nend\n",
            level: LuaLanguageLevel::default(),
        };

        let first = reformat_lua_code_new(&source, &config);
        let second = reformat_lua_code_new(
            &SourceText {
                text: &first,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );

        assert_eq!(first, second);
    }

    #[test]
    fn test_new_formatter_closure_shell_comments_idempotent() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "local fn = function -- before params\n(a) -- before body\n-- body comment\nreturn a\nend\n",
            level: LuaLanguageLevel::default(),
        };

        let first = reformat_lua_code_new(&source, &config);
        let second = reformat_lua_code_new(
            &SourceText {
                text: &first,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );

        assert_eq!(first, second);
    }

    #[test]
    fn test_new_formatter_renders_table_field_key_value_shapes() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "local tbl={a=1,[\"b\"]=2,[3]=4,[foo]=bar}\n",
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code_new(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_table_comment_attachment_idempotent() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: "local tbl = {\n    -- lead\n    a = 1, -- trailing\n    b = 2,\n    -- tail\n}\n",
            level: LuaLanguageLevel::default(),
        };

        let first = reformat_lua_code_new(&source, &config);
        let second = reformat_lua_code_new(
            &SourceText {
                text: &first,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );

        assert_eq!(first, second);
    }
}
