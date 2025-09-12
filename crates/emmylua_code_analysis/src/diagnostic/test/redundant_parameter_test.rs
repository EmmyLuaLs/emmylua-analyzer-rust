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
}
