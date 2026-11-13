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

    fn diagnostic_codes_for(version: EmmyrcLuaVersion, source: &str) -> Vec<String> {
        let mut config = Emmyrc::default();
        config.runtime.version = version;
        let mut ws = VirtualWorkspace::new_with_config(config);

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
        let mut config = Emmyrc::default();
        config.runtime.version = version;
        let mut ws = VirtualWorkspace::new_with_config(config);

        let ty = ws.expr_ty(expr);
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
}
