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
    fn test_apply_return_infer_prefers_structural_callback_over_function_fallback() {
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
            ---@param cb function
            ---@return boolean
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

        // `function` is an erased fallback. A structural `fun(): unknown` callback should keep
        // the generic overload and preserve the unknown return.
        assert_eq!(ws.expr_ty("run_unknown"), ws.ty("unknown"));

        // An unresolved closure return is treated the same as an explicit `unknown` return.
        assert_eq!(ws.expr_ty("run_unresolved"), ws.ty("unknown"));

        // The named-callback path should stay aligned with the inline unresolved callback case.
        assert_eq!(ws.expr_ty("run_named_unresolved"), ws.ty("unknown"));

        // The structural overload still wins when the unknown is nested in the return shape.
        assert_eq!(
            ws.expr_ty("wrap_named_unresolved"),
            ws.ty("{ value: unknown }")
        );

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

    #[test]
    fn test_apply_return_infer_leaves_result_unknown_when_no_callable_member_matches_arg_shape() {
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

            ---@alias FnInt fun(x: integer): integer
            ---@alias FnString fun(x: string): string

            ---@type FnInt | FnString
            local run

            ---@type boolean
            local b

            result = apply(run, b)
            "#,
        );

        let result_ty = ws.expr_ty("result");
        assert_eq!(result_ty, ws.ty("unknown"));
    }

    #[test]
    fn test_apply_return_infer_uses_function_fallback_when_no_structural_overload_matches() {
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

            ---@param cb function
            ---@return boolean
            local function run(cb) end

            ---@type fun(): unknown
            local cb

            result = apply(run, cb)
            "#,
        );

        assert_eq!(ws.expr_ty("result"), ws.ty("boolean"));
    }

    #[test]
    fn test_apply_return_infer_keeps_only_arity_compatible_fallbacks() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic A, B, R
            ---@param f fun(x: A, y: B): R
            ---@param x A
            ---@param y B
            ---@return R
            local function apply2(f, x, y)
                return f(x, y)
            end

            ---@overload fun(x: integer): integer
            ---@param x integer
            ---@param y string
            ---@return string
            local function run(x, y) end

            local source ---@type table

            result = apply2(run, 1, source.missing)
            "#,
        );

        let result_ty = ws.expr_ty("result");
        assert_eq!(ws.humanize_type(result_ty), "string");
    }

    #[test]
    fn test_apply_return_infer_keeps_same_arity_overload_returns_when_tail_is_unknown() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic A, B, R
            ---@param f fun(x: A, y: B): R
            ---@param x A
            ---@param y B
            ---@return R
            local function apply2(f, x, y)
                return f(x, y)
            end

            ---@overload fun(x: integer, y: number): number
            ---@param x integer
            ---@param y string
            ---@return string
            local function run(x, y) end

            local source ---@type table

            result = apply2(run, 1, source.missing)
            "#,
        );

        let result_ty = ws.expr_ty("result");
        assert_eq!(result_ty, ws.ty("number|string"));
    }

    #[test]
    fn test_apply_return_infer_keeps_unknown_return_when_arg_shape_is_unknown() {
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

            ---@overload fun(x: integer): unknown
            ---@param x string
            ---@return string
            local function run(x) end

            local source ---@type table

            result = apply(run, source.missing)
            "#,
        );

        let result_ty = ws.expr_ty("result");
        assert_eq!(result_ty, ws.ty("unknown|string"));
    }

    #[test]
    fn test_union_call_ignores_non_matching_generic_callable_member() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@type (fun<T: string>(x: T): T) | fun(x: integer): integer
            local run

            result = run(1)
            "#,
        );

        let result_ty = ws.expr_ty("result");
        assert_eq!(ws.humanize_type(result_ty), "integer");
    }

    #[test]
    fn test_union_call_ignores_non_matching_generic_alias_member() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias GenericStr<T: string> fun(x: T): T

            ---@type GenericStr | fun(x: integer): integer
            local run

            result = run(1)
            "#,
        );

        let result_ty = ws.expr_ty("result");
        assert_eq!(ws.humanize_type(result_ty), "integer");
    }

    #[test]
    fn test_direct_callable_union_unions_same_domain_returns() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias FnA fun(x: integer): integer
            ---@alias FnB fun(x: integer): boolean

            ---@type FnA | FnB
            local run

            result = run(1)
            "#,
        );

        assert_eq!(ws.expr_ty("result"), ws.ty("integer|boolean"));
    }

    #[test]
    fn test_plain_function_call_returns_unknown_values() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@type function
            local f

            a, b = f(1)
            "#,
        );

        assert_eq!(ws.expr_ty("a"), ws.ty("unknown"));
        assert_eq!(ws.expr_ty("b"), ws.ty("unknown"));
    }
}
