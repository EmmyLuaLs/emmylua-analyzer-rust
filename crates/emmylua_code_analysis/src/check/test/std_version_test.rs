#[cfg(test)]
mod test {
    use lsp_types::NumberOrString;
    use tokio_util::sync::CancellationToken;

    use crate::{Emmyrc, EmmyrcLuaVersion, VirtualWorkspace};

    fn diagnostic_codes_for(version: EmmyrcLuaVersion, source: &str) -> Vec<String> {
        let mut ws = VirtualWorkspace::new();
        let mut config = Emmyrc::default();
        config.runtime.version = version;
        ws.update_emmyrc(config);
        ws.analysis.init_std_lib(None);

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
        let mut ws = VirtualWorkspace::new();
        let mut config = Emmyrc::default();
        config.runtime.version = version;
        ws.update_emmyrc(config);
        ws.analysis.init_std_lib(None);

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
}
