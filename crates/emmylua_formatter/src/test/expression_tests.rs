#[cfg(test)]
mod tests {
    // ========== unary / binary / concat ==========

    use crate::{
        assert_format, assert_format_with_config,
        config::{LayoutConfig, LuaFormatConfig},
    };

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
        assert_format!("local a = 1 + 2 * 3\n", "local a = 1 + 2 * 3\n");
    }

    #[test]
    fn test_concat_expr() {
        assert_format!("local s = a .. b .. c\n", "local s = a .. b .. c\n");
    }

    #[test]
    fn test_multiline_binary_layout_reflows_when_width_allows() {
        assert_format!(
            "local result = first\n    + second\n    + third\n",
            "local result = first + second + third\n"
        );
    }

    #[test]
    fn test_binary_expr_preserves_standalone_comment_before_operator() {
        assert_format!(
            "local result = a\n-- separator\n+ b\n",
            "local result = a\n-- separator\n+ b\n"
        );
    }

    #[test]
    fn test_binary_expr_keeps_inline_doc_long_comment_before_operator() {
        assert_format!(
            "local x = x--[[@cast -?]] * 60\n",
            "local x = x--[[@cast -?]] * 60\n"
        );
    }

    #[test]
    fn test_binary_expr_keeps_inline_long_comment_before_operator() {
        assert_format!(
            "local x = x--[[cast]] * 60\n",
            "local x = x--[[cast]] * 60\n"
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
            "local value = alpha_beta_gamma + delta_theta + epsilon + zeta\n",
            "local value = alpha_beta_gamma + delta_theta\n    + epsilon + zeta\n",
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
            "local total = alpha + beta + gamma + delta\n",
            "local total = alpha + beta\n    + gamma + delta\n",
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
            "local value = aaaa + bbbb + cccc + dddd + eeee + ffff\n",
            "local value = aaaa + bbbb\n    + cccc + dddd\n    + eeee + ffff\n",
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
            "local value = t[\n-- separator\nkey\n]\n",
            "local value = t[\n-- separator\nkey\n]\n"
        );
    }

    #[test]
    fn test_index_expr_preserves_standalone_comment_before_suffix() {
        assert_format!(
            "local value = t\n-- separator\n[key]\n",
            "local value = t\n-- separator\n[key]\n"
        );
    }

    #[test]
    fn test_paren_expr_preserves_standalone_comment_inside() {
        assert_format!(
            "local value = (\n-- separator\na\n)\n",
            "local value = (\n-- separator\na\n)\n"
        );
    }

    // ========== table ==========

    #[test]
    fn test_table_expr() {
        assert_format!(
            "local t = { a = 1, b = 2, c = 3 }\n",
            "local t = { a = 1, b = 2, c = 3 }\n"
        );
    }

    #[test]
    fn test_table_expr_preserves_inline_comment_after_open_brace() {
        assert_format!(
            "local d = { -- enne\n    a = 1, -- hf\n    b = 2,\n}\n",
            "local d = { -- enne\n    a = 1, -- hf\n    b = 2\n}\n"
        );
    }

    #[test]
    fn test_table_expr_formats_body_with_after_open_delimiter_comment() {
        assert_format!(
            "local d = { -- enne\na=1,-- hf\nb=2,\n}\n",
            "local d = { -- enne\n    a = 1, -- hf\n    b = 2\n}\n"
        );
    }

    #[test]
    fn test_table_expr_formats_separator_comment_with_attached_field() {
        assert_format!(
            "local t = {\na=1,\n-- separator\nb=2\n}\n",
            "local t = {\n    a = 1,\n    -- separator\n    b = 2\n}\n"
        );
    }

    #[test]
    fn test_table_expr_formats_before_close_comment_attachment() {
        assert_format!(
            "local t = {\na=1,\n-- tail\n}\n",
            "local t = {\n    a = 1\n    -- tail\n}\n"
        );
    }

    #[test]
    fn test_empty_table() {
        assert_format!("local t = {}\n", "local t = {}\n");
    }

    #[test]
    fn test_multiline_table_layout_reflows_when_width_allows() {
        assert_format!(
            "local t = {\n    a = 1,\n    b = 2,\n}\n",
            "local t = { a = 1, b = 2 }\n"
        );
    }

    #[test]
    fn test_table_with_nested_table_expands_by_shape() {
        assert_format!(
            "local t = { user = { name = \"a\", age = 1 }, enabled = true }\n",
            "local t = { user = { name = \"a\", age = 1 }, enabled = true }\n"
        );
    }

    #[test]
    fn test_mixed_table_style_expands_by_shape() {
        assert_format!(
            "local t = { answer = 42, compute() }\n",
            "local t = { answer = 42, compute() }\n"
        );
    }

    #[test]
    fn test_mixed_named_and_bracket_key_table_expands_by_shape() {
        assert_format!(
            "local t = { answer = 42, [\"name\"] = user_name }\n",
            "local t = { answer = 42, [\"name\"] = user_name }\n"
        );
    }

    #[test]
    fn test_dsl_style_call_list_table_expands_by_shape() {
        assert_format!(
            "local pipeline = { step_one(), step_two(), step_three() }\n",
            "local pipeline = { step_one(), step_two(), step_three() }\n"
        );
    }

    // ========== call ==========

    #[test]
    fn test_string_call() {
        assert_format!("require \"module\"\n", "require \"module\"\n");
    }

    #[test]
    fn test_table_call() {
        assert_format!("foo { 1, 2, 3 }\n", "foo { 1, 2, 3 }\n");
    }

    #[test]
    fn test_call_expr_preserves_inline_comment_in_args() {
        assert_format!(
            "foo(a -- first\n, b)\n",
            "foo(\n    a, -- first\n    b\n)\n"
        );
    }

    #[test]
    fn test_call_expr_formats_after_open_comment_attachment() {
        assert_format!(
            "foo( -- first\na,-- second\nb\n)\n",
            "foo( -- first\n    a, -- second\n    b\n)\n"
        );
    }

    #[test]
    fn test_call_expr_formats_separator_comment_attachment() {
        assert_format!(
            "foo(\na,\n-- separator\nb\n)\n",
            "foo(\n    a,\n    -- separator\n    b\n)\n"
        );
    }

    #[test]
    fn test_call_expr_preserves_before_close_comment_attachment() {
        assert_format!("foo(\na,\n-- tail\n)\n", "foo(\na,\n-- tail\n)\n");
    }

    #[test]
    fn test_call_expr_formats_inline_comment_between_prefix_and_args() {
        assert_format!(
            "local value = foo -- note\n(a, b)\n",
            "local value = foo -- note\n(a, b)\n"
        );
    }

    #[test]
    fn test_closure_expr_preserves_inline_comment_in_params() {
        assert_format!(
            "local f = function(a -- first\n, b)\n    return a + b\nend\n",
            "local f = function(\n    a, -- first\n    b\n)\n    return a + b\nend\n"
        );
    }

    #[test]
    fn test_closure_expr_formats_after_open_comment_in_params() {
        assert_format!(
            "local f = function( -- first\na,-- second\nb\n)\n    return a + b\nend\n",
            "local f = function( -- first\n    a, -- second\n    b\n)\n    return a + b\nend\n"
        );
    }

    #[test]
    fn test_closure_expr_preserves_before_close_comment_in_params() {
        assert_format!(
            "local f = function(\na,\n-- tail\n)\n    return a\nend\n",
            "local f = function(\na,\n-- tail\n)\n    return a\nend\n"
        );
    }

    #[test]
    fn test_closure_expr_formats_inline_comment_before_end() {
        assert_format!(
            "local f = function() -- note\nend\n",
            "local f = function() -- note\nend\n"
        );
    }

    #[test]
    fn test_closure_expr_comment_only_body_does_not_insert_space_before_end() {
        assert_format!(
            "Execute(function(data)\n    -- comment\n\n end)\n",
            "Execute(function(data)\n    -- comment\n\nend)\n"
        );
    }

    #[test]
    fn test_simple_inline_lambda_stays_inline() {
        assert_format!(
            "local f = function() return  true end\n",
            "local f = function() return true end\n"
        );
    }

    #[test]
    fn test_simple_inline_lambda_callback_stays_inline() {
        assert_format!(
            "map(items, function(x) return  x + 1 end)\n",
            "map(items, function(x) return x + 1 end)\n"
        );
    }

    #[test]
    fn test_multiline_call_args_layout_reflow_when_width_allows() {
        assert_format!(
            "some_function(\n    first,\n    second,\n    third\n)\n",
            "some_function(first, second, third)\n"
        );
    }

    #[test]
    fn test_nested_call_args_do_not_force_outer_multiline_by_shape() {
        assert_format!(
            "cannotload(\"attempt to load a text chunk\", load(read1(x), \"modname\", \"b\", {}))\n",
            "cannotload(\"attempt to load a text chunk\", load(read1(x), \"modname\", \"b\", {}))\n"
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
            "cannotload(\"attempt to load a text chunk\", load(read1(x), \"modname\", \"b\", {}))\n",
            "cannotload(\n    \"attempt to load a text chunk\",\n    load(read1(x), \"modname\", \"b\", {})\n)\n",
            config
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
            "some_function(first_arg, second_arg, third_arg, fourth_arg)\n",
            "some_function(\n    first_arg, second_arg, third_arg,\n    fourth_arg\n)\n",
            config
        );
    }

    #[test]
    fn test_callback_arg_with_multiline_closure_resets_tail_width() {
        assert_format!(
            "check(function()\n    return not not k3\nend, 'LOADTRUE', 'RETURN1')\n",
            "check(function()\n    return not not k3\nend,\n    'LOADTRUE', 'RETURN1')\n"
        );
    }

    #[test]
    fn test_first_table_arg_keeps_short_tail_packed_after_multiline_block() {
        assert_format!(
            "configure({\n    key = value,\n    another = other,\n}, option_one, option_two)\n",
            "configure({\n    key = value,\n    another = other\n},\n    option_one, option_two)\n"
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
            "check(function()\n    return not not k3\nend, 'LOADTRUE', 'RETURN1', 'EXTRA')\n",
            "check(function()\n    return not not k3\nend,\n    'LOADTRUE', 'RETURN1',\n    'EXTRA')\n",
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
            "test(1, \"hello\", function()\n        return 1, 2, 34\n    end, 4, 5, 6, 7, 8, 9, 10)\n",
            "test(1, \"hello\", function()\n    return 1, 2, 34\nend, 4, 5, 6, 7, 8, 9, 10\n)\n",
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
            "test(1, \"hello\", function()\n        return 1, 2, 34\n    end, 4, 5, 6, 7, 8, 9, 10)\n",
            "test(1, \"hello\", function()\n    return 1, 2, 34\nend, 4, 5, 6, 7, 8, 9, 10\n)\n",
            config
        );
    }

    #[test]
    fn test_non_first_multiline_callback_end_aligns_with_call_anchor() {
        assert_format!(
            "table.sort({ 3, 1, 2 }, function(a, b)\n        return a < b\n    end)\n",
            "table.sort({ 3, 1, 2 }, function(a, b)\n    return a < b\nend)\n"
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
            "assert(check(function()\n    return true\nend, 'LOADTRUE', 'RETURN1') == \"hiho\")\n",
            "assert(check(function()\n        return true\n    end,\n        'LOADTRUE', 'RETURN1') == \"hiho\")\n",
            config
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
            "local t = { alpha, beta, gamma, delta }\n",
            "local t = {\n    alpha, beta, gamma,\n    delta\n}\n",
            config
        );
    }

    #[test]
    fn test_table_field_preserves_multiline_closure_value_shape() {
        assert_format!(
            "local spec = {\n    callback = function()\n        return true\n    end,\n    fallback = another_value,\n}\n",
            "local spec = {\n    callback = function()\n        return true\n    end,\n    fallback = another_value\n}\n"
        );
    }

    #[test]
    fn test_table_field_multiline_closure_value_still_formats_interior() {
        assert_format!(
            "local mt = {\n    __eq = function (a, b)\n        coroutine.yield(nil, \"eq\")\n        return  val(a) ==       val(b)\n    end\n}\n",
            "local mt = {\n    __eq = function(a, b)\n        coroutine.yield(nil, \"eq\")\n        return val(a) == val(b)\n    end\n}\n"
        );
    }

    #[test]
    fn test_table_field_preserves_multiline_nested_table_value_shape() {
        assert_format!(
            "local spec = {\n    nested = {\n        foo=1,\n        bar =    2,\n    },\n    fallback = another_value,\n}\n",
            "local spec = {\n    nested = {\n        foo = 1,\n        bar = 2\n    },\n    fallback = another_value\n}\n"
        );
    }

    #[test]
    fn test_deep_nested_table_field_keeps_expanded_shape_and_formats_interior() {
        assert_format!(
            "local spec = {\n    outer = {\n        callback = function (a, b)\n            return  val(a) ==       val(b)\n        end,\n        nested = {\n            foo=1,\n            bar =    2,\n        },\n    },\n}\n",
            "local spec = {\n    outer = {\n        callback = function(a, b)\n            return val(a) == val(b)\n        end,\n        nested = {\n            foo = 1,\n            bar = 2\n        }\n    }\n}\n"
        );
    }

    #[test]
    fn test_multiline_call_arg_nested_table_keeps_expanded_shape_and_formats_interior() {
        assert_format!(
            "local spec = {\n    outer = {\n        callback = wrap(function (a, b)\n            return  val(a) ==       val(b)\n        end, {\n            foo=1,\n            bar =    2,\n        }),\n        fallback = another_value,\n    },\n}\n",
            "local spec = {\n    outer = {\n        callback = wrap(function(a, b)\n            return val(a) == val(b)\n        end,\n            {\n                foo = 1,\n                bar = 2\n            }),\n        fallback = another_value\n    }\n}\n"
        );
    }

    // ========== chain call ==========

    #[test]
    fn test_method_chain_short() {
        assert_format!("a:b():c():d()\n", "a:b():c():d()\n");
    }

    #[test]
    fn test_method_chain_with_args() {
        assert_format!(
            "builder:setName(\"foo\"):setAge(25):build()\n",
            "builder:setName(\"foo\"):setAge(25):build()\n"
        );
    }

    #[test]
    fn test_property_chain() {
        assert_format!("local a = t.x.y.z\n", "local a = t.x.y.z\n");
    }

    #[test]
    fn test_mixed_chain() {
        assert_format!("a.b:c():d()\n", "a.b:c():d()\n");
    }

    #[test]
    fn test_multiline_chain_layout_reflows_when_width_allows() {
        assert_format!(
            "builder\n    :set_name(name)\n    :set_age(age)\n    :build()\n",
            "builder:set_name(name):set_age(age):build()\n"
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
            "builder:set_name(name):set_age(age):build()\n",
            "builder\n    :set_name(name):set_age(age)\n    :build()\n",
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
            "builder:set_name(name):set_age(age):build()\n",
            "builder\n    :set_name(name)\n    :set_age(age)\n    :build()\n",
            config
        );
    }

    #[test]
    fn test_chain_keeps_single_multiline_table_payload_attached() {
        assert_format!(
            "builder:with_config({\n    key = value,\n    another = other,\n}):set_name(name):build()\n",
            "builder:with_config({\n    key = value,\n    another = other\n}):set_name(name):build()\n"
        );
    }

    #[test]
    fn test_chain_keeps_mixed_closure_and_multiline_table_payloads_expanded() {
        assert_format!(
            "builder:with_config(function (a, b)\n    return  val(a) ==       val(b)\nend, {\n    foo=1,\n    bar =    2,\n}):set_name(name):build()\n",
            "builder:with_config(function(a, b)\n    return val(a) == val(b)\nend,\n    {\n        foo = 1,\n        bar = 2\n    }):set_name(name):build()\n"
        );
    }

    #[test]
    fn test_chain_keeps_mixed_closure_table_and_fallback_payloads_expanded() {
        assert_format!(
            "builder:with_config(function (a, b)\n    return  val(a) ==       val(b)\nend, {\n    foo=1,\n    bar =    2,\n}, fallback):set_name(name):build()\n",
            "builder:with_config(function(a, b)\n    return val(a) == val(b)\nend,\n    {\n        foo = 1,\n        bar = 2\n    }, fallback):set_name(name):build()\n"
        );
    }

    #[test]
    fn test_if_header_keeps_short_comparison_tail_with_multiline_callback_call() {
        assert_format!(
            "if check(function()\n    return true\nend, 'LOADTRUE', 'RETURN1') == \"hiho\" then\n    print('ok')\nend\n",
            "if check(function()\n    return true\nend,\n    'LOADTRUE', 'RETURN1') == \"hiho\" then\n    print('ok')\nend\n"
        );
    }

    // ========== and / or expression ==========

    #[test]
    fn test_and_or_expr() {
        assert_format!(
            "local x = condition_one and value_one or condition_two and value_two or default_value\n",
            "local x = condition_one and value_one or condition_two and value_two or default_value\n"
        );
    }
}
