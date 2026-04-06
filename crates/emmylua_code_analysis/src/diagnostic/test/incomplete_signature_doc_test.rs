#[cfg(test)]
mod tests {

    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_290() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_full_diagnostic();

        assert!(ws.check_code_for(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
                ---@param a string
                local function foo(_, a)
                    _ = a
                end
            "#
        ));
    }

    #[test]
    fn test_return() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_full_diagnostic();

        assert!(!ws.check_code_for(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            local c = function(x, y)
                return x + y
            end
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            local function do_add(x, y)
                return x + y
            end
            "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            local function noop()
            end
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            ---@param p number
            local function FLPR3(p, e)
                return 0
            end
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            ---@param p number
            local function FLPR3(p)
                return 0
            end
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            --- function without param signature
            local function FLPR3(p)
                return 0
            end
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"

                ---@class Test
                local Test = {}

                ---@param test Test
                function Test:add(test, c)
                end
            "#
        ));
    }

    #[test]
    fn test_return_overload() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_full_diagnostic();

        assert!(ws.check_code_for(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            ---@return_overload true, integer
            ---@return_overload false, string
            local function f()
                return true, 1
            end
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            ---@return_overload true, integer
            ---@return_overload false, string
            local function f()
                return true, 1, "extra"
            end
            "#
        ));
    }

    #[test]
    fn test_variadic_return_overload_does_not_trigger_incomplete_signature_doc() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_full_diagnostic();

        assert!(ws.check_code_for(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            ---@return_overload true, integer...
            ---@return_overload false, string
            local function f()
                return true, 1, 2, 3, 4
            end
            "#
        ));
    }

    #[test]
    fn test_global() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_full_diagnostic();

        assert!(!ws.check_code_for(
            DiagnosticCode::MissingGlobalDoc,
            r#"
                function FLPR1()
                end
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::MissingGlobalDoc,
            r#"
                ---
                function FLPR1(a)
                end
            "#
        ));
        assert!(!ws.check_code_for(
            DiagnosticCode::MissingGlobalDoc,
            r#"
                ---
                function FLPR1()
                    return 1
                end
            "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::MissingGlobalDoc,
            r#"
                ---
                function FLPR2()
                end
            "#
        ));
    }

    #[test]
    fn test_incomplete_signature_doc_no_param_no_return() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_full_diagnostic();

        assert!(ws.check_code_for(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
                local function a()
                end
            "#
        ));
    }
}
