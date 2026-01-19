#[cfg(test)]
mod test {
    use crate::VirtualWorkspace;

    #[test]
    fn test_simple_infer_through_generic_func() {
        // First verify that basic infer through generic function works
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Identity<T> T extends infer P and P or never

            ---@generic T
            ---@param v T
            ---@return Identity<T>
            function identity(v) end

            Z = identity("hello")
            "#,
        );

        let z_ty = ws.expr_ty("Z");
        // Should be "string" if basic infer works through generic functions
        assert_eq!(ws.humanize_type(z_ty), "string");
    }

    #[test]
    fn test_object_literal_infer_basic() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias ExtractFoo<T> T extends { foo: infer F } and F or never

            ---@generic T
            ---@param v T
            ---@return ExtractFoo<T>
            function extractFoo(v) end

            ---@type { foo: string, bar: number }
            local myTable

            A = extractFoo(myTable)
            "#,
        );

        let a_ty = ws.expr_ty("A");
        assert_eq!(ws.humanize_type(a_ty), "string");
    }

    #[test]
    fn test_object_literal_infer_from_class() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias ExtractFoo<T> T extends { foo: infer F } and F or never

            ---@class MyClass
            ---@field foo number
            ---@field bar string

            ---@generic T
            ---@param v T
            ---@return ExtractFoo<T>
            function extractFoo(v) end

            ---@type MyClass
            local myObj

            B = extractFoo(myObj)
            "#,
        );

        let b_ty = ws.expr_ty("B");
        assert_eq!(ws.humanize_type(b_ty), "number");
    }

    #[test]
    fn test_object_literal_infer_constructor_params_multiple() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias ConstructorParams<T> T extends { constructor: fun(self: any, ...: infer P): any } and P or never

            ---@class Widget
            ---@field constructor fun(self: Widget, name: string, width: number): Widget

            ---@generic T
            ---@param v T
            ---@return ConstructorParams<T>
            function getParams(v) end

            ---@type Widget
            local widget

            C = getParams(widget)
            "#,
        );

        let c_ty = ws.expr_ty("C");
        // Should be a tuple of the inferred parameters
        assert_eq!(ws.humanize_type(c_ty), "(string,number)");
    }

    #[test]
    fn test_object_literal_infer_constructor_params_single() {
        // Test that single parameter constructors also return a tuple for consistent spreading
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias ConstructorParams<T> T extends { constructor: fun(self: any, ...: infer P): any } and P or never

            ---@class SimpleWidget
            ---@field constructor fun(self: SimpleWidget, name: string): SimpleWidget

            ---@generic T
            ---@param v T
            ---@return ConstructorParams<T>
            function getParams(v) end

            ---@type SimpleWidget
            local widget

            D = getParams(widget)
            "#,
        );

        let d_ty = ws.expr_ty("D");
        // Single parameter should also be a tuple for consistent variadic spreading
        // This ensures `fun(...: ConstructorParams<T>...)` works correctly
        assert_eq!(ws.humanize_type(d_ty), "(string)");
    }

    #[test]
    fn test_object_literal_infer_nested() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias ExtractNested<T> T extends { outer: { inner: infer I } } and I or never

            ---@generic T
            ---@param v T
            ---@return ExtractNested<T>
            function extractNested(v) end

            ---@type { outer: { inner: boolean } }
            local nested

            D = extractNested(nested)
            "#,
        );

        let d_ty = ws.expr_ty("D");
        assert_eq!(ws.humanize_type(d_ty), "boolean");
    }

    #[test]
    fn test_object_literal_infer_no_match() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias ExtractFoo<T> T extends { foo: infer F } and F or never

            ---@generic T
            ---@param v T
            ---@return ExtractFoo<T>
            function extractFoo(v) end

            ---@type { bar: string }
            local noFoo

            E = extractFoo(noFoo)
            "#,
        );

        let e_ty = ws.expr_ty("E");
        assert_eq!(ws.humanize_type(e_ty), "never");
    }

    #[test]
    fn test_object_literal_infer_function_field() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias ExtractCallback<T> T extends { callback: infer C } and C or never

            ---@generic T
            ---@param v T
            ---@return ExtractCallback<T>
            function extractCallback(v) end

            ---@type { callback: fun(x: number): string }
            local obj

            F = extractCallback(obj)
            "#,
        );

        let f_ty = ws.expr_ty("F");
        assert_eq!(ws.humanize_type(f_ty), "fun(x: number) -> string");
    }

    #[test]
    fn test_object_literal_infer_true_variadic_params() {
        // Test that true variadic functions (fun(self, ...: T)) preserve variadic behavior
        // This should NOT be wrapped in a tuple - it should stay as the base type
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias ExtractVariadic<T> T extends { handler: fun(self: any, ...: infer P): any } and P or never

            ---@class VariadicWidget
            ---@field handler fun(self: VariadicWidget, ...: string): VariadicWidget

            ---@generic T
            ---@param v T
            ---@return ExtractVariadic<T>
            function getVariadicType(v) end

            ---@type VariadicWidget
            local widget

            V = getVariadicType(widget)
            "#,
        );

        let v_ty = ws.expr_ty("V");
        // True variadic should return the base type (not wrapped in tuple)
        // so that variadic spreading continues to work as expected
        assert_eq!(ws.humanize_type(v_ty), "string");
    }
}
