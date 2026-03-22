#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_return_overload_narrow_after_not() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@generic T, E
            ---@param ok boolean
            ---@param success T
            ---@param failure E
            ---@return boolean
            ---@return T|E
            ---@return_overload true, T
            ---@return_overload false, E
            local function pick(ok, success, failure)
                if ok then
                    return true, success
                end
                return false, failure
            end

            local cond ---@type boolean
            local ok, result = pick(cond, 1, "error")

            if not ok then
                error(result)
            end

            a = result
            "#,
        );

        assert_eq!(ws.expr_ty("a"), ws.ty("integer"));
    }

    #[test]
    fn test_return_overload_reassign_clears_multi_return_mapping() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@generic T, E
            ---@param ok boolean
            ---@param success T
            ---@param failure E
            ---@return boolean
            ---@return T|E
            ---@return_overload true, T
            ---@return_overload false, E
            local function pick(ok, success, failure)
                if ok then
                    return true, success
                end
                return false, failure
            end

            local cond ---@type boolean
            local random ---@type boolean
            local ok, result = pick(cond, 1, "error")
            result = random and 1 or "override"

            if not ok then
                error(result)
            end

            f = result
            "#,
        );

        assert_eq!(ws.expr_ty("f"), ws.ty("integer|string"));
    }

    #[test]
    fn test_return_overload_narrow_with_swapped_operand_eq() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@generic T, E
            ---@param ok boolean
            ---@param success T
            ---@param failure E
            ---@return "ok"|"err"
            ---@return T|E
            ---@return_overload "ok", T
            ---@return_overload "err", E
            local function pick(ok, success, failure)
                if ok then
                    return "ok", success
                end
                return "err", failure
            end

            local cond ---@type boolean
            local tag, result = pick(cond, 1, "error")

            if "err" == tag then
                error(result)
            end

            d = result
            "#,
        );

        assert_eq!(ws.expr_ty("d"), ws.ty("integer"));
    }

    #[test]
    fn test_return_overload_narrow_with_type_guard_broad_discriminant() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@param ok boolean
            ---@return string|integer
            ---@return string|boolean
            ---@return_overload string, string
            ---@return_overload integer, boolean
            local function pick(ok)
                if ok then
                    return "ok", "value"
                end
                return 1, false
            end

            local cond ---@type boolean
            local tag, result = pick(cond)

            if type(tag) == "string" then
                string_branch = result
            else
                integer_branch = result
            end
            "#,
        );

        assert_eq!(ws.expr_ty("string_branch"), ws.ty("string"));
        assert_eq!(ws.expr_ty("integer_branch"), ws.ty("boolean"));
    }

    #[test]
    fn test_return_overload_narrow_with_swapped_type_guard_alias() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@param ok boolean
            ---@return string|integer
            ---@return string|boolean
            ---@return_overload string, string
            ---@return_overload integer, boolean
            local function pick(ok)
                if ok then
                    return "ok", "value"
                end
                return 1, false
            end

            local cond ---@type boolean
            local tag, result = pick(cond)
            local kind = type(tag)

            if "string" == kind then
                string_branch = result
            else
                integer_branch = result
            end
            "#,
        );

        assert_eq!(ws.expr_ty("string_branch"), ws.ty("string"));
        assert_eq!(ws.expr_ty("integer_branch"), ws.ty("boolean"));
    }

    #[test]
    fn test_return_overload_narrow_with_type_guard_number_matches_integer_row() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@param ok boolean
            ---@return integer|string
            ---@return integer|boolean
            ---@return_overload integer, boolean
            ---@return_overload string, integer
            local function pick(ok)
                if ok then
                    return 1, false
                end
                return "err", 2
            end

            local cond ---@type boolean
            local tag, result = pick(cond)

            if type(tag) == "number" then
                number_branch = result
            else
                string_branch = result
            end
            "#,
        );

        assert_eq!(ws.expr_ty("number_branch"), ws.ty("boolean"));
        assert_eq!(ws.expr_ty("string_branch"), ws.ty("integer"));
    }

    #[test]
    fn test_return_overload_narrow_with_mixed_rhs_calls() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@generic T, E
            ---@param ok boolean
            ---@param success T
            ---@param failure E
            ---@return boolean
            ---@return T|E
            ---@return_overload true, T
            ---@return_overload false, E
            local function pick(ok, success, failure)
                if ok then
                    return true, success
                end
                return false, failure
            end

            local cond ---@type boolean
            local left_ok, right_ok, right_result = pick(cond, "left-ok", "left-err"), pick(cond, 1, "right-err")

            if not left_ok then
                error("left failed")
            end
            a = right_result

            if not right_ok then
                error(right_result)
            end
            b = right_result
            "#,
        );

        assert_eq!(ws.expr_ty("a"), ws.ty("integer|string"));
        assert_eq!(ws.expr_ty("b"), ws.ty("integer"));
    }

    #[test]
    fn test_return_overload_late_discriminant_rebind_does_not_affect_prior_narrowing() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@generic T, E
            ---@param ok boolean
            ---@param success T
            ---@param failure E
            ---@return boolean
            ---@return T|E
            ---@return_overload true, T
            ---@return_overload false, E
            local function pick(ok, success, failure)
                if ok then
                    return true, success
                end
                return false, failure
            end

            local cond ---@type boolean
            local ok, result = pick(cond, 1, "error")

            if not ok then
                error(result)
            end

            a = result
            ok = cond
            "#,
        );

        assert_eq!(ws.expr_ty("a"), ws.ty("integer"));
    }

    #[test]
    fn test_return_overload_branch_reassign_should_not_override_join_mapping() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@generic T, E
            ---@param ok boolean
            ---@param success T
            ---@param failure E
            ---@return boolean
            ---@return T|E
            ---@return_overload true, T
            ---@return_overload false, E
            local function pick(ok, success, failure)
                if ok then
                    return true, success
                end
                return false, failure
            end

            local cond ---@type boolean
            local branch ---@type boolean

            local ok, result = pick(cond, 1, "left-err")
            if branch then
                ok, result = pick(cond, "branch-ok", false)
            end

            if not ok then
                error(result)
            end

            a = result
            "#,
        );

        assert_eq!(ws.expr_ty("a"), ws.ty("integer|string"));
    }

    #[test]
    fn test_return_overload_join_with_noncorrelated_origin_keeps_extra_type() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@param ok boolean
            ---@return_overload true, integer
            ---@return_overload false, string
            local function pick(ok)
                if ok then
                    return true, 1
                end
                return false, "err"
            end

            ---@return false
            local function as_false()
                return false
            end

            local cond ---@type boolean
            local branch ---@type boolean
            local ok, result = pick(cond)

            if branch then
                ok, result = true, as_false()
            end

            at_join = result

            if not ok then
                in_error_path = result
                error(result)
            end

            after_guard = result
            "#,
        );

        let in_error_path_ty = ws.expr_ty("in_error_path");
        assert!(ws.humanize_type(in_error_path_ty).contains("string"));
        assert_eq!(ws.expr_ty("after_guard"), ws.ty("false|integer"));
    }

    #[test]
    fn test_return_overload_branch_noncall_reassign_keeps_noncorrelated_origin() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@generic T, E
            ---@param ok boolean
            ---@param success T
            ---@param failure E
            ---@return boolean
            ---@return T|E
            ---@return_overload true, T
            ---@return_overload false, E
            local function pick(ok, success, failure)
                if ok then
                    return true, success
                end
                return false, failure
            end

            local cond ---@type boolean
            local branch ---@type boolean
            local ok, result = pick(cond, 1, "err")

            if branch then
                result = false
            end

            if not ok then
                error(result)
            end

            after_guard = result
            "#,
        );

        let after_guard_ty = ws.expr_ty("after_guard");
        let after_guard = ws.humanize_type(after_guard_ty);
        assert!(after_guard.contains("false"));
        assert!(after_guard.contains("integer"));
        assert!(!after_guard.contains("string"));
    }

    #[test]
    fn test_return_overload_direct_discriminant_rebind_after_join_breaks_correlation() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@generic T, E
            ---@param ok boolean
            ---@param success T
            ---@param failure E
            ---@return boolean
            ---@return T|E
            ---@return_overload true, T
            ---@return_overload false, E
            local function pick(ok, success, failure)
                if ok then
                    return true, success
                end
                return false, failure
            end

            local cond ---@type boolean
            local branch ---@type boolean
            local ok, result = pick(cond, 1, "err")

            if branch then
                local noop = 1
            end

            ok = true

            if not ok then
                error(result)
            end

            after_guard = result
            "#,
        );

        assert_eq!(ws.expr_ty("after_guard"), ws.ty("integer|string"));
    }

    #[test]
    fn test_swapped_literal_eq_narrow_without_return_overload() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@return "x"
            local function test()
                local a ---@type "x"|nil
                if "x" == a then
                    return a
                end
                return "x"
            end
            "#,
        ));
    }

    #[test]
    fn test_var_eq_var_narrow_right_operand_without_return_overload() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@return "x"
            local function test()
                local a ---@type "x"
                local b ---@type "x"|nil
                if a == b then
                    return b
                end
                return "x"
            end
            "#,
        ));
    }

    #[test]
    fn test_return_overload_nested_clear_keeps_noncorrelated_origin() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@generic T, E
            ---@param ok boolean
            ---@param success T
            ---@param failure E
            ---@return boolean
            ---@return T|E
            ---@return_overload true, T
            ---@return_overload false, E
            local function pick(ok, success, failure)
                if ok then
                    return true, success
                end
                return false, failure
            end

            local cond ---@type boolean
            local branch ---@type boolean
            local inner ---@type boolean
            local ok, result = pick(cond, 1, "err")

            if branch then
                if inner then
                    result = false
                end
            end

            if not ok then
                error(result)
            end

            after_guard = result
            "#,
        );

        let after_guard_ty = ws.expr_ty("after_guard");
        let after_guard = ws.humanize_type(after_guard_ty);
        assert!(after_guard.contains("false"));
        assert!(after_guard.contains("integer"));
        assert!(!after_guard.contains("string"));
    }
}
