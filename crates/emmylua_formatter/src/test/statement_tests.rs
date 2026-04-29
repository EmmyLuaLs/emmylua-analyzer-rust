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
            r#"if ok
-- separator
then
    print(1)
end
"#,
            r#"if ok
-- separator
then
    print(1)
end
"#
        );
    }

    #[test]
    fn test_if_comment_before_then_does_not_force_raw_preserve() {
        assert_format!(
            r#"if alpha + beta + gamma
-- separator
then
print(1)
end
"#,
            r#"if alpha + beta + gamma
-- separator
then
    print(1)
end
"#
        );
    }

    #[test]
    fn test_if_stat_preserves_inline_comment_after_then() {
        assert_format!(
            r#"if ok then -- keep header note
    print(1)
end
"#,
            r#"if ok then -- keep header note
    print(1)
end
"#
        );
    }

    #[test]
    fn test_elseif_stat_preserves_inline_comment_after_then() {
        assert_format!(
            r#"if a then
    print(1)
elseif b then -- keep elseif note
    print(2)
end
"#,
            r#"if a then
    print(1)
elseif b then -- keep elseif note
    print(2)
end
"#
        );
    }

    #[test]
    fn test_else_clause_preserves_inline_comment_after_else() {
        assert_format!(
            r#"if a then
    print(1)
else -- keep else note
    print(2)
end
"#,
            r#"if a then
    print(1)
else -- keep else note
    print(2)
end
"#
        );
    }

    #[test]
    fn test_if_then_and_else_inline_comments_stay_with_their_clauses() {
        assert_format!(
            r#"if a then -- hello
    local x = 123
else -- ii
end
"#,
            r#"if a then -- hello
    local x = 123
else -- ii
end
"#
        );
    }

    #[test]
    fn test_if_body_comment_does_not_force_raw_preserve() {
        assert_format!(
            r#"if ok then
-- note
print(1)
end
"#,
            r#"if ok then
    -- note
    print(1)
end
"#
        );
    }

    #[test]
    fn test_elseif_stat_preserves_standalone_comment_before_then() {
        assert_format!(
            r#"if a then
    print(1)
elseif b
-- separator
then
    print(2)
end
"#,
            r#"if a then
    print(1)
elseif b
-- separator
then
    print(2)
end
"#
        );
    }

    #[test]
    fn test_elseif_comment_before_then_does_not_force_raw_preserve() {
        assert_format!(
            r#"if a then
    print(1)
elseif alpha + beta + gamma
-- separator
then
print(2)
end
"#,
            r#"if a then
    print(1)
elseif alpha + beta + gamma
-- separator
then
    print(2)
end
"#
        );
    }

    #[test]
    fn test_single_line_if_return_preserved() {
        assert_format!(
            r#"if ok then return value end
"#,
            r#"if ok then return value end
"#
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
        assert_format!(
            r#"if stop then break end
"#,
            r#"if stop then break end
"#
        );
    }

    #[test]
    fn test_single_line_if_call_preserved() {
        assert_format!(
            r#"if ready then notify(user) end
"#,
            r#"if ready then notify(user) end
"#
        );
    }

    #[test]
    fn test_single_line_if_assign_preserved() {
        assert_format!(
            r#"if ready then result = value end
"#,
            r#"if ready then result = value end
"#
        );
    }

    #[test]
    fn test_single_line_if_local_preserved() {
        assert_format!(
            r#"if ready then local x = value end
"#,
            r#"if ready then local x = value end
"#
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
            r#"if ready then notify_with_long_name(first_argument, second_argument, third_argument) end
"#,
            r#"if ready then
    notify_with_long_name(
        first_argument, second_argument,
        third_argument
    )
end
"#,
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
            r#"if alpha_beta_gamma + delta_theta + epsilon + zeta then
    print(result)
end
"#,
            r#"if alpha_beta_gamma + delta_theta
    + epsilon + zeta then
    print(result)
end
"#,
            config
        );
    }

    #[test]
    fn test_if_header_keeps_short_logical_tail_with_multiline_callback_call() {
        assert_format!(
            r#"if check(function()
    return true
end, 'LOADTRUE', 'RETURN1') and another_predicate then
    print('ok')
end
"#,
            r#"if check(function()
    return true
end,
    'LOADTRUE', 'RETURN1') and another_predicate then
    print('ok')
end
"#
        );
    }

    #[test]
    fn test_if_block_reindents_attached_multiline_table_call_arg() {
        assert_format!(
            r#"if ok then
    configure({
key = value,
another = other,
}, option_one, option_two)
end
"#,
            r#"if ok then
    configure({
        key = value,
        another = other
    }, option_one, option_two)
end
"#
        );
    }

    #[test]
    fn test_if_end_inline_comment_is_preserved() {
        assert_format!(
            r#"function abi.get_pos()
if false then
return "" -- hhh
end -- ennene

return { yafafa = 1, x = 2 } -- ccc
end
"#,
            r#"function abi.get_pos()
    if false then
        return "" -- hhh
    end -- ennene

    return { yafafa = 1, x = 2 } -- ccc
end
"#
        );
    }

    #[test]
    fn test_while_header_keeps_short_logical_tail_with_multiline_callback_call() {
        assert_format!(
            r#"while check(function()
    return true
end, 'LOADTRUE', 'RETURN1') and another_predicate do
    print('ok')
end
"#,
            r#"while check(function()
    return true
end,
    'LOADTRUE', 'RETURN1') and another_predicate do
    print('ok')
end
"#
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
            r#"for i = 1, 10
-- separator
do
    print(i)
end
"#,
            r#"for i = 1, 10
-- separator
do
    print(i)
end
"#
        );
    }

    #[test]
    fn test_for_loop_comment_before_do_does_not_force_raw_preserve() {
        assert_format!(
            r#"for i = 1, 10
-- separator
do
print(i+1)
end
"#,
            r#"for i = 1, 10
-- separator
do
    print(i + 1)
end
"#
        );
    }

    #[test]
    fn test_for_loop_preserves_inline_comment_after_do() {
        assert_format!(
            r#"for i = 1, 10 do -- loop note
    print(i)
end
"#,
            r#"for i = 1, 10 do -- loop note
    print(i)
end
"#
        );
    }

    #[test]
    fn test_for_range_preserves_standalone_comment_before_in() {
        assert_format!(
            r#"for k, v
-- separator
in pairs(t) do
    print(k, v)
end
"#,
            r#"for k, v
-- separator
in pairs(t) do
    print(k, v)
end
"#
        );
    }

    #[test]
    fn test_for_range_comment_before_in_does_not_force_raw_preserve() {
        assert_format!(
            r#"for k,v
-- separator
in pairs(t) do
print(k,v)
end
"#,
            r#"for k, v
-- separator
in pairs(t) do
    print(k, v)
end
"#
        );
    }

    #[test]
    fn test_for_range_preserves_inline_comment_after_in() {
        assert_format!(
            r#"for k, v in -- iterator note
pairs(t) do
    print(k, v)
end
"#,
            r#"for k, v in -- iterator note
pairs(t) do
    print(k, v)
end
"#
        );
    }

    #[test]
    fn test_for_range_preserves_inline_comment_after_do() {
        assert_format!(
            r#"for k, v in pairs(t) do -- body note
    print(k, v)
end
"#,
            r#"for k, v in pairs(t) do -- body note
    print(k, v)
end
"#
        );
    }

    #[test]
    fn test_for_range_comment_before_do_does_not_force_raw_preserve() {
        assert_format!(
            r#"for k, v in pairs(t)
-- separator
do
print(k,v)
end
"#,
            r#"for k, v in pairs(t)
-- separator
do
    print(k, v)
end
"#
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
            r#"for i = very_long_start_expr, very_long_stop_expr, very_long_step_expr do
    print(i)
end
"#,
            r#"for i = very_long_start_expr,
    very_long_stop_expr, very_long_step_expr do
    print(i)
end
"#,
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
            r#"for key, value in very_long_iterator_expr, another_long_iterator_expr, fallback_iterator_expr do
    print(key, value)
end
"#,
            r#"for key, value in very_long_iterator_expr,
    another_long_iterator_expr, fallback_iterator_expr do
    print(key, value)
end
"#,
            config
        );
    }

    #[test]
    fn test_for_range_keeps_first_multiline_iterator_shape_when_breaking() {
        assert_format!(
            r#"for key, value in iterate(function()
    return true
end, 'LOADTRUE', 'RETURN1'), fallback_iterator do
    print(key, value)
end
"#,
            r#"for key, value in iterate(function()
    return true
end,
    'LOADTRUE', 'RETURN1'),
    fallback_iterator do
    print(key, value)
end
"#
        );
    }

    #[test]
    fn test_for_range_header_prefers_balanced_packed_expr_list() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 44,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"for key, value in first_long_expr, second_long_expr, third_long_expr, fourth_long_expr, fifth_long_expr do
    print(key, value)
end
"#,
            r#"for key, value in first_long_expr,
    second_long_expr, third_long_expr,
    fourth_long_expr, fifth_long_expr do
    print(key, value)
end
"#,
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
            r#"while x > 0
-- separator
do
    x = x - 1
end
"#,
            r#"while x > 0
-- separator
do
    x = x - 1
end
"#
        );
    }

    #[test]
    fn test_while_trivia_header_preserves_comment_before_do_with_shared_helper() {
        assert_format!(
            r#"while alpha_beta_gamma
-- separator
do
    work()
end
"#,
            r#"while alpha_beta_gamma
-- separator
do
    work()
end
"#
        );
    }

    #[test]
    fn test_while_body_comment_does_not_force_raw_preserve() {
        assert_format!(
            r#"while x > 0 do
-- note
x = x-1
end
"#,
            r#"while x > 0 do
    -- note
    x = x - 1
end
"#
        );
    }

    #[test]
    fn test_while_preserves_inline_comment_after_do() {
        assert_format!(
            r#"while x > 0 do -- loop note
    x = x - 1
end
"#,
            r#"while x > 0 do -- loop note
    x = x - 1
end
"#
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
            r#"while alpha_beta_gamma + delta_theta + epsilon + zeta do
    consume()
end
"#,
            r#"while alpha_beta_gamma + delta_theta
    + epsilon + zeta do
    consume()
end
"#,
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
            r#"repeat
    work()
until alpha_beta_gamma + delta_theta + epsilon + zeta
"#,
            r#"repeat
    work()
until alpha_beta_gamma + delta_theta
    + epsilon + zeta
"#,
            config
        );
    }

    #[test]
    fn test_repeat_comment_before_until_does_not_force_raw_preserve() {
        assert_format!(
            r#"repeat
x=x+1
-- guard
until ready(a,b)
"#,
            r#"repeat
    x = x + 1
    -- guard
until ready(a, b)
"#
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

    #[test]
    fn test_do_block_preserves_inline_comment_after_do() {
        assert_format!(
            r#"do -- block note
local x=1
end
"#,
            r#"do -- block note
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
            r#"function foo(
    first,
    second,
    third
)
    return first
end
"#,
            r#"function foo(first, second, third)
    return first
end
"#
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
            r#"function foo(first, second, third, fourth)
    return first
end
"#,
            r#"function foo(
    first, second, third,
    fourth
)
    return first
end
"#,
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
            r#"function module_name.deep_property.compute(first_argument, second_argument, third_argument)
    return first_argument
end
"#,
            r#"function module_name.deep_property.compute(
    first_argument, second_argument, third_argument
)
    return first_argument
end
"#,
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
            r#"local f = function(
    first,
    second
)
    return first + second
end
"#,
            r#"local f = function(first, second)
    return first + second
end
"#
        );
    }

    // ========== assignment ==========

    #[test]
    fn test_multi_assign() {
        assert_format!(
            r#"a, b = 1, 2
"#,
            r#"a, b = 1, 2
"#
        );
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
            r#"result = alpha_beta_gamma + delta_theta + epsilon + zeta
"#,
            r#"result = alpha_beta_gamma + delta_theta
    + epsilon + zeta
"#,
            config
        );
    }

    #[test]
    fn test_assign_expr_list_prefers_balanced_packed_layout_with_long_prefix() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 44,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"very_long_result_name = first_long_expr, second_long_expr, third_long_expr, fourth_long_expr, fifth_long_expr
"#,
            r#"very_long_result_name = first_long_expr,
    second_long_expr, third_long_expr,
    fourth_long_expr, fifth_long_expr
"#,
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
            r#"function f()
return alpha_beta_gamma + delta_theta + epsilon + zeta
end
"#,
            r#"function f()
    return alpha_beta_gamma + delta_theta
        + epsilon + zeta
end
"#,
            config
        );
    }

    #[test]
    fn test_return_preserves_first_multiline_closure_shape_when_breaking() {
        assert_format!(
            r#"function f()
    return function()
        return true
    end, first_result, second_result
end
"#,
            r#"function f()
    return function()
        return true
    end,
        first_result,
        second_result
end
"#
        );
    }

    #[test]
    fn test_return_preserves_first_multiline_table_shape_when_breaking() {
        assert_format!(
            r#"function f()
    return {
        key = value,
        another = other,
    }, first_result, second_result
end
"#,
            r#"function f()
    return {
        key = value,
        another = other,
    },
        first_result,
        second_result
end
"#
        );
    }

    #[test]
    fn test_local_assign_preserves_first_multiline_closure_shape_when_breaking() {
        assert_format!(
            r#"local first, second, third = function()
    return true
end, alpha_result, beta_result
"#,
            r#"local first, second, third = function()
    return true
end,
    alpha_result,
    beta_result
"#
        );
    }

    #[test]
    fn test_assign_preserves_first_multiline_table_shape_when_breaking() {
        assert_format!(
            r#"target, fallback = {
    key = value,
    another = other,
}, alpha_result, beta_result
"#,
            r#"target, fallback = {
    key = value,
    another = other,
},
    alpha_result,
    beta_result
"#
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
            r#"function foo() end
"#
        );
    }

    #[test]
    fn test_empty_function_with_params() {
        assert_format!(
            r#"
function foo(a, b)
end
"#,
            r#"function foo(a, b) end
"#
        );
    }

    #[test]
    fn test_empty_do_block() {
        assert_format!(
            r#"
do
end
"#,
            r#"do end
"#
        );
    }

    #[test]
    fn test_empty_while_loop() {
        assert_format!(
            r#"
while true do
end
"#,
            r#"while true do end
"#
        );
    }

    #[test]
    fn test_empty_for_loop() {
        assert_format!(
            r#"
for i = 1, 10 do
end
"#,
            r#"for i = 1, 10 do end
"#
        );
    }

    // ========== semicolon ==========

    #[test]
    fn test_semicolon_preserved() {
        assert_format!(
            r#";
"#, r#";
"#
        );
    }

    // ========== local attributes ==========

    #[test]
    fn test_local_const() {
        assert_format!(
            r#"local x <const> = 42
"#,
            r#"local x <const> = 42
"#
        );
    }

    #[test]
    fn test_local_close() {
        assert_format!(
            r#"local f <close> = io.open("test.txt")
"#,
            r#"local f <close> = io.open("test.txt")
"#
        );
    }

    #[test]
    fn test_local_const_multi() {
        assert_format!(
            r#"local a <const>, b <const> = 1, 2
"#,
            r#"local a <const>, b <const> = 1, 2
"#
        );
    }

    #[test]
    fn test_global_const_star() {
        assert_format!(
            r#"global <const> *
"#,
            r#"global <const> *
"#
        );
    }

    #[test]
    fn test_global_preserves_name_attributes() {
        assert_format!(
            r#"global <const> a, b <const>
"#,
            r#"global <const> a, b <const>
"#
        );
    }

    #[test]
    fn test_local_stat_preserves_inline_comment_before_assign() {
        assert_format!(
            r#"local a -- hiihi
= 123
"#,
            r#"local a -- hiihi
= 123
"#
        );
    }

    #[test]
    fn test_function_stat_preserves_inline_comment_before_end() {
        assert_format!(
            r#"function t:a() -- this comment will stay the same
end
"#,
            r#"function t:a() -- this comment will stay the same
end
"#
        );
    }

    #[test]
    fn test_function_stat_preserves_inline_comment_before_non_empty_body() {
        assert_format!(
            r#"function name13()  --hhii
    return "name13" --jj
end
"#,
            r#"function name13() -- hhii
    return "name13" -- jj
end
"#
        );
    }

    #[test]
    fn test_if_body_inline_return_comment_does_not_block_previous_statement_formatting() {
        assert_format!(
            r#"if nState ~= self.StarBoxType.GetNormal then
    pPlayer     .Msg("请先领取该星级的普通宝箱奖励后再来购买钻石宝箱")
    return -- 还未领取普通宝箱奖励
end
"#,
            r#"if nState ~= self.StarBoxType.GetNormal then
    pPlayer.Msg("请先领取该星级的普通宝箱奖励后再来购买钻石宝箱")
    return -- 还未领取普通宝箱奖励
end
"#
        );
    }

    #[test]
    fn test_if_inline_header_comment_does_not_drop_first_call_statement() {
        assert_format!(
            r#"if nState ~= 1 then --hiihii
    c.    Msg("hihi")
    return -- 111
end
"#,
            r#"if nState ~= 1 then -- hiihii
    c.Msg("hihi")
    return -- 111
end
"#
        );
    }

    #[test]
    fn test_chain_call_statement_preserves_inline_comments_between_segments() {
        assert_format!(
            r#"builder.new()
    .setName("test") -- 222
    .setVersion("1.0.0") -- 333
"#,
            r#"builder.new()
    .setName("test") -- 222
    .setVersion("1.0.0") -- 333
"#
        );
    }

    #[test]
    fn test_chain_call_statement_formats_multiline_closure_with_comment_gap() {
        assert_format!(
            r#"-- hihi
builder.new()
    -- hihi
    .setName("test", function()
    return "1.0.0" + 1
end).setVersion("1.0.0", function()
    return "1.0.0" + 1
end) -- 333
"#,
            r#"-- hihi
builder.new()
    -- hihi
    .setName("test", function()
        return "1.0.0" + 1
    end).setVersion("1.0.0", function()
        return "1.0.0" + 1
    end) -- 333
"#
        );
    }

    #[test]
    fn test_chain_call_statement_formats_comment_between_segments_without_raw_preserve() {
        assert_format!(
            r#"builder.new()
 -- nofowo
    .setName("test", function()
        return "1.0.0" + 1
    end).setVersion("1.0.0", function()
        return "1.0.0" + 1
    end) -- 333
"#,
            r#"builder.new()
    -- nofowo
    .setName("test", function()
        return "1.0.0" + 1
    end).setVersion("1.0.0", function()
        return "1.0.0" + 1
    end) -- 333
"#
        );
    }

    #[test]
    fn test_function_body_comment_does_not_force_raw_preserve() {
        assert_format!(
            r#"function JiuJieXunZong:LoadMissionTimeAward()
        -- 策划填的是分钟
    for i = 3, #tbSettings do
        tbSet[nPoolTime] =  true
            table.insert( self.tbMissionTimeReward[nChapterId][nPoolTime], {
            tbRewardItem = tbRewardItem,
            nWeight = nWeight
        }
        )
    end


end
"#,
            r#"function JiuJieXunZong:LoadMissionTimeAward()
    -- 策划填的是分钟
    for i = 3, #tbSettings do
        tbSet[nPoolTime] = true
        table.insert(self.tbMissionTimeReward[nChapterId][nPoolTime], {
            tbRewardItem = tbRewardItem,
            nWeight = nWeight
        })
    end
end
"#
        );
    }

    #[test]
    fn test_function_stat_preserves_inline_comment_in_params() {
        assert_format!(
            r#"function foo(a -- first
, b)
    return a + b
end
"#,
            r#"function foo(
    a, -- first
    b
)
    return a + b
end
"#
        );
    }

    #[test]
    fn test_function_stat_preserves_standalone_comment_before_params() {
        assert_format!(
            r#"function foo
-- separator
(a, b)
    return a + b
end
"#,
            r#"function foo
-- separator
(a, b)
    return a + b
end
"#
        );
    }

    #[test]
    fn test_local_function_stat_preserves_standalone_comment_before_params() {
        assert_format!(
            r#"local function foo
-- separator
(a, b)
    return a + b
end
"#,
            r#"local function foo
-- separator
(a, b)
    return a + b
end
"#
        );
    }

    #[test]
    fn test_function_stat_preserves_comment_before_params_with_method_name() {
        assert_format!(
            r#"function module.subsystem:build
-- separator
(first, second)
    return first + second
end
"#,
            r#"function module.subsystem:build
-- separator
(first, second)
    return first + second
end
"#
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
            r#"if alpha_beta_gamma then return delta_theta end
"#,
            r#"if alpha_beta_gamma then
    return delta_theta
end
"#,
            config
        );
    }

    #[test]
    fn test_local_stat_preserves_standalone_comment_between_name_and_assign() {
        assert_format!(
            r#"local a
-- separator
= 123
"#,
            r#"local a
-- separator
= 123
"#
        );
    }

    #[test]
    fn test_assign_stat_preserves_standalone_comment_before_assign_op() {
        assert_format!(
            r#"value
-- separator
= 123
"#,
            r#"value
-- separator
= 123
"#
        );
    }

    #[test]
    fn test_return_stat_preserves_standalone_comment_before_expr() {
        assert_format!(
            r#"return
-- separator
value
"#,
            r#"return
-- separator
value
"#
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
            r#"local function foo() end
"#
        );
    }

    #[test]
    fn test_empty_local_function_with_params() {
        assert_format!(
            r#"
local function foo(a, b)
end
"#,
            r#"local function foo(a, b) end
"#
        );
    }
}
