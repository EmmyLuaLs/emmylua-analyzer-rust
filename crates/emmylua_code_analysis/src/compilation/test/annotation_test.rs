#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_issue_223() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.check_code_for(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
        --- @return integer
        function foo()
            local a
            return a --[[@as integer]]
        end
        "#,
        );
    }

    // workaround for table
    #[test]
    fn test_issue_234() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        GG = {} --- @type table

        GG.f = {}

        function GG.fun() end

        function GG.f.fun() end
        "#,
        );

        let ty = ws.expr_ty("GG.fun");
        assert_eq!(
            format!("{:?}", ty),
            "Signature(LuaSignatureId { file_id: FileId { id: 14 }, position: 76 })"
        );
    }
}
