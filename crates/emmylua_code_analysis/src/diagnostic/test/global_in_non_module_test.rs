#[cfg(test)]
mod tests {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_global_in_non_module() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::GlobalInNonModule,
            r#"
            local function name()
                bbbb = 123
            end
        "#
        ));
    }
}
