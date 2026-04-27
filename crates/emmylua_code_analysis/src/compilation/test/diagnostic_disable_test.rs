#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_disable_nextline() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::SyntaxError,
            r#"
        ---@diagnostic disable-next-line: syntax-error
        ---@param
        local function f() end
        "#,
        ));
    }

    #[test]
    fn test_file_disable_from_summary_sync() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::SyntaxError,
            r#"
        ---@diagnostic disable: syntax-error

        ---@param
        local function f() end
        "#,
        ));
    }

    #[test]
    fn test_file_enable_from_summary_sync() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::MissingReturn,
            r#"
        ---@diagnostic disable: missing-return
        ---@diagnostic enable: missing-return

        ---@return integer
        local function f()
        end
        "#,
        ));
    }
}
