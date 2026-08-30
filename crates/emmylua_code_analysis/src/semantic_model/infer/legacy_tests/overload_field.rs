#[cfg(test)]
mod test {
    use crate::VirtualWorkspace;

    #[test]
    fn test_overload_field() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@class MyClass
        ---@field event fun(a: string): string
        ---@field event fun(b: number): number
        ---@field f number
        x = {}
        "#,
        );

        let string_ty = ws.expr_ty("x.event('hello')");
        let expected_string = ws.ty("string");
        assert_eq!(string_ty, expected_string);
        let number_ty = ws.expr_ty("x.event(123)");
        let expected_number = ws.ty("number");
        assert_eq!(number_ty, expected_number);
    }
}
