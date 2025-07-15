#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_cmp() {
        let mut ws = VirtualWorkspace::new();

        let left_1 = ws.expr_ty("1 < 2");
        let right_1 = ws.expr_ty("true");
        assert_eq!(left_1, right_1);

        let left_2 = ws.expr_ty("1 <= 2");
        let right_2 = ws.expr_ty("true");
        assert_eq!(left_2, right_2);

        let left_3 = ws.expr_ty("1 > 2");
        let right_3 = ws.expr_ty("false");
        assert_eq!(left_3, right_3);

        let left_4 = ws.expr_ty("1 >= 2");
        let right_4 = ws.expr_ty("false");
        assert_eq!(left_4, right_4);

        let left_5 = ws.expr_ty("1 == 2");
        let right_5 = ws.expr_ty("false");

        assert_eq!(left_5, right_5);

        let left_6 = ws.expr_ty("1 ~= 2");
        let right_6 = ws.expr_ty("true");
        assert_eq!(left_6, right_6);

        let left_7 = ws.expr_ty("1 == 1");
        let right_7 = ws.expr_ty("true");
        assert_eq!(left_7, right_7);
    }

    #[test]
    fn test_and() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        local a 
        d = a and 1
        "#,
        );
        let left = ws.expr_ty("d");
        assert_eq!(ws.humanize_type(left), "unknown");
    }

    #[test]
    fn test_issue_219() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::UnnecessaryAssert,
            r#"
        local a --- @type integer?
        assert(a and 1)
        "#,
        ));
    }
}
