#[cfg(test)]
mod tests {

    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_290() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_full_diagnostic();

        assert!(ws.has_no_diagnostic(
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

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            local c = function(x, y)
                return x + y
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            local function do_add(x, y)
                return x + y
            end
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            local function noop()
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            ---@param p number
            local function FLPR3(p, e)
                return 0
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            ---@param p number
            local function FLPR3(p)
                return 0
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            --- function without param signature
            local function FLPR3(p)
                return 0
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
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

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            ---@return_overload true, integer
            ---@return_overload false, string
            local function f()
                return true, 1
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
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

        assert!(ws.has_no_diagnostic(
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

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingGlobalDoc,
            r#"
                function FLPR1()
                end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingGlobalDoc,
            r#"
                ---
                function FLPR1(a)
                end
            "#
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingGlobalDoc,
            r#"
                ---
                function FLPR1()
                    return 1
                end
            "#
        ));

        assert!(ws.has_no_diagnostic(
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

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
                local function a()
                end
            "#
        ));
    }

    /// A closure stored at a nested path (`L.out.to = function(line)`) takes its params from the
    /// field it is stored in, as one stored through a one-level path does.
    #[test]
    fn test_nested_member_closure_params_from_field() {
        fn param_type(ws: &mut VirtualWorkspace, file: &str, src: &str) -> String {
            let file_id = ws.def_file(file, src);
            let model = ws.analysis.semantic_model(file_id);
            let facts = model.file_facts().expect("facts");
            let decl = facts
                .decls
                .iter()
                .find(|decl| decl.name == "line")
                .expect("param decl")
                .id
                .clone();
            let ty = model.type_of_decl(&decl).unwrap_or(crate::LuaType::Unknown);
            ws.humanize_type(ty)
        }
        let mut ws = VirtualWorkspace::new();
        let decl =
            "local L = { out = { to = nil } } ---@type { out: { to: fun(line: string)? } }\n";
        let assigned = format!("{decl}L.out.to = function(line) return line end\n");
        assert_eq!(param_type(&mut ws, "a.lua", &assigned), "string");
        let declared = format!("{decl}function L.out.to(line) return line end\n");
        assert_eq!(param_type(&mut ws, "f.lua", &declared), "string");
        // Across files, the shape luagnd's `gradle.out.to = function(line)` has.
        ws.def_file(
            "reg.lua",
            "local out = { to = nil } ---@type { to: fun(line: string)? }\nlocal M = { out = out }\nreturn M\n",
        );
        let required = "local reg = require 'reg'\nreg.out.to = function(line) return line end\n";
        assert_eq!(param_type(&mut ws, "r.lua", required), "string");
    }
}
