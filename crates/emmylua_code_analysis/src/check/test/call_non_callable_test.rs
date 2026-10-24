#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};
    use lsp_types::NumberOrString;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn test_vararg_first_function_is_callable() {
        let diagnostics = crate::check::test::check_source(
            r#"
            ---@param ... fun(value: integer): integer
            local function apply(...)
                return (...)(1)
            end
            print(apply(function(value) return value end))
        "#,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    }

    #[test]
    fn test_vararg_first_function_checks_arguments() {
        let diagnostics = crate::check::test::check_source(
            r#"
            ---@param ... fun(value: integer): integer
            local function apply(...)
                return (...)('bad')
            end
            print(apply(function(value) return value end))
        "#,
        );
        assert_eq!(
            crate::check::test::count_by_code(&diagnostics, DiagnosticCode::ParamTypeMismatch),
            1
        );
        assert_eq!(
            crate::check::test::count_by_code(&diagnostics, DiagnosticCode::CallNonCallable),
            0
        );
    }

    #[test]
    fn test_vararg_first_number_is_not_callable() {
        let diagnostics = crate::check::test::check_source(
            r#"
            ---@param ... integer
            local function apply(...)
                return (...)(1)
            end
            print(apply(2))
        "#,
        );
        assert_eq!(
            crate::check::test::count_by_code(&diagnostics, DiagnosticCode::CallNonCallable),
            1
        );
    }

    #[test]
    fn test_callable_table_accepts_valid_arguments() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            local command = setmetatable({}, {
                ---@param arg string
                __call = function(_, arg) print(arg) end,
            })
            command('hello')
            local api = { cmd = command }
            api.cmd('hello')
            "#,
        );
        let diagnostics = ws
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    }

    #[test]
    fn test_callable_table_preserves_invalid_call_diagnostics() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            local command = setmetatable({}, {
                ---@param arg string
                __call = function(_, arg) print(arg) end,
            })
            command(42)
            local plain = {}
            plain()
            local without_call = setmetatable({}, {})
            without_call()
            "#,
        );
        let mut diagnostics = ws
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .unwrap();
        diagnostics.sort_by_key(|diagnostic| diagnostic.range.start);
        let codes: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.clone())
            .collect();
        assert_eq!(
            codes,
            vec![
                Some(NumberOrString::String("param-type-mismatch".into())),
                Some(NumberOrString::String("call-non-callable".into())),
                Some(NumberOrString::String("call-non-callable".into())),
            ],
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn test_callable_table_member_assignment_across_files() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def_file(
            "api.lua",
            r#"
            local api = {}
            api.cmd = setmetatable({}, {
                ---@param arg string
                __call = function(_, arg) print(arg) end,
            })
            api.plain = {}
            return api
            "#,
        );
        let file_id = ws.def(
            r#"
            local api = require('api')
            api.cmd('hello')
            api.cmd(42)
            api.plain()
            "#,
        );
        let mut diagnostics = ws
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .unwrap();
        diagnostics.sort_by_key(|diagnostic| diagnostic.range.start);
        let codes: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.clone())
            .collect();
        assert_eq!(
            codes,
            vec![
                Some(NumberOrString::String("param-type-mismatch".into())),
                Some(NumberOrString::String("call-non-callable".into())),
            ],
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn test_call_non_callable() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                local i = 1
                i()
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                local s = "hi"
                s()
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                local b = true
                b()
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                ---@type thread
                local t
                t()
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                local function f() end
                f()
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                ---@type function
                local f
                f()
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                ---@type function|integer
                local f
                f()
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                local f ---@type { field: string } & fun()
                f()
            "#
        ));

        // nil is covered by need-check-nil instead of call-non-callable
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                ---@type function|nil
                local f
                f()
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                (1)()
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                ("hi")()
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                (true)()
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                (function() end)()
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                local i --- @type integer|fun():string
                _ = i()
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                ---@class Callable
                ---@operator call: string
                ---@type Callable
                local c
                c()
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                local c = {}
                c()
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                ---@class NonCallable
                ---@type NonCallable
                local c = {}
                c()
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
                ---@type any
                local c
                c()
            "#
        ));
    }

    #[test]
    fn test_call_non_callable_fallback_from_initializer() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_check(DiagnosticCode::CallNonCallable);
        let file_id = ws.def("local i = 1\n i()\n");

        let diagnostics = ws
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .unwrap();
        let code = Some(NumberOrString::String(
            DiagnosticCode::CallNonCallable.get_name().to_string(),
        ));
        assert!(diagnostics.iter().any(|diag| diag.code == code));
    }

    #[test]
    fn test_no_call_non_callable_on_undefined_type() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
            local a --- @type NotDefined
            a()
            "#
        ));
    }

    #[test]
    fn test_no_call_non_callable_on_alias() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
            ---@alias FunAlias function
            local a --- @type FunAlias
            a()
            "#
        ));
    }

    #[test]
    fn test_no_call_non_callable_for_generic_function_param_in_a_lua() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::CallNonCallable,
            r#"
            --- @generic F: function
            --- @param fn F
            --- @return F
            function once(fn)
              return function(...)
                return fn(...)
              end
            end
            "#,
        ));
    }

    /// #1334: a `table<string, integer>` read is `integer`, which is not callable.
    #[test]
    fn test_index_signature_value_is_not_callable() {
        let diagnostics = crate::check::test::check_source(
            r#"
            ---@param pt table<string, integer>
            local function g(pt)
                return pt.X()
            end
            return g
        "#,
        );
        assert_eq!(
            crate::check::test::count_by_code(&diagnostics, DiagnosticCode::CallNonCallable),
            1
        );
    }
}
