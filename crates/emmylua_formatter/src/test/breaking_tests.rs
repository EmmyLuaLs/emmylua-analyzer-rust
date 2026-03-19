#[cfg(test)]
mod tests {
    use crate::{
        assert_format_with_config,
        config::{LayoutConfig, LuaFormatConfig},
    };

    #[test]
    fn test_long_binary_expr_breaking() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 80,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            "local result = very_long_variable_name_aaa + another_long_variable_name_bbb + yet_another_variable_name_ccc + final_variable_name_ddd\n",
            r#"
local result =
    very_long_variable_name_aaa + another_long_variable_name_bbb
        + yet_another_variable_name_ccc + final_variable_name_ddd
"#,
            config
        );
    }

    #[test]
    fn test_long_call_args_breaking() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 60,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            "some_function(very_long_argument_one, very_long_argument_two, very_long_argument_three, very_long_argument_four)\n",
            r#"
some_function(
    very_long_argument_one,
    very_long_argument_two,
    very_long_argument_three,
    very_long_argument_four
)
"#,
            config
        );
    }

    #[test]
    fn test_long_table_breaking() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 60,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            "local t = { first_key = 1, second_key = 2, third_key = 3, fourth_key = 4, fifth_key = 5 }\n",
            r#"
local t = {
    first_key  = 1,
    second_key = 2,
    third_key  = 3,
    fourth_key = 4,
    fifth_key  = 5
}
"#,
            config
        );
    }

    #[test]
    fn test_multiline_table_input_stays_multiline_in_auto_mode() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 120,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            "local t = {\n    a = 1,\n    b = 2,\n}\n",
            "local t = {\n    a = 1,\n    b = 2\n}\n",
            config
        );
    }

    #[test]
    fn test_table_with_nested_values_stays_inline_when_width_allows() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 120,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            "local t = { user = { name = \"a\", age = 1 }, enabled = true }\n",
            "local t = { user = { name = \"a\", age = 1 }, enabled = true }\n",
            config
        );
    }
}
