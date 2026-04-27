#[cfg(test)]
mod tests {
    use crate::{SourceText, assert_format, config::LuaFormatConfig, reformat_lua_code};
    use emmylua_parser::LuaLanguageLevel;

    // ========== shebang ==========

    #[test]
    fn test_shebang_preserved() {
        assert_format!(
            r#"#!/usr/bin/lua
local a=1
"#,
            r#"#!/usr/bin/lua
local a = 1
"#
        );
    }

    #[test]
    fn test_shebang_env() {
        assert_format!(
            r#"#!/usr/bin/env lua
print(1)
"#,
            r#"#!/usr/bin/env lua
print(1)
"#
        );
    }

    #[test]
    fn test_shebang_with_code() {
        assert_format!(
            r#"#!/usr/bin/lua
local x=1
local y=2
"#,
            r#"#!/usr/bin/lua
local x = 1
local y = 2
"#
        );
    }

    #[test]
    fn test_no_shebang() {
        // Ensure normal code without shebang still works
        assert_format!(
            r#"local a = 1
"#,
            r#"local a = 1
"#
        );
    }

    // ========== long string preservation ==========

    #[test]
    fn test_long_string_preserves_trailing_spaces() {
        // Long string content including trailing spaces must be preserved exactly
        assert_format!(
            r#"local s = [[  hello   
  world   
]]
"#,
            r#"local s = [[  hello   
  world   
]]
"#
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
            r#"Formatter is not idempotent!
First pass:
{first}
Second pass:
{second}"#
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
            r#"Formatter is not idempotent for tables!
First pass:
{first}
Second pass:
{second}"#
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
            r#"Formatter is not idempotent for complex code!
First pass:
{first}
Second pass:
{second}"#
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
            r#"Formatter is not idempotent for aligned code!
First pass:
{first}
Second pass:
{second}"#
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
        let input = r#"local x = obj:method1():method2():method3()
"#;

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
            r#"Formatter is not idempotent for method chains!
First pass:
{first}
Second pass:
{second}"#
        );
    }

    #[test]
    fn test_format_disable_range_preserves_block_contents() {
        assert_format!(
            r#"local a=1
-- fmt: off
local   ugly   =    { 1,2,3 }
print(  ugly [ 1 ] )
-- fmt: on
local b=2
"#,
            r#"local a = 1
-- fmt: off
local   ugly   =    { 1,2,3 }
print(  ugly [ 1 ] )
-- fmt: on
local b = 2
"#
        );
    }

    #[test]
    fn test_format_disable_range_can_cover_single_statement() {
        assert_format!(
            r#"local a=1
-- fmt: off
local   ugly   =    { 1,2,3 }
-- fmt: on
local c=3
"#,
            r#"local a = 1
-- fmt: off
local   ugly   =    { 1,2,3 }
-- fmt: on
local c = 3
"#
        );
    }

    #[test]
    fn test_idempotency_shebang() {
        let config = LuaFormatConfig::default();
        let input = r#"#!/usr/bin/lua
local a   =   1
"#;

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
            r#"Formatter is not idempotent with shebang!
First pass:
{first}
Second pass:
{second}"#
        );
    }

    #[test]
    fn test_new_formatter_root_pipeline_smoke() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: r#"local value = 1
"#,
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_comment_and_block_path() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: r#"--hello
local value=1
"#,
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_local_assign_return_statements() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: r#"local a,b=foo,bar
a,b=foo(),bar()
return foo, bar, baz
"#,
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_statement_spacing_config_parity() {
        let mut config = LuaFormatConfig::default();
        config.spacing.space_around_assign_operator = false;

        let source = SourceText {
            text: r#"local a, b = foo, bar
x, y = 1, 2
return a, y
"#,
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_trivia_aware_statement_sequences() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: r#"local a, -- lhs
    b = -- eq
    foo, -- rhs
    bar
a, -- lhs
    b = -- eq
    foo, -- rhs
    bar
return -- head
    foo, -- rhs
    bar
"#,
            level: LuaLanguageLevel::default(),
        };

        let first = reformat_lua_code(&source, &config);
        let second = reformat_lua_code(
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
            text: r#"local a, -- lhs
    b = -- eq
    foo, -- rhs
    bar
return -- head
    foo, -- rhs
    bar
"#,
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_call_and_table_sequences() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: r#"local result=foo(1,2,3)
local tbl={1,2,3}
"#,
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_while_statements() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: r#"while foo(a, b) do
    local x = 1
    return x
end
"#,
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_for_statements() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: r#"for i = foo(), bar, baz do
    local x = i
end
for k, v in pairs(tbl), next(tbl) do
    return v
end
"#,
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_repeat_statements() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: r#"repeat
    local x = foo()
until bar(x)
"#,
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_if_statements() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: r#"if ok then return value end
if foo(a, b) then
    local x = 1
elseif bar then
    return baz
else
    return qux
end
"#,
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_trivia_aware_while_header_parity() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: r#"while foo -- cond
do
    return bar
end
"#,
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_trivia_aware_for_header_parity() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: r#"for i, -- lhs
    j = -- eq
    foo, -- rhs
    bar do
    return i
end
for k, -- lhs
    v in -- in
    pairs(tbl), -- rhs
    next(tbl) do
    return v
end
"#,
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_trivia_aware_repeat_header_parity() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: r#"repeat
    return foo
until -- cond
    bar(baz)
"#,
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_trivia_aware_if_parity() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: r#"if foo -- cond
then
    return a
elseif bar -- cond
then
    return b
else
    return c
end
"#,
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_renders_basic_call_arg_shapes() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: r#"local result = foo(a, {1,2}, bar(b, c))
"#,
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_call_arg_comment_attachment_idempotent() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: r#"local result = foo(
    -- first
    a, -- trailing a
    b,
    -- last
)
"#,
            level: LuaLanguageLevel::default(),
        };

        let first = reformat_lua_code(&source, &config);
        let second = reformat_lua_code(
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
            text: r#"local fn = function(a,b,c)
return a
end
"#,
            level: LuaLanguageLevel::default(),
        };

        let first = reformat_lua_code(&source, &config);
        let second = reformat_lua_code(
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
            text: r#"local fn = function(
    -- first
    a, -- trailing a
    b,
    -- tail
)
return a
end
"#,
            level: LuaLanguageLevel::default(),
        };

        let first = reformat_lua_code(&source, &config);
        let second = reformat_lua_code(
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
            text: r#"local fn = function -- before params
(a) -- before body
-- body comment
return a
end
"#,
            level: LuaLanguageLevel::default(),
        };

        let first = reformat_lua_code(&source, &config);
        let second = reformat_lua_code(
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
            text: r#"local tbl={a=1,["b"]=2,[3]=4,[foo]=bar}
"#,
            level: LuaLanguageLevel::default(),
        };

        assert_eq!(
            reformat_lua_code(&source, &config),
            reformat_lua_code(&source, &config)
        );
    }

    #[test]
    fn test_new_formatter_table_comment_attachment_idempotent() {
        let config = LuaFormatConfig::default();
        let source = SourceText {
            text: r#"local tbl = {
    -- lead
    a = 1, -- trailing
    b = 2,
    -- tail
}
"#,
            level: LuaLanguageLevel::default(),
        };

        let first = reformat_lua_code(&source, &config);
        let second = reformat_lua_code(
            &SourceText {
                text: &first,
                level: LuaLanguageLevel::default(),
            },
            &config,
        );

        assert_eq!(first, second);
    }
}
