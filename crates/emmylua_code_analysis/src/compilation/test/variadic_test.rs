#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_unexpected_variadic_expansion() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @type integer...
                local _
            "#,
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @type (integer...)[]
                local _
            "#,
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @type [integer..., integer]
                local _
            "#,
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @class Foo
                --- @operator add(Foo): Foo...
            "#,
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @class Foo
                --- @operator add(Foo...): Foo
            "#,
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @class Foo
                --- @operator call(Foo...): Foo
            "#,
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @return integer..., integer
                function foo() end
            "#,
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @return integer...
                --- @return integer
                function foo() end
            "#,
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @param x any
                --- @return boolean
                --- @return_cast x integer...
                function isInt(x) end
            "#,
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @param x integer...
                function foo(x) end
            "#,
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @param x integer...
                function foo(...) end
            "#,
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @parameter x fun(): integer..., integer
                function foo(x) end
            "#,
        ));
    }

    #[test]
    fn test_expected_variadic_expansion() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @type [string, integer...]
                local _
            "#,
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @class Foo
                --- @operator call(Foo): Foo...
            "#,
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @parameter x fun(_: integer...)
                function foo(x) end
            "#,
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @parameter x fun(_: integer..., _: integer)
                function foo(x) end
            "#,
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @parameter x fun(_: integer..., _: integer...)
                function foo(x) end
            "#,
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @parameter x fun(): integer...
                function foo(x) end
            "#,
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @return integer...
                function foo() end
            "#,
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @type Foo<integer, integer...>
                local _
            "#,
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @type Foo<integer..., integer...>
                local _
            "#,
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::DocTypeUnexpectedVariadic,
            r#"
                --- @type Foo<integer..., integer>
                local _
            "#,
        ));
    }

    #[test]
    fn test_common_variadic_infer_errors() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            --- @return integer..., integer
            function foo() end

            a, b = foo()
            a1, a2 = a
        "#,
        );

        assert_eq!(ws.expr_ty("a"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("b"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("a1"), ws.ty("integer"));
        assert_ne!(ws.expr_ty("a2"), ws.ty("integer"));

        ws.def(
            r#"
            x = nil --- @type integer...
            x1, x2 = x
        "#,
        );

        assert_eq!(ws.expr_ty("x"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("x1"), ws.ty("integer"));
        assert_ne!(ws.expr_ty("x2"), ws.ty("integer"));
    }
}
