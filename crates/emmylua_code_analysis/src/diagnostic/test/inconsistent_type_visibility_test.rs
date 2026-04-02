#[cfg(test)]
mod tests {
    use crate::{DiagnosticCode, VirtualWorkspace};

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
        ws.analysis
            .add_library_workspace(ws.virtual_url_generator.new_path("lib"));
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
}
