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
        assert_format!(
            r#"local a = 1 -- trailing
"#,
            r#"local a = 1 -- trailing
"#
        );
    }

    #[test]
    fn test_normal_comment_inserts_space_after_dash_by_default() {
        assert_format!(
            r#"--comment
local a = 1
"#,
            r#"-- comment
local a = 1
"#
        );
    }

    #[test]
    fn test_normal_comment_can_keep_no_space_after_dash() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            comments: crate::config::CommentConfig {
                space_after_comment_dash: false,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"--comment
local a = 1
"#,
            r#"--comment
local a = 1
"#,
            config
        );
    }

    #[test]
    fn test_single_line_normal_comment_uses_spacing_normalization() {
        assert_format!(
            r#"--hello
"#,
            r#"-- hello
"#
        );
    }

    #[test]
    fn test_single_line_normal_comment_preserves_body_tokens() {
        assert_format!(
            r#"-- ffi.cdata* ptr # an uint8_t * pointer
"#,
            r#"-- ffi.cdata* ptr # an uint8_t * pointer
"#
        );
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
    fn test_table_field_trailing_comment_alignment() {
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
local dd = {
    aaa = 123, -- hihi
    cc = 123, -- ookko
}
"#,
            r#"
local dd = {
    aaa = 123, -- hihi
    cc = 123   -- ookko
}
"#,
            config
        );
    }

    #[test]
    fn test_table_field_trailing_comment_alignment_with_multiline_trailing_comma() {
        use crate::{
            assert_format_with_config,
            config::{LayoutConfig, LuaFormatConfig, OutputConfig, TrailingTableSeparator},
        };

        let config = LuaFormatConfig {
            layout: LayoutConfig {
                table_expand: crate::config::ExpandStrategy::Always,
                ..Default::default()
            },
            output: OutputConfig {
                trailing_table_separator: TrailingTableSeparator::Multiline,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"
local dd = {
    aaa = 123, -- hihi
    cc = 123,   -- ookko
}
"#,
            r#"
local dd = {
    aaa = 123, -- hihi
    cc = 123,  -- ookko
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
    fn test_empty_table_standalone_comment_is_preserved() {
        assert_format!(
            r#"
local t = {
    --123
}
"#,
            r#"
local t = {
    --123
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
    -- hello
    -- yyyy
end
"#
        );
    }

    #[test]
    fn test_multiline_normal_comment_keeps_line_structure_from_comment_node() {
        assert_format!(
            r#"
-- alpha
--   beta gamma
--delta
local value = 1
"#,
            r#"
-- alpha
-- beta gamma
-- delta
local value = 1
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
    fn test_call_arg_comments_stay_unaligned_without_alignment_signal() {
        assert_format!(
            r#"
foo(
    a, -- first
    long_name -- second
)
"#,
            r#"
foo(
    a, -- first
    long_name -- second
)
"#
        );
    }

    #[test]
    fn test_call_arg_comments_align_when_input_has_alignment_signal() {
        assert_format!(
            r#"
foo(
    a,  -- first
    long_name -- second
)
"#,
            r#"
foo(
    a,        -- first
    long_name -- second
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

    #[test]
    fn test_function_param_comments_stay_unaligned_without_alignment_signal() {
        assert_format!(
            r#"
function foo(
    a, -- first
    long_name -- second
)
    return a
end
"#,
            r#"
function foo(
    a, -- first
    long_name -- second
)
    return a
end
"#
        );
    }

    // ========== alignment ==========

    #[test]
    fn test_trailing_comment_alignment() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            comments: crate::config::CommentConfig {
                align_in_statements: true,
                align_across_standalone_comments: true,
                ..Default::default()
            },
            align: crate::config::AlignConfig {
                continuous_assign_statement: true,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"
local a = 1 -- short
local bbb = 2 -- long var
local cc = 3 -- medium
"#,
            r#"
local a   = 1 -- short
local bbb = 2 -- long var
local cc  = 3 -- medium
"#,
            config
        );
    }

    #[test]
    fn test_assign_alignment() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            align: crate::config::AlignConfig {
                continuous_assign_statement: true,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"
local x = 1
local yy = 2
local zzz = 3
"#,
            r#"
local x   = 1
local yy  = 2
local zzz = 3
"#,
            config
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
    long_name =  2,
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
            r#"local t = { x = 1, long_name =  2, yy = 3 }
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
    long_name =  2, -- second
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
    fn test_table_comment_alignment_uses_contiguous_subgroups() {
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
    a = "very very long",  -- first
    b =  2, -- second
    c = 3,
    d = 4,  -- third
    e = 5 -- fourth
}
"#,
            r#"
local t = {
    a = "very very long", -- first
    b = 2,                -- second
    c = 3,
    d = 4, -- third
    e = 5  -- fourth
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
            r#"local a = 1 -- trailing
"#,
            r#"local a = 1   -- trailing
"#,
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
                align_in_statements: true,
                align_across_standalone_comments: true,
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
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            comments: crate::config::CommentConfig {
                align_in_statements: true,
                align_across_standalone_comments: true,
                ..Default::default()
            },
            align: crate::config::AlignConfig {
                continuous_assign_statement: true,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
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
"#,
            config
        );
    }

    #[test]
    fn test_alignment_group_preserves_standalone_comment() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            comments: crate::config::CommentConfig {
                align_in_statements: true,
                align_across_standalone_comments: true,
                ..Default::default()
            },
            align: crate::config::AlignConfig {
                continuous_assign_statement: true,
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
local a         = 1 -- x
-- divider
local long_name = 2 -- y
"#,
            config
        );
    }

    #[test]
    fn test_alignment_group_can_break_on_standalone_comment() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            comments: crate::config::CommentConfig {
                align_in_statements: true,
                align_across_standalone_comments: false,
                ..Default::default()
            },
            align: crate::config::AlignConfig {
                continuous_assign_statement: true,
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
                align_in_statements: true,
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

    #[test]
    fn test_table_field_without_alignment_signal_stays_unaligned() {
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
    x = 1,
    long_name = 2,
    yy = 3
}
"#,
            config
        );
    }

    // ========== doc comment formatting ==========

    #[test]
    fn test_doc_comment_normalize_whitespace() {
        // Extra spaces in doc comment should be normalized to single space
        assert_format!(
            r#"---@param  name   string
local function f(name) end
"#,
            r#"---@param name string
local function f(name) end
"#
        );
    }

    #[test]
    fn test_doc_comment_preserved() {
        // Well-formatted doc comment should be unchanged
        assert_format!(
            r#"---@param name string
local function f(name) end
"#,
            r#"---@param name string
local function f(name) end
"#
        );
    }

    #[test]
    fn test_doc_long_comment_cast_preserved() {
        assert_format!(
            r#"--[[@cast -?]]
"#,
            r#"--[[@cast -?]]
"#
        );
    }

    #[test]
    fn test_doc_long_comment_multiline_preserved() {
        assert_format!(
            r#"--[[@as string
second line
]]
local value = nil
"#,
            r#"--[[@as string
second line
]]
local value = nil
"#
        );
    }

    #[test]
    fn test_doc_comment_multi_tag() {
        assert_format!(
            r#"---@param a number
---@param b string
---@return boolean
local function f(a, b) end
"#,
            r#"---@param a number
---@param b string
---@return boolean
local function f(a, b) end
"#
        );
    }

    #[test]
    fn test_doc_comment_align_param_columns() {
        assert_format!(
            r#"---@param short string desc
---@param much_longer integer longer desc
local function f(short, much_longer) end
"#,
            r#"---@param short       string  desc
---@param much_longer integer longer desc
local function f(short, much_longer) end
"#
        );
    }

    #[test]
    fn test_doc_comment_param_sync_fun_stays_single_type_column() {
        assert_format!(
            r#"---@param f sync fun(...: T...): R...
---@param g async fun(...: A...): B...
local function apply(f, g) end
"#,
            r#"---@param f sync fun(...: T...): R...
---@param g async fun(...: A...): B...
local function apply(f, g) end
"#
        );
    }

    #[test]
    fn test_doc_comment_align_param_columns_keeps_complex_type_intact() {
        assert_format!(
            r#"---@param short fun(x: string, y: number): table<string, number> desc
---@param much_longer integer longer desc
local function f(short, much_longer) end
"#,
            r#"---@param short       fun(x: string, y: number): table<string, number> desc
---@param much_longer integer                                          longer desc
local function f(short, much_longer) end
"#
        );
    }

    #[test]
    fn test_doc_comment_structured_tag_mapping_survives_unstructured_tag_between_lines() {
        assert_format!(
            r#"---@param short fun(x: string, y: number): table<string, number> desc
---@version >5.3
---@param much_longer integer longer desc
local function f(short, much_longer) end
"#,
            r#"---@param short       fun(x: string, y: number): table<string, number> desc
---@version >5.3
---@param much_longer integer                                          longer desc
local function f(short, much_longer) end
"#
        );
    }

    #[test]
    fn test_doc_comment_align_param_columns_with_interleaved_descriptions() {
        assert_format!(
            r#"--- first parameter docs
---@param short string desc
--- second parameter docs
---@param much_longer integer longer desc
local function f(short, much_longer) end
"#,
            r#"--- first parameter docs
---@param short       string  desc
--- second parameter docs
---@param much_longer integer longer desc
local function f(short, much_longer) end
"#
        );
    }

    #[test]
    fn test_doc_comment_align_param_columns_does_not_duplicate_following_lines() {
        assert_format!(
            r#"    --- @param a     any
    --- @param bbbbb string
    --- @param c     any
function f(a, bbbbb, c)
end
"#,
            r#"---@param a     any
---@param bbbbb string
---@param c     any
function f(a, bbbbb, c) end
"#
        );
    }

    #[test]
    fn test_doc_comment_align_param_columns_keeps_nullable_marker_attached_to_name() {
        assert_format!(
            r#"--- @param name     string   The name parameter
--- @param age      number   The age parameter
--- @param optional ? string Optional parameter
"#,
            r#"---@param name      string The name parameter
---@param age       number The age parameter
---@param optional? string Optional parameter
"#
        );
    }

    #[test]
    fn test_doc_comment_param_parenthesized_function_type_keeps_leading_paren() {
        assert_format!(
            r#"--- @param chunk      (fun(...: any): string)|Language<"Lua">
local function f(chunk) end
"#,
            r#"---@param chunk (fun(...: any): string)|Language<"Lua">
local function f(chunk) end
"#
        );
    }

    #[test]
    fn test_doc_comment_param_type_uses_spacing_normalization() {
        assert_format!(
            r#"--- @param value table<K, V>  |V[]|{[K]: V }
local function f(value) end
"#,
            r#"---@param value table<K, V>|V[]|{[K]: V }
local function f(value) end
"#
        );
    }

    #[test]
    fn test_doc_comment_version_keeps_space_before_comparison() {
        assert_format!(
            r#"---@version >5.3
local value = nil
"#,
            r#"---@version >5.3
local value = nil
"#
        );
    }

    #[test]
    fn test_meta_doc_line_followed_by_normal_comment_block_stays_mixed() {
        assert_format!(
            r#"---@meta
-- Copyright (c) 2018. tangzx(love.tangzx@qq.com)
--
-- Licensed under the Apache License, Version 2.0 (the "License"); you may not
-- use this file except in compliance with the License. You may obtain a copy of
-- the License at
--
-- http://www.apache.org/licenses/LICENSE-2.0
--
-- Unless required by applicable law or agreed to in writing, software
-- distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
-- WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
-- License for the specific language governing permissions and limitations under
-- the License.

local value = nil
"#,
            r#"---@meta
-- Copyright (c) 2018. tangzx(love.tangzx@qq.com)
--
-- Licensed under the Apache License, Version 2.0 (the "License"); you may not
-- use this file except in compliance with the License. You may obtain a copy of
-- the License at
--
-- http://www.apache.org/licenses/LICENSE-2.0
--
-- Unless required by applicable law or agreed to in writing, software
-- distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
-- WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
-- License for the specific language governing permissions and limitations under
-- the License.

local value = nil
"#
        );
    }

    #[test]
    fn test_pure_doc_meta_line_does_not_panic() {
        assert_format!(
            r#"---@meta
"#,
            r#"---@meta
"#
        );
    }

    #[test]
    fn test_doc_comment_align_field_columns() {
        assert_format!(
            r#"---@field x string desc
---@field longer_name integer another desc
local t = {}
"#,
            r#"---@field x           string  desc
---@field longer_name integer another desc
local t = {}
"#
        );
    }

    #[test]
    fn test_doc_comment_align_field_columns_with_spaced_tag_prefix() {
        assert_format!(
            r#"--- @class Position1
--- @field x integer
--- @field yafafa integer
"#,
            r#"---@class Position1
---@field x      integer
---@field yafafa integer
"#
        );
    }

    #[test]
    fn test_doc_comment_align_field_columns_with_interleaved_descriptions() {
        assert_format!(
            r#"---@class schema.EmmyrcStrict
--- Whether to enable strict mode array indexing.
---@field arrayIndex boolean?
--- Base constant types defined in doc can match base types, allowing int to match `---@alias id 1|2|3`, same for string.
---@field docBaseConstMatchBaseType boolean?
--- meta define overrides file define
---@field metaOverrideFileDefine boolean?
"#,
            r#"---@class schema.EmmyrcStrict
--- Whether to enable strict mode array indexing.
---@field arrayIndex                boolean?
--- Base constant types defined in doc can match base types, allowing int to match `---@alias id 1|2|3`, same for string.
---@field docBaseConstMatchBaseType boolean?
--- meta define overrides file define
---@field metaOverrideFileDefine    boolean?
"#
        );
    }

    #[test]
    fn test_doc_comment_align_return_columns() {
        assert_format!(
            r#"---@return number ok success
---@return string, integer err failure
function f() end
"#,
            r#"---@return number ok           success
---@return string, integer err failure
function f() end
"#
        );
    }

    #[test]
    fn test_doc_comment_align_return_columns_with_interleaved_descriptions() {
        assert_format!(
            r#"--- first return docs
---@return number ok success
--- second return docs
---@return string, integer err failure
function f() end
"#,
            r#"--- first return docs
---@return number ok           success
--- second return docs
---@return string, integer err failure
function f() end
"#
        );
    }

    #[test]
    fn test_doc_comment_empty_return_keeps_following_continue_or_lines_separate() {
        assert_format!(
            r#"--- @param co thread
--- @return
--- | "running" # Is running.
--- | "suspended" # Is suspended or not started.
local function status(co) end
"#,
            r#"---@param co thread
---@return
--- | "running" # Is running.
--- | "suspended" # Is suspended or not started.
local function status(co) end
"#
        );
    }

    #[test]
    fn test_doc_comment_return_hash_description_preserves_body_text() {
        assert_format!(
            r#"---@return ffi.cdata* ptr # an uint8_t * FFI cdata pointer that points to the buffer data.
---@return integer len # length of the buffer data in bytes
local function f()
end
"#,
            r#"---@return ffi.cdata* ptr # an uint8_t * FFI cdata pointer that points to the buffer data.
---@return integer len    # length of the buffer data in bytes
local function f() end
"#
        );
    }

    #[test]
    fn test_doc_comment_param_hash_description_preserves_body_text() {
        assert_format!(
            r#"---@param f sync fun(...: T...): R... # async and sync should stay near the fun type body
local function apply(f) end
"#,
            r#"---@param f sync fun(...: T...): R... # async and sync should stay near the fun type body
local function apply(f) end
"#
        );
    }

    #[test]
    fn test_doc_comment_align_complex_field_columns() {
        assert_format!(
            r#"---@field public ["foo"] string?
---@field private [bar] integer
---@field protected baz fun(x: string): boolean
local t = {}
"#,
            r#"---@field public ["foo"] string?
---@field private [bar]  integer
---@field protected baz  fun(x: string): boolean
local t = {}
"#
        );
    }

    #[test]
    fn test_doc_comment_field_function_type_spacing_stays_inside_type_column() {
        assert_format!(
            r#"--- @class std.metatable
--- @field __mode?      'v'|'k'|'kv'
--- @field __metatable? any
--- @field __tostring?  (fun(t): string)
--- @field __gc?        fun(t)
--- @field __add?       fun(t1,         t2): any
--- @field __sub?       fun(t1,         t2): any
"#,
            r#"---@class std.metatable
---@field __mode?      'v'|'k'|'kv'
---@field __metatable? any
---@field __tostring?  (fun(t): string)
---@field __gc?        fun(t)
---@field __add?       fun(t1, t2): any
---@field __sub?       fun(t1, t2): any
"#
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
            r#"---@param short string desc
---@param much_longer integer longer desc
local function f(short, much_longer) end
"#,
            r#"---@param short string desc
---@param much_longer integer longer desc
local function f(short, much_longer) end
"#,
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
            r#"---@class Short short desc
---@class LongerName<T> longer desc
local value = {}
"#,
            r#"---@class Short short desc
---@class LongerName<T> longer desc
local value = {}
"#,
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
            r#"---@param short string desc
---@param much_longer integer longer desc
local function f(short, much_longer) end
"#,
            r#"---@param short string desc
---@param much_longer integer longer desc
local function f(short, much_longer) end
"#,
            config
        );
    }

    #[test]
    fn test_doc_comment_align_class_columns() {
        assert_format!(
            r#"---@class Short short desc
---@class LongerName<T> longer desc
local value = {}
"#,
            r#"---@class Short         short desc
---@class LongerName<T> longer desc
local value = {}
"#
        );
    }

    #[test]
    fn test_doc_comment_align_class_columns_keeps_complex_generic_head_intact() {
        assert_format!(
            r#"---@class ExtremelyLongSimpleName short desc
---@class H<T, Result: fun(x: string, y: number): table<string, number>> handler desc
local value = {}
"#,
            r#"---@class ExtremelyLongSimpleName                                        short desc
---@class H<T, Result: fun(x: string, y: number): table<string, number>> handler desc
local value = {}
"#
        );
    }

    #[test]
    fn test_doc_comment_enum_attached_table_prefers_expanded_declaration() {
        assert_format!(
            r#"---@enum MyEnum
local cc = { xxx = 123 }
"#,
            r#"---@enum MyEnum
local cc = {
    xxx = 123
}
"#
        );
    }

    #[test]
    fn test_doc_comment_class_attached_table_prefers_expanded_declaration() {
        assert_format!(
            r#"---@class MyClass
local cc = { xxx = 123 }
"#,
            r#"---@class MyClass
local cc = {
    xxx = 123
}
"#
        );
    }

    #[test]
    fn test_doc_comment_align_alias_columns() {
        assert_format!(
            r#"---@alias Id integer identifier
---@alias DisplayName string user facing name
local value = nil
"#,
            r#"---@alias Id integer         identifier
---@alias DisplayName string user facing name
local value = nil
"#
        );
    }

    #[test]
    fn test_doc_comment_align_alias_columns_keeps_function_type_intact() {
        assert_format!(
            r#"---@alias ExtremelyLongAliasName string description
---@alias H fun(x: string, y: number): table<string, number> handler desc
local value = nil
"#,
            r#"---@alias ExtremelyLongAliasName string                      description
---@alias H fun(x: string, y: number): table<string, number> handler desc
local value = nil
"#
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
            r#"---@alias Id   integer|nil identifier
---@alias DisplayName    string user facing name
local value = nil
"#,
            r#"---@alias Id   integer|nil identifier
---@alias DisplayName    string user facing name
local value = nil
"#,
            config
        );
    }

    #[test]
    fn test_doc_comment_description_spacing_can_omit_space_after_dash() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            emmy_doc: crate::config::EmmyDocConfig {
                space_between_tag_columns: false,
                space_after_description_dash: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"--- keep tight
local value = nil
"#,
            r#"---keep tight
local value = nil
"#,
            config
        );
    }

    #[test]
    fn test_doc_tag_prefix_can_omit_space_before_at() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            emmy_doc: crate::config::EmmyDocConfig {
                space_between_tag_columns: false,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"--- @param  name   string
local function f(name) end
"#,
            r#"---@param name string
local function f(name) end
"#,
            config
        );
    }

    #[test]
    fn test_doc_tag_prefix_is_independent_from_description_spacing() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            emmy_doc: crate::config::EmmyDocConfig {
                space_between_tag_columns: false,
                space_after_description_dash: true,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"---@enum MyEnum
local cc = { xxx = 123 }
"#,
            r#"---@enum MyEnum
local cc = {
    xxx = 123
}
"#,
            config
        );
    }

    #[test]
    fn test_doc_continue_or_prefix_can_omit_space() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            emmy_doc: crate::config::EmmyDocConfig {
                space_between_tag_columns: false,
                space_after_description_dash: false,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"--- @alias Complex
--- | string
--- | integer
local value = nil
"#,
            r#"---@alias Complex
---| string
---| integer
local value = nil
"#,
            config
        );
    }

    #[test]
    fn test_doc_continue_or_variants_are_detected_from_tokens() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            emmy_doc: crate::config::EmmyDocConfig {
                space_between_tag_columns: false,
                space_after_description_dash: false,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"--- @alias Complex
--- |+ string
--- |> integer
local value = nil
"#,
            r#"---@alias Complex
---|+ string
---|> integer
local value = nil
"#,
            config
        );
    }

    #[test]
    fn test_doc_comment_single_line_description_preserves_body_spacing() {
        assert_format!(
            r#"---   spaced    words
local value = nil
"#,
            r#"---   spaced    words
local value = nil
"#
        );
    }

    #[test]
    fn test_doc_comment_multiline_description_without_tags_uses_token_prefixes() {
        assert_format!(
            r#"--- first line
---   second   line
local value = nil
"#,
            r#"--- first line
---   second   line
local value = nil
"#
        );
    }

    #[test]
    fn test_doc_tag_prefix_omits_space_before_at_by_default() {
        assert_format!(
            r#"---@param  name   string
local function f(name) end
"#,
            r#"---@param name string
local function f(name) end
"#
        );
    }

    #[test]
    fn test_doc_description_spacing_is_independent_from_tag_prefix_spacing() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            emmy_doc: crate::config::EmmyDocConfig {
                space_between_tag_columns: true,
                space_after_description_dash: false,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"---@enum MyEnum
--- keep tight
local value = nil
"#,
            r#"--- @enum MyEnum
---keep tight
local value = nil
"#,
            config
        );
    }

    #[test]
    fn test_doc_comment_multiline_description_preserves_line_structure() {
        assert_format!(
            r#"---@class Test first line
---   second   line
local value = {}
"#,
            r#"---@class Test first line
---   second   line
local value = {}
"#
        );
    }

    #[test]
    fn test_doc_comment_multiline_description_only_preserves_explicit_indentation() {
        assert_format!(
            r#"--- hihi
---   jgiwigw
---  jgiwigw
---fjajwiofw
local value = nil
"#,
            r#"--- hihi
---   jgiwigw
---  jgiwigw
--- fjajwiofw
local value = nil
"#
        );
    }

    #[test]
    fn test_doc_comment_field_range_description_preserves_commas() {
        assert_format!(
            r#"---@class std.osdateparam
---@field year                  integer|string four digits
---@field month                 integer|string 1-12
---@field day                   integer|string 1-31
---@field hour(integer|string)? 0-23
---@field min(integer|string)?  0-59
---@field sec(integer|string)?  0-61,due to leap seconds
---@field wday(integer|string)? 1-7,Sunday is 1
---@field yday(integer|string)? 1-366
---@field isdst                 boolean? daylight saving flag, a boolean.
local t = {}
"#,
            r#"---@class std.osdateparam
---@field year  integer|string    four digits
---@field month integer|string    1-12
---@field day   integer|string    1-31
---@field hour  (integer|string)? 0-23
---@field min   (integer|string)? 0-59
---@field sec   (integer|string)? 0-61,due to leap seconds
---@field wday  (integer|string)? 1-7,Sunday is 1
---@field yday  (integer|string)? 1-366
---@field isdst boolean?          daylight saving flag, a boolean.
local t = {}
"#
        );
    }

    #[test]
    fn test_doc_comment_align_generic_columns() {
        assert_format!(
            r#"---@generic T value type
---@generic Value, Result: number mapped result
local function f() end
"#,
            r#"---@generic T                     value type
---@generic Value, Result: number mapped result
local function f() end
"#
        );
    }

    #[test]
    fn test_doc_comment_format_type_and_overload() {
        assert_format!(
            r#"---@type   string|integer value
---@overload   fun(x: string): integer callable
local fn = nil
"#,
            r#"---@type string|integer value
---@overload fun(x: string): integer callable
local fn = nil
"#
        );
    }

    #[test]
    fn test_doc_comment_type_normalizes_generic_spacing() {
        assert_format!(
            r#"--- @type table < number, Person >
local d = {}
"#,
            r#"---@type table<number, Person>
local d = {}
"#
        );
    }

    #[test]
    fn test_doc_comment_type_normalizes_group_and_array_spacing() {
        assert_format!(
            r#"--- @type ( string|number)[]
local c
"#,
            r#"---@type (string|number)[]
local c
"#
        );
    }

    #[test]
    fn test_doc_comment_type_normalizes_object_index_field_spacing() {
        assert_format!(
            r#"--- @type {[string]: number,[number]: string }
local x
"#,
            r#"---@type { [string]: number, [number]: string }
local x
"#
        );
    }

    #[test]
    fn test_doc_comment_generic_uses_spacing_normalization() {
        assert_format!(
            r#"--- @generic Value , Result : number mapped result
local function f() end
"#,
            r#"---@generic Value, Result: number mapped result
local function f() end
"#
        );
    }

    #[test]
    fn test_doc_comment_generic_hash_string_literal_is_not_treated_as_description() {
        assert_format!(
            r#"--- @generic T, Num: integer|'#'
local function f() end
"#,
            r#"---@generic T, Num: integer|'#'
local function f() end
"#
        );
    }

    #[test]
    fn test_doc_comment_type_uses_spacing_normalization_for_function_types() {
        assert_format!(
            r#"--- @type fun( x : string ) : integer
local fn
"#,
            r#"---@type fun(x: string): integer
local fn
"#
        );
    }

    #[test]
    fn test_doc_type_with_inline_comment_marker_is_preserved_raw() {
        assert_format!(
            r#"---@type string --1
local s
"#,
            r#"---@type string --1
local s
"#
        );
    }

    #[test]
    fn test_nonstandard_dash_comment_is_preserved_raw() {
        assert_format!(
            r#"----    keep odd prefix
local value = nil
"#,
            r#"----    keep odd prefix
local value = nil
"#
        );
    }

    #[test]
    fn test_doc_comment_multiline_alias_falls_back() {
        assert_format!(
            r#"---@alias Complex
---| string
---| integer
local value = nil
"#,
            r#"---@alias Complex
--- | string
--- | integer
local value = nil
"#
        );
    }

    #[test]
    fn test_doc_comment_align_multiline_alias_descriptions() {
        assert_format!(
            r#"---@alias schema.DiagnosticCode
---| "none"
---| "syntax-error" # Syntax error
---| "doc-syntax-error" # Doc syntax error
---| "type-not-found" # Type not found
---| "missing-return" # Missing return statement
---| "param-type-mismatch" # Param Type not match
"#,
            r#"---@alias schema.DiagnosticCode
--- | "none"
--- | "syntax-error"        # Syntax error
--- | "doc-syntax-error"    # Doc syntax error
--- | "type-not-found"      # Type not found
--- | "missing-return"      # Missing return statement
--- | "param-type-mismatch" # Param Type not match
"#
        );
    }

    #[test]
    fn test_doc_comment_alias_continue_or_does_not_duplicate_marker() {
        assert_format!(
            r#"---@alias std.collectgarbage_opt
---|>"collect" # performs a full garbage-collection cycle. This is the default option.
"#,
            r#"---@alias std.collectgarbage_opt
--- |> "collect" # performs a full garbage-collection cycle. This is the default option.
"#
        );
    }

    #[test]
    fn test_doc_comment_multiline_alias_description_alignment_can_be_disabled() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            emmy_doc: crate::config::EmmyDocConfig {
                align_multiline_alias_descriptions: false,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_format_with_config!(
            r#"---@alias schema.DiagnosticCode
---| "syntax-error" # Syntax error
---| "doc-syntax-error" # Doc syntax error
"#,
            r#"---@alias schema.DiagnosticCode
--- | "syntax-error" # Syntax error
--- | "doc-syntax-error" # Doc syntax error
"#,
            config
        );
    }

    #[test]
    fn test_long_comment_preserved() {
        // Long comments should be preserved as-is (including content)
        assert_format!(
            r#"--[[ some content ]]
local a = 1
"#,
            r#"--[[ some content ]]
local a = 1
"#
        );
    }
}
