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
            config::{ExpandStrategy, LuaFormatConfig},
        };

        let config = LuaFormatConfig {
            table_expand: ExpandStrategy::Always,
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
            config::{ExpandStrategy, LuaFormatConfig},
        };

        let config = LuaFormatConfig {
            table_expand: ExpandStrategy::Always,
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
    fn test_alignment_disabled() {
        use crate::{assert_format_with_config, config::LuaFormatConfig};

        let config = LuaFormatConfig {
            align_continuous_line_comment: false,
            align_continuous_assign_statement: false,
            align_table_field: false,
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
}
