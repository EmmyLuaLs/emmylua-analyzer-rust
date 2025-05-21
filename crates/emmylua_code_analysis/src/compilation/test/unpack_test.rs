#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_unpack() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        a, b = table.unpack({ 1, 2, 3 })

        ---@type string[]
        local ddd 

        e = table.unpack(ddd)
        "#,
        );

        let a_ty = ws.expr_ty("a");
        let a_expected = ws.expr_ty("1");
        assert_eq!(a_ty, a_expected);

        let b_ty = ws.expr_ty("b");
        let b_expected = ws.expr_ty("2");
        assert_eq!(b_ty, b_expected);

        let e_ty = ws.expr_ty("e");
        let e_expected = ws.ty("string?");
        assert_eq!(e_ty, e_expected);
    }

    #[test]
    fn test_issue_484() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_code_for(
            DiagnosticCode::AssignTypeMismatch,
            r#"
        --- @type integer,integer,integer
        local _a, _b, _c = unpack({ 1, 2, 3 })
        "#,
        ));
    }
}
