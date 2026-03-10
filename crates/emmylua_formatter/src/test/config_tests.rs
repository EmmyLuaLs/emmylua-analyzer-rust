#[cfg(test)]
mod tests {
    use crate::{
        assert_format_with_config,
        config::{EndOfLine, ExpandStrategy, IndentStyle, LuaFormatConfig, TrailingComma},
    };

    // ========== spacing options ==========

    #[test]
    fn test_space_before_func_paren() {
        let config = LuaFormatConfig {
            space_before_func_paren: true,
            ..Default::default()
        };
        assert_format_with_config!(
            r#"
function foo(a, b)
return a
end
"#,
            r#"
function foo (a, b)
    return a
end
"#,
            config
        );
    }

    #[test]
    fn test_space_before_call_paren() {
        let config = LuaFormatConfig {
            space_before_call_paren: true,
            ..Default::default()
        };
        assert_format_with_config!("print(1)\n", "print (1)\n", config);
    }

    #[test]
    fn test_space_inside_parens() {
        let config = LuaFormatConfig {
            space_inside_parens: true,
            ..Default::default()
        };
        assert_format_with_config!("local a = (1 + 2)\n", "local a = ( 1 + 2 )\n", config);
    }

    #[test]
    fn test_space_inside_braces() {
        let config = LuaFormatConfig {
            space_inside_braces: true,
            ..Default::default()
        };
        assert_format_with_config!("local t = {1, 2, 3}\n", "local t = { 1, 2, 3 }\n", config);
    }

    #[test]
    fn test_no_space_inside_braces() {
        let config = LuaFormatConfig {
            space_inside_braces: false,
            ..Default::default()
        };
        assert_format_with_config!("local t = { 1, 2, 3 }\n", "local t = {1, 2, 3}\n", config);
    }

    // ========== table expand strategy ==========

    #[test]
    fn test_table_expand_always() {
        let config = LuaFormatConfig {
            table_expand: ExpandStrategy::Always,
            ..Default::default()
        };
        assert_format_with_config!(
            "local t = {a = 1, b = 2}\n",
            r#"
local t = {
    a = 1,
    b = 2
}
"#,
            config
        );
    }

    #[test]
    fn test_table_expand_never() {
        let config = LuaFormatConfig {
            table_expand: ExpandStrategy::Never,
            ..Default::default()
        };
        assert_format_with_config!(
            r#"
local t = {
a = 1,
b = 2
}
"#,
            "local t = { a = 1, b = 2 }\n",
            config
        );
    }

    // ========== trailing comma ==========

    #[test]
    fn test_trailing_comma_always_table() {
        let config = LuaFormatConfig {
            trailing_comma: TrailingComma::Always,
            table_expand: ExpandStrategy::Always,
            ..Default::default()
        };
        assert_format_with_config!(
            r#"
local t = {
a = 1,
b = 2
}
"#,
            r#"
local t = {
    a = 1,
    b = 2,
}
"#,
            config
        );
    }

    #[test]
    fn test_trailing_comma_never() {
        let config = LuaFormatConfig {
            trailing_comma: TrailingComma::Never,
            table_expand: ExpandStrategy::Always,
            ..Default::default()
        };
        assert_format_with_config!(
            r#"
local t = {
a = 1,
b = 2,
}
"#,
            r#"
local t = {
    a = 1,
    b = 2
}
"#,
            config
        );
    }

    // ========== indentation ==========

    #[test]
    fn test_tab_indent() {
        let config = LuaFormatConfig {
            indent_style: IndentStyle::Tab,
            ..Default::default()
        };
        // Keep escaped strings: raw strings can't represent \t visually
        assert_format_with_config!(
            "if true then\nprint(1)\nend\n",
            "if true then\n\tprint(1)\nend\n",
            config
        );
    }

    // ========== blank lines ==========

    #[test]
    fn test_max_blank_lines() {
        let config = LuaFormatConfig {
            max_blank_lines: 1,
            ..Default::default()
        };
        assert_format_with_config!(
            r#"
local a = 1




local b = 2
"#,
            r#"
local a = 1

local b = 2
"#,
            config
        );
    }

    // ========== end of line ==========

    #[test]
    fn test_crlf_end_of_line() {
        let config = LuaFormatConfig {
            end_of_line: EndOfLine::CRLF,
            ..Default::default()
        };
        // Keep escaped strings: raw strings can't represent \r\n distinctly
        assert_format_with_config!(
            "if true then\nprint(1)\nend\n",
            "if true then\r\n    print(1)\r\nend\r\n",
            config
        );
    }
}
