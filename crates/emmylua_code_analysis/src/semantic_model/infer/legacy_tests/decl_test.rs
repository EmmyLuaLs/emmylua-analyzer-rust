#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_1() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@return any ...
        ---@return integer offset
        local function unpack() end
        a, b, c, d = unpack()
        "#,
        );

        assert_eq!(ws.expr_ty("a"), ws.ty("any"));
        assert_eq!(ws.expr_ty("b"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("c"), ws.ty("nil"));
        assert_eq!(ws.expr_ty("d"), ws.ty("nil"));
    }

    #[test]
    fn test_2() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@return integer offset
        ---@return any ...
        local function unpack() end
        a, b, c, d = unpack()
        "#,
        );

        assert_eq!(ws.expr_ty("a"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("b"), ws.ty("any"));
        assert_eq!(ws.expr_ty("c"), ws.ty("any"));
        assert_eq!(ws.expr_ty("d"), ws.ty("any"));
    }

    #[test]
    fn test_3() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@return any ...
                ---@return integer offset
                local function unpack() end

                ---@param a nil|integer|'l'|'L'
                local function test(a) end
                local len = unpack()
                test(len)
        "#,
        ));
    }

    #[test]
    fn test_repeat_closure_in_until_can_access_body_locals() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::UndefinedGlobal,
            r#"
                repeat
                    local x = 1
                until (function() return x end)() > 0
            "#,
        ));
    }

    #[test]
    fn test_repeat_closure_in_body_can_access_body_locals() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::UndefinedGlobal,
            r#"
                repeat
                    local x = 1
                    local f = function() return x + 1 end
                until f() > 0
            "#,
        ));
    }

    #[test]
    fn test_repeat_body_local_visible_in_until() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::UndefinedGlobal,
            r#"
                repeat
                    local x = 1
                until x > 0
            "#,
        ));
    }

    #[test]
    fn test_repeat_closure_in_until_type_infer() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@param n integer
                local function check(n) end
                repeat
                    local x = 1
                until (function()
                    check(x)
                    return x
                end)() > 0
            "#,
        ));
    }

    #[test]
    fn test_repeat_body_local_not_visible_after_until() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::UndefinedGlobal,
            r#"
                repeat
                    local x = 12
                until false
                print(x)
            "#,
        ));
    }

    #[test]
    fn test_do_block_local_not_visible_outside() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::UndefinedGlobal,
            r#"
                do
                    local x = 1
                end
                print(x)
            "#,
        ));
    }

    #[test]
    fn test_local_assignment_closure_cannot_see_self() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::UndefinedGlobal,
            r#"
                local x = function()
                    return x
                end
            "#,
        ));
    }

    #[test]
    fn test_local_function_closure_can_see_self() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::UndefinedGlobal,
            r#"
                local function x()
                    return x
                end
            "#,
        ));
    }
}
