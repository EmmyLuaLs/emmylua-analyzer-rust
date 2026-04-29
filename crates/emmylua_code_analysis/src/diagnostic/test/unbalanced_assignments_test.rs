#[cfg(test)]
mod tests {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::UnbalancedAssignments,
            r#"
            local x, y, z = print()
        "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::UnbalancedAssignments,
            r#"
            local x, y, z
            x, y, z = 1
        "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::UnbalancedAssignments,
            r#"
            local x, y, z = 1
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::UnbalancedAssignments,
            r#"
            local x, y, z
        "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::UnbalancedAssignments,
            r#"
                local x, y, z
                x, y, z = 1
        "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::UnbalancedAssignments,
            r#"
                X, Y, Z = 1
        "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::UnbalancedAssignments,
            r#"
            T = {}
            T.x, T.y, T.z = 1
        "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::UnbalancedAssignments,
            r#"
            T = {}
            T['x'], T['y'], T['z'] = 1
        "#
        ));
    }

    #[test]
    fn test_issue_232() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::UnbalancedAssignments,
            r#"
            local a, b, c = string.match("hello world", "(%w+) (%w+)")
            "#
        ));
    }

    #[test]
    fn test_2() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::UnbalancedAssignments,
            r#"
            ---@return any
            local function test()
            end

            local a, b, c = test()
            "#
        ));
    }

    #[test]
    fn test_3() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::UnbalancedAssignments,
            r#"
            ---@class D18
            local M
            function M:send()
            end
            local suc, err = M:send()
            "#
        ));
    }

    #[test]
    fn test_pcall() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::UnbalancedAssignments,
            r#"
                ---@type any
                local f
                local ok, result, err = pcall(f)
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::UnbalancedAssignments,
            r#"
                ---@type any
                local f
                local ok, result, err = xpcall(f, debug.traceback)
            "#
        ));
    }
}
