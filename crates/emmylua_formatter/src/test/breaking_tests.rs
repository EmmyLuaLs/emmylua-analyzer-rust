#[cfg(test)]
mod tests {
    use crate::{assert_format_with_config, config::LuaFormatConfig};

    #[test]
    fn test_long_binary_expr_breaking() {
        let config = LuaFormatConfig {
            max_line_width: 80,
            ..Default::default()
        };
        assert_format_with_config!(
            "local result = very_long_variable_name_aaa + another_long_variable_name_bbb + yet_another_variable_name_ccc + final_variable_name_ddd\n",
            r#"
local result =
    very_long_variable_name_aaa + another_long_variable_name_bbb
        + yet_another_variable_name_ccc
        + final_variable_name_ddd
"#,
            config
        );
    }

    #[test]
    fn test_long_call_args_breaking() {
        let config = LuaFormatConfig {
            max_line_width: 60,
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
            max_line_width: 60,
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
}
