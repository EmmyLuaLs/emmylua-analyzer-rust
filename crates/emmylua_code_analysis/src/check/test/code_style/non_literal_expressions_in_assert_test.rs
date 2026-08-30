#[cfg(test)]
mod test {
    use crate::DiagnosticCode;

    #[test]
    fn test_feat_209() {
        let mut ws = crate::VirtualWorkspace::new();
        ws.enable_check(DiagnosticCode::NonLiteralExpressionsInAssert);

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::NonLiteralExpressionsInAssert,
            r#"
            -- msg is global or unknown
            local a = assert(foo(), msg)
            "#,
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NonLiteralExpressionsInAssert,
            r#"
            local msg = "msg"

            local a = assert(foo(), msg)
            "#,
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NonLiteralExpressionsInAssert,
            r#"
            local a = assert(foo(), "msg")
            "#,
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::NonLiteralExpressionsInAssert,
            r#"
            local a = assert(foo(), "msg" .. "msg2")
            "#,
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NonLiteralExpressionsInAssert,
            r#"
            local t = { a = "msg" }

            local a = assert(foo(), t.a)
            "#,
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::NonLiteralExpressionsInAssert,
            r#"
            local function get_des() return "msg" end

            local a = assert(foo(), get_des())
            "#,
        ));
    }
}
