#[cfg(test)]
mod tests {
    use crate::{
        assert_format_with_config,
        config::{
            EndOfLine, ExpandStrategy, IndentConfig, IndentKind, LayoutConfig, LuaFormatConfig,
            OutputConfig, SpacingConfig, TrailingComma,
        },
    };

    // ========== spacing options ==========

    #[test]
    fn test_space_before_func_paren() {
        let config = LuaFormatConfig {
            spacing: SpacingConfig {
                space_before_func_paren: true,
                ..Default::default()
            },
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
            spacing: SpacingConfig {
                space_before_call_paren: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!("print(1)\n", "print (1)\n", config);
    }

    #[test]
    fn test_space_inside_parens() {
        let config = LuaFormatConfig {
            spacing: SpacingConfig {
                space_inside_parens: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!("local a = (1 + 2)\n", "local a = ( 1 + 2 )\n", config);
    }

    #[test]
    fn test_space_inside_braces() {
        let config = LuaFormatConfig {
            spacing: SpacingConfig {
                space_inside_braces: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!("local t = {1, 2, 3}\n", "local t = { 1, 2, 3 }\n", config);
    }

    #[test]
    fn test_no_space_inside_braces() {
        let config = LuaFormatConfig {
            spacing: SpacingConfig {
                space_inside_braces: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!("local t = { 1, 2, 3 }\n", "local t = {1, 2, 3}\n", config);
    }

    // ========== table expand strategy ==========

    #[test]
    fn test_table_expand_always() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                table_expand: ExpandStrategy::Always,
                ..Default::default()
            },
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
            layout: LayoutConfig {
                table_expand: ExpandStrategy::Never,
                ..Default::default()
            },
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
            output: OutputConfig {
                trailing_comma: TrailingComma::Always,
                ..Default::default()
            },
            layout: LayoutConfig {
                table_expand: ExpandStrategy::Always,
                ..Default::default()
            },
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
            output: OutputConfig {
                trailing_comma: TrailingComma::Never,
                ..Default::default()
            },
            layout: LayoutConfig {
                table_expand: ExpandStrategy::Always,
                ..Default::default()
            },
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
            indent: IndentConfig {
                kind: IndentKind::Tab,
                ..Default::default()
            },
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
            layout: LayoutConfig {
                max_blank_lines: 1,
                ..Default::default()
            },
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
            output: OutputConfig {
                end_of_line: EndOfLine::CRLF,
                ..Default::default()
            },
            ..Default::default()
        };
        // Keep escaped strings: raw strings can't represent \r\n distinctly
        assert_format_with_config!(
            "if true then\nprint(1)\nend\n",
            "if true then\r\n    print(1)\r\nend\r\n",
            config
        );
    }

    // ========== operator spacing options ==========

    #[test]
    fn test_no_space_around_math_operator() {
        let config = LuaFormatConfig {
            spacing: SpacingConfig {
                space_around_math_operator: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            "local a = 1 + 2 * 3 - 4 / 5\n",
            "local a = 1+2*3-4/5\n",
            config
        );
    }

    #[test]
    fn test_space_around_math_operator_default() {
        // Default: spaces around math operators
        assert_format_with_config!(
            "local a = 1+2*3\n",
            "local a = 1 + 2 * 3\n",
            LuaFormatConfig::default()
        );
    }

    #[test]
    fn test_no_space_around_concat_operator() {
        let config = LuaFormatConfig {
            spacing: SpacingConfig {
                space_around_concat_operator: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!("local s = a .. b .. c\n", "local s = a..b..c\n", config);
    }

    #[test]
    fn test_space_around_concat_operator_default() {
        assert_format_with_config!(
            "local s = a..b\n",
            "local s = a .. b\n",
            LuaFormatConfig::default()
        );
    }

    #[test]
    fn test_float_concat_no_space_keeps_space() {
        // When no-space concat is enabled, `1. .. x` must keep the space to
        // avoid producing the invalid token `1...`
        let config = LuaFormatConfig {
            spacing: SpacingConfig {
                space_around_concat_operator: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            "local s = 1. .. \"str\"\n",
            "local s = 1. ..\"str\"\n",
            config
        );
    }

    #[test]
    fn test_no_math_space_keeps_comparison_space() {
        // Disabling math operator spaces should NOT affect comparison operators
        let config = LuaFormatConfig {
            spacing: SpacingConfig {
                space_around_math_operator: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!("local x = a+b == c*d\n", "local x = a+b == c*d\n", config);
    }

    #[test]
    fn test_no_math_space_keeps_logical_space() {
        // Disabling math operator spaces should NOT affect logical operators
        let config = LuaFormatConfig {
            spacing: SpacingConfig {
                space_around_math_operator: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            "local a = b and c or d\n",
            "local a = b and c or d\n",
            config
        );
    }

    // ========== space around assign operator ==========

    #[test]
    fn test_no_space_around_assign() {
        let config = LuaFormatConfig {
            spacing: SpacingConfig {
                space_around_assign_operator: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!("local a = 1\n", "local a=1\n", config);
    }

    #[test]
    fn test_no_space_around_assign_table() {
        let config = LuaFormatConfig {
            spacing: SpacingConfig {
                space_around_assign_operator: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!("local t = { a = 1 }\n", "local t={ a=1 }\n", config);
    }

    #[test]
    fn test_space_around_assign_default() {
        assert_format_with_config!("local a=1\n", "local a = 1\n", LuaFormatConfig::default());
    }

    #[test]
    fn test_structured_toml_deserialize() {
        let config: LuaFormatConfig = toml_edit::de::from_str(
            r#"
[indent]
kind = "Space"
width = 2

[layout]
max_line_width = 88
table_expand = "Always"

[spacing]
space_before_call_paren = true

[comments]
align_line_comments = false

[emmy_doc]
space_after_description_dash = false

[align]
table_field = false
"#,
        )
        .expect("structured toml config should deserialize");

        assert_eq!(config.indent.kind, IndentKind::Space);
        assert_eq!(config.indent.width, 2);
        assert_eq!(config.layout.max_line_width, 88);
        assert_eq!(config.layout.table_expand, ExpandStrategy::Always);
        assert!(config.spacing.space_before_call_paren);
        assert!(!config.comments.align_line_comments);
        assert!(!config.emmy_doc.space_after_description_dash);
        assert!(!config.align.table_field);
    }
}
