#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_vararg_unknown_and_boolean_conditions_are_not_constant() {
        let diagnostics = crate::check::test::check_source(
            r#"
            local function unknown(...)
                if ... then print('present') end
                assert(...)
            end
            ---@param ... boolean
            local function boolean(...)
                if ... then print('true') end
                assert(...)
            end
            unknown(false)
            boolean(false)
        "#,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    }

    #[test]
    fn test_vararg_false_conditions_still_report_impossible_branches() {
        let diagnostics = crate::check::test::check_source(
            r#"
            ---@param ... false
            local function always_false(...)
                if ... then print('unreachable') end
                assert(...)
            end
            always_false(false)
        "#,
        );
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|d| (d.code, d.message.as_str()))
            .collect();
        assert_eq!(
            messages,
            vec![
                (
                    DiagnosticCode::UnnecessaryAssert,
                    "Impossible assert: this expression is always falsy; prefer `error()`"
                ),
                (
                    DiagnosticCode::UnnecessaryIf,
                    "Impossible `if` statement: this condition is always falsy"
                ),
            ]
        );
    }

    #[test]
    fn test_issue_392() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(DiagnosticCode::UnnecessaryIf,
        r#"
        local a = false ---@type boolean|nil
        if a == nil or a then -- Unnecessary `if` statement: this condition is always truthy [unnecessary-if]
            print('a is not false')
        end
        "#
        ));
    }

    #[test]
    fn test_issue_396() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(DiagnosticCode::UnnecessaryIf,
        r#"
        local a = false ---@type 'a'|'b'
        if a ~= 'a' then -- Unnecessary `if` statement: this condition is always truthy [unnecessary-if]
        end
        "#
        ));
    }
    /// #1319: a declared `---@type` on a member wins over the metatable's `__index` return
    /// (Neovim declares `vim.env` this way).
    #[test]
    fn test_issue_1319_declared_type_wins_over_metatable_index() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def_file(
            "def.lua",
            r#"
            G = {}

            --- @type table<string, string?>
            G.envA = setmetatable({}, {
              __index = function(_, k)
                local v = os.getenv(k)
                if v == nil then return nil end
                return v
              end,
            })

            --- @type table<string, string?>
            G.envD = setmetatable({}, { __index = function(_, k) return 'always' end })
            "#,
        );
        for member in ["G.envA", "G.envD"] {
            let ty = ws.expr_ty(member);
            assert_eq!(ws.humanize_type(ty), "table<string,string?>", "{member}");
        }
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::UnnecessaryIf,
            r#"
            if G.envA.X then print 'A' end
            if G.envD.X then print 'D' end
            "#
        ));
    }

    /// #1319: `if v == nil then return nil end return v` returns `string?`, not `nil`, also
    /// as an `__index` function and when the inner condition is checked first.
    #[test]
    fn test_issue_1319_guarded_return_keeps_both_paths() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def_file(
            "def.lua",
            r#"
            F = function(_, k)
              local v = os.getenv(k)
              if v == nil then return nil end
              return v
            end
            ENV = setmetatable({}, { __index = F })
            "#,
        );
        let fun = ws.expr_ty("F");
        assert!(ws.humanize_type(fun).ends_with("-> string?"));
        let field = ws.expr_ty("ENV.X");
        assert_eq!(ws.humanize_type(field), "string?");
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::UnnecessaryIf,
            r#"
            local env = setmetatable({}, {
              __index = function(_, k)
                local v = os.getenv(k)
                if v == nil then return nil end
                return v
              end,
            })
            if env.X then print 'A' end
            "#
        ));
    }
}
