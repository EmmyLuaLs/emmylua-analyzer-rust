#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};
    use emmylua_parser::{LuaCallExpr, LuaExpr};

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
    fn test_no_flow_overload_call_keeps_shared_return_when_arg_declines() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            ---@overload fun(value: string): boolean
            ---@overload fun(value: integer): boolean
            ---@param value string|integer
            ---@return boolean
            local function classify(value)
            end

            local result = classify({})
        "#,
        );

        let call_expr = ws.get_node::<LuaCallExpr>(file_id);
        let semantic_model = ws
            .analysis
            .compilation
            .get_semantic_model(file_id)
            .expect("Semantic model must exist");
        let ty = crate::semantic::infer::try_infer_expr_no_flow(
            semantic_model.get_db(),
            &mut semantic_model.get_cache().borrow_mut(),
            LuaExpr::CallExpr(call_expr),
        )
        .expect("no-flow call replay should not error")
        .expect("no-flow call replay should keep shared overload return");

        assert_eq!(ty, ws.ty("boolean"));
    }

    #[test]
    fn test_no_flow_overload_call_declines_when_declined_arg_returns_differ() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            ---@overload fun(value: string): string
            ---@overload fun(value: integer): integer
            ---@param value string|integer
            ---@return string|integer
            local function classify(value)
            end

            local result = classify({})
        "#,
        );

        let call_expr = ws.get_node::<LuaCallExpr>(file_id);
        let semantic_model = ws
            .analysis
            .compilation
            .get_semantic_model(file_id)
            .expect("Semantic model must exist");
        let ty = crate::semantic::infer::try_infer_expr_no_flow(
            semantic_model.get_db(),
            &mut semantic_model.get_cache().borrow_mut(),
            LuaExpr::CallExpr(call_expr),
        )
        .expect("no-flow call replay should not error");

        assert!(ty.is_none());
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

    #[test]
    fn test_issue_1071_repeated_any_member_arithmetic_loads() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            function fun()
                local a = {} ---@type any
                if a then
                    c = c + a.f_a1b2c3 / 100 * b.f_a1b2c3;
                    c = c + a.f_d4e5f6 / 100 * b.f_d4e5f6;
                    c = c + a.f_111aaa / 100 * b.f_111aaa;
                    c = c + a.f_222bbb / 100 * b.f_222bbb;
                    c = c + a.f_333ccc / 100 * b.f_333ccc;
                    c = c + a.f_444ddd / 100 * b.f_444ddd;
                    c = c + a.f_555eee / 100 * b.f_555eee;
                    c = c + a.f_666fff / 100 * b.f_666fff;
                    c = c + a.f_777000 / 100 * b.f_777000;
                    c = c + a.f_888111 / 100 * b.f_888111;
                    c = c + a.f_999222 / 100 * b.f_999222;
                    c = c + a.f_aaa333 / 100 * b.f_aaa333;
                    c = c + a.f_bbb444 / 100 * b.f_bbb444;
                    c = c + a.f_ccc555 / 100 * b.f_ccc555;
                    c = c + a.f_ddd666 * b.f_ddd666;
                    c = c + a.f_eee777 * b.f_eee777;
                    c = c + a.f_fff888 * b.f_fff888;
                    c = c + a.f_000999 * b.f_000999;
                    c = c + a.f_123abc * b.f_123abc;
                    c = c + a.f_234bcd * b.f_234bcd;
                    c = c + a.f_345cde * b.f_345cde;
                    c = c + a.f_456def * b.f_456def;
                    c = c + a.f_567ef0 * b.f_567ef0;
                    c = c + a.f_678f01 * b.f_678f01;
                    c = c + a.f_789012 * b.f_789012;
                    c = c + a.f_89a123 * b.f_89a123;
                    c = c + a.f_9ab234/100 * b.f_9ab234;
                    c = c + a.f_abd345 * b.f_abd345;
                    c = c + a.f_bce456 / 100 * b.f_bce456;
                    c = c + a.f_cdf567 / 100 * b.f_cdf567;
                    c = c + a.f_def678 / 100 * b.f_def678;
                    c = c + a.f_ef0123 / 100 * b.f_ef0123;
                    c = c + a.f_f01234 / 100 * b.f_f01234;
                end
                return c
            end
        "#,
        );

        assert_eq!(ws.expr_ty("c"), ws.ty("any"));
    }
}
