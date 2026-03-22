#[cfg(test)]
mod tests {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_unresolved_require() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.check_code_for(
            DiagnosticCode::UnresolvedRequire,
            r#"
            local a = require("missing.module")
            "#,
        ));
    }

    #[test]
    fn test_resolved_require() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "test.lua",
            r#"
            local M = {}
            return M
            "#,
        );

        assert!(ws.check_code_for(
            DiagnosticCode::UnresolvedRequire,
            r#"
            local a = require("test")
            "#,
        ));
    }

    #[test]
    fn test_non_literal_require() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UnresolvedRequire,
            r#"
            local function module_name()
                return "missing.module"
            end
            local a = require(module_name)
            "#,
        ));
    }
}
