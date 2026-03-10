#[cfg(test)]
mod tests {
    // ========== unary / binary / concat ==========

    use crate::assert_format;

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

    // ========== table ==========

    #[test]
    fn test_table_expr() {
        assert_format!(
            "local t = { a = 1, b = 2, c = 3 }\n",
            "local t = { a = 1, b = 2, c = 3 }\n"
        );
    }

    #[test]
    fn test_empty_table() {
        assert_format!("local t = {}\n", "local t = {}\n");
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

    // ========== and / or expression ==========

    #[test]
    fn test_and_or_expr() {
        assert_format!(
            "local x = condition_one and value_one or condition_two and value_two or default_value\n",
            "local x = condition_one and value_one or condition_two and value_two or default_value\n"
        );
    }
}
