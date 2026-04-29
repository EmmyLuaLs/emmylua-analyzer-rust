#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_issue_250() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::UndefinedGlobal,
            r#"
            --- @class A
            --- @field field any
            local A = {}

            function A:method()
            pcall(function()
                return self.field
            end)
            end
            "#
        ));
    }

    #[test]
    fn test_globals() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::UndefinedGlobal,
            r#"
            local fact = function(n)
                if n == 0 then
                    return 1
                end
                return n * fact(n - 1)
            end
            "#
        ));
    }
}
