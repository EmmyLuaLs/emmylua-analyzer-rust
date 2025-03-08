#[cfg(test)]
mod tests {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_return_type_mismatch() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
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

        assert!(!ws.check_code_for(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@return string
            local test = function()
                return 1
            end
        "#
        ));

        assert!(!ws.check_code_for(
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
        assert!(!ws.check_code_for(
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
    fn test_variadic_return_type_mismatch() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
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

        assert!(ws.check_code_for(
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

        assert!(ws.check_code_for(
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

        assert!(!ws.check_code_for(
            DiagnosticCode::LuaSyntaxError,
            r#"
            function bar()
                return ...
            end
            "#
        ));
        assert!(!ws.check_code_for(
            DiagnosticCode::LuaSyntaxError,
            r#"
            function bar()
                local args = {...}
            end
            "#
        ));
    }
}
