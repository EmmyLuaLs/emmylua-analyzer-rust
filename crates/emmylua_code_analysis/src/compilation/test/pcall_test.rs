#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, LuaType, VirtualWorkspace};

    const STACKED_PCALL_ALIAS_GUARDS: usize = 180;

    #[test]
    fn test_issue_263() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@alias aaa fun(a: string, b: integer): integer

        ---@type aaa
        local a

        d, b = pcall(a, "", 1)
        "#,
        );

        let aaa_ty = ws.expr_ty("b");
        let expected = ws.ty("integer|string");
        assert_eq!(aaa_ty, expected);
    }

    #[test]
    fn test_issue_280() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
        ---@class D11.AAA
        local AAA = {}

        ---@param a string
        ---@param b number
        function AAA:name(a, b)
        end

        ---@param a string
        ---@param b number
        function AAA:t(a, b)
            local ok, err = pcall(self.name, self, a, b)
        end
        "#
        ));
    }

    #[test]
    fn test_nested_pcall_higher_order_return_shape() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@return integer
        local function f()
            return 1
        end

        ok, status, payload = pcall(pcall, f)
        "#,
        );

        assert_eq!(ws.expr_ty("status"), ws.ty("true|false|string"));
        assert_eq!(ws.expr_ty("payload"), ws.ty("string|integer|nil"));
    }

    #[test]
    fn test_pcall_return_overload_narrow_after_error_guard() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
            ---@return integer
            local function foo()
                return 2
            end

            local ok, result = pcall(foo)

            if not ok then
                error(result)
            end

            a = result
            "#,
        );

        assert_eq!(ws.expr_ty("a"), ws.ty("integer"));
    }

    #[test]
    fn test_nested_pcall_like_without_return_overload() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@generic T, R
        ---@param f fun(...: T...): R...
        ---@param ... T...
        ---@return boolean, R...
        local function safe_call(f, ...)
            return true, f(...)
        end

        ---@return integer
        local function produce()
            return 1
        end

        ok, status, payload = safe_call(safe_call, produce)
        "#,
        );

        assert_eq!(ws.expr_ty("status"), ws.ty("boolean"));
        assert_eq!(ws.expr_ty("payload"), ws.ty("integer"));
    }

    #[test]
    fn test_nested_pcall_like_without_return_overload2() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@generic T, R, R1
        ---@param f sync fun(...: T...): R1, R...
        ---@param ... T...
        ---@return boolean, R1|string, R...
        local function pcall_like(f, ...) end

        ---@return integer
        local function produce()
            return 1
        end

        ok, status, payload = pcall_like(pcall_like, produce)
        "#,
        );

        assert_eq!(ws.expr_ty("ok"), ws.ty("boolean"));
        assert_eq!(ws.expr_ty("status"), ws.ty("boolean|string"));
        assert_eq!(ws.expr_ty("payload"), ws.ty("integer|string"));
    }

    #[test]
    fn test_pcall_stacked_alias_guards_build_semantic_model() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let repeated_guards =
            "if failed then error(result) end\n".repeat(STACKED_PCALL_ALIAS_GUARDS);
        let block = format!(
            r#"
        ---@return integer
        local function foo()
            return 1
        end

        local ok, result = pcall(foo)
        local failed = ok == false

        {repeated_guards}
        narrowed = result
        "#,
        );

        let file_id = ws.def(&block);

        assert!(
            ws.analysis
                .compilation
                .get_semantic_model(file_id)
                .is_some(),
            "expected semantic model for stacked pcall alias guard repro"
        );
        assert_eq!(ws.expr_ty("narrowed"), ws.ty("integer"));
    }

    #[test]
    fn test_pcall_alias_callee_narrows_return_after_error_guard() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@return integer
        local function foo()
            return 1
        end

        local runner = pcall
        local ok, result = runner(foo)

        if not ok then
            error(result)
        end

        narrowed = result
        "#,
        );

        assert_eq!(ws.expr_ty("narrowed"), ws.ty("integer"));
    }

    #[test]
    fn test_pcall_any_callable_splits_success_unknown_and_failure_string() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@type any
        local x

        local ok, result = pcall(x)
        outside = result
        if ok then
            success = result
        else
            failure = result
        end
        "#,
        );

        assert_eq!(ws.expr_ty("outside"), ws.ty("unknown|string"));
        assert_eq!(ws.expr_ty("success"), ws.ty("unknown"));
        assert_eq!(ws.expr_ty("failure"), ws.ty("string"));
    }

    #[test]
    fn test_issue_1020_pcall_preserves_pairs_value_function_return() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        local t = {
            { id = 1, func = function() return a > 0 end },
            { id = 2, func = function() return a > 0 end },
            { id = 3, func = function() return a > 0 end },
        }

        for _, v in pairs(t) do
            local f = v.func
            captured_f = f
            local success, result = pcall(f)
            outside = result
            if success then
                success_result = result
            else
                failure_result = result
            end
        end
        "#,
        );

        let captured_f = ws.expr_ty("captured_f");
        let outside = ws.expr_ty("outside");
        let success_result = ws.expr_ty("success_result");
        let failure_result = ws.expr_ty("failure_result");
        assert_eq!(ws.humanize_type(captured_f), "fun() -> boolean");
        assert_eq!(ws.humanize_type(outside), "(boolean|string)");
        assert_eq!(ws.humanize_type(success_result), "boolean");
        assert_eq!(ws.humanize_type(failure_result), "string");
    }

    #[test]
    fn test_return_overload_unions_callable_union_members_with_same_domain() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@alias FnA fun(x: integer): integer
        ---@alias FnB fun(x: integer): boolean

        ---@type FnA | FnB
        local run

        _, a = pcall(run, 1)
        "#,
        );

        assert_eq!(
            ws.expr_ty("a"),
            LuaType::from_vec(vec![LuaType::Integer, LuaType::Boolean, LuaType::String])
        );
    }

    #[test]
    fn test_return_overload_callable_union_is_not_order_dependent() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@alias FnA fun(x: integer): integer
        ---@alias FnB fun(x: integer): boolean

        ---@type FnB | FnA
        local run

        _, a = pcall(run, 1)
        "#,
        );

        assert_eq!(
            ws.expr_ty("a"),
            LuaType::from_vec(vec![LuaType::Integer, LuaType::Boolean, LuaType::String])
        );
    }

    #[test]
    fn test_return_overload_unions_callable_intersection_members_with_same_domain() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@alias FnA fun(x: integer): integer
        ---@alias FnB fun(x: integer): boolean

        ---@type FnA & FnB
        local run

        _, a = pcall(run, 1)
        "#,
        );

        assert_eq!(
            ws.expr_ty("a"),
            LuaType::from_vec(vec![LuaType::Integer, LuaType::Boolean, LuaType::String])
        );
    }

    #[test]
    fn test_return_overload_narrows_callable_union_member_by_arg_shape() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@alias FnA fun(x: integer): integer
        ---@alias FnB fun(x: string): integer

        ---@type FnA | FnB
        local run

        _, a = pcall(run, 1)
        "#,
        );

        assert_eq!(ws.expr_ty("a"), ws.ty("integer|string"));
    }

    #[test]
    fn test_return_overload_narrows_callable_intersection_member_by_arg_shape() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@alias FnA fun(x: integer): integer
        ---@alias FnB fun(x: string): boolean

        ---@type FnA & FnB
        local run

        _, a = pcall(run, 1)
        "#,
        );

        assert_eq!(ws.expr_ty("a"), ws.ty("integer|string"));
    }
}
