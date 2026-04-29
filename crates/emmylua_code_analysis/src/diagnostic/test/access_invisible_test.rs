#[cfg(test)]
mod tests {
    use crate::{DiagnosticCode, Emmyrc, EmmyrcLuaVersion, VirtualWorkspace};

    #[test]
    fn test_issue_289() {
        let mut ws = VirtualWorkspace::new();
        let mut config = Emmyrc::default();
        config.runtime.version = EmmyrcLuaVersion::LuaJIT;
        ws.analysis.update_config(config.into());
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AccessInvisible,
            r#"
            local file = io.open("test.txt", "r")
            if file then
                file:close()
            end
            "#
        ));
    }

    #[test]
    fn test_1() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AccessInvisible,
            r#"
                ---@class (partial) Log
                ---@field private logLevel table<Log.Level, integer>
                local M

                ---@enum (key) Log.Level
                M.logLevel = {
                    trace = 1,
                }
            "#
        ));
    }

    #[test]
    fn test_2() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "test.lua",
            r#"
            ---@class (partial) Log
            ---@field private logLevel table<Log.Level, integer>
            local M

            ---@enum (key) Log.Level
            M.logLevel = {
                trace = 1,
            }

            return M
        "#,
        );

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AccessInvisible,
            r#"
                local Log = require("test")
                Log.logLevel = 1
            "#
        ));
    }

    #[test]
    fn test_3() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AccessInvisible,
            r#"
                local M = {}
                ---@private
                function M.init()
                    ---@private
                    M.log = 1
                end

                local function step_collector()
                    if M.log then
                    end
                end
            "#
        ));
    }
}
