#[cfg(test)]
mod tests {
    use lsp_types::{Diagnostic, NumberOrString};
    use tokio_util::sync::CancellationToken;

    use crate::{DiagnosticCode, VirtualWorkspace};

    fn assign_type_diagnostics(ws: &mut VirtualWorkspace, source: &str) -> Vec<Diagnostic> {
        ws.analysis
            .diagnostic
            .enable_only(DiagnosticCode::AssignTypeMismatch);
        let file_id = ws.def(source);
        let code = Some(NumberOrString::String(
            DiagnosticCode::AssignTypeMismatch.get_name().to_string(),
        ));
        ws.analysis
            .diagnose_file(file_id, CancellationToken::new())
            .unwrap_or_default()
            .into_iter()
            .filter(|diagnostic| diagnostic.code == code)
            .collect()
    }

    #[test]
    fn test_annotated_table_field_value_mismatch() {
        let mut ws = VirtualWorkspace::new();
        let diagnostics = assign_type_diagnostics(
            &mut ws,
            r#"
            ---@class AnnotatedNestedTarget
            ---@field value string

            ---@type AnnotatedNestedTarget
            local target = {
                ---@type integer
                value = "",
            }
            "#,
        );

        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert!(
            diagnostics[0]
                .message
                .contains("Cannot assign `string` to the `@type integer` annotation")
        );
        assert_eq!(diagnostics[0].range.start.line, 7);
        assert_eq!(diagnostics[0].range.end.line, 7);
    }

    #[test]
    fn test_annotated_table_field_reports_once_without_outer_type() {
        let mut ws = VirtualWorkspace::new();
        let diagnostics = assign_type_diagnostics(
            &mut ws,
            r#"
            local target = {
                ---@type integer
                value = "",
            }
            "#,
        );

        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert!(diagnostics[0].message.contains("@type integer"));
    }

    #[test]
    fn test_annotated_table_field_reports_once_with_object_target() {
        let mut ws = VirtualWorkspace::new();
        let diagnostics = assign_type_diagnostics(
            &mut ws,
            r#"
            ---@type { value: string }
            local target = {
                ---@type integer
                value = "",
            }
            "#,
        );

        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert!(diagnostics[0].message.contains("@type integer"));
    }

    #[test]
    fn test_matching_annotated_table_field_value() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            local target = {
                ---@type integer
                value = 1,
            }
            "#,
        ));
    }
}
