#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::RedundantParameter,
            r#"
            ---@class Test
            local Test = {}

            ---@param a string
            function Test.name(a)
            end

            Test:name("")
        "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::RedundantParameter,
            r#"
            ---@class Test2
            local Test = {}

            ---@param a string
            function Test.name(a)
            end

            Test.name("", "")
        "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::RedundantParameter,
            r#"
            ---@class A
            ---@field event fun()

            ---@type A
            local a = {
                event = function(aaa)
                end,
            }
        "#
        ));
    }

    #[test]
    fn test_1() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::RedundantParameter,
            r#"
                ---@type fun(...)[]
                local a = {}

                a[1] = function(ccc, ...)
                end
        "#
        ));
    }

    #[test]
    fn test_dots() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::RedundantParameter,
            r#"
            ---@class Test
            local Test = {}

            ---@param a string
            ---@param ... any
            function Test.dots(a, ...)
                print(a, ...)
            end

            Test.dots(1, 2, 3)
            Test:dots(1, 2, 3)
        "#
        ));
    }

    #[test]
    fn test_issue_360() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::RedundantParameter,
            r#"
                ---@alias buz number

                ---@param a buz
                ---@overload fun(): number
                function test(a)
                end

                local c = test({'test'})
        "#
        ));
    }

    #[test]
    fn test_function_param() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.check_code_for(
            DiagnosticCode::RedundantParameter,
            r#"
                ---@class D30
                local M = {}

                ---@param callback fun()
                local function with_local(callback)
                end

                function M:add_local_event()
                    with_local(function(local_player) end)
                end
        "#
        ));
    }

    #[test]
    fn test_generic_infer_function() {
        // temp disable this test, future fix this
        // let mut ws = VirtualWorkspace::new();
        // ws.def(
        //     r#"
        //     ---@alias Parameters<T extends function> T extends (fun(...: infer P): any) and P or never

        //     ---@alias Procedure fun(...: any[]): any

        //     ---@alias MockParameters<T> T extends Procedure and Parameters<T> or never

        //     ---@class Mock<T>
        //     ---@field calls MockParameters<T>[]
        //     ---@overload fun(...: MockParameters<T>...)

        //     ---@generic T: Procedure
        //     ---@param a T
        //     ---@return Mock<T>
        //     function fn(a)
        //     end

        //     sum = fn(function(a, b)
        //         return a + b
        //     end)
        //     "#,
        // );
        // assert!(!ws.check_code_for(
        //     DiagnosticCode::RedundantParameter,
        //     r#"
        //     sum(1, 2, 3)
        // "#
        // ));
    }

    #[test]
    fn test_issue_894() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            _nop = function() end
            "#,
        );
        assert!(ws.check_code_for(
            DiagnosticCode::RedundantParameter,
            r#"
            function a(...) _nop(...) end
        "#
        ));
    }

    #[test]
    fn test_last_arg_multi_return_call() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        // last arg is a multi-return function call (table.unpack), should not report redundant-parameter
        assert!(ws.check_code_for(
            DiagnosticCode::RedundantParameter,
            r#"
            ---@param args table
            local function f(args) end

            ---@type any[]
            local cmds = {}
            f(cmds, table.unpack(cmds))
        "#
        ));
    }

    #[test]
    fn test_last_arg_multi_return_call_with_alias() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        // table.unpack aliased to a local variable, should not report redundant-parameter
        assert!(ws.check_code_for(
            DiagnosticCode::RedundantParameter,
            r#"
            local tunpack = table.unpack

            ---@param args table
            local function f(args) end

            ---@type any[]
            local cmds = {}
            f(cmds, tunpack(cmds, 2))
        "#
        ));
    }
}
