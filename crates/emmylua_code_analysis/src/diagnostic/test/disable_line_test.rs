#[cfg(test)]
mod test {
    use crate::DiagnosticCode;

    #[test]
    fn test_issue_158() {
        let mut ws = crate::VirtualWorkspace::new();

        ws.def(
            r#"
        a = {} --- @deprecated
        "#,
        );

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::Deprecated,
            r#"
            ---@diagnostic disable-next-line: deprecated
            local _b = a
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::Deprecated,
            r#"
            local _c = a ---@diagnostic disable-next-line: deprecated
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::Deprecated,
            r#"
            local _d = a ---@diagnostic disable-line: deprecated
            "#
        ));
    }
}
