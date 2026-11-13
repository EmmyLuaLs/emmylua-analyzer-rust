#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, Emmyrc, EmmyrcLuaVersion, VirtualWorkspace};

    #[test]
    fn test_issue_276() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingParameter,
            r#"
                --- @param a string
                --- @param b? string
                --- @param c? string
                --- @return string
                --- @overload fun(a: string, b: string): number
                local function myfun2(a, b, c) end

                local a = myfun2('string')
        "#
        ));
    }

    #[test]
    fn test_issue_249() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingParameter,
            r#"
            ---@param path string
            ---@return string? realpath
            ---@overload fun(path:string, callback:function):userdata
            function realpath(path)
            end

            local path = realpath('/', function(err, path)
            end)

        "#
        ));
    }

    #[test]
    fn test_overload_param_count_gap_reports_missing_parameter() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingParameter,
            r#"
                ---@class Callable
                ---@overload fun(a: string)
                ---@overload fun(a: string, b: string, c: string)
                ---@type Callable
                local callable

                callable("a", "b")
        "#
        ));
    }

    #[test]
    fn test_generic_required_param_reports_missing_parameter() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingParameter,
            r#"
                ---@generic T
                ---@param v T
                ---@return T
                local function getBox(v)
                    return v
                end

                getBox()
        "#
        ));
    }

    #[test]
    fn test_generic_return_does_not_make_unknown_param_required() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingParameter,
            r#"
                ---@generic T
                ---@param v unknown
                ---@return T
                local function getBox(v)
                    return v
                end

                getBox()
        "#
        ));
    }

    #[test]
    fn test_1() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingParameter,
            r#"
            ---@class A
            ---@field event fun(aaa: integer)

            ---@type A
            local a = {
                event = function()
                end,
            }
        "#
        ));
    }

    #[test]
    fn test_issue_98() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingParameter,
            r#"
        ---@param callback fun(i?: integer)
        function foo(callback)
            callback()
            callback(1123)
        end
        "#
        ));
    }

    #[test]
    fn test_multi_return() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingParameter,
            r#"
            ---@param a number
            ---@param b number
            ---@param c number
            local function testA(a, b, c)
            end

            ---@return number
            ---@return number
            ---@return string
            local function testB()
                return 1, 2, 3, 4, 5
            end

            testA(1, testB())
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingParameter,
            r#"
            ---@param a number
            ---@param b number
            ---@param c number
            local function testA(a, b, c)
            end

            ---@return number
            ---@return number
            local function testB()
                return 1, 2, 3
            end

            testA(testB())
            "#
        ));
    }

    #[test]
    fn test_table_unpack() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingParameter,
            r#"
            local table = {}
            ---@generic T
            ---@param list [T...] | T[] | table<any, T>
            ---@param i? integer
            ---@param j? integer
            ---@return T...
            function table.unpack(list, i, j) end

            ---@param a number
            ---@param b number
            local function test(a,b)
            end

            ---@type number[]
            local a = {1,2,3}

            test(table.unpack(a))
        "#
        ));
    }

    #[test]
    fn test_alias() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingParameter,
            r#"
            ---@alias Serialization.SupportTypes
            ---| number
            ---| nil

            ---@param data Serialization.SupportTypes
            local function send(data)
            end
            send()
        "#
        ));
    }

    #[test]
    fn test_issue_450() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::MissingParameter,
            r#"
                ---@class D31.A
                local A = {}

                function A:foo()
                end

                local a = A.foo()
        "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::MissingParameter,
            r#"
                ---@class D31.A
                local A = {}

                function A:foo()
                end

                local a = A.foo(A)
        "#
        ));
    }

    #[test]
    fn test_issue_633() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "test.lua",
            r#"
            ---@param mode number
            ---@param a number
            ---@param b number
            ---@param c number
            ---@param d number?
            ---@return string
            ---@overload fun(mode:number, a:number, b:number):number
            function test(mode, a, b, c, d)
            end

            ---@return number, number
            function getNumbers()
                return 1, 2
            end
        "#,
        );

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::MissingParameter,
            r#"
            test(1, getNumbers())
        "#
        ));
    }

    #[test]
    fn test_call_operator_implicit_self() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingParameter,
            r#"
                ---@class Task
                local Task = {}

                ---@param callback fun()
                function Task:await(callback)
                end

                ---@class TaskFun
                ---@operator call: Task

                ---@type TaskFun[]
                local tasks = {}

                tasks[1]():await(function()
                end)
        "#
        ));
    }

    #[test]
    fn test_call_overload_named_self_is_not_stripped() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingParameter,
            r#"
                ---@class Callable
                ---@overload fun(self: string)
                ---@type Callable
                local c

                c()
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingParameter,
            r#"
                ---@class Callable
                ---@overload fun(self: string)
                ---@type Callable
                local c

                c("x")
        "#
        ));
    }

    #[test]
    fn test_call_overload_self_type_is_not_stripped() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingParameter,
            r#"
                ---@class Callable
                ---@overload fun(self: self)
                ---@type Callable
                local c

                c()
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingParameter,
            r#"
                ---@class Callable
                ---@overload fun(self: self)
                ---@type Callable
                local c

                c(c)
        "#
        ));
    }

    #[test]
    fn test_issue_1249_newproxy_optional_argument() {
        for version in [EmmyrcLuaVersion::Lua51, EmmyrcLuaVersion::LuaJIT] {
            let mut config = Emmyrc::default();
            config.runtime.version = version;
            let mut ws = VirtualWorkspace::new_with_config(config);

            assert!(
                ws.has_no_diagnostic(
                    DiagnosticCode::MissingParameter,
                    r#"
                    local p = newproxy()
                    print(p)
                    "#
                ),
                "newproxy() must be accepted in {version:?}"
            );
        }
    }

    #[test]
    fn test_issue_1250_table_literal_is_single_variadic_argument() {
        for version in [EmmyrcLuaVersion::Lua54, EmmyrcLuaVersion::LuaJIT] {
            let mut config = Emmyrc::default();
            config.runtime.version = version;
            let mut ws = VirtualWorkspace::new_with_config(config);

            assert!(
                !ws.has_no_diagnostic(DiagnosticCode::MissingParameter, "pcall()"),
                "pcall() must still report the required callback in {version:?}"
            );

            let chunk = r#"
                local ok = pcall(function() end, { 1, 2, 3 })
                print(ok)
            "#;
            assert!(
                ws.has_no_diagnostic(DiagnosticCode::MissingParameter, chunk),
                "pcall must accept a table literal as one argument in {version:?}"
            );
            assert!(
                ws.has_no_diagnostic(DiagnosticCode::ParamTypeMismatch, chunk),
                "pcall table literal must not mismatch in {version:?}"
            );

            let chunk = r#"
                local home = assert(os.getenv("HOME"), { 1, 2, 3 })
                print(home)
            "#;
            assert!(
                ws.has_no_diagnostic(DiagnosticCode::MissingParameter, chunk),
                "assert must accept a table literal as one argument in {version:?}"
            );
            assert!(
                ws.has_no_diagnostic(DiagnosticCode::ParamTypeMismatch, chunk),
                "assert table literal must not mismatch in {version:?}"
            );

            let chunk = r#"
                local err = { 1, 2, 3 }
                local home = assert(os.getenv("HOME"), err)
                print(home)
            "#;
            assert!(
                ws.has_no_diagnostic(DiagnosticCode::MissingParameter, chunk),
                "assert must accept a table variable as one argument in {version:?}"
            );
        }
    }

    #[test]
    fn test_issue_1248_version_gated_callee_params() {
        let chunk = r#"
            local ok, err = pcall(function() error("boom") end)
            if not ok then
                local x = err:unpack()
                print(x)
            end
        "#;

        let mut config = Emmyrc::default();
        config.runtime.version = EmmyrcLuaVersion::LuaJIT;
        let mut ws = VirtualWorkspace::new_with_config(config);
        assert!(
            ws.has_no_diagnostic(DiagnosticCode::MissingParameter, chunk),
            "version-hidden string.unpack must not report missing parameters"
        );
        assert!(
            ws.has_no_diagnostic(DiagnosticCode::ParamTypeMismatch, chunk),
            "version-hidden string.unpack must not report parameter mismatches"
        );

        let mut config = Emmyrc::default();
        config.runtime.version = EmmyrcLuaVersion::Lua54;
        let mut ws = VirtualWorkspace::new_with_config(config);
        assert!(
            !ws.has_no_diagnostic(DiagnosticCode::MissingParameter, chunk),
            "string.unpack is available in Lua 5.4 and must still be checked"
        );
        assert!(
            !ws.has_no_diagnostic(
                DiagnosticCode::MissingParameter,
                r#"
                ---@type string
                local s
                s:sub()
                "#
            ),
            "visible members must still report missing parameters"
        );
    }
}
