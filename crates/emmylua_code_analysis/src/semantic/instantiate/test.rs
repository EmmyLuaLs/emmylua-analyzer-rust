#[cfg(test)]
mod test {
    #[test]
    fn test_variadic_func() {
        let mut ws = crate::VirtualWorkspace::new();
        ws.def(r#"
        ---@generic T, R
        ---@param call async fun(...: T...): R...
        ---@return async fun(...: T...): R...
        function async_create(call)

        end


        ---@param a number
        ---@param b string
        ---@param c boolean
        ---@return number
        function locaf(a, b, c)
            
        end
        "#);

        let ty = ws.expr_ty("async_create(locaf)");
        let expected = ws.ty("async fun(a: number, b: string, c:boolean): number");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_select_type() {
        let mut ws = crate::VirtualWorkspace::new_with_init_std_lib();
        ws.def(r#"
        ---@param ... string
        function ffff(...)
            a, b, c = select(2, ...)
        end
        "#);

        let a_ty = ws.expr_ty("a");
        let b_ty = ws.expr_ty("b");
        let c_ty = ws.expr_ty("c");
        let expected = ws.ty("string");
        assert_eq!(a_ty, expected);
        assert_eq!(b_ty, expected);
        assert_eq!(c_ty, expected);

        ws.def(r#"
        e, f = select(2, "a", "b", "c")
        "#);

        let e = ws.expr_ty("e");
        let expected = ws.expr_ty("'b'");
        let f = ws.expr_ty("f");
        let expected_f = ws.expr_ty("'c'");
        assert_eq!(e, expected);
        assert_eq!(f, expected_f);

        ws.def(r#"
        h = select('#', "a", "b")
        "#);

        let h = ws.expr_ty("h");
        let expected = ws.expr_ty("2");
        assert_eq!(h, expected);
    }

    #[test]
    fn test_unpack() {
        let mut ws = crate::VirtualWorkspace::new_with_init_std_lib();

        ws.def(r#"
        local h ---@type number[]
        a, b, c = table.unpack(h)
        "#);

        let a = ws.expr_ty("a");
        let expected = ws.ty("number");
        let b = ws.expr_ty("b");
        let expected_b = ws.ty("number");
        let c = ws.expr_ty("c");
        let expected_c = ws.ty("number");
        assert_eq!(a, expected);
        assert_eq!(b, expected_b);
        assert_eq!(c, expected_c);
    }
}
