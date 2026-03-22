#[cfg(test)]
mod tests {
    use crate::assert_format;

    #[test]
    fn test_leading_comment() {
        assert_format!(
            r#"
-- this is a comment
local a = 1
"#,
            r#"
-- this is a comment
local a = 1
"#
        );
    }

    #[test]
    fn test_trailing_comment() {
        assert_format!("local a = 1 -- trailing\n", "local a = 1 -- trailing\n");
    }

    #[test]
    fn test_multiple_comments() {
        assert_format!(
            r#"
-- comment 1
-- comment 2
local x = 1
"#,
            r#"
-- comment 1
-- comment 2
local x = 1
"#
        );
    }

    // ========== table field trailing comments ==========

    #[test]
    fn test_table_field_trailing_comment() {
        use crate::{
            assert_format_with_config,
            config::{LayoutConfig, LuaFormatConfig},
        };

        let config = LuaFormatConfig {
            layout: LayoutConfig {
                table_expand: crate::config::ExpandStrategy::Always,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"
local t = {
    a = 1, -- first
    b = 2, -- second
    c = 3
}
"#,
            r#"
local t = {
    a = 1, -- first
    b = 2, -- second
    c = 3
}
"#,
            config
        );
    }

    #[test]
    fn test_table_field_comment_forces_expand() {
        assert_format!(
            r#"
local t = {a = 1, -- comment
b = 2}
"#,
            r#"
local t = {
    a = 1, -- comment
    b = 2
}
"#
        );
    }

    // ========== standalone comments ==========

    #[test]
    fn test_table_standalone_comment() {
        assert_format!(
            r#"
local t = {
    a = 1,
    -- separator
    b = 2,
}
"#,
            r#"
local t = {
    a = 1,
    -- separator
    b = 2
}
"#
        );
    }

    #[test]
    fn test_comment_only_block() {
        assert_format!(
            r#"
if x then
    -- only comment
end
"#,
            r#"
if x then
    -- only comment
end
"#
        );
    }

    #[test]
    fn test_comment_only_while_block() {
        assert_format!(
            r#"
while true do
    -- todo
end
"#,
            r#"
while true do
    -- todo
end
"#
        );
    }

    #[test]
    fn test_comment_only_do_block() {
        assert_format!(
            r#"
do
    -- scoped comment
end
"#,
            r#"
do
    -- scoped comment
end
"#
        );
    }

    #[test]
    fn test_comment_only_function_block() {
        assert_format!(
            r#"
function foo()
    -- stub
end
"#,
            r#"
function foo()
    -- stub
end
"#
        );
    }

    #[test]
    fn test_multiline_normal_comment_in_block() {
        assert_format!(
            r#"
if ok then
    -- hihihi
    --     hello
    --yyyy
end
"#,
            r#"
if ok then
    -- hihihi
    --     hello
    --yyyy
end
"#
        );
    }

    // ========== param comments ==========

    #[test]
    fn test_function_param_comments() {
        assert_format!(
            r#"
function foo(
    a, -- first
    b, -- second
    c
)
    return a + b + c
end
"#,
            r#"
function foo(
    a, -- first
    b, -- second
    c
)
    return a + b + c
end
"#
        );
    }

    #[test]
    fn test_local_function_param_comments() {
        assert_format!(
            r#"
local function bar(
    x, -- coord x
    y  -- coord y
)
    return x + y
end
"#,
            r#"
local function bar(
    x, -- coord x
    y  -- coord y
)
    return x + y
end
"#
        );
    }

    #[test]
    fn test_function_param_standalone_comment_preserved() {
        assert_format!(
            r#"
function foo(
    a,
    -- separator
    b
)
    return a + b
end
"#,
            r#"
function foo(
    a,
    -- separator
    b
)
    return a + b
end
"#
        );
    }

    #[test]
    fn test_call_arg_standalone_comment_preserved() {
        assert_format!(
            r#"
foo(
    a,
    -- separator
    b
)
"#,
            r#"
foo(
    a,
    -- separator
    b
)
"#
        );
    }

    #[test]
    fn test_closure_param_comments() {
        assert_format!(
            r#"
local f = function(
    a, -- first
    b  -- second
)
    return a + b
end
"#,
            r#"
local f = function(
    a, -- first
    b  -- second
)
    return a + b
end
"#
        );
    }

    // ========== alignment ==========

    #[test]
    fn test_trailing_comment_alignment() {
        assert_format!(
            r#"
local a = 1 -- short
local bbb = 2 -- long var
local cc = 3 -- medium
"#,
            r#"
local a   = 1 -- short
local bbb = 2 -- long var
local cc  = 3 -- medium
"#
        );
    }

    #[test]
    fn test_assign_alignment() {
        assert_format!(
            r#"
local x = 1
local yy = 2
local zzz = 3
"#,
            r#"
local x   = 1
local yy  = 2
local zzz = 3
"#
        );
    }

    #[test]
    fn test_table_field_alignment() {
        use crate::{
            assert_format_with_config,
            config::{LayoutConfig, LuaFormatConfig},
        };

        let config = LuaFormatConfig {
            layout: LayoutConfig {
                table_expand: crate::config::ExpandStrategy::Always,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"
local t = {
    x = 1,
    long_name = 2,
    yy = 3,
}
"#,
            r#"
local t = {
    x         = 1,
    long_name = 2,
    yy        = 3
}
"#,
            config
        );
    }

    #[test]
    fn test_table_field_alignment_in_auto_mode_when_width_exceeded() {
        use crate::{
            assert_format_with_config,
            config::{LayoutConfig, LuaFormatConfig},
        };

        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 28,
                table_expand: crate::config::ExpandStrategy::Auto,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            "local t = { x = 1, long_name = 2, yy = 3 }\n",
            r#"
local t = {
    x         = 1,
    long_name = 2,
    yy        = 3
}
"#,
            config
        );
    }

    #[test]
    fn test_alignment_disabled() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            comments: crate::config::CommentConfig {
                align_line_comments: false,
                ..Default::default()
            },
            align: crate::config::AlignConfig {
                continuous_assign_statement: false,
                table_field: false,
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"
local a = 1 -- x
local bbb = 2 -- y
"#,
            r#"
local a = 1 -- x
local bbb = 2 -- y
"#,
            config
        );
    }

    #[test]
    fn test_statement_comment_alignment_can_be_disabled_separately() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            comments: crate::config::CommentConfig {
                align_in_statements: false,
                ..Default::default()
            },
            align: crate::config::AlignConfig {
                continuous_assign_statement: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"
local a = 1 -- x
local long_name = 2 -- y
"#,
            r#"
local a = 1 -- x
local long_name = 2 -- y
"#,
            config
        );
    }

    #[test]
    fn test_param_comment_alignment_can_be_disabled_separately() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            comments: crate::config::CommentConfig {
                align_in_params: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"
local f = function(
    a, -- first
    long_name -- second
)
    return a
end
"#,
            r#"
local f = function(
    a, -- first
    long_name -- second
)
    return a
end
"#,
            config
        );
    }

    #[test]
    fn test_table_comment_alignment_can_be_disabled_separately() {
        use crate::{
            assert_format_with_config,
            config::{LayoutConfig, LuaFormatConfig},
        };

        let config = LuaFormatConfig {
            layout: LayoutConfig {
                table_expand: crate::config::ExpandStrategy::Always,
                ..Default::default()
            },
            align: crate::config::AlignConfig {
                table_field: true,
                ..Default::default()
            },
            comments: crate::config::CommentConfig {
                align_in_table_fields: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"
local t = {
    x = 100, -- first
    long_name = 2, -- second
}
"#,
            r#"
local t = {
    x         = 100, -- first
    long_name = 2 -- second
}
"#,
            config
        );
    }

    #[test]
    fn test_line_comment_min_spaces_before() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            comments: crate::config::CommentConfig {
                align_line_comments: false,
                line_comment_min_spaces_before: 3,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            "local a = 1 -- trailing\n",
            "local a = 1   -- trailing\n",
            config
        );
    }

    #[test]
    fn test_line_comment_min_column() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            align: crate::config::AlignConfig {
                continuous_assign_statement: false,
                ..Default::default()
            },
            comments: crate::config::CommentConfig {
                line_comment_min_column: 16,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"
local a = 1 -- x
local bb = 2 -- y
"#,
            r#"
local a = 1     -- x
local bb = 2    -- y
"#,
            config
        );
    }

    #[test]
    fn test_alignment_group_broken_by_blank_line() {
        assert_format!(
            r#"
local a = 1 -- x
local b = 2 -- y

local cc = 3 -- z
local d = 4 -- w
"#,
            r#"
local a = 1 -- x
local b = 2 -- y

local cc = 3 -- z
local d  = 4 -- w
"#
        );
    }

    #[test]
    fn test_alignment_group_preserves_standalone_comment() {
        assert_format!(
            r#"
local a = 1 -- x
-- divider
local long_name = 2 -- y
"#,
            r#"
local a         = 1 -- x
-- divider
local long_name = 2 -- y
"#
        );
    }

    #[test]
    fn test_alignment_group_can_break_on_standalone_comment() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            comments: crate::config::CommentConfig {
                align_across_standalone_comments: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"
local a = 1 -- x
-- divider
local long_name = 2 -- y
"#,
            r#"
local a = 1 -- x
-- divider
local long_name = 2 -- y
"#,
            config
        );
    }

    #[test]
    fn test_alignment_group_can_require_same_statement_kind() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            align: crate::config::AlignConfig {
                continuous_assign_statement: false,
                ..Default::default()
            },
            comments: crate::config::CommentConfig {
                align_same_kind_only: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"
local a = 1 -- x
bbbb = 2 -- y
"#,
            r#"
local a = 1 -- x
bbbb = 2 -- y
"#,
            config
        );
    }

    // ========== doc comment formatting ==========

    #[test]
    fn test_doc_comment_normalize_whitespace() {
        // Extra spaces in doc comment should be normalized to single space
        assert_format!(
            "---@param  name   string\nlocal function f(name) end\n",
            "---@param name string\nlocal function f(name) end\n"
        );
    }

    #[test]
    fn test_doc_comment_preserved() {
        // Well-formatted doc comment should be unchanged
        assert_format!(
            "---@param name string\nlocal function f(name) end\n",
            "---@param name string\nlocal function f(name) end\n"
        );
    }

    #[test]
    fn test_doc_comment_multi_tag() {
        assert_format!(
            "---@param a number\n---@param b string\n---@return boolean\nlocal function f(a, b) end\n",
            "---@param a number\n---@param b string\n---@return boolean\nlocal function f(a, b) end\n"
        );
    }

    #[test]
    fn test_doc_comment_align_param_columns() {
        assert_format!(
            "---@param short string desc\n---@param much_longer integer longer desc\nlocal function f(short, much_longer) end\n",
            "---@param short       string  desc\n---@param much_longer integer longer desc\nlocal function f(short, much_longer) end\n"
        );
    }

    #[test]
    fn test_doc_comment_align_field_columns() {
        assert_format!(
            "---@field x string desc\n---@field longer_name integer another desc\nlocal t = {}\n",
            "---@field x           string  desc\n---@field longer_name integer another desc\nlocal t = {}\n"
        );
    }

    #[test]
    fn test_doc_comment_align_return_columns() {
        assert_format!(
            "---@return number ok success\n---@return string, integer err failure\nfunction f() end\n",
            "---@return number ok           success\n---@return string, integer err failure\nfunction f() end\n"
        );
    }

    #[test]
    fn test_doc_comment_alignment_can_be_disabled() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            emmy_doc: crate::config::EmmyDocConfig {
                align_tag_columns: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            "---@param short string desc\n---@param much_longer integer longer desc\nlocal function f(short, much_longer) end\n",
            "---@param short string desc\n---@param much_longer integer longer desc\nlocal function f(short, much_longer) end\n",
            config
        );
    }

    #[test]
    fn test_doc_comment_declaration_alignment_can_be_disabled_separately() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            emmy_doc: crate::config::EmmyDocConfig {
                align_declaration_tags: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            "---@class Short short desc\n---@class LongerName<T> longer desc\nlocal value = {}\n",
            "---@class Short short desc\n---@class LongerName<T> longer desc\nlocal value = {}\n",
            config
        );
    }

    #[test]
    fn test_doc_comment_reference_alignment_can_be_disabled_separately() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            emmy_doc: crate::config::EmmyDocConfig {
                align_reference_tags: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            "---@param short string desc\n---@param much_longer integer longer desc\nlocal function f(short, much_longer) end\n",
            "---@param short string desc\n---@param much_longer integer longer desc\nlocal function f(short, much_longer) end\n",
            config
        );
    }

    #[test]
    fn test_doc_comment_align_class_columns() {
        assert_format!(
            "---@class Short short desc\n---@class LongerName<T> longer desc\nlocal value = {}\n",
            "---@class Short         short desc\n---@class LongerName<T> longer desc\nlocal value = {}\n"
        );
    }

    #[test]
    fn test_doc_comment_align_alias_columns() {
        assert_format!(
            "---@alias Id integer identifier\n---@alias DisplayName string user facing name\nlocal value = nil\n",
            "---@alias Id integer         identifier\n---@alias DisplayName string user facing name\nlocal value = nil\n"
        );
    }

    #[test]
    fn test_doc_comment_alias_body_spacing_is_preserved() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            emmy_doc: crate::config::EmmyDocConfig {
                align_tag_columns: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            "---@alias Id   integer|nil identifier\n---@alias DisplayName    string user facing name\nlocal value = nil\n",
            "---@alias Id   integer|nil identifier\n---@alias DisplayName    string user facing name\nlocal value = nil\n",
            config
        );
    }

    #[test]
    fn test_doc_comment_description_spacing_can_omit_space_after_dash() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            emmy_doc: crate::config::EmmyDocConfig {
                space_after_description_dash: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            "--- keep tight\nlocal value = nil\n",
            "---keep tight\nlocal value = nil\n",
            config
        );
    }

    #[test]
    fn test_doc_comment_align_generic_columns() {
        assert_format!(
            "---@generic T value type\n---@generic Value, Result: number mapped result\nlocal function f() end\n",
            "---@generic T                     value type\n---@generic Value, Result: number mapped result\nlocal function f() end\n"
        );
    }

    #[test]
    fn test_doc_comment_format_type_and_overload() {
        assert_format!(
            "---@type   string|integer value\n---@overload   fun(x: string): integer callable\nlocal fn = nil\n",
            "---@type string|integer value\n---@overload fun(x: string): integer callable\nlocal fn = nil\n"
        );
    }

    #[test]
    fn test_doc_comment_multiline_alias_falls_back() {
        assert_format!(
            "---@alias Complex\n---| string\n---| integer\nlocal value = nil\n",
            "---@alias Complex\n---| string\n---| integer\nlocal value = nil\n"
        );
    }

    #[test]
    fn test_long_comment_preserved() {
        // Long comments should be preserved as-is (including content)
        assert_format!(
            "--[[ some content ]]\nlocal a = 1\n",
            "--[[ some content ]]\nlocal a = 1\n"
        );
    }
}
