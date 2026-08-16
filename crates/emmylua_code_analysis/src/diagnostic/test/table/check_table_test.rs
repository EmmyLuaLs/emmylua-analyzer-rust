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
    fn nested_table_fields_report_each_deepest_mismatch() {
        let mut ws = VirtualWorkspace::new();
        let source = r#"---@class NestedLeaf
---@field a integer
---@field b integer
---@class NestedRoot
---@field x NestedLeaf
---@type NestedRoot
local target = {
    x = {
        a = "a",
        b = "b",
    },
}"#;
        let expected_lines = source
            .lines()
            .enumerate()
            .filter_map(|(line, text)| {
                (text.contains("a = \"") || text.contains("b = \"")).then_some(line as u32)
            })
            .collect::<Vec<_>>();

        let diagnostics = assign_type_diagnostics(&mut ws, source);

        assert_eq!(diagnostics.len(), 2, "{diagnostics:#?}");
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.range.start.line)
                .collect::<Vec<_>>(),
            expected_lines
        );
    }

    #[test]
    fn nullable_nested_table_reports_deepest_mismatch() {
        let mut ws = VirtualWorkspace::new();
        let source = r#"---@class NullableNestedLeaf
---@field value integer
---@class NullableNestedRoot
---@field child NullableNestedLeaf?
---@type NullableNestedRoot
local target = {
    child = {
        value = "invalid",
    },
}"#;
        let expected_line = source
            .lines()
            .position(|line| line.contains("value ="))
            .unwrap() as u32;

        let diagnostics = assign_type_diagnostics(&mut ws, source);

        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert_eq!(diagnostics[0].range.start.line, expected_line);
    }

    #[test]
    fn nullable_array_element_reports_incompatible_non_nil_branch() {
        let mut ws = VirtualWorkspace::new();
        let source = r#"---@type string?
local value

---@type boolean[]
local target = {
    value,
}"#;

        assert!(!ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, source));
    }

    #[test]
    fn tail_variadic_uses_sequence_index_after_named_fields() {
        let mut ws = VirtualWorkspace::new();
        let source = r#"---@return number, string
local function pair() end

---@type { tag: boolean, [1]: boolean, [2]: string, [3]: string }
local target = {
    tag = true,
    true,
    pair(),
}"#;
        let expected_line = source
            .lines()
            .position(|line| line.contains("pair(),"))
            .unwrap() as u32;

        let diagnostics = assign_type_diagnostics(&mut ws, source);

        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert_eq!(diagnostics[0].range.start.line, expected_line);
    }

    // #[test]
    // fn test_last_variadic() {
    //     let mut ws = VirtualWorkspace::new();

    //     let source = r#"            local function values()
    //         return 1, "a", true
    //         end

    //         ---@type [integer, string, string]
    //         local t = { values() }
    //         "#;

    //     let diagnostics = assign_type_diagnostics(&mut ws, source);
    //     dbg!(&diagnostics);
    //     // assert!(!ws.has_no_diagnostic(
    //     //     DiagnosticCode::AssignTypeMismatch,
    //     //     r#"
    //     //     local function values()
    //     //     return 1, "a", true
    //     //     end

    //     //     ---@type [integer, string, string]
    //     //     local t = { values() }
    //     // "#
    //     // ));
    // }
}
