#[cfg(test)]
mod test {
    use lsp_types::NumberOrString;
    use tokio_util::sync::CancellationToken;

    use crate::{Emmyrc, EmmyrcLuaVersion, FileId, LuaType, VirtualWorkspace};

    fn decl_type(ws: &VirtualWorkspace, file_id: FileId, name: &str) -> LuaType {
        let model = ws.analysis.semantic_model(file_id);
        let facts = model.file_facts().expect("file facts");
        let decl = facts
            .decl_named(name)
            .unwrap_or_else(|| panic!("declaration {name} must exist"));
        model
            .type_of_decl(&decl.id)
            .unwrap_or_else(|| panic!("type of {name} must exist"))
    }

    fn workspace_for(version: EmmyrcLuaVersion) -> VirtualWorkspace {
        let mut config = Emmyrc::default();
        config.runtime.version = version;
        VirtualWorkspace::new_with_config(config)
    }

    fn diagnostic_codes_for(version: EmmyrcLuaVersion, source: &str) -> Vec<String> {
        let mut ws = workspace_for(version);

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

    /// Humanized type of `expr` under the given runtime version.
    fn expr_type_for(version: EmmyrcLuaVersion, expr: &str) -> String {
        let mut ws = workspace_for(version);

        let ty = ws.expr_ty(expr);
        ws.humanize_type(ty)
    }

    /// Humanized type of the local declaration `name` under the given runtime
    /// version. Useful for checking secondary return values, which
    /// `expr_type_for` cannot observe.
    fn local_type_for(version: EmmyrcLuaVersion, source: &str, name: &str) -> String {
        let mut ws = workspace_for(version);

        let file_id = ws.def(source);
        let ty = decl_type(&ws, file_id, name);
        ws.humanize_type(ty)
    }

    #[test]
    fn test_string_dump_strip_visibility_by_version() {
        let source = r#"
            local function f() end
            string.dump(f, true)
        "#;

        assert!(
            diagnostic_codes_for(EmmyrcLuaVersion::Lua51, source)
                .iter()
                .any(|code| code == "redundant-parameter"),
            "Lua 5.1 must not expose string.dump strip"
        );
        assert!(
            diagnostic_codes_for(EmmyrcLuaVersion::Lua52, source)
                .iter()
                .any(|code| code == "redundant-parameter"),
            "Lua 5.2 must not expose string.dump strip"
        );
        assert!(
            diagnostic_codes_for(EmmyrcLuaVersion::Lua53, source).is_empty(),
            "Lua 5.3 must accept a boolean strip"
        );
        assert!(
            diagnostic_codes_for(EmmyrcLuaVersion::Lua54, source).is_empty(),
            "Lua 5.4 must accept a boolean strip"
        );
    }

    #[test]
    fn test_string_dump_luajit_mode_strings() {
        let function = "local function f() end\n";

        assert!(
            diagnostic_codes_for(
                EmmyrcLuaVersion::LuaJIT,
                &format!("{function}string.dump(f, \"s\")")
            )
            .is_empty(),
            "LuaJIT must accept the \"s\" mode string"
        );
        assert!(
            diagnostic_codes_for(
                EmmyrcLuaVersion::LuaJIT,
                &format!("{function}string.dump(f, \"d\")")
            )
            .is_empty(),
            "LuaJIT must accept the \"d\" mode string"
        );
        assert!(
            diagnostic_codes_for(
                EmmyrcLuaVersion::LuaJIT,
                &format!("{function}string.dump(f, 1)")
            )
            .iter()
            .any(|code| code == "param-type-mismatch"),
            "LuaJIT must still reject a number as strip"
        );
        assert!(
            diagnostic_codes_for(
                EmmyrcLuaVersion::Lua53,
                &format!("{function}string.dump(f, \"s\")")
            )
            .iter()
            .any(|code| code == "param-type-mismatch"),
            "Lua 5.3 must not accept the LuaJIT-only \"s\" mode string"
        );
    }

    #[test]
    fn test_global_overloads_respect_version() {
        let source = r#"
            --- @version 5.1, 5.2
            --- @param a integer
            function foo(a) end

            --- @version >5.3
            --- @param a integer
            --- @param b? boolean
            function foo(a, b) end

            foo(1, true)
        "#;

        assert!(
            diagnostic_codes_for(EmmyrcLuaVersion::Lua51, source)
                .iter()
                .any(|code| code == "redundant-parameter"),
            "Lua 5.1 must use the one-argument global overload"
        );
        assert!(
            !diagnostic_codes_for(EmmyrcLuaVersion::Lua54, source)
                .iter()
                .any(|code| code == "redundant-parameter"),
            "Lua 5.4 must use the two-argument global overload"
        );
    }

    #[test]
    fn test_string_pack_accepts_formats_without_values() {
        let diagnostics = diagnostic_codes_for(
            EmmyrcLuaVersion::Lua54,
            r#"
            local _empty = string.pack("")
            local _padding = string.pack("x")
            "#,
        );

        assert!(
            diagnostics.is_empty(),
            "string.pack without values must be accepted: {diagnostics:?}"
        );
    }

    #[test]
    fn test_table_move_visibility_by_version() {
        // `table.move` is a 5.3 API; the `>5.3, JIT` tag must also expose it
        // in LuaJIT/Tarantool, where the runtime provides it. The inferred
        // return type also shows that the call resolves the main signature.
        for version in [
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::Lua55,
            EmmyrcLuaVersion::LuaJIT,
        ] {
            assert_eq!(
                expr_type_for(version, "table.move({}, 1, 2, 3)"),
                "table",
                "table.move must be available in {version:?}"
            );
        }

        for version in [EmmyrcLuaVersion::Lua51, EmmyrcLuaVersion::Lua52] {
            assert_eq!(
                expr_type_for(version, "table.move({}, 1, 2, 3)"),
                "unknown",
                "table.move must be hidden in {version:?}"
            );
        }
    }

    #[test]
    fn test_table_move_default_destination_without_overload() {
        // `a2` is optional, so the removed four-argument `@overload` is no
        // longer needed: both forms must call the main signature.
        for source in [
            "local t = {1, 2, 3}\ntable.move(t, 1, 2, 2)\n",
            "local t = {1, 2, 3}\ntable.move(t, 1, 2, 2, nil)\n",
        ] {
            let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua53, source);
            assert!(
                diagnostics.is_empty(),
                "table.move(t, 1, 2, 2[, nil]) must be accepted: {diagnostics:?}"
            );
        }

        // Guard against a vacuous pass: parameter types are still checked.
        let diagnostics = diagnostic_codes_for(
            EmmyrcLuaVersion::Lua53,
            "local t = {1, 2, 3}\ntable.move(t, \"x\", 2, 2)\n",
        );
        assert!(
            diagnostics.iter().any(|code| code == "param-type-mismatch"),
            "wrong parameter types must still be reported: {diagnostics:?}"
        );
    }

    #[test]
    fn test_bit_variadic_functions_accept_one_argument() {
        for source in [
            "local _ = bit.band(1)",
            "local _ = bit.bor(1)",
            "local _ = bit.bxor(1)",
        ] {
            let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::LuaJIT, source);
            assert!(
                !diagnostics.iter().any(|code| code == "missing-parameter"),
                "{source} must accept a single argument: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_bit_variadic_functions_require_first_argument() {
        for source in [
            "local _ = bit.band()",
            "local _ = bit.bor()",
            "local _ = bit.bxor()",
        ] {
            let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::LuaJIT, source);
            assert!(
                diagnostics.iter().any(|code| code == "missing-parameter"),
                "{source} must require the first argument: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_bit_accepts_number_operands() {
        let diagnostics = diagnostic_codes_for(
            EmmyrcLuaVersion::LuaJIT,
            r#"
            local _ = bit.tobit(2^40 + 1234)
            local _2 = bit.tohex(2^8)
            local _3 = bit.band(2^8, 0xff)
            "#,
        );
        assert!(
            !diagnostics.iter().any(|code| code == "param-type-mismatch"),
            "number operands must be accepted: {diagnostics:?}"
        );
    }

    #[test]
    fn test_bit_return_types() {
        assert_eq!(
            expr_type_for(EmmyrcLuaVersion::LuaJIT, "bit.tobit(1)"),
            "integer"
        );
        assert_eq!(
            expr_type_for(EmmyrcLuaVersion::LuaJIT, "bit.tohex(1)"),
            "string"
        );
        assert_eq!(
            expr_type_for(EmmyrcLuaVersion::LuaJIT, "bit.tobit(2^40 + 1234)"),
            "integer"
        );
        assert_eq!(
            expr_type_for(EmmyrcLuaVersion::LuaJIT, "bit.band(1, 2)"),
            "integer"
        );
    }

    #[test]
    fn test_bit_is_only_available_in_jit() {
        for version in [
            EmmyrcLuaVersion::Lua51,
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::Lua55,
        ] {
            assert!(
                diagnostic_codes_for(version, "local _ = bit")
                    .iter()
                    .any(|code| code == "undefined-global"),
                "bit must be undefined in {version:?}"
            );
        }
        assert!(
            !diagnostic_codes_for(EmmyrcLuaVersion::LuaJIT, "local _ = bit")
                .iter()
                .any(|code| code == "undefined-global"),
            "bit must be defined in LuaJIT"
        );
    }

    #[test]
    fn test_table_maxn_visibility_and_return_type_by_version() {
        // `table.maxn` exists in 5.1 and 5.2 (LUA_COMPAT_MAXN) and in LuaJIT,
        // and was removed in 5.3. It scans every numeric key, so a fractional
        // key makes it return a fractional value: maxn({ [1.5] = 1 }) == 1.5.
        for version in [
            EmmyrcLuaVersion::Lua51,
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::LuaJIT,
        ] {
            assert_eq!(
                expr_type_for(version, "table.maxn({})"),
                "number",
                "table.maxn must be available and return number in {version:?}"
            );
        }

        for version in [EmmyrcLuaVersion::Lua53, EmmyrcLuaVersion::Lua54] {
            assert_eq!(
                expr_type_for(version, "table.maxn({})"),
                "unknown",
                "table.maxn must be hidden in {version:?}"
            );
        }
    }

    #[test]
    fn test_os_difftime_arity_by_version() {
        // PUC Lua 5.1/5.2 and LuaJIT implement `t1` via `luaL_optnumber`
        // (default 0); Lua 5.3+ require both arguments.
        for version in [
            EmmyrcLuaVersion::Lua51,
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::LuaJIT,
        ] {
            let diagnostics = diagnostic_codes_for(version, "local _ = os.difftime(1)");
            assert!(
                !diagnostics.iter().any(|code| code == "missing-parameter"),
                "os.difftime(1) must be accepted in {version:?}: {diagnostics:?}"
            );
        }

        for version in [
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::Lua55,
        ] {
            let diagnostics = diagnostic_codes_for(version, "local _ = os.difftime(1)");
            assert!(
                diagnostics.iter().any(|code| code == "missing-parameter"),
                "os.difftime(1) must require t1 in {version:?}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_os_time_return_type_by_version() {
        // 5.1/5.2/LuaJIT push a Lua number; 5.3+ push an integer. From 5.3 on,
        // an invalid time raises instead of returning nil; 5.1/5.2/LuaJIT can
        // still return nil when mktime fails, which the normal-path annotation
        // intentionally does not model (same as `os.date`).
        for version in [
            EmmyrcLuaVersion::Lua51,
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::LuaJIT,
        ] {
            assert_eq!(
                expr_type_for(version, "os.time()"),
                "number",
                "os.time must return number in {version:?}"
            );
        }

        for version in [
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::Lua55,
        ] {
            assert_eq!(
                expr_type_for(version, "os.time()"),
                "integer",
                "os.time must return integer in {version:?}"
            );
        }
    }

    #[test]
    fn test_os_setlocale_locale_is_optional() {
        // All supported runtimes accept `os.setlocale()` and return the
        // current locale for the default category.
        for version in [
            EmmyrcLuaVersion::Lua51,
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::LuaJIT,
        ] {
            let diagnostics = diagnostic_codes_for(version, "local _ = os.setlocale()");
            assert!(
                !diagnostics.iter().any(|code| code == "missing-parameter"),
                "os.setlocale() must be accepted in {version:?}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_os_remove_reports_error_code() {
        // `os.remove`/`os.rename` return the errno as their third result on
        // failure (the third result is absent on success, hence `nil`).
        // `select(3, ...)` selects that declared result statically; this test
        // only checks the annotation and never executes the Lua code.
        for version in [
            EmmyrcLuaVersion::Lua51,
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::Lua55,
            EmmyrcLuaVersion::LuaJIT,
        ] {
            for (call, name) in [
                ("select(3, os.remove('/nonexistent'))", "os.remove"),
                (
                    "select(3, os.rename('/nonexistent', '/nonexistent2'))",
                    "os.rename",
                ),
            ] {
                let ty = expr_type_for(version, call);
                assert_eq!(
                    ty, "integer?",
                    "{name} must expose an integer errcode in {version:?}"
                );
            }
        }
    }

    #[test]
    fn test_newproxy_rejects_non_proxy_argument() {
        let diagnostics =
            diagnostic_codes_for(EmmyrcLuaVersion::LuaJIT, "newproxy(function() end)");
        assert!(
            diagnostics.iter().any(|code| code == "param-type-mismatch"),
            "newproxy must reject a function: {diagnostics:?}"
        );

        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::LuaJIT, "newproxy(true)");
        assert!(
            diagnostics.is_empty(),
            "newproxy(true) must be accepted: {diagnostics:?}"
        );
    }

    #[test]
    fn test_load_reader_may_return_nil() {
        let diagnostics =
            diagnostic_codes_for(EmmyrcLuaVersion::Lua54, "load(function() return nil end)");
        assert!(
            diagnostics.is_empty(),
            "a load reader may signal the end with nil: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collectgarbage_param_value_is_optional() {
        let diagnostics = diagnostic_codes_for(
            EmmyrcLuaVersion::Lua55,
            r#"collectgarbage("param", "pause")"#,
        );
        assert!(
            diagnostics.is_empty(),
            "collectgarbage param value must be optional: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collectgarbage_options_respect_version() {
        for option in ["incremental", "generational"] {
            let source = format!(r#"collectgarbage("{option}")"#);

            for version in [EmmyrcLuaVersion::Lua51, EmmyrcLuaVersion::Lua53] {
                let diagnostics = diagnostic_codes_for(version, &source);
                assert!(
                    diagnostics.iter().any(|code| code == "param-type-mismatch"),
                    "{option} must be hidden in {version:?}: {diagnostics:?}"
                );
            }
            let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::LuaJIT, &source);
            assert!(
                diagnostics.iter().any(|code| code == "param-type-mismatch"),
                "{option} must be hidden in LuaJIT: {diagnostics:?}"
            );

            for version in [
                EmmyrcLuaVersion::Lua52,
                EmmyrcLuaVersion::Lua54,
                EmmyrcLuaVersion::Lua55,
            ] {
                let diagnostics = diagnostic_codes_for(version, &source);
                assert!(
                    diagnostics.is_empty(),
                    "{option} must be accepted in {version:?}: {diagnostics:?}"
                );
            }
        }

        let isrunning = r#"collectgarbage("isrunning")"#;
        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua51, isrunning);
        assert!(
            diagnostics.iter().any(|code| code == "param-type-mismatch"),
            "isrunning must be hidden in Lua 5.1: {diagnostics:?}"
        );
        for version in [
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::LuaJIT,
        ] {
            let diagnostics = diagnostic_codes_for(version, isrunning);
            assert!(
                diagnostics.is_empty(),
                "isrunning must be accepted in {version:?}: {diagnostics:?}"
            );
        }

        let param = r#"collectgarbage("param", "pause")"#;
        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua54, param);
        assert!(
            diagnostics.iter().any(|code| code == "param-type-mismatch"),
            "param must be hidden in Lua 5.4: {diagnostics:?}"
        );

        // Lua 5.5 removed the deprecated `setpause`/`setstepmul` options.
        for option in ["setpause", "setstepmul"] {
            let source = format!(r#"collectgarbage("{option}", 100)"#);
            for version in [
                EmmyrcLuaVersion::Lua51,
                EmmyrcLuaVersion::Lua52,
                EmmyrcLuaVersion::Lua53,
                EmmyrcLuaVersion::Lua54,
            ] {
                let diagnostics = diagnostic_codes_for(version, &source);
                assert!(
                    diagnostics.is_empty(),
                    "{option} must be accepted in {version:?}: {diagnostics:?}"
                );
            }
            let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua55, &source);
            assert!(
                diagnostics.iter().any(|code| code == "param-type-mismatch"),
                "{option} must be hidden in Lua 5.5: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_collectgarbage_return_types() {
        assert_eq!(
            expr_type_for(EmmyrcLuaVersion::Lua51, r#"collectgarbage("count")"#),
            "number",
            "collectgarbage(\"count\") must keep its number return type"
        );
        assert_eq!(
            expr_type_for(EmmyrcLuaVersion::Lua51, r#"collectgarbage("step")"#),
            "boolean",
            "collectgarbage(\"step\") must keep its boolean return type"
        );
        assert_eq!(
            expr_type_for(EmmyrcLuaVersion::Lua54, r#"collectgarbage("isrunning")"#),
            "boolean",
            "collectgarbage(\"isrunning\") must keep its boolean return type"
        );
        assert_eq!(
            expr_type_for(
                EmmyrcLuaVersion::Lua55,
                r#"collectgarbage("param", "pause")"#
            ),
            "integer",
            "collectgarbage(\"param\") must keep its integer return type"
        );
    }

    #[test]
    fn test_load_string_visibility_by_version() {
        let source = r#"load("return 1")"#;
        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua51, source);
        assert!(
            diagnostics.iter().any(|code| code == "param-type-mismatch"),
            "Lua 5.1 load must reject a string chunk: {diagnostics:?}"
        );
        for version in [EmmyrcLuaVersion::Lua54, EmmyrcLuaVersion::LuaJIT] {
            let diagnostics = diagnostic_codes_for(version, source);
            assert!(
                diagnostics.is_empty(),
                "load must accept a string chunk in {version:?}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_loadstring_visibility_by_version() {
        for version in [EmmyrcLuaVersion::Lua51, EmmyrcLuaVersion::LuaJIT] {
            let ty = expr_type_for(version, "loadstring");
            assert_ne!(
                ty, "unknown",
                "loadstring must be visible in {version:?}: {ty}"
            );
        }
        for version in [
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::Lua55,
        ] {
            let ty = expr_type_for(version, "loadstring");
            assert_eq!(
                ty, "unknown",
                "loadstring must be hidden in {version:?}: {ty}"
            );
        }
    }

    #[test]
    fn test_versioned_table_globals_visibility() {
        for version in [EmmyrcLuaVersion::Lua54, EmmyrcLuaVersion::Lua55] {
            assert_eq!(
                expr_type_for(version, "bit"),
                "unknown",
                "bit must be hidden outside LuaJIT in {version:?}"
            );
            assert_eq!(
                expr_type_for(version, "bit32"),
                "unknown",
                "bit32 must be hidden outside Lua 5.2 in {version:?}"
            );
        }
        for version in [EmmyrcLuaVersion::Lua51, EmmyrcLuaVersion::Lua52] {
            assert_eq!(
                expr_type_for(version, "utf8"),
                "unknown",
                "utf8 must be hidden before Lua 5.4 in {version:?}"
            );
        }

        for (version, name) in [
            (EmmyrcLuaVersion::LuaJIT, "bit"),
            (EmmyrcLuaVersion::Lua52, "bit32"),
            (EmmyrcLuaVersion::Lua54, "utf8"),
        ] {
            let ty = expr_type_for(version, name);
            assert_ne!(ty, "unknown", "`{name}` must be visible in {version:?}");
        }
    }

    #[test]
    fn test_loadstring_and_loadfile_version_signatures() {
        let loadstring = r#"loadstring("return 1", "chunk", "t", {})"#;
        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua51, loadstring);
        assert!(
            diagnostics.iter().any(|code| code == "redundant-parameter"),
            "Lua 5.1 loadstring must not accept mode/env: {diagnostics:?}"
        );
        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::LuaJIT, loadstring);
        assert!(
            diagnostics.is_empty(),
            "LuaJIT loadstring must accept mode/env: {diagnostics:?}"
        );

        let loadfile = r#"loadfile("x.lua", "t", {})"#;
        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua51, loadfile);
        assert!(
            diagnostics.iter().any(|code| code == "redundant-parameter"),
            "Lua 5.1 loadfile must not accept mode/env: {diagnostics:?}"
        );
        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua54, loadfile);
        assert!(
            diagnostics.is_empty(),
            "Lua 5.4 loadfile must accept mode/env: {diagnostics:?}"
        );
    }

    #[test]
    fn test_xpcall_extra_arguments_by_version() {
        let source = "xpcall(function() end, tostring, 1)";
        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua51, source);
        assert!(
            diagnostics.iter().any(|code| code == "redundant-parameter"),
            "Lua 5.1 xpcall must not accept extra arguments: {diagnostics:?}"
        );
        for version in [
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::LuaJIT,
        ] {
            let diagnostics = diagnostic_codes_for(version, source);
            assert!(
                diagnostics.is_empty(),
                "xpcall must accept extra arguments in {version:?}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_version_global_is_plain_string() {
        for version in [
            EmmyrcLuaVersion::Lua51,
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::LuaJIT,
        ] {
            assert_eq!(
                expr_type_for(version, "_VERSION"),
                "string",
                "_VERSION must not keep the Lua 5.5 literal in {version:?}"
            );
        }
    }

    #[test]
    fn test_assert_removes_nil_and_false() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@type boolean
            local bool_value
            local asserted_bool = assert(bool_value)

            ---@type string?
            local string_value
            local asserted_string = assert(string_value)

            ---@return false | string
            local function maybe_string()
                return "value"
            end
            local asserted_union = assert(maybe_string())
            "#,
        );

        let bool_ty = decl_type(&ws, file_id, "asserted_bool");
        assert_eq!(
            ws.humanize_type(bool_ty),
            "true",
            "assert must remove false from a boolean result"
        );

        let string_ty = decl_type(&ws, file_id, "asserted_string");
        assert_eq!(
            ws.humanize_type(string_ty),
            "string",
            "assert must remove nil from a nullable result"
        );

        let union_ty = decl_type(&ws, file_id, "asserted_union");
        assert_eq!(
            ws.humanize_type(union_ty),
            "string",
            "assert must remove false from a union result"
        );
    }

    #[test]
    fn test_rawset_preserves_table_type() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@class RawSetTarget
            local RawSetTarget = {}

            ---@type RawSetTarget
            local target

            local result = rawset(target, "x", 1)
            "#,
        );

        let result_ty = decl_type(&ws, file_id, "result");
        assert_eq!(
            ws.humanize_type(result_ty),
            "RawSetTarget",
            "rawset must preserve the table argument type"
        );
    }

    #[test]
    fn test_hidden_local_function_skips_param_diagnostics() {
        let source = r#"
            ---@version 5.1
            ---@param x integer
            local function hidden(x) end
            hidden("bad")
        "#;

        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua54, source);
        assert!(
            !diagnostics
                .iter()
                .any(|code| code == "missing-parameter" || code == "param-type-mismatch"),
            "hidden local function must not be param-checked: {diagnostics:?}"
        );

        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua51, source);
        assert!(
            diagnostics.iter().any(|code| code == "param-type-mismatch"),
            "visible local function must still be param-checked: {diagnostics:?}"
        );
    }

    fn ws_with_version(version: EmmyrcLuaVersion) -> VirtualWorkspace {
        let mut config = Emmyrc::default();
        config.runtime.version = version;
        VirtualWorkspace::new_with_config(config)
    }

    #[test]
    fn test_hidden_std_member_skips_param_diagnostics() {
        for version in [EmmyrcLuaVersion::LuaJIT, EmmyrcLuaVersion::Lua51] {
            let diagnostics = diagnostic_codes_for(version, "string.unpack(1)");
            assert!(
                !diagnostics
                    .iter()
                    .any(|code| code == "missing-parameter" || code == "param-type-mismatch"),
                "hidden string.unpack must not be param-checked in {version:?}: {diagnostics:?}"
            );
            assert!(
                diagnostics.iter().any(|code| code == "undefined-field"),
                "hidden string.unpack must be reported as undefined in {version:?}: {diagnostics:?}"
            );
        }

        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua54, "string.unpack(1)");
        assert!(
            diagnostics
                .iter()
                .any(|code| code == "missing-parameter" || code == "param-type-mismatch"),
            "visible string.unpack must still be checked in Lua 5.4: {diagnostics:?}"
        );
    }

    #[test]
    fn test_hidden_class_member_skips_param_diagnostics() {
        let source = r#"
            ---@class VersionedClass
            local VersionedClass = {}

            ---@version 5.1
            ---@param x integer
            function VersionedClass:hidden(x) end

            local instance = setmetatable({}, { __index = VersionedClass })
            instance:hidden("bad")
        "#;

        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua54, source);
        assert!(
            !diagnostics
                .iter()
                .any(|code| code == "missing-parameter" || code == "param-type-mismatch"),
            "hidden class member must not be param-checked: {diagnostics:?}"
        );

        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua51, source);
        assert!(
            diagnostics.iter().any(|code| code == "param-type-mismatch"),
            "visible class member must still be param-checked: {diagnostics:?}"
        );
    }

    #[test]
    fn test_hidden_global_is_reported_as_undefined() {
        for (version, name, source) in [
            (
                EmmyrcLuaVersion::Lua54,
                "loadstring",
                "loadstring(\"return 1\")",
            ),
            (EmmyrcLuaVersion::Lua51, "utf8", "utf8.char(65)"),
            (EmmyrcLuaVersion::Lua54, "unpack", "unpack({})"),
        ] {
            let diagnostics = diagnostic_codes_for(version, source);
            assert!(
                diagnostics.iter().any(|code| code == "undefined-global"),
                "hidden global `{name}` must be undefined in {version:?}: {diagnostics:?}"
            );
        }

        let diagnostics =
            diagnostic_codes_for(EmmyrcLuaVersion::LuaJIT, "loadstring(\"return 1\")");
        assert!(
            !diagnostics.iter().any(|code| code == "undefined-global"),
            "loadstring is available in LuaJIT: {diagnostics:?}"
        );
    }

    #[test]
    fn test_numeric_for_variable_does_not_inherit_version_conds() {
        let mut ws = ws_with_version(EmmyrcLuaVersion::Lua54);
        let file_id = ws.def(
            r#"
            ---@version 5.1
            local versioned = 1

            for i = 1, 10 do
                print(versioned, i)
            end
            "#,
        );

        let model = ws.analysis.semantic_model(file_id);
        let facts = model.file_facts().expect("file facts");
        let decl = facts.decl_named("i").expect("numeric for variable");
        assert!(
            decl.version_conds.is_empty(),
            "numeric for variables must not inherit @version from a previous statement"
        );
    }

    #[test]
    fn test_duplicate_unversioned_global_uses_last_declaration() {
        let mut ws = ws_with_version(EmmyrcLuaVersion::Lua54);
        let file_id = ws.def(
            r#"
            ---@return number
            function duplicate_global() end

            ---@return string
            function duplicate_global() end
            "#,
        );

        let model = ws.analysis.semantic_model(file_id);
        let facts = model.file_facts().expect("file facts");
        let decls: Vec<_> = facts
            .decls
            .iter()
            .filter(|decl| decl.name == "duplicate_global")
            .collect();
        assert_eq!(decls.len(), 2, "both declarations must be indexed");
        assert_eq!(
            model.global_decl("duplicate_global"),
            Some(decls[1].id.clone()),
            "duplicate globals must keep last-declaration-wins"
        );
    }

    #[test]
    fn test_hidden_last_global_does_not_shadow_visible_declaration() {
        let source = r#"
            ---@return number
            function duplicate_global() end

            ---@version 5.1
            ---@return string
            function duplicate_global() end
        "#;

        let mut ws = ws_with_version(EmmyrcLuaVersion::Lua54);
        let file_id = ws.def(source);
        let model = ws.analysis.semantic_model(file_id);
        let facts = model.file_facts().expect("file facts");
        let decls: Vec<_> = facts
            .decls
            .iter()
            .filter(|decl| decl.name == "duplicate_global")
            .collect();
        assert_eq!(decls.len(), 2, "both declarations must be indexed");
        assert_eq!(
            model.global_decl("duplicate_global"),
            Some(decls[0].id.clone()),
            "a version-hidden last declaration must not shadow an earlier visible one"
        );

        let mut ws = ws_with_version(EmmyrcLuaVersion::Lua51);
        let file_id = ws.def(source);
        let model = ws.analysis.semantic_model(file_id);
        let facts = model.file_facts().expect("file facts");
        let decls: Vec<_> = facts
            .decls
            .iter()
            .filter(|decl| decl.name == "duplicate_global")
            .collect();
        assert_eq!(
            model.global_decl("duplicate_global"),
            Some(decls[1].id.clone()),
            "the version-visible last declaration must win in its runtime"
        );
    }

    #[test]
    fn test_visible_class_member_sibling_is_not_undefined() {
        let source = r#"
            ---@class VersionedClass
            local VersionedClass = {}

            ---@version 5.1
            ---@param x integer
            function VersionedClass:action(x) end

            ---@version >5.2
            ---@param x integer
            function VersionedClass:action(x) end

            local instance = setmetatable({}, { __index = VersionedClass })
            instance:action("bad")
        "#;

        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua54, source);
        assert!(
            !diagnostics.iter().any(|code| code == "undefined-field"),
            "a visible sibling member must not be reported as undefined: {diagnostics:?}"
        );
        assert!(
            diagnostics.iter().any(|code| code == "param-type-mismatch"),
            "the visible sibling member must still be param-checked: {diagnostics:?}"
        );
    }

    #[test]
    fn test_setfenv_zero_returns_no_values() {
        let ty = expr_type_for(EmmyrcLuaVersion::Lua51, "setfenv(0, {})");
        assert_ne!(
            ty, "function",
            "setfenv(0, env) returns no values, not the function at a stack level"
        );
    }

    #[test]
    fn test_coroutine_isyieldable_signature_by_version() {
        // `coroutine.isyieldable` appeared in Lua 5.3 without arguments; LuaJIT
        // exposes the same no-argument form. Lua 5.1 and Lua 5.2 do not have it.
        for version in [EmmyrcLuaVersion::Lua51, EmmyrcLuaVersion::Lua52] {
            assert_eq!(
                expr_type_for(version, "coroutine.isyieldable()"),
                "unknown",
                "coroutine.isyieldable must be hidden in {version:?}"
            );
            let diagnostics = diagnostic_codes_for(version, "coroutine.isyieldable()");
            assert!(
                diagnostics.iter().any(|code| code == "undefined-field"),
                "hidden coroutine.isyieldable must be reported as undefined in {version:?}: {diagnostics:?}"
            );
        }

        for version in [
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::LuaJIT,
            EmmyrcLuaVersion::LuaJIT2,
            EmmyrcLuaVersion::LuaJIT3,
        ] {
            assert_eq!(
                expr_type_for(version, "coroutine.isyieldable()"),
                "boolean",
                "coroutine.isyieldable() must return boolean in {version:?}"
            );
            let source = "local co = coroutine.running()\ncoroutine.isyieldable(co)";
            let diagnostics = diagnostic_codes_for(version, source);
            assert!(
                diagnostics.iter().any(|code| code == "redundant-parameter"),
                "coroutine.isyieldable must not accept `co` in {version:?}: {diagnostics:?}"
            );
        }

        // Lua 5.4 added the optional `co` argument.
        for version in [
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::Lua55,
            EmmyrcLuaVersion::LuaLatest,
        ] {
            assert_eq!(
                expr_type_for(version, "coroutine.isyieldable()"),
                "boolean",
                "coroutine.isyieldable() must return boolean in {version:?}"
            );
            let source = "local co = coroutine.running()\ncoroutine.isyieldable(co)";
            let diagnostics = diagnostic_codes_for(version, source);
            assert!(
                diagnostics.is_empty(),
                "coroutine.isyieldable(co) must be accepted in {version:?}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_coroutine_close_signature_by_version() {
        let call = "coroutine.close(coroutine.create(function() end))";

        for version in [
            EmmyrcLuaVersion::Lua51,
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::LuaJIT,
            EmmyrcLuaVersion::LuaJIT2,
            EmmyrcLuaVersion::LuaJIT3,
        ] {
            assert_eq!(
                expr_type_for(version, call),
                "unknown",
                "coroutine.close must be hidden in {version:?}"
            );
            let diagnostics = diagnostic_codes_for(version, call);
            assert!(
                diagnostics.iter().any(|code| code == "undefined-field"),
                "hidden coroutine.close must be reported as undefined in {version:?}: {diagnostics:?}"
            );
        }

        // Lua 5.4 requires `co`.
        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua54, "coroutine.close()");
        assert!(
            diagnostics.iter().any(|code| code == "missing-parameter"),
            "coroutine.close() must require `co` in Lua 5.4: {diagnostics:?}"
        );

        // Lua 5.5 made `co` optional (defaults to the running coroutine).
        for version in [
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::Lua55,
            EmmyrcLuaVersion::LuaLatest,
        ] {
            let diagnostics = diagnostic_codes_for(version, call);
            assert!(
                diagnostics.is_empty(),
                "coroutine.close(co) must be accepted in {version:?}: {diagnostics:?}"
            );
        }
        for version in [EmmyrcLuaVersion::Lua55, EmmyrcLuaVersion::LuaLatest] {
            let diagnostics = diagnostic_codes_for(version, "coroutine.close()");
            assert!(
                diagnostics.is_empty(),
                "coroutine.close() must be accepted in {version:?}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_coroutine_running_shape_by_version() {
        // 5.1/LuaJIT return only the running coroutine; 5.2+ add a boolean.
        for version in [
            EmmyrcLuaVersion::Lua51,
            EmmyrcLuaVersion::LuaJIT,
            EmmyrcLuaVersion::LuaJIT2,
            EmmyrcLuaVersion::LuaJIT3,
        ] {
            assert_eq!(
                expr_type_for(version, "coroutine.running()"),
                "thread?",
                "coroutine.running() must return thread? in {version:?}"
            );
        }
        for version in [
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::Lua55,
            EmmyrcLuaVersion::LuaLatest,
        ] {
            assert_eq!(
                expr_type_for(version, "coroutine.running()"),
                "thread",
                "coroutine.running() must return thread in {version:?}"
            );
            assert_eq!(
                local_type_for(version, "local co, ismain = coroutine.running()", "ismain"),
                "boolean",
                "coroutine.running() must return a boolean second value in {version:?}"
            );
        }
    }

    #[test]
    fn test_coroutine_resume_and_wrap_annotations() {
        let source = r#"
            local co = coroutine.create(function(a) return a end)
            local ok, value = coroutine.resume(co, 1)
            if ok then
                print(value)
            end
            local wrapped = coroutine.wrap(function(a) return a end)
            print(wrapped(1))
        "#;
        for version in [
            EmmyrcLuaVersion::Lua51,
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::Lua55,
            EmmyrcLuaVersion::LuaJIT,
            EmmyrcLuaVersion::LuaJIT2,
            EmmyrcLuaVersion::LuaJIT3,
            EmmyrcLuaVersion::LuaLatest,
        ] {
            let diagnostics = diagnostic_codes_for(version, source);
            assert!(
                diagnostics.is_empty(),
                "coroutine.resume/wrap annotations must typecheck in {version:?}: {diagnostics:?}"
            );
            // `@return_overload true, any...` / `false, any` yields the
            // literal boolean union for the success flag. Both the success and
            // error payloads are `any`, so there is no further branch
            // narrowing to distinguish here.
            assert_eq!(
                local_type_for(version, source, "ok"),
                "(true|false)",
                "coroutine.resume's first result must be a boolean union in {version:?}"
            );
            assert_eq!(
                local_type_for(version, source, "value"),
                "any",
                "coroutine.resume's second result must be any in {version:?}"
            );
        }
    }

    #[test]
    fn test_io_type_accepts_any_value() {
        // `io.type` only inspects the value at runtime (`luaL_checkany`); it
        // must not require an actual file handle.
        let diagnostics = diagnostic_codes_for(
            EmmyrcLuaVersion::Lua54,
            r#"
            local _n = io.type(42)
            local _s = io.type("file")
            local _t = io.type({})
            "#,
        );

        assert!(
            diagnostics.is_empty(),
            "io.type must accept any value: {diagnostics:?}"
        );
    }

    #[test]
    fn test_io_seek_arguments_are_optional() {
        // `whence` defaults to "cur" and `offset` to 0, so all three call
        // shapes are valid; the previous `@overload fun()` is no longer
        // needed once both parameters are optional.
        let diagnostics = diagnostic_codes_for(
            EmmyrcLuaVersion::Lua54,
            r#"
            local f = io.open("test.txt", "w") --[[@as file]]
            f:seek()
            f:seek("set")
            f:seek("end", 0)
            "#,
        );

        assert!(
            !diagnostics
                .iter()
                .any(|code| code == "missing-parameter" || code == "redundant-parameter"),
            "file:seek arguments must be optional: {diagnostics:?}"
        );
    }

    #[test]
    fn test_io_union_parameters_keep_return_types() {
        // `io.open` has an optional union-typed `mode`, and `file:seek` has an
        // optional union-typed `whence`. Calls must still resolve to the main
        // signature instead of falling back to `unknown`.
        assert_eq!(
            expr_type_for(EmmyrcLuaVersion::Lua54, "io.open('test.txt', 'w')"),
            "file?"
        );
        assert_eq!(
            expr_type_for(
                EmmyrcLuaVersion::Lua54,
                "(assert(io.open('test.txt', 'w'))):seek()"
            ),
            "integer?"
        );
        assert_eq!(
            expr_type_for(
                EmmyrcLuaVersion::Lua54,
                "(assert(io.open('test.txt', 'w'))):seek('set')"
            ),
            "integer?"
        );
    }

    #[test]
    fn test_io_literal_options_reject_unknown_values() {
        // `luaL_checkoption` only accepts the documented literals; dropping the
        // redundant `string |` lets the checker catch typos.
        let diagnostics = diagnostic_codes_for(
            EmmyrcLuaVersion::Lua54,
            r#"
            local f = io.open("test.txt", "w") --[[@as file]]
            f:seek("seet")
            f:setvbuf("bogus")
            "#,
        );

        let mismatches = diagnostics
            .iter()
            .filter(|code| *code == "param-type-mismatch")
            .count();
        assert_eq!(
            mismatches, 2,
            "unknown seek/setvbuf options must be rejected: {diagnostics:?}"
        );
    }

    #[test]
    fn test_io_close_visible_in_every_version() {
        // `io.close` is split into version-gated blocks; every supported
        // runtime must still resolve it.
        for version in [
            EmmyrcLuaVersion::Lua51,
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::Lua55,
            EmmyrcLuaVersion::LuaJIT,
        ] {
            let diagnostics = diagnostic_codes_for(version, "io.close()");
            assert!(
                !diagnostics.iter().any(|code| code == "undefined-global"),
                "io.close must be available in {version:?}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_math_randomseed_seed_visibility_by_version() {
        fn has_code(version: EmmyrcLuaVersion, source: &str, code: &str) -> bool {
            diagnostic_codes_for(version, source)
                .iter()
                .any(|candidate| candidate.as_str() == code)
        }

        // LuaJIT's math.randomseed([x]) accepts an omitted seed and seeds from
        // system entropy. It also takes a double, so a fractional seed is valid.
        for source in [
            "math.randomseed()",
            "math.randomseed(1)",
            "math.randomseed(1.5)",
        ] {
            assert!(
                diagnostic_codes_for(EmmyrcLuaVersion::LuaJIT, source).is_empty(),
                "LuaJIT must accept {source:?}"
            );
        }
        assert!(
            has_code(
                EmmyrcLuaVersion::LuaJIT,
                "math.randomseed(\"x\")",
                "param-type-mismatch"
            ),
            "LuaJIT must reject a non-number seed"
        );
        assert!(
            has_code(
                EmmyrcLuaVersion::LuaJIT,
                "math.randomseed(1, 2)",
                "redundant-parameter"
            ),
            "LuaJIT must reject a second seed"
        );

        // Lua 5.1-5.3 require exactly one seed argument.
        for version in [
            EmmyrcLuaVersion::Lua51,
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::Lua53,
        ] {
            assert!(
                has_code(version, "math.randomseed()", "missing-parameter"),
                "math.randomseed() must require a seed in {version:?}"
            );
            assert!(
                diagnostic_codes_for(version, "math.randomseed(1)").is_empty(),
                "math.randomseed(1) must be accepted in {version:?}"
            );
            assert!(
                has_code(version, "math.randomseed(1, 2)", "redundant-parameter"),
                "math.randomseed(1, 2) must be rejected in {version:?}"
            );
        }

        // Lua 5.4+ accepts the no-argument form and the two-seed form, but its
        // seeds are integers: luaL_checkinteger rejects a fractional seed.
        for version in [EmmyrcLuaVersion::Lua54, EmmyrcLuaVersion::Lua55] {
            for source in [
                "math.randomseed()",
                "math.randomseed(1)",
                "math.randomseed(1, 2)",
            ] {
                assert!(
                    diagnostic_codes_for(version, source).is_empty(),
                    "{source:?} must be accepted in {version:?}"
                );
            }
            assert!(
                has_code(version, "math.randomseed(1.5)", "param-type-mismatch"),
                "math.randomseed(1.5) must be rejected in {version:?}"
            );
            assert!(
                has_code(version, "math.randomseed(1, 2, 3)", "redundant-parameter"),
                "math.randomseed(1, 2, 3) must be rejected in {version:?}"
            );
        }
    }

    #[test]
    fn test_bit32_replace_returns_integer() {
        // `bit32.replace` was missing its `@return`; the result is always an
        // unsigned 32-bit value, which the std library types as `integer`.
        assert_eq!(
            expr_type_for(EmmyrcLuaVersion::Lua52, "bit32.replace(0, 1, 0)"),
            "integer"
        );
    }

    #[test]
    fn test_bit32_vararg_operands_are_numbers() {
        // Lua 5.2 normalizes and truncates any numeric operand, so fractional
        // numbers are valid too. The negative case proves that the annotation
        // is actually applied to each variadic function.
        for function in ["band", "bor", "bxor", "btest"] {
            for args in ["1, 2, 3", "1.5, 2.5"] {
                let source = format!("bit32.{function}({args})");
                let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua52, &source);
                assert!(
                    !diagnostics.iter().any(|code| code == "param-type-mismatch"),
                    "numeric operands must be accepted in {source}: {diagnostics:?}"
                );
            }

            let source = format!("bit32.{function}(1, \"x\")");
            let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua52, &source);
            assert!(
                diagnostics.iter().any(|code| code == "param-type-mismatch"),
                "a string operand must be reported in {source}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_debug_sethook_hook_first() {
        // `debug.sethook([thread,] hook, mask [, count])`: the common
        // hook-first form must not be checked against the leading `thread`,
        // and the turn-off forms must stay accepted.
        let source = r#"
            local function hook(event, line) end
            local mask = "l"
            debug.sethook()
            debug.sethook(nil)
            debug.sethook(hook, "l")
            debug.sethook(hook, "l", 1)
            debug.sethook(hook, "l", nil)
            debug.sethook(hook, mask)
            debug.sethook(hook, mask, 1)
            local co = coroutine.create(function() end)
            debug.sethook(co)
            debug.sethook(co, nil)
            debug.sethook(co, hook, "l")
            debug.sethook(co, hook, "l", 1)
        "#;

        let versions = [
            EmmyrcLuaVersion::Lua51,
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::LuaJIT,
        ];
        for version in versions {
            let diagnostics = diagnostic_codes_for(version, source);
            assert!(
                !diagnostics.iter().any(|code| code == "param-type-mismatch"
                    || code == "redundant-parameter"
                    || code == "missing-parameter"),
                "valid debug.sethook calls must be accepted in {version:?}: {diagnostics:?}"
            );
        }

        // The mask is required whenever a hook is given, including the
        // thread-first form that does not go through the hook-first overload.
        let bad = [
            (
                "hook-only",
                "local function hook(event, line) end\ndebug.sethook(hook)",
                &["param-type-mismatch", "missing-parameter"][..],
            ),
            (
                "thread-first without mask",
                "local function hook(event, line) end\n\
                 local co = coroutine.create(function() end)\n\
                 debug.sethook(co, hook)",
                &["param-type-mismatch", "missing-parameter"][..],
            ),
            (
                "thread-first with nil mask",
                "local function hook(event, line) end\n\
                 local co = coroutine.create(function() end)\n\
                 debug.sethook(co, hook, nil)",
                &["param-type-mismatch"][..],
            ),
        ];
        for version in versions {
            for (case, source, expected) in bad {
                let diagnostics = diagnostic_codes_for(version, source);
                assert!(
                    diagnostics
                        .iter()
                        .any(|code| expected.contains(&code.as_str())),
                    "debug.sethook must reject {case} in {version:?}: {diagnostics:?}"
                );
            }
        }
    }

    #[test]
    fn test_debug_setuservalue_value_type_by_version() {
        // Lua 5.2 and LuaJIT (LJ_52) only accept a table (5.2 also accepts
        // nil); Lua 5.3+ accept any value.
        let source = r#"
            ---@param u userdata
            local function f(u)
                debug.setuservalue(u, 1)
            end
        "#;

        for version in [EmmyrcLuaVersion::Lua52, EmmyrcLuaVersion::LuaJIT] {
            let diagnostics = diagnostic_codes_for(version, source);
            assert!(
                diagnostics.iter().any(|code| code == "param-type-mismatch"),
                "setuservalue must reject a non-table value in {version:?}: {diagnostics:?}"
            );
        }
        for version in [EmmyrcLuaVersion::Lua53, EmmyrcLuaVersion::Lua54] {
            let diagnostics = diagnostic_codes_for(version, source);
            assert!(
                !diagnostics.iter().any(|code| code == "param-type-mismatch"),
                "setuservalue must accept any value in {version:?}: {diagnostics:?}"
            );
        }

        let nil_value = r#"
            ---@param u userdata
            local function f(u)
                debug.setuservalue(u, nil)
            end
        "#;
        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua52, nil_value);
        assert!(
            !diagnostics.iter().any(|code| code == "param-type-mismatch"),
            "Lua 5.2 setuservalue must accept nil: {diagnostics:?}"
        );

        // LuaJIT only accepts a table as the user value, so nil is invalid.
        let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::LuaJIT, nil_value);
        assert!(
            diagnostics.iter().any(|code| code == "param-type-mismatch"),
            "LuaJIT setuservalue must reject nil: {diagnostics:?}"
        );

        // The index is absent in 5.2/5.3 and optional from 5.4 on; LuaJIT
        // ignores it.
        let with_index = r#"
            ---@param u userdata
            local function f(u)
                debug.setuservalue(u, {}, 1)
            end
        "#;
        for version in [EmmyrcLuaVersion::Lua52, EmmyrcLuaVersion::Lua53] {
            let diagnostics = diagnostic_codes_for(version, with_index);
            assert!(
                diagnostics.iter().any(|code| code == "redundant-parameter"),
                "setuservalue must not expose the index in {version:?}: {diagnostics:?}"
            );
        }
        for version in [EmmyrcLuaVersion::Lua54, EmmyrcLuaVersion::LuaJIT] {
            let diagnostics = diagnostic_codes_for(version, with_index);
            assert!(
                !diagnostics.iter().any(|code| code == "redundant-parameter"),
                "setuservalue must accept the index in {version:?}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_debug_getuservalue_index_by_version() {
        // The index is absent in 5.2/5.3 and optional from 5.4 on. LuaJIT
        // ignores it, so the annotation accepts an extra argument.
        let source = r#"
            ---@param u userdata
            local function f(u)
                return debug.getuservalue(u, 1)
            end
        "#;

        for version in [EmmyrcLuaVersion::Lua52, EmmyrcLuaVersion::Lua53] {
            let diagnostics = diagnostic_codes_for(version, source);
            assert!(
                diagnostics.iter().any(|code| code == "redundant-parameter"),
                "getuservalue must not expose the index in {version:?}: {diagnostics:?}"
            );
        }
        for version in [EmmyrcLuaVersion::Lua54, EmmyrcLuaVersion::LuaJIT] {
            let diagnostics = diagnostic_codes_for(version, source);
            assert!(
                !diagnostics.iter().any(|code| code == "redundant-parameter"),
                "getuservalue must accept the index in {version:?}: {diagnostics:?}"
            );
        }

        let no_index = r#"
            ---@param u userdata
            local function f(u)
                return debug.getuservalue(u)
            end
        "#;
        for version in [
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::LuaJIT,
        ] {
            let diagnostics = diagnostic_codes_for(version, no_index);
            assert!(
                !diagnostics.iter().any(|code| code == "param-type-mismatch"
                    || code == "redundant-parameter"
                    || code == "missing-parameter"),
                "getuservalue must be callable without an index in {version:?}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_debug_uservalue_hidden_in_lua51() {
        for call in ["debug.getuservalue(u)", "debug.setuservalue(u, {})"] {
            let source = format!("---@param u userdata\nlocal function f(u)\n    {call}\nend");
            let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::Lua51, &source);
            assert!(
                diagnostics.iter().any(|code| code == "undefined-field"),
                "Lua 5.1 must not expose {call}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_package_loadlib_failure_location_type() {
        // `package.loadlib` reports where a failure happened as its third
        // result: "open" when the library cannot be loaded, "init" when the
        // function cannot be found or initialized. The third result is absent
        // on success, hence the nullable union.
        //
        // Lua 5.1's manual does not document the return list, but its
        // implementation (loadlib.c) and LuaJIT's lib_package.c both return
        // `nil, err, "open"|"init"` on failure, so every supported runtime is
        // covered here.
        for version in [
            EmmyrcLuaVersion::Lua51,
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::Lua55,
            EmmyrcLuaVersion::LuaJIT,
        ] {
            assert_eq!(
                expr_type_for(version, "package.loadlib(\"/lib/libfoo.so\", \"foo\")"),
                "(function)?",
                "package.loadlib must return a function or nil in {version:?}"
            );
            assert_eq!(
                expr_type_for(
                    version,
                    "select(3, package.loadlib(\"/lib/libfoo.so\", \"foo\"))"
                ),
                "(\"open\"|\"init\")?",
                "package.loadlib must expose the failure location in {version:?}"
            );
        }
    }

    #[test]
    fn test_string_buffer_new_accepts_options() {
        let diagnostics = diagnostic_codes_for(
            EmmyrcLuaVersion::LuaJIT,
            r#"
            local buffer = require("string.buffer")
            buffer.new()
            buffer.new(nil)
            buffer.new(nil, {})
            buffer.new(1024)
            buffer.new({ dict = { "a", "b" } })
            buffer.new({ dict = { "a", false }, metatable = { false } })
            buffer.new(1024, {})
            buffer.new(1024, { dict = { "a" } })
            "#,
        );
        for code in ["param-type-mismatch", "missing-fields", "missing-parameter"] {
            assert!(
                !diagnostics.iter().any(|diagnostic| diagnostic == code),
                "buffer.new must accept the documented arguments ({code}): {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_string_buffer_writers_accept_documented_arguments() {
        let diagnostics = diagnostic_codes_for(
            EmmyrcLuaVersion::LuaJIT,
            r#"
            local buffer = require("string.buffer")
            local ffi = require("ffi")
            local buf = buffer.new()
            buf:put()
            buf:put("x", 1, {})
            buf:putf("%s", true)
            buf:set("x")
            buf:set(ffi.new("char[4]"), 4)
            "#,
        );
        for code in [
            "param-type-mismatch",
            "missing-parameter",
            "redundant-parameter",
        ] {
            assert!(
                !diagnostics.iter().any(|diagnostic| diagnostic == code),
                "buffer writers must accept the documented arguments ({code}): {diagnostics:?}"
            );
        }

        for expression in [
            "require('string.buffer').new():put()",
            "require('string.buffer').new():put('x', 1)",
            "require('string.buffer').new():putf('%s', true)",
            "require('string.buffer').new():set('x')",
            "require('string.buffer').new():set(require('ffi').new('char[4]'), 4)",
        ] {
            assert_eq!(
                expr_type_for(EmmyrcLuaVersion::LuaJIT, expression),
                "string.buffer",
                "{expression} must return the buffer"
            );
        }
    }

    #[test]
    fn test_string_buffer_put_rejects_invalid_arguments() {
        for source in [
            r#"
            local buffer = require("string.buffer")
            local buf = buffer.new()
            buf:put(true)
            "#,
            r#"
            local buffer = require("string.buffer")
            local buf = buffer.new()
            buf:put("x", true)
            "#,
            r#"
            local buffer = require("string.buffer")
            local buf = buffer.new()
            buf:put(nil)
            "#,
        ] {
            let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::LuaJIT, source);
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic == "param-type-mismatch"),
                "{source} must be rejected: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_string_buffer_set_rejects_non_string_arguments() {
        for argument in ["1", "{}"] {
            let source = format!(
                r#"
                local buffer = require("string.buffer")
                local buf = buffer.new()
                buf:set({argument})
                "#
            );
            let diagnostics = diagnostic_codes_for(EmmyrcLuaVersion::LuaJIT, &source);
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic == "param-type-mismatch"),
                "{source} must be rejected: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_string_buffer_serializer_accepts_supported_values() {
        let diagnostics = diagnostic_codes_for(
            EmmyrcLuaVersion::LuaJIT,
            r#"
            local buffer = require("string.buffer")
            local ffi = require("ffi")
            local buf = buffer.new()
            buf:encode(nil)
            buf:encode(false)
            buf:encode(true)
            buf:encode(1)
            buf:encode("x")
            buf:encode({})
            buf:encode(ffi.new("int64_t", 1))
            buf:decode()
            buffer.encode(true)
            buffer.decode("x")
            "#,
        );
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic == "param-type-mismatch"),
            "the serializer must accept supported Lua types: {diagnostics:?}"
        );
    }

    #[test]
    fn test_string_buffer_free_returns_buffer() {
        assert_eq!(
            expr_type_for(
                EmmyrcLuaVersion::LuaJIT,
                "require('string.buffer').new():free()"
            ),
            "string.buffer"
        );

        let diagnostics = diagnostic_codes_for(
            EmmyrcLuaVersion::LuaJIT,
            r#"
            local buffer = require("string.buffer")
            buffer.new():free():reset():commit(0)
            "#,
        );
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic == "param-type-mismatch"
                    || diagnostic == "missing-parameter"
                    || diagnostic == "call-non-callable"),
            "free() must return the buffer for chaining: {diagnostics:?}"
        );
    }

    #[test]
    fn test_string_buffer_operators() {
        assert_eq!(
            expr_type_for(
                EmmyrcLuaVersion::LuaJIT,
                "#(require('string.buffer').new())"
            ),
            "integer"
        );
        assert_eq!(
            expr_type_for(
                EmmyrcLuaVersion::LuaJIT,
                "require('string.buffer').new() .. 'x'"
            ),
            "string"
        );
        assert_eq!(
            expr_type_for(
                EmmyrcLuaVersion::LuaJIT,
                "'x' .. require('string.buffer').new()"
            ),
            "string"
        );
    }

    #[test]
    fn test_string_buffer_is_only_available_in_jit() {
        let source = r#"local _ = require("string.buffer")"#;
        for version in [
            EmmyrcLuaVersion::Lua51,
            EmmyrcLuaVersion::Lua52,
            EmmyrcLuaVersion::Lua53,
            EmmyrcLuaVersion::Lua54,
            EmmyrcLuaVersion::Lua55,
        ] {
            assert!(
                diagnostic_codes_for(version, source)
                    .iter()
                    .any(|code| code == "unresolved-require"),
                "string.buffer must be unavailable in {version:?}"
            );
        }
        assert!(
            !diagnostic_codes_for(EmmyrcLuaVersion::LuaJIT, source)
                .iter()
                .any(|code| code == "unresolved-require"),
            "string.buffer must be available in LuaJIT"
        );
    }
}
