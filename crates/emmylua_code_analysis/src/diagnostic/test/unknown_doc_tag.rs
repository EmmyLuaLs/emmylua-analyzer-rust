#[cfg(test)]
mod tests {
    use crate::{DiagnosticCode, VirtualWorkspace};
    use std::ops::Deref;
    use std::sync::Arc;

    #[test]
    fn test_unknown_doc_tag() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.check_code_for(
            DiagnosticCode::UnknownDocTag,
            r#"
            ---@foobar
            function bar() end
        "#
        ));
    }

    #[test]
    fn test_known_doc_tag() {
        let mut ws = VirtualWorkspace::new();

        let mut emmyrc = ws.analysis.emmyrc.deref().clone();
        emmyrc.doc.known_tags.push("foobar".into());
        ws.analysis.update_config(Arc::new(emmyrc));

        assert!(ws.check_code_for(
            DiagnosticCode::UnknownDocTag,
            r#"
            ---@foobar
            function bar() end
        "#
        ));
    }
}
