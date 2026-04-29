#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, EmmyrcLuaVersion, VirtualWorkspace};

    #[test]
    fn test_1() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::SyntaxError,
            r#"
            local function aaa(..., n)
            end
        "#
        ));
    }

    #[test]
    fn test_luajit_ull() {
        let mut ws = VirtualWorkspace::new();
        let mut config = ws.get_emmyrc();
        config.runtime.version = EmmyrcLuaVersion::LuaJIT;
        ws.update_emmyrc(config);
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::SyntaxError,
            r#"
            local d = 0xFFFFFFFFFFFFFFFFULL
        "#
        ));
    }
}
