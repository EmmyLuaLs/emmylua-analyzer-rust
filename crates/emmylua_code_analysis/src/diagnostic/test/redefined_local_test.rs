#[cfg(test)]
mod tests {
    use crate::{DiagnosticCode, VirtualWorkspace};
    #[test]
    fn test_issue_226() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::RedefinedLocal,
            r#"
                function foo(...)
                local a = { ... }
                return function(...)
                    return { a, { ... } }
                end
                end
        "#
        ));
    }

    #[test]
    fn test() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::RedefinedLocal,
            r#"
                local x = 1
                local x = 2
        "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::RedefinedLocal,
            r#"
            local function aaa()
            end

            local function aaa()
            end
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::RedefinedLocal,
            r#"
            local function aaa(a, b)
            end
            local a = 2
            local b = 2
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::RedefinedLocal,
            r#"
            ---@class Test
            local Test = {}

            function Test:test(c)
            end

            local c = 1
        "#
        ));
    }

    #[test]
    fn test_do_end() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::RedefinedLocal,
            r#"
                do
                    local c = 1
                end

                do
                    local c = 1
                end
                local c = 1
        "#
        ));
    }

    #[test]
    fn test_for() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::RedefinedLocal,
            r#"
            local function aaa()
                for a = 1, 1 do
                    local fora = 1
                end
                local fora = 1
            end
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::RedefinedLocal,
            r#"
                for k, v in pairs({}) do
                end
                for k, v in pairs({}) do
                end
        "#
        ));
    }

    #[test]
    fn test_issue_481() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::RedefinedLocal,
            r#"
                local a = function(a) -- 不应该报错, 参数`a`先被定义, 然后再是局部变量`a`
                end
        "#
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::RedefinedLocal,
            r#"
                local a
                a = function(a) -- 报错
                end
        "#
        ));
    }
}
