#[cfg(test)]
mod tests {
    // ========== unary / binary / concat ==========

    use crate::{
        SourceText, assert_format, assert_format_with_config,
        config::{LayoutConfig, LuaFormatConfig},
        reformat_lua_code,
    };
    use emmylua_parser::LuaLanguageLevel;

    #[test]
    fn test_unary_expr() {
        assert_format!(
            r#"
local a = not b
local c = -d
local e = #t
"#,
            r#"
local a = not b
local c = -d
local e = #t
"#
        );
    }

    #[test]
    fn test_binary_expr() {
        assert_format!(
            r#"local a = 1 + 2 * 3
"#,
            r#"local a = 1 + 2 * 3
"#
        );
    }

    #[test]
    fn test_concat_expr() {
        assert_format!(
            r#"local s = a .. b .. c
"#,
            r#"local s = a .. b .. c
"#
        );
    }

    #[test]
    fn test_multiline_binary_layout_reflows_when_width_allows() {
        assert_format!(
            r#"local result = first
    + second
    + third
"#,
            r#"local result = first + second + third
"#
        );
    }

    #[test]
    fn test_binary_expr_preserves_standalone_comment_before_operator() {
        assert_format!(
            r#"local result = a
-- separator
+ b
"#,
            r#"local result = a
-- separator
+ b
"#
        );
    }

    #[test]
    fn test_binary_expr_keeps_inline_doc_long_comment_before_operator() {
        assert_format!(
            r#"local x = x--[[@cast -?]] * 60
"#,
            r#"local x = x--[[@cast -?]] * 60
"#
        );
    }

    #[test]
    fn test_binary_expr_keeps_inline_long_comment_before_operator() {
        assert_format!(
            r#"local x = x--[[cast]] * 60
"#,
            r#"local x = x--[[cast]] * 60
"#
        );
    }

    #[test]
    fn test_binary_chain_uses_progressive_line_packing() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 48,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"local value = alpha_beta_gamma + delta_theta + epsilon + zeta
"#,
            r#"local value = alpha_beta_gamma + delta_theta
    + epsilon + zeta
"#,
            config
        );
    }

    #[test]
    fn test_binary_chain_fill_keeps_multiple_segments_per_line() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 30,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"local total = alpha + beta + gamma + delta
"#,
            r#"local total = alpha + beta
    + gamma + delta
"#,
            config
        );
    }

    #[test]
    fn test_binary_chain_prefers_balanced_packed_layout() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 28,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"local value = aaaa + bbbb + cccc + dddd + eeee + ffff
"#,
            r#"local value = aaaa + bbbb
    + cccc + dddd
    + eeee + ffff
"#,
            config
        );
    }

    // ========== index ==========

    #[test]
    fn test_index_expr() {
        assert_format!(
            r#"
local a = t.x
local b = t[1]
"#,
            r#"
local a = t.x
local b = t[1]
"#
        );
    }

    #[test]
    fn test_index_expr_preserves_standalone_comment_inside_brackets() {
        assert_format!(
            r#"local value = t[
-- separator
key
]
"#,
            r#"local value = t[
-- separator
key
]
"#
        );
    }

    #[test]
    fn test_index_expr_preserves_standalone_comment_before_suffix() {
        assert_format!(
            r#"local value = t
-- separator
[key]
"#,
            r#"local value = t
-- separator
[key]
"#
        );
    }

    #[test]
    fn test_paren_expr_preserves_standalone_comment_inside() {
        assert_format!(
            r#"local value = (
-- separator
a
)
"#,
            r#"local value = (
-- separator
a
)
"#
        );
    }

    // ========== table ==========

    #[test]
    fn test_table_expr() {
        assert_format!(
            r#"local t = { a = 1, b = 2, c = 3 }
"#,
            r#"local t = { a = 1, b = 2, c = 3 }
"#
        );
    }

    #[test]
    fn test_table_expr_preserves_inline_comment_after_open_brace() {
        assert_format!(
            r#"local d = { -- enne
    a = 1, -- hf
    b = 2,
}
"#,
            r#"local d = { -- enne
    a = 1, -- hf
    b = 2
}
"#
        );
    }

    #[test]
    fn test_table_expr_formats_body_with_after_open_delimiter_comment() {
        assert_format!(
            r#"local d = { -- enne
a=1,-- hf
b=2,
}
"#,
            r#"local d = { -- enne
    a = 1, -- hf
    b = 2
}
"#
        );
    }

    #[test]
    fn test_table_expr_formats_separator_comment_with_attached_field() {
        assert_format!(
            r#"local t = {
a=1,
-- separator
b=2
}
"#,
            r#"local t = {
    a = 1,
    -- separator
    b = 2
}
"#
        );
    }

    #[test]
    fn test_table_expr_formats_before_close_comment_attachment() {
        assert_format!(
            r#"local t = {
a=1,
-- tail
}
"#,
            r#"local t = {
    a = 1
    -- tail
}
"#
        );
    }

    #[test]
    fn test_empty_table() {
        assert_format!(
            r#"local t = {}
"#,
            r#"local t = {}
"#
        );
    }

    #[test]
    fn test_multiline_table_layout_reflows_when_width_allows() {
        assert_format!(
            r#"local t = {
    a = 1,
    b = 2,
}
"#,
            r#"local t = { a = 1, b = 2 }
"#
        );
    }

    #[test]
    fn test_table_with_nested_table_expands_by_shape() {
        assert_format!(
            r#"local t = { user = { name = "a", age = 1 }, enabled = true }
"#,
            r#"local t = { user = { name = "a", age = 1 }, enabled = true }
"#
        );
    }

    #[test]
    fn test_mixed_table_style_expands_by_shape() {
        assert_format!(
            r#"local t = { answer = 42, compute() }
"#,
            r#"local t = { answer = 42, compute() }
"#
        );
    }

    #[test]
    fn test_mixed_named_and_bracket_key_table_expands_by_shape() {
        assert_format!(
            r#"local t = { answer = 42, ["name"] = user_name }
"#,
            r#"local t = { answer = 42, ["name"] = user_name }
"#
        );
    }

    #[test]
    fn test_dsl_style_call_list_table_expands_by_shape() {
        assert_format!(
            r#"local pipeline = { step_one(), step_two(), step_three() }
"#,
            r#"local pipeline = { step_one(), step_two(), step_three() }
"#
        );
    }

    // ========== call ==========

    #[test]
    fn test_string_call() {
        assert_format!(
            r#"require "module"
"#,
            r#"require "module"
"#
        );
    }

    #[test]
    fn test_table_call() {
        assert_format!(
            r#"foo { 1, 2, 3 }
"#,
            r#"foo { 1, 2, 3 }
"#
        );
    }

    #[test]
    fn test_call_expr_preserves_inline_comment_in_args() {
        assert_format!(
            r#"foo(a -- first
, b)
"#,
            r#"foo(
    a, -- first
    b
)
"#
        );
    }

    #[test]
    fn test_call_expr_formats_after_open_comment_attachment() {
        assert_format!(
            r#"foo( -- first
a,-- second
b
)
"#,
            r#"foo( -- first
    a, -- second
    b
)
"#
        );
    }

    #[test]
    fn test_call_expr_formats_separator_comment_attachment() {
        assert_format!(
            r#"foo(
a,
-- separator
b
)
"#,
            r#"foo(
    a,
    -- separator
    b
)
"#
        );
    }

    #[test]
    fn test_call_expr_preserves_before_close_comment_attachment() {
        assert_format!(
            r#"foo(
a,
-- tail
)
"#,
            r#"foo(
a,
-- tail
)
"#
        );
    }

    #[test]
    fn test_call_expr_formats_inline_comment_between_prefix_and_args() {
        assert_format!(
            r#"local value = foo -- note
(a, b)
"#,
            r#"local value = foo -- note
(a, b)
"#
        );
    }

    #[test]
    fn test_closure_expr_preserves_inline_comment_in_params() {
        assert_format!(
            r#"local f = function(a -- first
, b)
    return a + b
end
"#,
            r#"local f = function(
    a, -- first
    b
)
    return a + b
end
"#
        );
    }

    #[test]
    fn test_closure_expr_formats_after_open_comment_in_params() {
        assert_format!(
            r#"local f = function( -- first
a,-- second
b
)
    return a + b
end
"#,
            r#"local f = function( -- first
    a, -- second
    b
)
    return a + b
end
"#
        );
    }

    #[test]
    fn test_closure_expr_preserves_before_close_comment_in_params() {
        assert_format!(
            r#"local f = function(
a,
-- tail
)
    return a
end
"#,
            r#"local f = function(
a,
-- tail
)
    return a
end
"#
        );
    }

    #[test]
    fn test_closure_expr_formats_inline_comment_before_end() {
        assert_format!(
            r#"local f = function() -- note
end
"#,
            r#"local f = function() -- note
end
"#
        );
    }

    #[test]
    fn test_closure_expr_comment_only_body_does_not_insert_space_before_end() {
        assert_format!(
            r#"Execute(function(data)
    -- comment

 end)
"#,
            r#"Execute(function(data)
    -- comment

end)
"#
        );
    }

    #[test]
    fn test_simple_inline_lambda_stays_inline() {
        assert_format!(
            r#"local f = function() return  true end
"#,
            r#"local f = function() return true end
"#
        );
    }

    #[test]
    fn test_simple_inline_lambda_callback_stays_inline() {
        assert_format!(
            r#"map(items, function(x) return  x + 1 end)
"#,
            r#"map(items, function(x) return x + 1 end)
"#
        );
    }

    #[test]
    fn test_multiline_call_args_layout_reflow_when_width_allows() {
        assert_format!(
            r#"some_function(
    first,
    second,
    third
)
"#,
            r#"some_function(first, second, third)
"#
        );
    }

    #[test]
    fn test_nested_call_args_do_not_force_outer_multiline_by_shape() {
        assert_format!(
            r#"cannotload("attempt to load a text chunk", load(read1(x), "modname", "b", {}))
"#,
            r#"cannotload("attempt to load a text chunk", load(read1(x), "modname", "b", {}))
"#
        );
    }

    #[test]
    fn test_nested_call_args_keep_inner_inline_when_outer_breaks() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 50,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"cannotload("attempt to load a text chunk", load(read1(x), "modname", "b", {}))
"#,
            r#"cannotload(
    "attempt to load a text chunk",
    load(read1(x), "modname", "b", {})
)
"#,
            config
        );
    }

    #[test]
    fn test_call_expr_keeps_simple_tail_arg_on_same_line_after_multiline_first_arg() {
        assert_format!(
            r#"local self = setmetatable({
    _obj = obj,
    __flags = {
        message = msg,
    },
}, Assertion)
"#,
            r#"local self = setmetatable({
    _obj = obj,
    __flags = {
        message = msg
    }
}, Assertion)
"#
        );
    }

    #[test]
    fn test_call_args_use_progressive_fill_before_full_expansion() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 44,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"some_function(first_arg, second_arg, third_arg, fourth_arg)
"#,
            r#"some_function(
    first_arg, second_arg, third_arg,
    fourth_arg
)
"#,
            config
        );
    }

    #[test]
    fn test_callback_arg_with_multiline_closure_resets_tail_width() {
        assert_format!(
            r#"check(function()
    return not not k3
end, 'LOADTRUE', 'RETURN1')
"#,
            r#"check(function()
    return not not k3
end,
    'LOADTRUE', 'RETURN1')
"#
        );
    }

    #[test]
    fn test_first_table_arg_keeps_short_tail_packed_after_multiline_block() {
        assert_format!(
            r#"configure({
    key = value,
    another = other,
}, option_one, option_two)
"#,
            r#"configure({
    key = value,
    another = other
}, option_one, option_two)
"#
        );
    }

    #[test]
    fn test_multiline_callback_tail_still_uses_progressive_fill_when_needed() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 28,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"check(function()
    return not not k3
end, 'LOADTRUE', 'RETURN1', 'EXTRA')
"#,
            r#"check(function()
    return not not k3
end,
    'LOADTRUE', 'RETURN1',
    'EXTRA')
"#,
            config
        );
    }

    #[test]
    fn test_non_first_multiline_callback_resets_call_anchor() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 40,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"test(1, "hello", function()
        return 1, 2, 34
    end, 4, 5, 6, 7, 8, 9, 10)
"#,
            r#"test(1, "hello", function()
    return 1, 2, 34
end, 4, 5, 6, 7, 8, 9, 10
)
"#,
            config
        );
    }

    #[test]
    fn test_non_first_multiline_callback_tail_refills_after_end() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 28,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"test(1, "hello", function()
        return 1, 2, 34
    end, 4, 5, 6, 7, 8, 9, 10)
"#,
            r#"test(1, "hello", function()
    return 1, 2, 34
end, 4, 5, 6, 7, 8, 9, 10
)
"#,
            config
        );
    }

    #[test]
    fn test_non_first_multiline_callback_end_aligns_with_call_anchor() {
        assert_format!(
            r#"table.sort({ 3, 1, 2 }, function(a, b)
        return a < b
    end)
"#,
            r#"table.sort({ 3, 1, 2 }, function(a, b)
    return a < b
end)
"#
        );
    }

    #[test]
    fn test_chained_call_suffix_does_not_double_indent_multiline_args() {
        assert_format!(
            r#"a.a = function()
    return function(l)
        a.Add(
                aaaa,
                bbbb,
                cccc,
                dddd,
                eeee,
                nil,
                nil,
                nil,
                aafafa -- comment
            )()
    end
end
"#,
            r#"a.a = function()
    return function(l)
        a.Add(
            aaaa,
            bbbb,
            cccc,
            dddd,
            eeee,
            nil,
            nil,
            nil,
            aafafa -- comment
        )()
    end
end
"#
        );
    }

    #[test]
    fn test_chained_index_after_multiline_call_does_not_double_indent_args() {
        assert_format!(
            r#"a.a = function()
    return function(l)
        return a.Add(
                aaaa,
                bbbb,
                cccc,
                dddd,
                eeee,
                nil,
                nil,
                nil,
                aafafa -- comment
            )[1]
    end
end
"#,
            r#"a.a = function()
    return function(l)
        return a.Add(
            aaaa,
            bbbb,
            cccc,
            dddd,
            eeee,
            nil,
            nil,
            nil,
            aafafa -- comment
        )[1]
    end
end
"#
        );
    }

    #[test]
    fn test_chained_method_after_multiline_call_does_not_double_indent_args() {
        assert_format!(
            r#"a.a = function()
    return function(l)
        return a.Add(
                aaaa,
                bbbb,
                cccc,
                dddd,
                eeee,
                nil,
                nil,
                nil,
                aafafa -- comment
            ):next()
    end
end
"#,
            r#"a.a = function()
    return function(l)
        return a.Add(
            aaaa,
            bbbb,
            cccc,
            dddd,
            eeee,
            nil,
            nil,
            nil,
            aafafa -- comment
        ):next()
    end
end
"#
        );
    }

    #[test]
    fn test_multiline_call_comparison_keeps_short_rhs_on_closing_line() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 40,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"assert(check(function()
    return true
end, 'LOADTRUE', 'RETURN1') == "hiho")
"#,
            r#"assert(check(function()
        return true
    end,
        'LOADTRUE', 'RETURN1') == "hiho")
"#,
            config
        );
    }

    #[test]
    fn test_user_multiline_assert_comparison_keeps_eq_on_call_closing_line() {
        let input = r#"assert(
        T.checkpanic(
            [[
            pushstring "return {__close = function () Y = 'ho'; end}"
      newtable
      loadstring -2
      call 0 1
      setmetatable -2
      toclose -1
            pushstring "hi"
      error
    ]], [[
      getglobal Y
      concat 2         # concat original error with global Y
    ]]
        )
                        == "hiho"
    )
"#;
        let result = reformat_lua_code(
            &SourceText {
                text: input,
                level: LuaLanguageLevel::default(),
            },
            &LuaFormatConfig::default(),
        );

        assert!(
            result.contains(") == \"hiho\"") || result.contains(") == \"hiho\""),
            "expected comparison tail to stay on the call closing line, got:\n{result}"
        );
    }

    #[test]
    fn test_table_auto_without_alignment_uses_progressive_fill() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 28,
                ..Default::default()
            },
            align: crate::config::AlignConfig {
                table_field: false,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"local t = { alpha, beta, gamma, delta }
"#,
            r#"local t = {
    alpha, beta, gamma,
    delta
}
"#,
            config
        );
    }

    #[test]
    fn test_table_field_preserves_multiline_closure_value_shape() {
        assert_format!(
            r#"local spec = {
    callback = function()
        return true
    end,
    fallback = another_value,
}
"#,
            r#"local spec = {
    callback = function()
        return true
    end,
    fallback = another_value
}
"#
        );
    }

    #[test]
    fn test_table_field_multiline_closure_value_still_formats_interior() {
        assert_format!(
            r#"local mt = {
    __eq = function (a, b)
        coroutine.yield(nil, "eq")
        return  val(a) ==       val(b)
    end
}
"#,
            r#"local mt = {
    __eq = function(a, b)
        coroutine.yield(nil, "eq")
        return val(a) == val(b)
    end
}
"#
        );
    }

    #[test]
    fn test_table_field_preserves_multiline_nested_table_value_shape() {
        assert_format!(
            r#"local spec = {
    nested = {
        foo=1,
        bar =    2,
    },
    fallback = another_value,
}
"#,
            r#"local spec = {
    nested = {
        foo = 1,
        bar = 2
    },
    fallback = another_value
}
"#
        );
    }

    #[test]
    fn test_deep_nested_table_field_keeps_expanded_shape_and_formats_interior() {
        assert_format!(
            r#"local spec = {
    outer = {
        callback = function (a, b)
            return  val(a) ==       val(b)
        end,
        nested = {
            foo=1,
            bar =    2,
        },
    },
}
"#,
            r#"local spec = {
    outer = {
        callback = function(a, b)
            return val(a) == val(b)
        end,
        nested = {
            foo = 1,
            bar = 2
        }
    }
}
"#
        );
    }

    #[test]
    fn test_multiline_call_arg_nested_table_keeps_expanded_shape_and_formats_interior() {
        assert_format!(
            r#"local spec = {
    outer = {
        callback = wrap(function (a, b)
            return  val(a) ==       val(b)
        end, {
            foo=1,
            bar =    2,
        }),
        fallback = another_value,
    },
}
"#,
            r#"local spec = {
    outer = {
        callback = wrap(function(a, b)
            return val(a) == val(b)
        end,
            {
                foo = 1,
                bar = 2
            }),
        fallback = another_value
    }
}
"#
        );
    }

    // ========== chain call ==========

    #[test]
    fn test_method_chain_short() {
        assert_format!(
            r#"a:b():c():d()
"#,
            r#"a:b():c():d()
"#
        );
    }

    #[test]
    fn test_method_chain_with_args() {
        assert_format!(
            r#"builder:setName("foo"):setAge(25):build()
"#,
            r#"builder:setName("foo"):setAge(25):build()
"#
        );
    }

    #[test]
    fn test_property_chain() {
        assert_format!(
            r#"local a = t.x.y.z
"#,
            r#"local a = t.x.y.z
"#
        );
    }

    #[test]
    fn test_mixed_chain() {
        assert_format!(
            r#"a.b:c():d()
"#,
            r#"a.b:c():d()
"#
        );
    }

    #[test]
    fn test_multiline_chain_layout_reflows_when_width_allows() {
        assert_format!(
            r#"builder
    :set_name(name)
    :set_age(age)
    :build()
"#,
            r#"builder:set_name(name):set_age(age):build()
"#
        );
    }

    #[test]
    fn test_method_chain_uses_progressive_fill_when_width_exceeded() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 32,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"builder:set_name(name):set_age(age):build()
"#,
            r#"builder
    :set_name(name):set_age(age)
    :build()
"#,
            config
        );
    }

    #[test]
    fn test_method_chain_breaks_one_segment_per_line_when_width_exceeded() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 24,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"builder:set_name(name):set_age(age):build()
"#,
            r#"builder
    :set_name(name)
    :set_age(age)
    :build()
"#,
            config
        );
    }

    #[test]
    fn test_chain_keeps_single_multiline_table_payload_attached() {
        assert_format!(
            r#"builder:with_config({
    key = value,
    another = other,
}):set_name(name):build()
"#,
            r#"builder:with_config({
    key = value,
    another = other
}):set_name(name):build()
"#
        );
    }

    #[test]
    fn test_chain_keeps_mixed_closure_and_multiline_table_payloads_expanded() {
        assert_format!(
            r#"builder:with_config(function (a, b)
    return  val(a) ==       val(b)
end, {
    foo=1,
    bar =    2,
}):set_name(name):build()
"#,
            r#"builder:with_config(function(a, b)
    return val(a) == val(b)
end,
    {
        foo = 1,
        bar = 2
    }):set_name(name):build()
"#
        );
    }

    #[test]
    fn test_chain_keeps_mixed_closure_table_and_fallback_payloads_expanded() {
        assert_format!(
            r#"builder:with_config(function (a, b)
    return  val(a) ==       val(b)
end, {
    foo=1,
    bar =    2,
}, fallback):set_name(name):build()
"#,
            r#"builder:with_config(function(a, b)
    return val(a) == val(b)
end,
    {
        foo = 1,
        bar = 2
    }, fallback):set_name(name):build()
"#
        );
    }

    #[test]
    fn test_if_header_keeps_short_comparison_tail_with_multiline_callback_call() {
        assert_format!(
            r#"if check(function()
    return true
end, 'LOADTRUE', 'RETURN1') == "hiho" then
    print('ok')
end
"#,
            r#"if check(function()
    return true
end,
    'LOADTRUE', 'RETURN1') == "hiho" then
    print('ok')
end
"#
        );
    }

    // ========== and / or expression ==========

    #[test]
    fn test_and_or_expr() {
        assert_format!(
            r#"local x = condition_one and value_one or condition_two and value_two or default_value
"#,
            r#"local x = condition_one and value_one or condition_two and value_two or default_value
"#
        );
    }
}
