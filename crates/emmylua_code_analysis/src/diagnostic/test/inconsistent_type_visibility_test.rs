#[cfg(test)]
mod tests {
    use lsp_types::NumberOrString;
    use tokio_util::sync::CancellationToken;

    use crate::{DiagnosticCode, FileId, VirtualWorkspace, WorkspaceFolder};

    fn has_diagnostic(
        ws: &mut VirtualWorkspace,
        file_id: FileId,
        diagnostic_code: DiagnosticCode,
    ) -> bool {
        ws.analysis.diagnostic.enable_only(diagnostic_code);
        let diagnostics = ws
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .unwrap_or_default();
        let code = Some(NumberOrString::String(
            diagnostic_code.get_name().to_string(),
        ));

        diagnostics.iter().any(|diagnostic| diagnostic.code == code)
    }

    #[test]
    fn explicit_public_and_internal_visibility_report_inconsistency() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::InconsistentTypeVisibility,
            r#"
                ---@class (public) Foo
                local Foo = {}

                ---@class (internal) Foo
                local FooInternal = {}
            "#
        ));
    }

    #[test]
    fn default_visibility_difference_does_not_report_inconsistency() {
        let mut ws = VirtualWorkspace::new();
        ws.analysis.add_library_workspace(&WorkspaceFolder::new(
            ws.virtual_url_generator.new_path("lib"),
            true,
        ));
        ws.def_file(
            "lib/foo.lua",
            r#"
                ---@class Foo
                local Foo = {}
            "#,
        );

        assert!(ws.check_code_for(
            DiagnosticCode::InconsistentTypeVisibility,
            r#"
                ---@class Foo
                local Foo = {}
            "#
        ));
    }

    #[test]
    fn partial_internal_visibility_stays_consistent_within_same_workspace() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::InconsistentTypeVisibility,
            r#"
                ---@class (partial,internal) Foo
                local Foo = {}

                ---@class (partial,internal) Foo
                local FooInternal = {}
            "#
        ));
    }

    #[test]
    fn partial_public_and_internal_visibility_report_inconsistency() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::InconsistentTypeVisibility,
            r#"
                ---@class (partial,public) Foo
                local Foo = {}

                ---@class (partial,internal) Foo
                local FooInternal = {}
            "#
        ));
    }

    #[test]
    fn internal_partial_types_in_different_workspaces_do_not_affect_each_other() {
        let mut ws = VirtualWorkspace::new();
        ws.analysis.add_library_workspace(&WorkspaceFolder::new(
            ws.virtual_url_generator.new_path("lib"),
            true,
        ));
        let library_file = ws.def_file(
            "lib/foo.lua",
            r#"
                ---@class (partial,internal) Foo
                local Foo = {}
            "#,
        );
        let main_file = ws.def_file(
            "main.lua",
            r#"
                ---@class (partial,internal) Foo
                local Foo = {}
            "#,
        );

        assert!(!has_diagnostic(
            &mut ws,
            library_file,
            DiagnosticCode::InconsistentTypeVisibility,
        ));
        assert!(!has_diagnostic(
            &mut ws,
            main_file,
            DiagnosticCode::InconsistentTypeVisibility,
        ));
    }
}
