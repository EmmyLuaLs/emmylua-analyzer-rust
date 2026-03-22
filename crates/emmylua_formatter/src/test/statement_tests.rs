#[cfg(test)]
mod tests {
    // ========== if statement ==========

    use crate::{
        assert_format, assert_format_with_config,
        config::{LayoutConfig, LuaFormatConfig},
    };

    #[test]
    fn test_if_stat() {
        assert_format!(
            r#"
if true then
print(1)
end
"#,
            r#"
if true then
    print(1)
end
"#
        );
    }

    #[test]
    fn test_if_elseif_else() {
        assert_format!(
            r#"
if a then
print(1)
elseif b then
print(2)
else
print(3)
end
"#,
            r#"
if a then
    print(1)
elseif b then
    print(2)
else
    print(3)
end
"#
        );
    }

    #[test]
    fn test_if_stat_preserves_standalone_comment_before_then() {
        assert_format!(
            "if ok\n-- separator\nthen\n    print(1)\nend\n",
            "if ok\n-- separator\nthen\n    print(1)\nend\n"
        );
    }

    #[test]
    fn test_elseif_stat_preserves_standalone_comment_before_then() {
        assert_format!(
            "if a then\n    print(1)\nelseif b\n-- separator\nthen\n    print(2)\nend\n",
            "if a then\n    print(1)\nelseif b\n-- separator\nthen\n    print(2)\nend\n"
        );
    }

    #[test]
    fn test_single_line_if_return_preserved() {
        assert_format!(
            "if ok then return value end\n",
            "if ok then return value end\n"
        );
    }

    #[test]
    fn test_single_line_if_return_with_else_still_expands() {
        assert_format!(
            r#"
if ok then return value else return fallback end
"#,
            r#"
if ok then
    return value
else
    return fallback
end
"#
        );
    }

    #[test]
    fn test_single_line_if_break_preserved() {
        assert_format!("if stop then break end\n", "if stop then break end\n");
    }

    #[test]
    fn test_single_line_if_call_preserved() {
        assert_format!(
            "if ready then notify(user) end\n",
            "if ready then notify(user) end\n"
        );
    }

    #[test]
    fn test_single_line_if_assign_preserved() {
        assert_format!(
            "if ready then result = value end\n",
            "if ready then result = value end\n"
        );
    }

    #[test]
    fn test_single_line_if_local_preserved() {
        assert_format!(
            "if ready then local x = value end\n",
            "if ready then local x = value end\n"
        );
    }

    #[test]
    fn test_single_line_if_breaks_when_width_exceeded() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 40,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            "if ready then notify_with_long_name(first_argument, second_argument, third_argument) end\n",
            "if ready then\n    notify_with_long_name(\n        first_argument, second_argument,\n        third_argument\n    )\nend\n",
            config
        );
    }

    #[test]
    fn test_if_header_breaks_with_long_condition() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 44,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            "if alpha_beta_gamma + delta_theta + epsilon + zeta then\n    print(result)\nend\n",
            "if alpha_beta_gamma + delta_theta + epsilon\n    + zeta then\n    print(result)\nend\n",
            config
        );
    }

    // ========== for loop ==========

    #[test]
    fn test_for_loop() {
        assert_format!(
            r#"
for i = 1, 10 do
print(i)
end
"#,
            r#"
for i = 1, 10 do
    print(i)
end
"#
        );
    }

    #[test]
    fn test_for_range() {
        assert_format!(
            r#"
for k, v in pairs(t) do
print(k, v)
end
"#,
            r#"
for k, v in pairs(t) do
    print(k, v)
end
"#
        );
    }

    #[test]
    fn test_for_loop_preserves_standalone_comment_before_do() {
        assert_format!(
            "for i = 1, 10\n-- separator\ndo\n    print(i)\nend\n",
            "for i = 1, 10\n-- separator\ndo\n    print(i)\nend\n"
        );
    }

    #[test]
    fn test_for_range_preserves_standalone_comment_before_in() {
        assert_format!(
            "for k, v\n-- separator\nin pairs(t) do\n    print(k, v)\nend\n",
            "for k, v\n-- separator\nin pairs(t) do\n    print(k, v)\nend\n"
        );
    }

    #[test]
    fn test_for_loop_header_breaks_with_long_iter_exprs() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 60,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            "for i = very_long_start_expr, very_long_stop_expr, very_long_step_expr do\n    print(i)\nend\n",
            "for i = very_long_start_expr, very_long_stop_expr,\n    very_long_step_expr do\n    print(i)\nend\n",
            config
        );
    }

    #[test]
    fn test_for_range_header_breaks_with_long_exprs() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 64,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            "for key, value in very_long_iterator_expr, another_long_iterator_expr, fallback_iterator_expr do\n    print(key, value)\nend\n",
            "for key, value in very_long_iterator_expr,\n    another_long_iterator_expr, fallback_iterator_expr do\n    print(key, value)\nend\n",
            config
        );
    }

    // ========== while / repeat / do ==========

    #[test]
    fn test_while_loop() {
        assert_format!(
            r#"
while x > 0 do
x = x - 1
end
"#,
            r#"
while x > 0 do
    x = x - 1
end
"#
        );
    }

    #[test]
    fn test_while_loop_preserves_standalone_comment_before_do() {
        assert_format!(
            "while x > 0\n-- separator\ndo\n    x = x - 1\nend\n",
            "while x > 0\n-- separator\ndo\n    x = x - 1\nend\n"
        );
    }

    #[test]
    fn test_while_trivia_header_preserves_comment_before_do_with_shared_helper() {
        assert_format!(
            "while alpha_beta_gamma\n-- separator\ndo\n    work()\nend\n",
            "while alpha_beta_gamma\n-- separator\ndo\n    work()\nend\n"
        );
    }

    #[test]
    fn test_while_header_breaks_with_long_condition() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 44,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            "while alpha_beta_gamma + delta_theta + epsilon + zeta do\n    consume()\nend\n",
            "while alpha_beta_gamma + delta_theta\n    + epsilon + zeta do\n    consume()\nend\n",
            config
        );
    }

    #[test]
    fn test_repeat_until() {
        assert_format!(
            r#"
repeat
x = x + 1
until x > 10
"#,
            r#"
repeat
    x = x + 1
until x > 10
"#
        );
    }

    #[test]
    fn test_repeat_until_header_breaks_with_long_condition() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 44,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            "repeat\n    work()\nuntil alpha_beta_gamma + delta_theta + epsilon + zeta\n",
            "repeat\n    work()\nuntil alpha_beta_gamma + delta_theta\n    + epsilon + zeta\n",
            config
        );
    }

    #[test]
    fn test_do_block() {
        assert_format!(
            r#"
do
local x = 1
end
"#,
            r#"
do
    local x = 1
end
"#
        );
    }

    // ========== function definition ==========

    #[test]
    fn test_function_def() {
        assert_format!(
            r#"
function foo(a, b)
return a + b
end
"#,
            r#"
function foo(a, b)
    return a + b
end
"#
        );
    }

    #[test]
    fn test_local_function() {
        assert_format!(
            r#"
local function bar(x)
return x * 2
end
"#,
            r#"
local function bar(x)
    return x * 2
end
"#
        );
    }

    #[test]
    fn test_varargs_function() {
        assert_format!(
            r#"
function foo(a, b, ...)
print(a, b, ...)
end
"#,
            r#"
function foo(a, b, ...)
    print(a, b, ...)
end
"#
        );
    }

    #[test]
    fn test_multiline_function_params_layout_reflow_when_width_allows() {
        assert_format!(
            "function foo(\n    first,\n    second,\n    third\n)\n    return first\nend\n",
            "function foo(first, second, third)\n    return first\nend\n"
        );
    }

    #[test]
    fn test_function_params_use_progressive_fill_before_full_expansion() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 27,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            "function foo(first, second, third, fourth)\n    return first\nend\n",
            "function foo(\n    first, second, third,\n    fourth\n)\n    return first\nend\n",
            config
        );
    }

    #[test]
    fn test_function_header_keeps_name_and_breaks_params_progressively() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 52,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            "function module_name.deep_property.compute(first_argument, second_argument, third_argument)\n    return first_argument\nend\n",
            "function module_name.deep_property.compute(\n    first_argument, second_argument, third_argument\n)\n    return first_argument\nend\n",
            config
        );
    }

    #[test]
    fn test_varargs_closure() {
        assert_format!(
            r#"
local f = function(...)
return ...
end
"#,
            r#"
local f = function(...)
    return ...
end
"#
        );
    }

    #[test]
    fn test_multiline_closure_params_layout_reflow_when_width_allows() {
        assert_format!(
            "local f = function(\n    first,\n    second\n)\n    return first + second\nend\n",
            "local f = function(first, second)\n    return first + second\nend\n"
        );
    }

    // ========== assignment ==========

    #[test]
    fn test_multi_assign() {
        assert_format!("a, b = 1, 2\n", "a, b = 1, 2\n");
    }

    // ========== return ==========

    #[test]
    fn test_return_multi() {
        assert_format!(
            r#"
function f()
return 1, 2, 3
end
"#,
            r#"
function f()
    return 1, 2, 3
end
"#
        );
    }

    #[test]
    fn test_return_table_keeps_inline_with_keyword() {
        assert_format!(
            r#"
function f()
return {
key = value,
}
end
"#,
            r#"
function f()
    return { key = value }
end
"#
        );
    }

    #[test]
    fn test_assign_keeps_first_expr_on_operator_line_when_breaking() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 48,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            "result = alpha_beta_gamma + delta_theta + epsilon + zeta\n",
            "result = alpha_beta_gamma + delta_theta\n    + epsilon + zeta\n",
            config
        );
    }

    #[test]
    fn test_return_keeps_first_expr_on_keyword_line_when_breaking() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 48,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            "function f()\nreturn alpha_beta_gamma + delta_theta + epsilon + zeta\nend\n",
            "function f()\n    return alpha_beta_gamma + delta_theta\n        + epsilon + zeta\nend\n",
            config
        );
    }

    // ========== goto / label / break ==========

    #[test]
    fn test_goto_label() {
        assert_format!(
            r#"
goto done
::done::
print(1)
"#,
            r#"
goto done
::done::
print(1)
"#
        );
    }

    #[test]
    fn test_break_stat() {
        assert_format!(
            r#"
while true do
break
end
"#,
            r#"
while true do
    break
end
"#
        );
    }

    // ========== comprehensive reformat ==========

    #[test]
    fn test_reformat_lua_code() {
        assert_format!(
            r#"
    local a = 1
    local b =  2
    local c =   a+b
    print  (c     )
"#,
            r#"
local a = 1
local b = 2
local c = a + b
print(c)
"#
        );
    }

    // ========== empty body compact output ==========

    #[test]
    fn test_empty_function() {
        assert_format!(
            r#"
function foo()
end
"#,
            "function foo() end\n"
        );
    }

    #[test]
    fn test_empty_function_with_params() {
        assert_format!(
            r#"
function foo(a, b)
end
"#,
            "function foo(a, b) end\n"
        );
    }

    #[test]
    fn test_empty_do_block() {
        assert_format!(
            r#"
do
end
"#,
            "do end\n"
        );
    }

    #[test]
    fn test_empty_while_loop() {
        assert_format!(
            r#"
while true do
end
"#,
            "while true do end\n"
        );
    }

    #[test]
    fn test_empty_for_loop() {
        assert_format!(
            r#"
for i = 1, 10 do
end
"#,
            "for i = 1, 10 do end\n"
        );
    }

    // ========== semicolon ==========

    #[test]
    fn test_semicolon_preserved() {
        assert_format!(";\n", ";\n");
    }

    // ========== local attributes ==========

    #[test]
    fn test_local_const() {
        assert_format!("local x <const> = 42\n", "local x <const> = 42\n");
    }

    #[test]
    fn test_local_close() {
        assert_format!(
            "local f <close> = io.open(\"test.txt\")\n",
            "local f <close> = io.open(\"test.txt\")\n"
        );
    }

    #[test]
    fn test_local_const_multi() {
        assert_format!(
            "local a <const>, b <const> = 1, 2\n",
            "local a <const>, b <const> = 1, 2\n"
        );
    }

    #[test]
    fn test_global_const_star() {
        assert_format!("global <const> *\n", "global <const> *\n");
    }

    #[test]
    fn test_global_preserves_name_attributes() {
        assert_format!(
            "global <const> a, b <const>\n",
            "global <const> a, b <const>\n"
        );
    }

    #[test]
    fn test_local_stat_preserves_inline_comment_before_assign() {
        assert_format!("local a -- hiihi\n= 123\n", "local a -- hiihi\n= 123\n");
    }

    #[test]
    fn test_function_stat_preserves_inline_comment_before_end() {
        assert_format!(
            "function t:a() -- this comment will stay the same\nend\n",
            "function t:a() -- this comment will stay the same\nend\n"
        );
    }

    #[test]
    fn test_function_stat_preserves_inline_comment_in_params() {
        assert_format!(
            "function foo(a -- first\n, b)\n    return a + b\nend\n",
            "function foo(a -- first\n, b)\n    return a + b\nend\n"
        );
    }

    #[test]
    fn test_function_stat_preserves_standalone_comment_before_params() {
        assert_format!(
            "function foo\n-- separator\n(a, b)\n    return a + b\nend\n",
            "function foo\n-- separator\n(a, b)\n    return a + b\nend\n"
        );
    }

    #[test]
    fn test_local_function_stat_preserves_standalone_comment_before_params() {
        assert_format!(
            "local function foo\n-- separator\n(a, b)\n    return a + b\nend\n",
            "local function foo\n-- separator\n(a, b)\n    return a + b\nend\n"
        );
    }

    #[test]
    fn test_single_line_if_near_width_limit_prefers_expanded_layout() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 48,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            "if alpha_beta_gamma then return delta_theta end\n",
            "if alpha_beta_gamma then\n    return delta_theta\nend\n",
            config
        );
    }

    #[test]
    fn test_local_stat_preserves_standalone_comment_between_name_and_assign() {
        assert_format!(
            "local a\n-- separator\n= 123\n",
            "local a\n-- separator\n= 123\n"
        );
    }

    #[test]
    fn test_assign_stat_preserves_standalone_comment_before_assign_op() {
        assert_format!(
            "value\n-- separator\n= 123\n",
            "value\n-- separator\n= 123\n"
        );
    }

    #[test]
    fn test_return_stat_preserves_standalone_comment_before_expr() {
        assert_format!(
            "return\n-- separator\nvalue\n",
            "return\n-- separator\nvalue\n"
        );
    }

    // ========== local function empty body compact ==========

    #[test]
    fn test_empty_local_function() {
        assert_format!(
            r#"
local function foo()
end
"#,
            "local function foo() end\n"
        );
    }

    #[test]
    fn test_empty_local_function_with_params() {
        assert_format!(
            r#"
local function foo(a, b)
end
"#,
            "local function foo(a, b) end\n"
        );
    }
}
