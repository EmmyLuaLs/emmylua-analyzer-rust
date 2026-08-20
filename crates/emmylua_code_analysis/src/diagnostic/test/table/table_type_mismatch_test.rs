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

    fn first_assign_diagnostic_message(ws: &mut VirtualWorkspace, source: &str) -> String {
        assign_type_diagnostics(ws, source)
            .into_iter()
            .next()
            .map(|diagnostic| diagnostic.message)
            .unwrap_or_default()
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
    fn nullable_root_table_reports_deepest_mismatch() {
        let mut ws = VirtualWorkspace::new();
        let source = r#"---@class Icon
---@field icon string

---@class IconList
---@field one Icon

---@class A
---@field icon_list? IconList

---@type A
local tmp

---@type string?
local a

tmp.icon_list = {
    one = {
        icon = a
    }
}"#;
        let expected_line = source
            .lines()
            .position(|line| line.contains("icon = a"))
            .unwrap() as u32;

        let diagnostics = assign_type_diagnostics(&mut ws, source);

        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert_eq!(diagnostics[0].range.start.line, expected_line);
        assert!(
            diagnostics[0]
                .message
                .contains("Cannot assign `string?` to `string`.")
        );
    }

    #[test]
    fn generic_table_targets_report_deepest_mismatch() {
        let mut ws = VirtualWorkspace::new();
        let source = r#"---@class Box<T>
---@field value T

---@class Root
---@field child Box<string>
---@field optional_child Box<string>?

---@type Root
local root = {
    child = {
        value = 1,
    },
    optional_child = {
        value = 2,
    },
}

local direct ---@type Box<string>?
direct = {
    value = 3,
}"#;
        let expected_lines = source
            .lines()
            .enumerate()
            .filter_map(|(line, text)| text.contains("value =").then_some(line as u32))
            .collect::<Vec<_>>();

        let diagnostics = assign_type_diagnostics(&mut ws, source);

        assert_eq!(diagnostics.len(), 3, "{diagnostics:#?}");
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.range.start.line)
                .collect::<Vec<_>>(),
            expected_lines
        );
        assert!(diagnostics.iter().enumerate().all(|(i, diagnostic)| {
            diagnostic
                .message
                .contains(&format!("Cannot assign `{}` to `string`.", i + 1))
        }));
    }

    #[test]
    fn nullable_scalar_table_keeps_outer_mismatch() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let source = r#"---@type "x"?
local target

target = {
    len = 1,
}"#;
        let expected_line = source
            .lines()
            .position(|line| line.contains("target ="))
            .unwrap() as u32;

        let diagnostics = assign_type_diagnostics(&mut ws, source);

        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert_eq!(diagnostics[0].range.start.line, expected_line);
        assert!(diagnostics[0].message.contains("to `\"x\"?`"));
    }

    #[test]
    fn nullable_scalar_generic_alias_keeps_outer_mismatch() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let source = r#"---@alias Identity<T> T

---@type Identity<"x">?
local target

target = {
    len = 1,
}"#;
        let expected_line = source
            .lines()
            .position(|line| line.contains("target ="))
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
    fn nullable_union_mismatch_does_not_render_nil_branch() {
        let mut ws = VirtualWorkspace::new();
        let source = r#"local target ---@type boolean?
local source ---@type string?
target = source"#;

        let diagnostics = assign_type_diagnostics(&mut ws, source);

        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert_eq!(
            diagnostics[0].message,
            "Cannot assign `string?` to `boolean?`.\n  Type 'string' is not assignable to type 'boolean?'."
        );
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

    #[test]
    fn tail_variadic_ignores_trailing_comment() {
        let mut ws = VirtualWorkspace::new();
        let source = r#"---@return string, number
local function pair() end

---@type string[]
local target = {
    pair(), -- tail
}"#;
        let expected_line = source
            .lines()
            .position(|line| line.contains("pair(),"))
            .unwrap() as u32;

        let diagnostics = assign_type_diagnostics(&mut ws, source);

        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert_eq!(diagnostics[0].range.start.line, expected_line);
    }

    #[test]
    fn union_table_mismatch_reports_deepest_field_mismatch() {
        let mut ws = VirtualWorkspace::new();
        let source = r#"---@class C
---@field type "one"

---@class D
---@field type "two"

---@param cd C | D
local function cd(cd) end

cd({ type = "test" })"#;

        let message = first_assign_diagnostic_message(&mut ws, source);
        assert_eq!(message, "Cannot assign `\"test\"` to `(\"one\"|\"two\")`.");
    }
}
