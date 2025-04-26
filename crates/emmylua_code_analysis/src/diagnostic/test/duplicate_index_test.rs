#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_duplicate_index() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::DuplicateIndex,
            r#"
                local a = {
                    b = 1,
                    b = 1
                }
            "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::DuplicateIndex,
            r#"
                local a = {
                    a = 1,
                    b = 1
                }
            "#
        ));
    }
}
