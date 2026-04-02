#[cfg(test)]
mod tests {
    use crate::{DiagnosticCode, LuaType, VirtualWorkspace};

    #[test]
    fn internal_type_reference_reports_diagnostic_but_still_resolves() {
        let mut ws = VirtualWorkspace::new();
        ws.analysis
            .add_library_workspace(ws.virtual_url_generator.new_path("lib"));
        ws.def_file(
            "lib/types.lua",
            r#"
                ---@namespace Shared
                ---@class (internal) Hidden
                local Hidden = {}
            "#,
        );

        assert!(matches!(ws.ty("Shared.Hidden"), LuaType::Ref(_)));
        assert!(!ws.check_code_for(
            DiagnosticCode::InvisibleTypeReference,
            r#"
                ---@type Shared.Hidden
                local value
            "#
        ));
    }

    #[test]
    fn default_public_type_reference_does_not_report_visibility_diagnostic() {
        let mut ws = VirtualWorkspace::new();
        ws.analysis
            .add_library_workspace(ws.virtual_url_generator.new_path("lib"));
        ws.def_file(
            "lib/types.lua",
            r#"
                ---@namespace Shared
                ---@class Visible
                local Visible = {}
            "#,
        );

        assert!(ws.check_code_for(
            DiagnosticCode::InvisibleTypeReference,
            r#"
                ---@type Shared.Visible
                local value
            "#
        ));
    }
}
