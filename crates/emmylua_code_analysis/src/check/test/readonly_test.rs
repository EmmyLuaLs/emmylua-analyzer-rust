#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_issue_760() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ReadOnly,
            r#"
            ---@readonly
            local errorCode = {}

            errorCode.NOT_FOUND = 10 --- show warnings attempt to modify readonly variables.
        "#
        ));
    }
}
