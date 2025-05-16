#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_issue_263() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@alias aaa fun(a: string, b: integer): integer

        ---@type aaa
        local a

        d, b = pcall(a, "", 1)
        "#,
        );

        let aaa_ty = ws.expr_ty("b");
        let expected = ws.ty("integer|any");
        assert_eq!(aaa_ty, expected);
    }

    #[test]
    fn test_issue_280() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
        ---@class D11.AAA
        local AAA = {}

        ---@param a string
        ---@param b number
        function AAA:name(a, b)
        end

        ---@param a string
        ---@param b number
        function AAA:t(a, b)
            local ok, err = pcall(self.name, self, a, b)
        end
        "#
        ));
    }
}
