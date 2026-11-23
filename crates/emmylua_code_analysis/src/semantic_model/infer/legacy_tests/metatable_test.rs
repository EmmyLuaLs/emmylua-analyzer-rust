#[cfg(test)]
mod test {
    use lsp_types::NumberOrString;
    use tokio_util::sync::CancellationToken;

    use crate::{DiagnosticCode, VirtualWorkspace};

    fn diagnostic_codes(
        ws: &mut VirtualWorkspace,
        diagnostic_code: DiagnosticCode,
        source: &str,
    ) -> Vec<String> {
        ws.analysis.diagnostic.enable_only(diagnostic_code);
        let file_id = ws.def(source);
        ws.analysis
            .diagnose_file(file_id, CancellationToken::new())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|diagnostic| match diagnostic.code {
                Some(NumberOrString::String(code)) => Some(code),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn test_metatable() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
            cmd = setmetatable({}, {
                --- @param command string|string[]
                __call = function (_, command)
                end,

                --- @param command string
                --- @return fun(...:string)
                __index = function(_, command)
                end,
            })
            "#,
        );

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            cmd(1)
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            cmd("hello)
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            cmd({ "hello", "world" })
        "#
        ));

        let ty = ws.expr_ty("cmd.hihihi");
        let ty_desc = ws.humanize_type(ty);
        assert_eq!(ty_desc, "fun(...: string)");
    }

    #[test]
    fn test_metatable_2() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class switch
            ---@field map table
            ---@field cachedCases table
            local switchMT = {}
            switchMT.__index = switchMT

            ---@return switch
            local function switch()
                local obj = setmetatable({
                    map = {},
                    cachedCases = {},
                }, switchMT)
                a =  obj
            end
            "#,
        );

        let ty = ws.expr_ty("a");
        assert_eq!(ws.humanize_type(ty), "switch");
    }

    #[test]
    fn test_issue_599() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class Class.Config
            ---@field abc string
            local ClassConfigMeta = {}

            ---@type table<string, Class.Config>
            local _classConfigMap = {}


            ---@param name string
            ---@return Class.Config
            local function getConfig(name)
                local config = _classConfigMap[name]
                if not config then
                    A = setmetatable({ name = name }, { __index = ClassConfigMeta })
                end
            end
            "#,
        );

        let ty = ws.expr_ty("A");
        assert_eq!(ws.humanize_type(ty), "Class.Config");
    }

    #[test]
    fn test_issue_1251_local_call_and_index_functions() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let source = r#"
            local function __call(self, opts)
                return self
            end

            ---@param key string
            ---@return any
            local function __index(_, key)
                return nil
            end

            local cfg = setmetatable({}, {__index = __index, __call = __call})
            cfg({})
        "#;

        for code in [
            DiagnosticCode::CallNonCallable,
            DiagnosticCode::MissingParameter,
            DiagnosticCode::ParamTypeMismatch,
        ] {
            let codes = diagnostic_codes(&mut ws, code, source);
            assert!(
                codes.is_empty(),
                "{code:?} must not be reported for a callable table with local metamethods: {codes:?}"
            );
        }

        // The resolved `__call` candidate must still validate its arguments.
        let codes = diagnostic_codes(
            &mut ws,
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@param opts table
            local function __call(self, opts)
                return self
            end

            local cfg = setmetatable({}, {__call = __call})
            cfg(1)
            "#,
        );
        assert!(
            codes.iter().any(|code| code == "param-type-mismatch"),
            "the local __call candidate must still check arguments: {codes:?}"
        );
    }

    #[test]
    fn test_issue_1251_index_expr_call_candidate() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let source = r#"
            local handlers = {}

            ---@param opts string
            function handlers.cb(self, opts)
                return self
            end

            local cfg = setmetatable({}, {__call = handlers.cb})
            cfg("hello")
        "#;

        for code in [
            DiagnosticCode::CallNonCallable,
            DiagnosticCode::MissingParameter,
            DiagnosticCode::ParamTypeMismatch,
        ] {
            let codes = diagnostic_codes(&mut ws, code, source);
            assert!(
                codes.is_empty(),
                "{code:?} must not be reported for __call = handlers.cb: {codes:?}"
            );
        }
    }

    #[test]
    fn test_issue_1251_union_call_candidates() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let source = r#"
            ---@type (fun(self: table, opts: integer): any) | (fun(self: table, opts: string): any)
            local handler

            local cfg = setmetatable({}, {__call = handler})
            cfg(1)
            cfg("hello")
        "#;

        for code in [
            DiagnosticCode::CallNonCallable,
            DiagnosticCode::MissingParameter,
            DiagnosticCode::ParamTypeMismatch,
        ] {
            let codes = diagnostic_codes(&mut ws, code, source);
            assert!(
                codes.is_empty(),
                "{code:?} must not be reported for a union __call: {codes:?}"
            );
        }
    }
}
