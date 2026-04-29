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
    fn explicit_public_and_internal_access_modifiers_report_inconsistency() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::InconsistentTypeAccessModifier,
            r#"
                ---@class (public) Foo
                local Foo = {}

                ---@class (internal) Foo
                local FooInternal = {}
            "#
        ));
    }

    #[test]
    fn implicit_and_explicit_public_access_modifiers_stay_consistent() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::InconsistentTypeAccessModifier,
            r#"
                ---@class Foo
                local Foo = {}

                ---@class (public) Foo
                local FooPublic = {}
            "#
        ));
    }

    #[test]
    fn implicit_public_and_internal_access_modifiers_report_inconsistency() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::InconsistentTypeAccessModifier,
            r#"
                ---@class Foo
                local Foo = {}

                ---@class (internal) Foo
                local FooInternal = {}
            "#
        ));
    }

    #[test]
    fn private_and_implicit_public_access_modifiers_report_inconsistency() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::InconsistentTypeAccessModifier,
            r#"
                ---@class (private) Foo
                local Foo = {}

                ---@class Foo
                local FooPublic = {}
            "#
        ));
    }

    #[test]
    fn partial_internal_access_modifiers_stay_consistent_within_same_workspace() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::InconsistentTypeAccessModifier,
            r#"
                ---@class (partial,internal) Foo
                local Foo = {}

                ---@class (partial,internal) Foo
                local FooInternal = {}
            "#
        ));
    }

    #[test]
    fn partial_public_and_internal_access_modifiers_report_inconsistency() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::InconsistentTypeAccessModifier,
            r#"
                ---@class (partial,public) Foo
                local Foo = {}

                ---@class (partial,internal) Foo
                local FooInternal = {}
            "#
        ));
    }

    #[test]
    fn private_types_in_other_files_do_not_affect_current_file() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "lib.lua",
            r#"
                ---@class (private) Foo
                local Foo = {}
            "#,
        );
        let main_file = ws.def_file(
            "main.lua",
            r#"
                ---@class Foo
                local Foo = {}
            "#,
        );

        assert!(!has_diagnostic(
            &mut ws,
            main_file,
            DiagnosticCode::InconsistentTypeAccessModifier,
        ));
    }

    #[test]
    fn internal_types_in_different_workspaces_do_not_affect_each_other() {
        let mut ws = VirtualWorkspace::new();
        ws.analysis.add_library_workspace(&WorkspaceFolder::new(
            ws.virtual_url_generator.new_path("lib"),
            true,
        ));
        ws.def_file(
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
            main_file,
            DiagnosticCode::InconsistentTypeAccessModifier,
        ));
    }

    #[test]
    fn public_types_in_different_workspaces_do_not_affect_each_other() {
        let mut ws = VirtualWorkspace::new();
        ws.analysis.add_library_workspace(&WorkspaceFolder::new(
            ws.virtual_url_generator.new_path("lib"),
            true,
        ));
        ws.def_file(
            "lib/foo.lua",
            r#"
                ---@class (partial,public) Foo
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

        assert!(has_diagnostic(
            &mut ws,
            main_file,
            DiagnosticCode::InconsistentTypeAccessModifier,
        ));
    }
}
