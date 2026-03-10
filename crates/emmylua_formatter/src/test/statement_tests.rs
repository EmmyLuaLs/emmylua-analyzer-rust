#[cfg(test)]
mod tests {
    // ========== if statement ==========

    use crate::assert_format;

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
