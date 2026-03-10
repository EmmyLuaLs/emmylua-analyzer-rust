#[cfg(test)]
mod tests {
    use crate::{assert_format, config::LuaFormatConfig};

    // ========== shebang ==========

    #[test]
    fn test_shebang_preserved() {
        assert_format!(
            "#!/usr/bin/lua\nlocal a=1\n",
            "#!/usr/bin/lua\nlocal a = 1\n"
        );
    }

    #[test]
    fn test_shebang_env() {
        assert_format!(
            "#!/usr/bin/env lua\nprint(1)\n",
            "#!/usr/bin/env lua\nprint(1)\n"
        );
    }

    #[test]
    fn test_shebang_with_code() {
        assert_format!(
            "#!/usr/bin/lua\nlocal x=1\nlocal y=2\n",
            "#!/usr/bin/lua\nlocal x = 1\nlocal y = 2\n"
        );
    }

    #[test]
    fn test_no_shebang() {
        // Ensure normal code without shebang still works
        assert_format!("local a = 1\n", "local a = 1\n");
    }

    // ========== long string preservation ==========

    #[test]
    fn test_long_string_preserves_trailing_spaces() {
        // Long string content including trailing spaces must be preserved exactly
        assert_format!(
            "local s = [[  hello   \n  world   \n]]\n",
            "local s = [[  hello   \n  world   \n]]\n"
        );
    }

    // ========== idempotency ==========

    #[test]
    fn test_idempotency_basic() {
        let config = LuaFormatConfig::default();
        let input = r#"
local a   =   1
local bbb   =   2
if true
then
return   a  +  bbb
end
"#
        .trim_start_matches('\n');

        let first = crate::reformat_lua_code(input, &config);
        let second = crate::reformat_lua_code(&first, &config);
        assert_eq!(
            first, second,
            "Formatter is not idempotent!\nFirst pass:\n{first}\nSecond pass:\n{second}"
        );
    }

    #[test]
    fn test_idempotency_table() {
        let config = LuaFormatConfig::default();
        let input = r#"
local t = {
    a = 1,
    bbb = 2,
    cc = 3,
}
"#
        .trim_start_matches('\n');

        let first = crate::reformat_lua_code(input, &config);
        let second = crate::reformat_lua_code(&first, &config);
        assert_eq!(
            first, second,
            "Formatter is not idempotent for tables!\nFirst pass:\n{first}\nSecond pass:\n{second}"
        );
    }

    #[test]
    fn test_idempotency_complex() {
        let config = LuaFormatConfig::default();
        let input = r#"
local function foo(a, b, c)
    local x = a + b * c
    if x > 10 then
        return {
            result = x,
            name = "test",
            flag = true,
        }
    end

    for i = 1, 10 do
        print(i)
    end

    local t = { 1, 2, 3 }
    return t
end
"#
        .trim_start_matches('\n');

        let first = crate::reformat_lua_code(input, &config);
        let second = crate::reformat_lua_code(&first, &config);
        assert_eq!(
            first, second,
            "Formatter is not idempotent for complex code!\nFirst pass:\n{first}\nSecond pass:\n{second}"
        );
    }

    #[test]
    fn test_idempotency_alignment() {
        let config = LuaFormatConfig::default();
        let input = r#"
local a = 1 -- comment a
local bbb = 2 -- comment b
local cc = 3 -- comment c
"#
        .trim_start_matches('\n');

        let first = crate::reformat_lua_code(input, &config);
        let second = crate::reformat_lua_code(&first, &config);
        assert_eq!(
            first, second,
            "Formatter is not idempotent for aligned code!\nFirst pass:\n{first}\nSecond pass:\n{second}"
        );
    }

    #[test]
    fn test_idempotency_method_chain() {
        let config = LuaFormatConfig {
            max_line_width: 40,
            ..Default::default()
        };
        let input = "local x = obj:method1():method2():method3()\n";

        let first = crate::reformat_lua_code(input, &config);
        let second = crate::reformat_lua_code(&first, &config);
        assert_eq!(
            first, second,
            "Formatter is not idempotent for method chains!\nFirst pass:\n{first}\nSecond pass:\n{second}"
        );
    }

    #[test]
    fn test_idempotency_shebang() {
        let config = LuaFormatConfig::default();
        let input = "#!/usr/bin/lua\nlocal a   =   1\n";

        let first = crate::reformat_lua_code(input, &config);
        let second = crate::reformat_lua_code(&first, &config);
        assert_eq!(
            first, second,
            "Formatter is not idempotent with shebang!\nFirst pass:\n{first}\nSecond pass:\n{second}"
        );
    }
}
