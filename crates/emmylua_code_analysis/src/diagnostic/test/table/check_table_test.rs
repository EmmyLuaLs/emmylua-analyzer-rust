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
    fn nested_table_depth_limit_reports_current_field() {
        let mut ws = VirtualWorkspace::new();
        let mut source = String::new();
        for depth in 0..33 {
            source.push_str(&format!(
                "---@class NestedDepth{depth}\n---@field next NestedDepth{}\n",
                depth + 1
            ));
        }
        source.push_str(
            "---@class NestedDepth33\n---@field value integer\n---@type NestedDepth0\nlocal target = {\n",
        );

        let mut expected_line = 0;
        for depth in 0..33 {
            if depth == 32 {
                expected_line = source.lines().count() as u32;
            }
            source.push_str(&format!("{}next = {{\n", "    ".repeat(depth + 1)));
        }
        source.push_str(&format!("{}value = \"invalid\",\n", "    ".repeat(34)));
        for depth in (0..=33).rev() {
            source.push_str(&format!("{}}},\n", "    ".repeat(depth)));
        }

        let diagnostics = assign_type_diagnostics(&mut ws, &source);

        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert_eq!(diagnostics[0].range.start.line, expected_line);
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
