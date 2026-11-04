#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_pcall_return() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        a, b = pcall(string.rep, "a", 1000000000)
        "#,
        );

        let ty = ws.expr_ty("b");
        let expected = ws.ty("string");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_unpack_return() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        a, b, c = table.unpack({1, 2, 3})
        "#,
        );

        let a_ty = ws.expr_ty("a");
        let b_ty = ws.expr_ty("b");
        let c_ty = ws.expr_ty("c");
        let a_expected = ws.expr_ty("1");
        let b_expected = ws.expr_ty("2");
        let c_expected = ws.expr_ty("3");
        assert_eq!(a_ty, a_expected);
        assert_eq!(b_ty, b_expected);
        assert_eq!(c_ty, c_expected);
    }

    #[test]
    fn test_assert_return() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@return string?
        ---@return string?
        function cwd() end

        a, b = assert(cwd())
        "#,
        );

        let a_ty = ws.expr_ty("a");
        let b_ty = ws.expr_ty("b");
        let a_expected = ws.ty("string");
        let b_expected = ws.ty("string?");
        assert_eq!(a_ty, a_expected);
        assert_eq!(b_ty, b_expected);
    }

    #[test]
    fn test_issue_237() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::UnbalancedAssignments,
            r#"
        local fmt = ""
        local scol, ecol, match, key, time_fmt = fmt:find('(<([^:>]+):?([^>]*)>)')
        "#,
        ));
    }

    #[test]
    fn test_issue_244() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
        ---@return string
        local function foo()
            return "ok"
        end
        ok, err = pcall(foo)
        "#,
        );

        let err_ty = ws.expr_ty("err");
        let expected = ws.ty("string");
        assert_eq!(err_ty, expected);
    }

    #[test]
    fn test_issue_342() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        function bar()
            local a, b = foo()
            e = b
        end

        --- @return string, integer
        function foo() end
        "#,
        );

        let e_ty = ws.expr_ty("e");
        let expected = ws.ty("integer");
        assert_eq!(e_ty, expected);
    }

    /// A call whose argument mismatches its parameter still assigns its returns slot by slot,
    /// never the whole return list to every target.
    #[test]
    fn test_mismatched_call_assigns_return_slots() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@param y number
        ---@return string text
        ---@return number count
        local function two(y) return 'a', 1 end

        ---@param y number
        ---@return string text
        local function one(y) return 'a' end

        local a1, b1 = two('x')
        local a2, b2 = one('x')
        A1, B1, A2, B2 = a1, b1, a2, b2
        "#,
        );

        assert_eq!(ws.expr_ty("A1"), ws.ty("string"));
        assert_eq!(ws.expr_ty("B1"), ws.ty("number"));
        assert_eq!(ws.expr_ty("A2"), ws.ty("string"));
        assert_eq!(ws.expr_ty("B2"), ws.ty("any"));
    }

    /// A field assigned from a multi-return local has that local's slot type, not the whole list.
    #[test]
    fn test_field_from_multi_return_local_has_slot_type() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@return any result
        ---@return string? err
        ---@return string output
        local function captured() end

        local result, err, output = captured()
        local payload = {}
        payload.output = output
        P = payload.output
        "#,
        );

        assert_eq!(ws.expr_ty("P"), ws.ty("string"));
    }

    /// The database's slot rule for every shape of result list, reached through mismatched calls.
    #[test]
    fn test_mismatched_call_slots_by_result_shape() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@param y number
        ---@return string a
        ---@return number b
        ---@return boolean c
        local function three(y) end

        ---@param y number
        ---@return string a
        ---@return number b
        local function two(y) end

        ---@param y number
        ---@return string a
        ---@return number ...
        local function open_tail(y) end

        ---@param y number
        ---@return number ...
        local function repeated(y) end

        ---@param y number
        ---@return number...
        local function variadic(y) end

        ---@param y number
        ---@return [string, number]...
        local function tuple_variadic(y) end

        ---@param y number
        ---@return [string, number]
        local function tuple(y) end

        ---@param y number
        ---@param ... number
        local function forwards(y, ...) return true, ... end

        local _, middle, last = three('x')
        local _, _, past_end = two('x')
        local _, tail1, tail2 = open_tail('x')
        local _, repeated2 = repeated('x')
        local _, variadic2 = variadic('x')
        local _, tuple_variadic2 = tuple_variadic('x')
        local _, past_tuple = tuple('x')
        local _, _, forwarded = forwards('x')
        MIDDLE, LAST, PAST_END, TAIL1, TAIL2 = middle, last, past_end, tail1, tail2
        REPEATED2, VARIADIC2, TUPLE_VARIADIC2 = repeated2, variadic2, tuple_variadic2
        PAST_TUPLE, FORWARDED = past_tuple, forwarded
        "#,
        );

        assert_eq!(ws.expr_ty("MIDDLE"), ws.ty("number"));
        assert_eq!(ws.expr_ty("LAST"), ws.ty("boolean"));
        assert_eq!(ws.expr_ty("PAST_END"), ws.ty("nil"));
        assert_eq!(ws.expr_ty("TAIL1"), ws.ty("number"));
        assert_eq!(ws.expr_ty("TAIL2"), ws.ty("number"));
        assert_eq!(ws.expr_ty("REPEATED2"), ws.ty("number"));
        assert_eq!(ws.expr_ty("VARIADIC2"), ws.ty("number"));
        assert_eq!(ws.expr_ty("TUPLE_VARIADIC2"), ws.ty("number"));
        assert_eq!(ws.expr_ty("PAST_TUPLE"), ws.ty("any"));
        assert_eq!(ws.expr_ty("FORWARDED"), ws.ty("number"));
    }
}
