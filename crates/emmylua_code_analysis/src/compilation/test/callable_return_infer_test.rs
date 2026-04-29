#[cfg(test)]
mod test {
    use crate::VirtualWorkspace;

    #[test]
    fn test_higher_order_generic_return_infer() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T, R
            ---@param f fun(...: T...): R...
            ---@param ... T...
            ---@return boolean, R...
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

        assert_eq!(ws.expr_ty("ok"), ws.ty("boolean"));
        assert_eq!(ws.expr_ty("status"), ws.ty("boolean"));
        assert_eq!(ws.expr_ty("payload"), ws.ty("integer"));
    }

    #[test]
    fn test_higher_order_return_infer_keeps_concrete_callable_result() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T, R
            ---@param f fun(...: T...): R...
            ---@param ... T...
            ---@return boolean, R...
            local function wrap(f, ...)
                return true, f(...)
            end

            ---@param x integer
            ---@return integer
            local function take_int(x)
                return x
            end

            ---@class Box
            ---@field value integer
            local box

            ok, payload = wrap(take_int, box.missing)
            "#,
        );

        assert_eq!(ws.expr_ty("ok"), ws.ty("boolean"));
        assert_eq!(ws.expr_ty("payload"), ws.ty("integer"));
    }

    #[test]
    fn test_higher_order_return_infer_uses_callable_constraint_fallback() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T, R
            ---@param f fun(...: T...): R
            ---@param ... T...
            ---@return R
            local function call_once(f, ...)
                return f(...)
            end

            ---@generic U: string
            ---@param n integer
            ---@return U
            local function constrained_return(n)
            end

            result = call_once(constrained_return, 1)
            "#,
        );

        assert_eq!(ws.expr_ty("result"), ws.ty("string"));
    }

    #[test]
    fn test_higher_order_return_infer_uses_callable_default() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T, R
            ---@param f fun(...: T...): R
            ---@param ... T...
            ---@return R
            local function call_once(f, ...)
                return f(...)
            end

            ---@generic U = string
            ---@param n integer
            ---@return U
            local function default_return(n)
            end

            result = call_once(default_return, 1)
            "#,
        );

        assert_eq!(ws.expr_ty("result"), ws.ty("string"));
    }
}
