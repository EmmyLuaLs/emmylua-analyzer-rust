#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, FileId, LuaType, VirtualWorkspace};
    use emmylua_parser::{LuaAstNode, LuaCallExpr};

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

    fn find_instantiated_type(ws: &mut VirtualWorkspace, file_id: FileId) -> LuaType {
        let semantic_model = ws.analysis.compilation.get_semantic_model(file_id).unwrap();
        let call_expr = semantic_model
            .get_root()
            .descendants::<LuaCallExpr>()
            .next()
            .unwrap();
        LuaType::DocFunction(
            semantic_model
                .infer_call_expr_func(call_expr, None)
                .unwrap(),
        )
    }

    #[test]
    fn test_non_variadic_param_non_variadic_template() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                --- @generic T
                --- @param x T
                --- @return T
                function f(x) end
            "#,
        );

        let file_id = ws.def(r#"
            local _ = { f() }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: nil): nil"),
            "non-variadic param doesn't match empty variadic"
        );

        let file_id = ws.def(r#"
            local _ = { f(1) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: integer): integer"),
            "non-variadic param doesn't become variadic"
        );

        let file_id = ws.def(r#"
            local _ = { f(1, "") }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: integer): integer"),
            "non-variadic param doesn't expand into a variadic"
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): integer...
            local _ = { f(a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: integer): integer"),
            "non-variadic param doesn't expand into a variadic"
        );
    }

    #[test]
    fn test_non_variadic_param_nested_non_variadic_template() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                --- @generic T
                --- @param x [T]
                --- @return T
                function f(x) end
            "#,
        );

        let file_id = ws.def(r#"
            local a --- @type [integer]
            local _ = { f(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: [integer]): integer"),
            "non-variadic param doesn't become variadic"
        );

        let file_id = ws.def(r#"
            local a --- @type [integer, integer]
            local _ = { f(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: [integer]): integer"),
            "non-variadic param doesn't expand into a variadic"
        );

        let file_id = ws.def(r#"
            local a --- @type [integer...]
            local _ = { f(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: [integer]): integer"),
            "non-variadic param doesn't expand into a variadic"
        );

        let file_id = ws.def(r#"
            local a --- @type [integer, integer...]
            local _ = { f(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: [integer]): integer"),
            "non-variadic param doesn't expand into a variadic"
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): [integer...]...
            local _ = { f(a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: [integer]): integer"),
            "non-variadic param doesn't expand into a variadic"
        );
    }

    #[test]
    fn test_non_variadic_param_nested_variadic_template() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                --- @generic T
                --- @param x [T...]
                --- @return T...
                function f(x) end
            "#,
        );

        let file_id = ws.def(r#"
            local a --- @type []
            local _ = { f(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: [])"),
        );

        let file_id = ws.def(r#"
            local a --- @type [integer]
            local _ = { f(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: [integer]): integer"),
        );

        let file_id = ws.def(r#"
            local a --- @type [integer, string]
            local _ = { f(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: [integer, string]): integer, string"),
        );

        let file_id = ws.def(r#"
            local a --- @type [integer...]
            local _ = { f(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: [integer...]): integer..."),
        );

        let file_id = ws.def(r#"
            local a --- @type [integer, string...]
            local _ = { f(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: [integer, string...]): integer, string..."),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): [integer...]...
            local _ = { f(a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: [integer...]): integer..."),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): [integer, string...]...
            local _ = { f(a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: [integer, string...]): integer, string..."),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): string...
            local _ = { f({10, a()}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: [integer, string...]): integer, string..."),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): string...
            local _ = { f({10, a(), 10}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(x: [integer, string, integer]): integer, string, integer"),
        );
    }

    #[test]
    fn test_variadic_param_non_variadic_template() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                --- @generic T
                --- @param ... T
                --- @return T...
                function f(...) end
            "#,
        );

        let file_id = ws.def(r#"
            local _ = { f(1) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: integer): integer"),
        );

        let file_id = ws.def(r#"
            local _ = { f(1, "") }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: integer): integer"),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): integer...
            local _ = { f(a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: integer): integer"),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): integer...
            local _ = { f(1, "", a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: integer): integer"),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): integer...
            local _ = { f(1, a(), 2) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: integer): integer"),
        );
    }

    #[test]
    fn test_variadic_param_variadic_template() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                --- @generic T
                --- @param ... T...
                --- @return T...
                function f(...) end
            "#,
        );

        let file_id = ws.def(r#"
            local _ = { f() }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun()"),
        );

        let file_id = ws.def(r#"
            local _ = { f(1) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: integer): integer"),
        );

        let file_id = ws.def(r#"
            local _ = { f(1, "") }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: integer, p1: string): integer, string"),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): integer...
            local _ = { f(a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: integer...): integer..."),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): integer...
            local _ = { f(1, "", a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: integer, p1: string, p2: integer...): integer, string, integer..."),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): integer...
            local _ = { f(1, "", a(), 0.5) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: integer, p1: string, p2: integer, p3: number): integer, string, integer, number"),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): number, integer...
            local _ = { f(1, "", a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: integer, p1: string, p2: number, p3: integer...): integer, string, number, integer..."),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): number, integer...
            local _ = { f(1, "", a(), 0.5) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: integer, p1: string, p2: number, p3: number): integer, string, number, number"),
        );
    }

    #[test]
    fn test_variadic_param_nested_non_variadic_template_expanded() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                --- @generic T
                --- @param ... [T]...
                --- @return T...
                function f(...) end
            "#,
        );

        let file_id = ws.def(r#"
            local _ = { f() }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun()"),
        );

        let file_id = ws.def(r#"
            local _ = { f({1}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [integer]): integer"),
        );

        let file_id = ws.def(r#"
            local _ = { f({1, ""}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [integer]): integer"),
        );

        let file_id = ws.def(r#"
            local _ = { f({1}, {""}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [integer], p1: [string]): integer, string"),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): [string]...
            local _ = { f({1}, a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [integer], p1: [string]...): integer, string..."),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): [number], [string]...
            local _ = { f({1}, a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [integer], p1: [number], p2: [string]...): integer, number, string..."),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): [number], [string]...
            local _ = { f({1}, a(), {2}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [integer], p1: [number], p2: [integer]): integer, number, integer"),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): [string]...
            local _ = { f({1}, a(), {2}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [integer], p1: [string], p2: [integer]): integer, string, integer"),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): [string, number]...
            local _ = { f({1}, a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [integer], p1: [string]...): integer, string..."),
        );
    }

    #[test]
    fn test_variadic_param_nested_non_variadic_template() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                --- @generic T
                --- @param ... [T]
                --- @return T...
                function f(...) end
            "#,
        );

        // let file_id = ws.def(r#"
        //     local _ = { f() }
        // "#);
        // assert_eq!(
        //     find_instantiated_type(&mut ws, file_id),
        //     ws.ty("fun(...: [unknown])"),
        // );

        let file_id = ws.def(r#"
            local _ = { f({1}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: [integer]): integer"),
        );

        let file_id = ws.def(r#"
            local _ = { f({1, ""}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: [integer]): integer"),
        );

        let file_id = ws.def(r#"
            local _ = { f({1}, {""}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: [integer]): integer"),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): [string]...
            local _ = { f({1}, a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: [integer]): integer"),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): [integer]...
            local _ = { f(a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: [integer]): integer"),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): [integer]...
            local _ = { f(a(), {1}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: [integer]): integer"),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): [integer, string]...
            local _ = { f(a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: [integer]): integer"),
        );
    }

    #[test]
    fn test_variadic_param_nested_variadic_template() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                --- @generic T
                --- @param ... [T...]
                --- @return T...
                function f(...) end
            "#,
        );

        let file_id = ws.def(r#"
            local a --- @type []
            local _ = { f(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: [])"),
        );

        let file_id = ws.def(r#"
            local _ = { f({1}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: [integer]): integer"),
        );

        let file_id = ws.def(r#"
            local _ = { f({1, ""}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: [integer, string]): integer, string"),
        );

        let file_id = ws.def(r#"
            local _ = { f({1, ""}, {2, 3}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: [integer, string]): integer, string"),
        );

        let file_id = ws.def(r#"
            local a --- @type [integer...]
            local _ = { f(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: [integer...]): integer..."),
        );

        let file_id = ws.def(r#"
            local a --- @type [string, integer...]
            local _ = { f(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(...: [string, integer...]): string, integer..."),
        );
    }

    #[test]
    fn test_variadic_param_nested_variadic_template_expanded() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                --- @generic T
                --- @param ... [T...]...
                --- @return [T...]...
                function f(...) end
            "#,
        );

        let file_id = ws.def(r#"
            local a --- @type []
            local _ = { f(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: []): []"),
        );

        let file_id = ws.def(r#"
            local _ = { f({1}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [integer]): [integer]"),
        );

        let file_id = ws.def(r#"
            local _ = { f({1, ""}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [integer, string]): [integer, string]"),
        );

        let file_id = ws.def(r#"
            local a --- @type [integer...]
            local _ = { f(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [integer...]): [integer...]"),
        );

        let file_id = ws.def(r#"
            local a --- @type [string, integer...]
            local _ = { f(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [string, integer...]): [string, integer...]"),
        );

        let file_id = ws.def(r#"
            local _ = { f({1}, {2, "x"}, {0.0}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [integer], p1: [integer, string], p2: [number]): [integer], [integer, string], [number]"),
        );

        let file_id = ws.def(r#"
            local a --- @type [integer...]
            local b --- @type [string, integer...]
            local _ = { f({1}, a, b, {0.0}) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [integer], p1: [integer...], p2: [string, integer...], p3: [number]): [integer], [integer...], [string, integer...], [number]"),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): [integer]...
            local _ = { f(a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [integer]...): [integer]..."),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): [integer...]...
            local _ = { f(a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [integer...]...): [integer...]..."),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): [integer, string]...
            local _ = { f({0.0}, a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [number], p1: [integer, string]...): [number], [integer, string]..."),
        );

        let file_id = ws.def(r#"
            local a --- @type fun(): [integer, string...]...
            local _ = { f({0.0}, a()) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: [number], p1: [integer, string...]...): [number], [integer, string...]..."),
        );
    }

    #[test]
    fn test_complex_variadic_functions() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                --- @class Future<T>

                --- @generic T
                --- @param ... Future<T>...
                --- @return Future<[T...]>
                function join(...) end

                --- @generic T
                --- @param f Future<[T...]>
                --- @return Future<T>...
                function split(f) end

                --- @generic L, R
                --- @param l [L...]
                --- @param r [R...]
                --- @return [[L, R]...]
                function zipTuples2(l, r) end
            "#,
        );

        let file_id = ws.def(r#"
            local a, b --- @type Future<integer>, Future<string>
            local _ = { join(a, b) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(p0: Future<integer>, p1: Future<string>): Future<[integer, string]>"),
        );

        let file_id = ws.def(r#"
            local a --- @type Future<[integer, string]>
            local _ = { split(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(f: Future<[integer, string]>): Future<integer>, Future<string>"),
        );

        let file_id = ws.def(r#"
            local a --- @type Future<[integer, string...]>
            local _ = { split(a) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(f: Future<[integer, string...]>): Future<integer>, Future<string>..."),
        );

        let file_id = ws.def(r#"
            local l, r --- @type [integer, string], [number, table]
            local _ = { zipTuples2(l, r) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(l: [integer, string], r: [number, table]): [[integer, number], [string, table]]"),
        );

        let file_id = ws.def(r#"
            local l, r --- @type [integer, string, number...], [number, table, number...]
            local _ = { zipTuples2(l, r) }
        "#);
        assert_eq!(
            find_instantiated_type(&mut ws, file_id),
            ws.ty("fun(l: [integer, string, number...], r: [number, table, number...]): [[integer, number], [string, table], [number, number]...]"),
        );
    }
}
