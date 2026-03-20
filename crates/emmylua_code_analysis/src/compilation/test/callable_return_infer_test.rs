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
    fn test_higher_order_return_infer_uses_callable_constraint() {
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
    fn test_apply_return_infer_prefers_informative_callback_overloads_and_keeps_param_shape() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic A, R
            ---@param f fun(x: A): R
            ---@param x A
            ---@return R
            local function apply(f, x)
                return f(x)
            end

            ---@overload fun<T>(cb: fun(): T): T
            ---@overload fun(cb: function): boolean
            local function run(cb) end

            ---@overload fun<T>(cb: fun(x: integer): T): integer
            ---@overload fun<T>(cb: fun(x: string): T): string
            ---@overload fun(cb: function): boolean
            local function classify(cb) end

            ---@overload fun<T>(cb: fun(): T): { value: T }
            ---@overload fun(cb: function): boolean
            local function wrap(cb) end

            local source ---@type table

            ---@return integer
            local function cb_concrete()
                return 1
            end

            ---@type fun(): unknown
            local cb_unknown

            ---@type fun(x: integer): unknown
            local cb_param_unknown

            ---@type fun(x: string): unknown
            local cb_param_unknown_string

            ---@param x integer
            local function cb_param_unresolved(x)
                return source.missing
            end

            ---@param x string
            local function cb_param_unresolved_string(x)
                return source.missing
            end

            local function cb_named_unresolved()
                return source.missing
            end

            run_concrete = apply(run, cb_concrete)

            run_unknown = apply(run, cb_unknown)

            run_unresolved = apply(run, function()
                return source.missing
            end)

            run_named_unresolved = apply(run, cb_named_unresolved)

            wrap_named_unresolved = apply(wrap, cb_named_unresolved)

            classify_unknown = apply(classify, cb_param_unknown)

            classify_unresolved = apply(classify, cb_param_unresolved)

            classify_string_unknown = apply(classify, cb_param_unknown_string)

            classify_string_unresolved = apply(classify, cb_param_unresolved_string)
            "#,
        );

        // The callback return is concrete, so `T` is inferred as `integer` and the generic
        // overload is more informative than the `function -> boolean` fallback.
        assert_eq!(ws.expr_ty("run_concrete"), ws.ty("integer"));

        // `fun(): unknown` matches the generic parameter shape, but its return stays opaque, so
        // the concrete `function -> boolean` fallback is the best available result.
        assert_eq!(ws.expr_ty("run_unknown"), ws.ty("boolean"));

        // An unresolved closure return should be treated the same as an explicit `unknown`
        // return, so this should resolve to the same fallback `boolean`.
        assert_eq!(ws.expr_ty("run_unresolved"), ws.ty("boolean"));

        // The named-callback path should detect the unresolved closure signature too, so this
        // should stay aligned with the inline unresolved callback case above.
        assert_eq!(ws.expr_ty("run_named_unresolved"), ws.ty("boolean"));

        // If the instantiated return still embeds the unresolved template, the generic overload
        // should be discarded and the concrete fallback should win.
        assert_eq!(ws.expr_ty("wrap_named_unresolved"), ws.ty("boolean"));

        // The callback's parameter type is known, so the generic `fun(x: integer): T` overload
        // should still win even though the callback return is only `unknown`.
        assert_eq!(ws.expr_ty("classify_unknown"), ws.ty("integer"));

        // The callback return is unresolved, but its parameter is still `integer`, so overload
        // ranking should keep using that known shape and pick the generic integer branch.
        assert_eq!(ws.expr_ty("classify_unresolved"), ws.ty("integer"));

        // The callback's parameter type is `string`, so overload selection should not fall back
        // to the first generic branch when the callback return is only `unknown`.
        assert_eq!(ws.expr_ty("classify_string_unknown"), ws.ty("string"));

        // The same `string`-parameter branch should still win when the callback return is
        // unresolved and carried through a named callback value.
        assert_eq!(ws.expr_ty("classify_string_unresolved"), ws.ty("string"));
    }
}
