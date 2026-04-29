#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_custom_binary() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@class AA
        ---@operator pow(number): AA

        ---@type AA
        a = {}
        "#,
        );

        let ty = ws.expr_ty(
            r#"
        a ^ 1
        "#,
        );
        let expected = ws.ty("AA");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_issue_559() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class Origin
            ---@operator add(Origin):Origin

            ---@alias AliasType Origin

            ---@type AliasType
            local x1
            ---@type AliasType
            local x2

            A = x1 + x2
        "#,
        );

        let ty = ws.expr_ty("A");
        let expected = ws.ty("Origin");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_issue_867() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            local a --- @type { foo? : { bar: { baz: number } } }

            local b = a.foo.bar -- a.foo may be nil (correct)

            c = b.baz -- b may be nil (incorrect)
        "#,
        );

        let ty = ws.expr_ty("c");
        let expected = ws.ty("number");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_intersection_call_infers_return_type() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@type { field: string } & fun(): string
            F = nil
        "#,
        );

        assert_eq!(ws.expr_ty("F()"), ws.ty("string"));
    }

    #[test]
    fn test_infer_expr_list_types_tolerates_infer_failures() {
        let mut ws = VirtualWorkspace::new();
        let code = r#"
            local t ---@type { a: number }

            ---@type string, string
            local y, x

            x, y = t.b, 1
        "#;

        assert!(!ws.has_no_diagnostic(DiagnosticCode::UndefinedField, code));
        assert!(!ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, code));
    }

    #[test]
    fn test_flow_assign_preserves_doc_type_on_infer_error() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            local t ---@type { a: number }
            local x ---@type string
            x = t.b
            R = x
        "#,
        );

        assert_eq!(ws.expr_ty("R"), ws.ty("nil"));
    }
}
