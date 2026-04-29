#[cfg(test)]
mod tests {
    use crate::{DiagnosticCode, VirtualWorkspace};

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
        // TODO 该测试被`setmetatable`强行覆盖, 未正常诊断`debug.setmetatable`
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
}
