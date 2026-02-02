#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, LuaIndex, VirtualWorkspace};
    use lsp_types::NumberOrString;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn test_call_non_callable() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::CallNonCallable,
            r#"
                local i = 1
                i()
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::CallNonCallable,
            r#"
                local s = "hi"
                s()
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::CallNonCallable,
            r#"
                local b = true
                b()
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::CallNonCallable,
            r#"
                ---@type thread
                local t
                t()
            "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::CallNonCallable,
            r#"
                local function f() end
                f()
            "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::CallNonCallable,
            r#"
                ---@type function
                local f
                f()
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::CallNonCallable,
            r#"
                ---@type function|integer
                local f
                f()
            "#
        ));

        // nil is covered by need-check-nil instead of call-non-callable
        assert!(ws.check_code_for(
            DiagnosticCode::CallNonCallable,
            r#"
                ---@type function|nil
                local f
                f()
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::CallNonCallable,
            r#"
                (1)()
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::CallNonCallable,
            r#"
                ("hi")()
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::CallNonCallable,
            r#"
                (true)()
            "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::CallNonCallable,
            r#"
                (function() end)()
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::CallNonCallable,
            r#"
                local i --- @type integer|fun():string
                _ = i()
            "#
        ));
    }

    #[test]
    fn test_call_non_callable_fallback_from_initializer() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_check(DiagnosticCode::CallNonCallable);
        let file_id = ws.def("local i = 1\n i()\n");
        let db = ws.get_db_mut();
        db.get_type_index_mut().clear();
        db.get_flow_index_mut().clear();

        let diagnostics = ws
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .unwrap();
        let code = Some(NumberOrString::String(
            DiagnosticCode::CallNonCallable.get_name().to_string(),
        ));
        assert!(diagnostics.iter().any(|diag| diag.code == code));
    }
}
