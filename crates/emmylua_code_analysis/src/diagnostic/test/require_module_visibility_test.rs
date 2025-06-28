#[cfg(test)]
mod tests {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_1() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_check(DiagnosticCode::RequireModuleNotVisible);

        // 定义具有 @export namespace 限制的模块
        ws.def_file(
            "test.lua",
            r#"
                ---@namespace Test

                ---@export namespace
                local M = {}

                return M
                "#,
        );

        assert!(!ws.check_code_for(
            DiagnosticCode::RequireModuleNotVisible,
            r#"
                local a = require("test")
            "#,
        ));
    }

    #[test]
    fn test_2() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_check(DiagnosticCode::RequireModuleNotVisible);

        // `@export`没有命名空间限制
        ws.def_file(
            "test.lua",
            r#"
                ---@namespace Test

                ---@export
                local M = {}

                return M
                "#,
        );

        assert!(ws.check_code_for(
            DiagnosticCode::RequireModuleNotVisible,
            r#"
                ---@namespace AA
                local a = require("test")
            "#,
        ));
    }
    #[test]
    fn test_3() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_check(DiagnosticCode::RequireModuleNotVisible);

        // 如果是`@export namespace` 但当前文件没有命名空间, 则视为不可见
        ws.def_file(
            "test.lua",
            r#"
                ---@export namespace
                local M = {}

                return M
                "#,
        );

        assert!(!ws.check_code_for(
            DiagnosticCode::RequireModuleNotVisible,
            r#"
                local a = require("test")
            "#,
        ));
    }
}
