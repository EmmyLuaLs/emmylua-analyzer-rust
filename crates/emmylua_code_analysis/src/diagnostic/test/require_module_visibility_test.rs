#[cfg(test)]
mod tests {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn internal_return_table_is_not_visible_outside_current_project() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_check(DiagnosticCode::RequireModuleNotVisible);
        ws.analysis
            .add_library_workspace(ws.virtual_url_generator.new_path("lib"));

        ws.def_file(
            "lib/test.lua",
            r#"
                ---@internal
                return {}
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
    fn appending_visibility_label_when_return_nameexpr_is_invalid() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_check(DiagnosticCode::RequireModuleNotVisible);
        ws.analysis
            .add_library_workspace(ws.virtual_url_generator.new_path("lib"));

        {
            // 返回 NameExpr 时, 附加在 return 语句上的可见性标签无效
            ws.def_file(
                "lib/testA.lua",
                r#"
                local m = {}

                ---@internal
                return m
                "#,
            );

            assert!(ws.check_code_for(
                DiagnosticCode::RequireModuleNotVisible,
                r#"
                local a = require("testA")
                "#,
            ));
        }

        {
            ws.def_file(
                "lib/testB.lua",
                r#"
                ---@internal
                local m = {}

                return m
                "#,
            );

            assert!(!ws.check_code_for(
                DiagnosticCode::RequireModuleNotVisible,
                r#"
                local a = require("testB")
                "#,
            ));
        }
    }

    #[test]
    fn public_return_owner_is_visible_outside_current_project() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_check(DiagnosticCode::RequireModuleNotVisible);
        ws.analysis
            .add_library_workspace(ws.virtual_url_generator.new_path("lib"));

        ws.def_file(
            "lib/test.lua",
            r#"
                ---@public
                local export = {}

                return export
                "#,
        );

        assert!(ws.check_code_for(
            DiagnosticCode::RequireModuleNotVisible,
            r#"
                local a = require("test")
            "#,
        ));
    }

    #[test]
    fn internal_return_owner_is_not_visible_outside_current_project() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_check(DiagnosticCode::RequireModuleNotVisible);
        ws.analysis
            .add_library_workspace(ws.virtual_url_generator.new_path("lib"));

        ws.def_file(
            "lib/test.lua",
            r#"
                ---@internal
                local export = {}

                return export
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
    fn default_return_is_public_outside_current_project() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_check(DiagnosticCode::RequireModuleNotVisible);
        ws.analysis
            .add_library_workspace(ws.virtual_url_generator.new_path("lib"));

        ws.def_file(
            "lib/test.lua",
            r#"
                local m = {}

                return m
                "#,
        );

        assert!(ws.check_code_for(
            DiagnosticCode::RequireModuleNotVisible,
            r#"
                local a = require("test")
            "#,
        ));
    }

    #[test]
    fn multiple_return_expressions_use_first_return_expression_as_base() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_check(DiagnosticCode::RequireModuleNotVisible);
        ws.analysis
            .add_library_workspace(ws.virtual_url_generator.new_path("lib"));
        // 对于模块, 我们取第一个返回表达式为基准, 因此后续 return 不会扩大可见性
        ws.def_file(
            "lib/test.lua",
            r#"
                ---@public
                local function export()
                end

                local flag = true
                if flag then
                    ---@internal
                    return {}
                end

                return export
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
    fn explicit_internal_return_path_keeps_internal_visibility() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_check(DiagnosticCode::RequireModuleNotVisible);
        ws.analysis
            .add_library_workspace(ws.virtual_url_generator.new_path("lib"));

        ws.def_file(
            "lib/test.lua",
            r#"
                local flag = true
                if flag then
                    ---@internal
                    return {
                        ping = function()
                        end,
                    }
                end

                return {
                    ping = function()
                    end,
                }
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
    fn return_call_expr_use_public_visibility() {
        let mut ws = VirtualWorkspace::new();
        ws.analysis
            .add_library_workspace(ws.virtual_url_generator.new_path("lib"));

        // todo: 处理直接返回函数调用表达式时附加可见性的情况
        ws.def_file(
            "lib/test.lua",
            r#"
                local flag = true
                local function make_api()
                    return {
                        ping = function()
                        end,
                    }
                end
                ---@public
                local export = make_api()
                return export

                -- 我们暂时不处理直接返回函数调用表达式时附加可见性的情况
                -- ---@public
                -- return make_api()
                "#,
        );

        assert!(ws.check_code_for(
            DiagnosticCode::RequireModuleNotVisible,
            r#"
                local a = require("test")
            "#,
        ));
    }
}
