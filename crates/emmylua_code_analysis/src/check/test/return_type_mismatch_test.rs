#[cfg(test)]
mod tests {
    use crate::check::test::{check_source, count_by_code};
    use crate::{DiagnosticCode, LuaType, VirtualWorkspace};
    use emmylua_parser::{LuaAstNode, LuaIndexExpr};

    // 数组长度作为数值循环初值时, 循环变量应保持整数.
    #[test]
    fn test_numeric_for_array_length_bound() {
        for value_type in ["function[]", "function[]?"] {
            let mut ws = VirtualWorkspace::new();
            let expected = ws.ty(value_type);
            let file = ws.def(&format!(
                r#"
                ---@class EventBus
                ---@field listeners table<string, {value_type}>
                local M = {{}}
                ---@param event string
                function M:off(event)
                    local list = self.listeners[event]
                    if not list then return end
                    for i = #list, 1, -1 do
                        local value = i
                    end
                end
                "#,
            ));
            let model = ws.analysis.semantic_model(file);
            let index = model
                .chunk()
                .unwrap()
                .descendants::<LuaIndexExpr>()
                .find(|index| index.syntax().text() == "self.listeners[event]")
                .unwrap();
            assert_eq!(
                model.type_of_expr_at(index.get_syntax_id(), index.get_position()),
                expected,
                "{value_type}"
            );
            let decl = model.file_facts().unwrap().decl_named("i").unwrap();
            assert_eq!(model.type_of_decl(&decl.id), Some(LuaType::Integer));
        }
    }

    // 数值 for 变量必须得到具体数值类型, 初值和步长决定是否为整数循环.
    #[test]
    fn test_numeric_for_variable_types() {
        for (bounds, expected) in [
            ("1, 10", LuaType::Integer),
            ("1, 10, 2", LuaType::Integer),
            ("1, 10.5", LuaType::Integer),
            ("0.5, 10", LuaType::Number),
            ("1, 10, 0.5", LuaType::Number),
            ("start, 10, step", LuaType::Integer),
        ] {
            let mut ws = VirtualWorkspace::new();
            let setup = if bounds.starts_with("start") {
                r#"
                ---@type integer
                local start = 1
                ---@type integer
                local step = 2
                "#
            } else {
                ""
            };
            let file = ws.def(&format!(
                r#"
                {setup}
                for i = {bounds} do
                    local value = i
                end
                "#,
            ));
            let model = ws.analysis.semantic_model(file);
            let decl = model.file_facts().unwrap().decl_named("i").unwrap();
            assert_eq!(model.type_of_decl(&decl.id), Some(expected), "{bounds}");
        }
    }

    // 整数循环中的算术表达式应保持整数, 加入小数后仍需报告返回类型错误.
    #[test]
    fn test_numeric_for_arithmetic_return_type() {
        for (body, expected) in [
            ("return (i + 1)", 0),
            (
                r#"local next = (i + 1)
                return next"#,
                0,
            ),
            ("return ((i + 1) % 3)", 0),
            ("return (i + 0.5)", 1),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@return integer
                local function next_id()
                    for i = 1, 10 do
                        {body}
                    end
                    return 0
                end
                return next_id
                "#,
            ));
            assert_eq!(
                diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.code == DiagnosticCode::ReturnTypeMismatch)
                    .count(),
                expected,
                "{body}: {diagnostics:#?}"
            );
        }
    }

    #[test]
    fn test_field_flow_does_not_change_the_returned_table_type() {
        let diagnostics = check_source(
            r#"
            ---@return {text: string}
            local function record()
                local value = {text = 'ok'}
                value.text = 'updated'
                return value
            end
            return record
        "#,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    }

    #[test]
    fn test_parenthesized_member_guards_narrow_return_value() {
        for (guard, access) in [
            ("(box).value", "box.value"),
            ("box.value", "(box).value"),
            ("((box)).value", "(box).value"),
        ] {
            let mut ws = VirtualWorkspace::new();
            let source = format!(
                r#"
                ---@class Box
                ---@field value string?
                ---@param box Box
                ---@return string
                local function read(box)
                    if {guard} then return {access} end
                    return ''
                end
                "#
            );
            assert!(
                ws.has_no_diagnostic(DiagnosticCode::ReturnTypeMismatch, &source),
                "{source}"
            );
        }
    }

    #[test]
    fn test_issue_1226() {
        let diagnostics = check_source(
            r#"
            ---@return boolean
            local function test1()
                return true ---@as true | false
            end

            ---@return true | false
            local function test2()
                return true ---@as boolean
            end
            "#,
        );
        assert!(
            diagnostics
                .iter()
                .all(|d| d.code != DiagnosticCode::ReturnTypeMismatch),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn test_inline_as_return_compatibility() {
        for (target, expression, cast, accepted) in [
            ("boolean", "true", "true|false", true),
            ("true", "true", "boolean", false),
            ("false", "false", "boolean", false),
            ("true", "true", "false", false),
            ("false", "false", "true", false),
            ("string", "1", "string", true),
            ("number", "'text'", "number", true),
            ("nil", "true", "nil", true),
            ("string", "'text'", "number", false),
            ("number", "1", "string", false),
            ("boolean", "true", "nil", false),
        ] {
            let mut ws = VirtualWorkspace::new();
            assert_eq!(
                ws.has_no_diagnostic(
                    DiagnosticCode::ReturnTypeMismatch,
                    &format!(
                        r#"
                        ---@return {target}
                        local function test()
                            return {expression} ---@as {cast}
                        end
                        "#,
                    ),
                ),
                accepted,
                "{expression} as {cast} -> {target}"
            );
        }
    }

    #[test]
    fn test_and_or_function_guard_needs_a_truthy_result() {
        for (result_type, accepted) in [("string", true), ("string?", false)] {
            let source = format!(
                r#"
                ---@param value string | fun(): {result_type}
                ---@return string
                local function text(value)
                    return type(value) == 'function' and value() or value
                end
                return text
            "#
            );
            let diagnostics = check_source(&source);
            let actual = diagnostics
                .iter()
                .filter(|d| d.code == DiagnosticCode::ReturnTypeMismatch)
                .map(|d| usize::from(d.range.start()))
                .collect::<Vec<_>>();
            let expected = if accepted {
                vec![]
            } else {
                vec![source.find("type(value)").unwrap()]
            };
            assert_eq!(actual, expected, "{diagnostics:#?}");
        }
    }

    #[test]
    fn test_issue_242() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                ---@class A
                local A = {}
                A.__index = A

                function A:method() end

                ---@return A
                function new()
                    local a = setmetatable({}, A);
                    return a
                end
        "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                local setmetatable = setmetatable
                ---@class A
                local A = {}

                function A:method() end

                ---@return A
                function new()
                    return setmetatable({}, { __index = A })
                end
        "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                ---@class A
                local A = {}
                A.__index = A

                function A:method() end

                ---@return A
                function new()
                return setmetatable({}, A)
                end
        "#
        ));
    }

    #[test]
    fn test_issue_220() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            --- @class A

            --- @return A?, integer?
            function bar()
            end

            --- @return A?, integer?
            function foo()
            return bar()
            end
        "#
        ));
    }

    #[test]
    fn test_return_type_mismatch() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@class diagnostic.Test1
            local Test = {}

            ---@return number
            function Test.n()
                return "1"
            end
        "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@return string
            local test = function()
                return 1
            end
        "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@class diagnostic.Test2
            local Test = {}

            ---@return number
            Test.n = function ()
                return "1"
            end
        "#
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@return number
            local function test3()
                if true then
                    return ""
                else
                    return 2, 3
                end
                return 2, 3
            end
        "#
        ));
    }

    #[test]
    fn test_type_mismatches_only_for_declared_return_slots() {
        let source = r#"
            ---@class Reader
            ---@field read fun(): string, integer

            ---@type Reader
            local valid = {
                read = function()
                    return 'hello', 1
                end,
            }

            ---@type Reader
            local invalid = {
                read = function()
                    return 'hello', 'oops'
                end,
            }

            ---@return string
            local function extra()
                return 'hello', 1
            end

            local overloads = {}
            ---@overload fun(flag: true): string
            ---@overload fun(flag: false): integer, integer
            function overloads.read(flag)
                if flag then return 'hello' end
                return 1, 2
            end

            return valid, invalid, extra, overloads
        "#;
        let diagnostics = check_source(source);
        let mut returns: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| {
                matches!(
                    diagnostic.code,
                    DiagnosticCode::ReturnTypeMismatch | DiagnosticCode::RedundantReturnValue
                )
            })
            .collect();
        returns.sort_by_key(|diagnostic| diagnostic.range.start());
        let actual: Vec<_> = returns
            .iter()
            .map(|diagnostic| {
                let range = diagnostic.range;
                let text = &source[usize::from(range.start())..usize::from(range.end())];
                (diagnostic.code, text)
            })
            .collect();

        assert_eq!(
            actual,
            [
                (DiagnosticCode::ReturnTypeMismatch, "'oops'"),
                (DiagnosticCode::RedundantReturnValue, "1"),
            ],
            "{diagnostics:#?}"
        );
    }

    #[googletest::gtest]
    fn test_declared_return_slots() {
        use googletest::prelude::*;

        for (annotation, values, expected) in [
            ("string, integer", "1, 1", 1),
            ("string, integer", "'hello', 'oops'", 1),
            ("string, integer", "pair()", 0),
            ("string, string", "pair()", 1),
            ("string, integer", "pair(), 1", 0),
            ("string, string", "pair(), 'hello'", 0),
            ("boolean, string, integer", "true, pair()", 0),
            ("boolean, string, string", "true, pair()", 1),
            ("string, integer...", "pair()", 0),
            ("string, string...", "pair()", 1),
            ("string, string...", "'hello', ...", 1),
            ("[string, integer]...", "'hello', 1", 0),
            ("[string, integer]...", "'hello', 'oops'", 1),
            ("[string, integer]...", "pair()", 0),
            ("Pair", "'hello', 1", 0),
            ("Pair", "'hello', 'oops'", 1),
            ("boolean, Pair", "true, pair()", 0),
            ("boolean, Pair", "true, 'hello', 'oops'", 1),
            ("string, integer", "aliased_pair()", 0),
            ("string, string", "aliased_pair()", 1),
            ("string, string", "aliased_pair(), 'ok'", 0),
            ("boolean, string, string", "prefixed_alias()", 1),
            ("string, string", "either_pair(flag)", 1),
            ("boolean, string, integer", "prefixed_callback()", 0),
            ("boolean, string, string", "prefixed_callback()", 1),
            (
                "string, string...",
                "wrap(function() return 'a', 'b', 1 end)",
                1,
            ),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@alias Pair [string, integer]...

                ---@return string, integer
                local function pair()
                    return 'hello', 1
                end

                ---@return Pair
                local function aliased_pair()
                    return 'hello', 1
                end

                local function prefixed_alias()
                    return true, aliased_pair()
                end

                local function either_pair(flag)
                    if flag then
                        return aliased_pair()
                    end
                    return 'ok', 'also ok'
                end

                ---@generic R
                ---@param fn fun(): R...
                ---@return R...
                local function wrap(fn) end

                local function prefixed_callback()
                    return true, wrap(function() return 'hello', 1 end)
                end

                ---@param flag boolean
                ---@param ... integer
                ---@return {annotation}
                local function check(flag, ...)
                    return {values}
                end
                "#,
            ));

            expect_that!(
                count_by_code(&diagnostics, DiagnosticCode::ReturnTypeMismatch),
                eq(expected),
                "---@return {annotation} with return {values}"
            );
        }
    }

    #[test]
    fn test_forwarded_generic_result_slots_across_files() {
        use std::sync::Arc;

        use crate::check::{CheckConfig, check_file};

        for (wrapper, valid, invalid, actual) in [
            (
                r#"
                ---@generic R
                ---@param callback fun(): R...
                ---@return R...
                local function wrap(callback)
                    return callback()
                end

                function M.prefix()
                    return true, wrap(function() return 'hello', 1 end)
                end
                "#,
                "string, integer",
                "string, string",
                "integer",
            ),
            (
                r#"
                ---@generic T
                ---@param value T
                ---@return T[]
                local function wrap(value)
                    return { value }
                end

                function M.prefix()
                    return true, wrap('hello')
                end
                "#,
                "string[]",
                "integer[]",
                "string[]",
            ),
        ] {
            let mut ws = VirtualWorkspace::new();
            ws.def_file(
                "source.lua",
                &format!("local M = {{}}\n{wrapper}\nreturn M"),
            );
            let source = format!(
                r#"
                local source = require 'source'

                ---@return boolean, {valid}
                local function valid()
                    return source.prefix()
                end

                ---@return boolean, {invalid}
                local function invalid()
                    return source.prefix()
                end
                "#,
            );
            let file = ws.def_file("consumer.lua", &source);
            let model = ws.analysis.semantic_model(file);
            let config = Arc::new(CheckConfig::new(&ws.analysis.get_emmyrc()));
            let diagnostics = check_file(&model, config);
            let mismatches: Vec<_> = diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == DiagnosticCode::ReturnTypeMismatch)
                .collect();

            let call = "source.prefix()";
            let start = rowan::TextSize::try_from(source.rfind(call).unwrap()).unwrap();
            assert_eq!(mismatches.len(), 1, "{diagnostics:#?}");
            assert_eq!(
                mismatches[0].range,
                rowan::TextRange::at(start, rowan::TextSize::of(call))
            );
            assert!(mismatches[0].message.contains(&format!("found `{actual}`")));
        }
    }

    #[test]
    fn test_forwarded_variadic_return_tail_may_be_empty() {
        for (tail, expected) in [("", 0), (", nil", 1)] {
            let diagnostics = check_source(&format!(
                r#"
                local function source(ok)
                    if ok then
                        return true, 'hello'
                    end
                    return false{tail}
                end

                ---@param ok boolean
                ---@return boolean, string...
                local function forward(ok)
                    return source(ok)
                end
                "#,
            ));

            assert_eq!(
                count_by_code(&diagnostics, DiagnosticCode::ReturnTypeMismatch),
                expected,
                "return false{tail}: {diagnostics:#?}"
            );
        }
    }

    #[test]
    fn test_forwarded_variadic_return_checks_nested_alternatives() {
        let diagnostics = check_source(
            r#"
            local function source(ok)
                if ok then
                    return 'a', 'b', 'c', 1
                end
                return 'd', 'e', 'f', 'g'
            end

            local function prefix(ok)
                return true, source(ok)
            end

            ---@param ok boolean
            ---@return boolean, string...
            local function forward(ok)
                return prefix(ok)
            end
            "#,
        );

        assert_eq!(
            count_by_code(&diagnostics, DiagnosticCode::ReturnTypeMismatch),
            1,
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn test_forwarded_fixed_return_slots_check_callback_alternatives() {
        for (result, expected) in [("1", 0), ("'wrong'", 1)] {
            let diagnostics = check_source(&format!(
                r#"
                ---@generic R
                ---@param callback fun(): R...
                ---@return boolean, R...
                local function prefix(callback) end

                ---@param ok boolean
                ---@return boolean, string, integer?
                local function forward(ok)
                    return prefix(function()
                        if ok then
                            return 'a', {result}
                        end
                        return 'b'
                    end)
                end
                "#,
            ));

            assert_eq!(
                count_by_code(&diagnostics, DiagnosticCode::ReturnTypeMismatch),
                expected,
                "return 'a', {result}: {diagnostics:#?}"
            );
            assert_eq!(
                count_by_code(&diagnostics, DiagnosticCode::MissingReturnValue),
                0
            );
            assert_eq!(
                count_by_code(&diagnostics, DiagnosticCode::RedundantReturnValue),
                0
            );
        }
    }

    #[googletest::gtest]
    fn test_declared_return_slots_keep_generic_identity() {
        use googletest::prelude::*;

        for (parameter, annotation, value, expected) in [
            ("T", "T", "value", 0),
            ("T", "T?", "value", 0),
            ("T?", "T?", "value", 0),
            ("T?", "T", "value", 1),
            ("T", "T", "42", 1),
            ("T[]", "T[]", "value", 0),
            ("number[]", "T[]", "value", 1),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@generic T: number
                ---@param value {parameter}
                ---@return boolean, {annotation}
                local function check(value)
                    return true, {value}
                end
                "#,
            ));

            expect_that!(
                count_by_code(&diagnostics, DiagnosticCode::ReturnTypeMismatch),
                eq(expected),
                "---@param value {parameter}, @return boolean, {annotation}, return true, {value}"
            );
        }
    }

    #[test]
    fn test_declared_return_slots_use_the_lexical_generic_constraint() {
        let source = r#"
            ---@generic T: string
            ---@param value T
            local function first(value) end

            ---@generic T: number
            ---@param value T
            ---@return string, boolean
            local function invalid(value)
                return value, true -- T is number here
            end

            ---@generic T: number
            ---@param value T
            local function outer(value)
                ---@return number, boolean
                local function inner()
                    return value, true
                end
                return inner()
            end
        "#;
        let diagnostics = check_source(source);
        let mismatches: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == DiagnosticCode::ReturnTypeMismatch)
            .collect();
        let invalid_value = source.find("value, true -- T is number here").unwrap();
        let start = rowan::TextSize::try_from(invalid_value).unwrap();

        assert_eq!(mismatches.len(), 1, "{diagnostics:#?}");
        assert_eq!(
            mismatches[0].range,
            rowan::TextRange::at(start, rowan::TextSize::of("value"))
        );
        assert!(mismatches[0].message.contains("found `number`"));
    }

    #[test]
    fn test_discriminated_union_assignment_keeps_branch_narrowing() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@class Foo
            ---@field kind "foo"
            ---@field a integer

            ---@class Bar
            ---@field kind "bar"
            ---@field b integer

            ---@param x Foo|Bar
            ---@return Foo
            local function test(x)
                if x.kind == "foo" then
                    x = { kind = "foo", a = 1 }
                    return x
                end

                return { kind = "foo", a = 2 }
            end
        "#
        ));
    }

    #[test]
    fn test_discriminated_union_partial_assignment_keeps_branch_narrowing() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@class Foo
            ---@field kind "foo"
            ---@field a integer

            ---@class Bar
            ---@field kind "bar"
            ---@field b integer

            ---@param x Foo|Bar
            ---@return Foo
            local function test(x)
                if x.kind == "foo" then
                    x = {}
                    x.kind = "foo"
                    x.a = 1
                    return x
                end

                return { kind = "foo", a = 2 }
            end
        "#
        ));
    }

    #[test]
    fn test_discriminated_union_partial_literal_assignment_keeps_branch_narrowing() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@class Foo
            ---@field kind "foo"
            ---@field a integer

            ---@class Bar
            ---@field kind "bar"
            ---@field b integer

            ---@param x Foo|Bar
            ---@return Foo
            local function test(x)
                if x.kind == "foo" then
                    x = { kind = "foo" }
                    x.a = 1
                    return x
                end

                return { kind = "foo", a = 2 }
            end
        "#
        ));
    }

    #[test]
    fn test_exact_string_reassignment_in_narrowed_branch_keeps_return_literal() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@param x string|number
            ---@return "a"
            local function test(x)
                if x == 1 then
                    x = "a"
                    return x
                end

                return "a"
            end
        "#
        ));
    }

    #[test]
    fn test_return_overload_mixed_guards_keep_return_narrowing() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@generic T, E
            ---@param ok boolean
            ---@param success T
            ---@param failure E
            ---@return boolean
            ---@return T|E
            ---@return_overload true, T
            ---@return_overload false, E
            local function pick(ok, success, failure)
                if ok then
                    return true, success
                end
                return false, failure
            end

            ---@param cond boolean
            ---@return integer
            local function test(cond)
                local ok, result = pick(cond, 1, "err")

                if ok == false then
                    error(result)
                end

                if not ok then
                    error(result)
                end

                return result
            end
        "#
        ));
    }

    #[test]
    fn test_pcall_return_after_type_guard() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            --- @param a string|fun(): integer
            --- @return integer?
            function foo(a)
                if type(a) == 'string' then
                    return
                end

                local ok, result = pcall(a)
                if not ok then
                    return
                end

                return result
            end
        "#
        ));
    }

    #[test]
    fn test_pcall_return_after_type_guard_with_table_arg() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            --- @param cb string|fun(a: {}): integer
            --- @return integer?
            function foo(cb)
                if type(cb) == 'string' then
                    return
                end

                local ok, result = pcall(cb, {})
                if not ok then
                    return
                end

                return result
            end
        "#
        ));
    }

    #[test]
    fn test_pcall_return_array_after_error_guard() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@class Runner

            ---@class File

            ---@param specs string[]
            ---@param runner Runner
            ---@return File[] files
            local function startTests(specs, runner)
                local ok, result = pcall(function ()
                    ---@type File[]
                    local files

                    return files
                end)
                if not ok then
                    error(result)
                end
                ---@cast result - string

                return result
            end
        "#
        ));
    }

    #[test]
    fn test_variadic_return_type_mismatch() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@return number, any...
            local function test()
                return 1, 2, 3
            end
        "#
        ));
    }

    #[test]
    fn test_issue_146() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            local function bar()
                return {}
            end

            ---@param _f fun():table 测试
            function foo(_f) end

            foo(function()
                return bar()
            end)
            "#
        ));
    }

    #[test]
    fn test_issue_150() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::RedundantReturnValue,
            r#"
            function bar(a)
                return tonumber(a)
            end
            "#
        ));
    }

    #[test]
    fn test_return_dots_syntax_error() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::SyntaxError,
            r#"
            function bar()
                return ...
            end
            "#
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::SyntaxError,
            r#"
            function bar()
                local args = {...}
            end
            "#
        ));
    }

    #[test]
    fn test_issue_167() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            --- @return integer?, integer?
            local function foo()
            end

            --- @return integer?, integer?
            local function bar()
                return foo()
            end
            "#
        ));
    }

    #[test]
    fn test_as_return_type() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            local function dd()
                return "11231"
            end

            ---@return integer
            local function f()

                return dd() ---@as integer
            end
        "#
        ));
    }

    #[test]
    fn test_1() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                ---@return string?
                local function a()
                    ---@type int?
                    local ccc
                    return ccc and a() or nil
                end
            "#
        ));
    }

    #[test]
    fn test_2() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                ---@return any[]
                local function a()
                    ---@type table|table<any, any>
                    local ccc
                    return ccc
                end
            "#
        ));
    }

    #[test]
    fn test_3() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                ---@return table<string, {old: any, new: any}>
                local function test()
                    ---@type table<string, {old: any, new: any}>|table
                    local a
                    return a
                end
            "#
        ));
    }

    #[test]
    fn test_4() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        // TODO: this test is forcibly covered by `setmetatable`, so `debug.setmetatable` is not diagnosed normally.
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@generic T
            ---@param value T
            ---@param meta? table
            ---@return T value
            ---@overload fun(value: table, meta: T): T
            local setmetatable = debug.setmetatable

            ---@class switch
            ---@field cachedCases string[]
            ---@field map table<string, function>
            ---@field _default fun(...):...
            local switchMT = {}

            ---@return switch
            local function switch()
                local obj = setmetatable({
                    map = {},
                    cachedCases = {},
                }, switchMT)
                return obj
            end
            "#
        ));
    }

    #[test]
    fn test_issue_341() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            --- @return integer
            local function foo()
                local a --- @type integer?
                return a or error("a is nil")
            end
            end
            "#
        ));
    }

    #[test]
    fn test_supper() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                ---@class key: integer

                ---@return key key
                local function get()
                    return 0
                end
            "#
        ));
    }

    #[test]
    fn test_return_self() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@class UI
            local M = {}

            ---@return self
            function M:get()
                return self
            end
            "#
        ));
    }
    #[test]
    fn test_issue_343() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                --- @return integer, integer
                function range() return 0, 0 end

                ---@class MyType
                ---@field [1] integer
                ---@field [2] integer

                --- @return MyType
                function foo()
                return { range() }
                end

                --- @return MyType
                function bar()
                return { 0, 0 }
                end
            "#
        ));
    }
    #[test]
    fn test_issue_474() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@class Range4
            ---@class TSNode: userdata
            ---@field range fun(self: TSNode): Range4

            ---@param node_or_range TSNode|Range4
            ---@return Range4
            function foo(node_or_range)
                if type(node_or_range) == 'table' then
                    return node_or_range
                else
                    return node_or_range:range()
                end
            end
            "#
        ));
    }

    #[test]
    fn test_super_alias() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                ---@namespace Test

                ---@alias A fun()

                ---@class B<T>: A

                ---@return A
                local function subscribe()
                    ---@type B<string>
                    local a

                    return a
                end
            "#
        ));
    }

    #[test]
    fn test_generic_type_extends() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                ---@class AnonymousObserver<T>: Observer<T>

                ---@class Observer<T>: IDisposable

                ---@class IDisposable

                ---@generic T
                ---@return IDisposable
                local function createAnonymousObserver()
                    ---@type AnonymousObserver<T>
                    local observer = {}

                    return observer
                end
            "#
        ));
    }

    #[test]
    fn test_generic_type_1() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Range: Observable<integer>
            ---@class Observable<T>

            ---@return Range
            function newRange()
            end
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"

            ---@return Observable<integer>
            function range()
                return newRange()
            end

            "#
        ));
    }

    #[test]
    fn test_generic_type_2() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@class Observable<T>
                ---@class CountObservable<T>: Observable<integer>
                CountObservable = {}
                ---@return CountObservable<T>
                function CountObservable:new()
                end
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                ---@return Observable<integer>
                local function count()
                    return CountObservable:new()
                end
            "#
        ));
    }

    #[test]
    fn test_generic_return_self() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                --- @class MockContext<T = string>
                local M

                --- @return MockContext
                function M:get()
                    return self
                end
            "#
        ));
    }

    #[test]
    fn test_asserted_array_member_return_field() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let code = r#"
            --- @return { a: integer }
            function foo()
                local arr --- @type integer[]
                local i --- @type integer?
                local a --- @type integer?
                i = _ --[[@as integer]]
                a = assert(arr[i])
                return { a = a }
            end
        "#;
        assert!(ws.has_no_diagnostic(DiagnosticCode::ReturnTypeMismatch, code));
        assert!(ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, code));
    }

    #[test]
    fn test_and_or_function_guard_return() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                --- @param f string|(fun():string)
                --- @return string
                function foo(f)
                    return type(f) == 'function' and f() or f
                end
            "#
        ));
    }

    #[test]
    fn test_generic_constraint_return_incompatible_type() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@class Animal
            ---@field name string

            ---@generic T: Animal
            ---@param animal T
            ---@return string
            local function checkAnimal(animal)
                return animal
            end
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@class Animal
            ---@field name string

            ---@generic T: Animal
            ---@param animal T
            ---@return Animal
            local function checkAnimal(animal)
                return animal
            end
        "#
        ));
    }

    #[test]
    fn test_recursive_alias_field_return_before_class() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@alias Recursive string | (Recursive[])

            ---@class Container
            ---@field recurse? Recursive

            local A = {}

            ---@param container? Container
            ---@return Recursive
            function A.return_recurse(container)
                if container and container.recurse then
                    return container.recurse
                end
                return A.return_recurse(container)
            end
            "#
        ));
    }
}
