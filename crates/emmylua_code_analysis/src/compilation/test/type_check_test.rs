#[cfg(test)]
mod test {

    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_issue_421() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
        local a         --- @type string?
        local b = { a } --- @type string[] error

        b[2] = nil
        "#,
        ));
    }

    #[test]
    fn test_issue_645() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
        --- @alias Dir -1|1

        ---@param d Dir
        local function foo(d) end

        foo(1)
        "#,
        ));
    }

    #[test]
    fn test_issue_925() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::TypeNotFound,
            r#"
            ---@alias Pick<T, K extends keyof T> { [P in K]: T[P]; }
        "#,
        ));
    }
}
