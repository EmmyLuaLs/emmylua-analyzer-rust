#[cfg(test)]
mod test {
    use crate::VirtualWorkspace;

    #[test]
    fn test_higher_order_generic_return_infer_with_return_overload() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T, R
            ---@param f fun(...: T...): R...
            ---@param ... T...
            ---@return_overload true, R...
            ---@return_overload false, string
            local function wrap(f, ...)
                return true, f(...)
            end

            ---@return integer
            local function produce()
                return 1
            end

            ok, status, payload = wrap(wrap, produce)
            "#,
        );

        assert_eq!(ws.expr_ty("ok"), ws.ty("false|true"));
        assert_eq!(ws.expr_ty("status"), ws.ty("false|string|true"));
        assert_eq!(ws.expr_ty("payload"), ws.ty("integer|string|nil"));
    }

    #[test]
    fn test_return_overload_variadic_tail_keeps_deep_slots() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T, R
            ---@param f fun(...: T...): R...
            ---@param ... T...
            ---@return_overload true, R...
            ---@return_overload false, string
            local function wrap(f, ...)
                return true, f(...)
            end

            ---@param n integer
            ---@return integer, string, boolean
            local function produce(n)
                return n, tostring(n), n > 0
            end

            ok, first, second, third = wrap(produce, 1)
            "#,
        );

        assert_eq!(ws.expr_ty("ok"), ws.ty("false|true"));
        assert_eq!(ws.expr_ty("first"), ws.ty("integer|string"));
        assert_eq!(ws.expr_ty("second"), ws.ty("string|nil"));
        assert_eq!(ws.expr_ty("third"), ws.ty("boolean|nil"));
    }

    #[test]
    fn test_return_overload_variadic_tpl_tail_pads_missing_slots_with_nil() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T, R
            ---@param f fun(...: T...): R...
            ---@param ... T...
            ---@return_overload true, R...
            ---@return_overload false, string
            local function wrap(f, ...)
                return true, f(...)
            end

            ---@param n integer
            ---@return integer, string, boolean
            local function produce(n)
                return n, tostring(n), n > 0
            end

            ok, first, second, third = wrap(produce, 1)
            "#,
        );

        assert_eq!(ws.expr_ty("third"), ws.ty("boolean|nil"));
    }

    #[test]
    fn test_return_overload_short_row_keeps_nil_in_missing_slots() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@param ok boolean
            ---@return_overload true, integer
            ---@return_overload false
            local function maybe(ok)
                if ok then
                    return true, 1
                end
                return false
            end

            local cond ---@type boolean
            status, value = maybe(cond)
            "#,
        );

        assert_eq!(ws.expr_ty("status"), ws.ty("false|true"));
        assert_eq!(ws.expr_ty("value"), ws.ty("integer|nil"));
    }

    #[test]
    fn test_return_overload_concrete_variadic_tail_keeps_unbounded_slots() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@param ok boolean
            ---@return_overload true, integer...
            ---@return_overload false, string
            local function wrap(ok)
                if ok then
                    return true, 1, 2, 3, 4
                end
                return false, "err"
            end

            local cond ---@type boolean
            status, first, second, third, fourth = wrap(cond)
            "#,
        );

        assert_eq!(ws.expr_ty("status"), ws.ty("false|true"));
        assert_eq!(ws.expr_ty("first"), ws.ty("integer|string"));
        assert_eq!(ws.expr_ty("second"), ws.ty("integer|nil"));
        assert_eq!(ws.expr_ty("third"), ws.ty("integer|nil"));
        assert_eq!(ws.expr_ty("fourth"), ws.ty("integer|nil"));
    }

    #[test]
    fn test_return_overload_partial_rows_preserve_return_docs_fallback() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@param ok boolean
            ---@return boolean
            ---@return integer|string
            ---@return_overload true, integer
            local function partial(ok)
                if ok then
                    return true, 1
                end
                return false, "err"
            end

            local cond ---@type boolean
            status, value = partial(cond)
            "#,
        );

        let status_ty = ws.expr_ty("status");
        let boolean_ty = ws.ty("boolean");
        assert!(ws.check_type(&status_ty, &boolean_ty));
        assert!(!status_ty.is_always_truthy());
        assert!(!status_ty.is_always_falsy());

        let value_ty = ws.expr_ty("value");
        let integer_or_string_ty = ws.ty("integer|string");
        let integer_ty = ws.ty("integer");
        let string_ty = ws.ty("string");
        assert!(ws.check_type(&value_ty, &integer_or_string_ty));
        assert!(ws.check_type(&value_ty, &integer_ty));
        assert!(ws.check_type(&value_ty, &string_ty));
    }

    #[test]
    fn test_return_overload_docs_merge_mixed_variadic_shapes() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@return string...
            ---@return_overload true, integer
            local function maybe()
            end

            status, value, extra = maybe()
            "#,
        );

        assert_eq!(ws.expr_ty("status"), ws.ty("string|true"));
        assert_eq!(ws.expr_ty("value"), ws.ty("integer|string"));
        assert_eq!(ws.expr_ty("extra"), ws.ty("nil|string"));
    }

    #[test]
    fn test_return_overload_fallback_docs_keep_trailing_nil_slots() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@return boolean
            ---@return_overload true, integer
            local function maybe_value()
            end

            status, value = maybe_value()
            "#,
        );

        assert_eq!(ws.expr_ty("status"), ws.ty("boolean"));
        assert_eq!(ws.expr_ty("value"), ws.ty("integer|nil"));
    }
}
