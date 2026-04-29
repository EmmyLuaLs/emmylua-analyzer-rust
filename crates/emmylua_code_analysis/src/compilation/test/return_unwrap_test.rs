#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, LuaType, VirtualWorkspace};

    #[test]
    fn test_issue_376() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@return any
        function get() end

        local sub = get()

        if sub and type(sub) == 'table' then
            -- sub is nil - wrong
            a = sub
        end
        "#,
        );

        let a_ty = ws.expr_ty("a");
        let expected = LuaType::Table;
        assert_eq!(a_ty, expected);
    }

    #[test]
    fn test_issue_476() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            crate::DiagnosticCode::ParamTypeMismatch,
            r#"
        ---Converts hex to char
        ---@param hex string
        ---@return string
        function hex_to_char2(hex)
            return string.char(assert(tonumber(hex, 16)))
        end
        "#,
        ));
    }

    #[test]
    fn test_issue_659() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
        --- @async
        --- @generic R
        --- @param fn fun(): R...
        --- @return R...
        function wrap(fn) end

        ---@async
        --- @param a {}?
        --- @return {}?
        --- @return string? err
        function get(a)
            return wrap(function()
                if not a then
                    return nil, 'err'
                end

                return a
            end)
        end
        "#,
        ));
    }

    #[test]
    fn test_issue_643() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            local function foo(b)
                if not b then
                    return
                end
                return 'a', 1
            end
            --- @type 'a'?
            local _ = foo()
        "#,
        ));
    }
}
