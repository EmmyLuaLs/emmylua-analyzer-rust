#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use crate::{DiagnosticCode, VirtualWorkspace};

    #[gtest]
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

        let diagnostics = ws.get_diagnostics(DiagnosticCode::AssignTypeMismatch, source);

        assert_that!(diagnostics.len(), eq(2));
        assert_that!(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.range.start.line)
                .collect::<Vec<_>>(),
            eq(&expected_lines)
        );
    }

    #[gtest]
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

        let diagnostics = ws.get_diagnostics(DiagnosticCode::AssignTypeMismatch, source);

        assert_that!(diagnostics.len(), eq(1));
        assert_that!(diagnostics[0].range.start.line, eq(expected_line));
        assert_that!(
            diagnostics[0].message,
            eq("Type `\"invalid\"` is not assignable to type `integer`.")
        );
    }

    #[gtest]
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

        let diagnostics = ws.get_diagnostics(DiagnosticCode::AssignTypeMismatch, source);

        assert_that!(diagnostics.len(), eq(1));
        assert_that!(diagnostics[0].range.start.line, eq(expected_line));
        assert_that!(
            diagnostics[0].message,
            eq("Type `string?` is not assignable to type `string`.
  Type `nil` is not assignable to type `string`.")
        );
    }

    #[gtest]
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

        let diagnostics = ws.get_diagnostics(DiagnosticCode::AssignTypeMismatch, source);

        assert_that!(diagnostics.len(), eq(3));
        assert_that!(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.range.start.line)
                .collect::<Vec<_>>(),
            eq(&expected_lines)
        );
        assert_that!(
            diagnostics[0].message,
            eq("Type `1` is not assignable to type `string`.")
        );
        assert_that!(
            diagnostics[1].message,
            eq("Type `2` is not assignable to type `string`.")
        );
        assert_that!(
            diagnostics[2].message,
            eq("Type `3` is not assignable to type `string`.")
        );
    }

    #[gtest]
    fn nullable_scalar_table_reports_deepest_mismatch() {
        let mut ws = VirtualWorkspace::new();
        let source = r#"---@type "x"?
local target

target = {
    len = 1,
}"#;
        let expected_line = source
            .lines()
            .position(|line| line.contains("target ="))
            .unwrap() as u32;

        let diagnostics = ws.get_diagnostics(DiagnosticCode::AssignTypeMismatch, source);

        assert_that!(diagnostics.len(), eq(1));
        assert_that!(diagnostics[0].range.start.line, eq(expected_line));
        assert_that!(
            diagnostics[0].message,
            eq("Type `{ len = 1 }` is not assignable to type `\"x\"`.")
        );
    }

    #[gtest]
    fn nullable_scalar_generic_alias_reports_deepest_mismatch() {
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

        let diagnostics = ws.get_diagnostics(DiagnosticCode::AssignTypeMismatch, source);

        assert_that!(diagnostics.len(), eq(1));
        assert_that!(diagnostics[0].range.start.line, eq(expected_line));
        assert_that!(
            diagnostics[0].message,
            eq("Type `{ len = 1 }` is not assignable to type `\"x\"`.")
        );
    }

    #[gtest]
    fn nullable_target_preserves_missing_member_detail() {
        let mut ws = VirtualWorkspace::new();
        let source = r#"---@class RequiredValue
---@field value string
---@type {}
local source
---@type RequiredValue?
local target = source"#;

        let diagnostics = ws.get_diagnostics(DiagnosticCode::AssignTypeMismatch, source);

        assert_that!(diagnostics.len(), eq(1));
        assert_that!(
            diagnostics[0].message,
            eq("Cannot assign `{  }` to `RequiredValue?`.
  Type `{  }` is missing the `value` field from type `RequiredValue`.")
        );
    }

    #[gtest]
    fn nested_alias_arrays_fold_to_dotted_path() {
        let mut ws = VirtualWorkspace::new();
        let diagnostics = ws.get_diagnostics(
            DiagnosticCode::AssignTypeMismatch,
            r#"---@alias LeafTarget { id: number }
---@alias ContainerTarget { items: LeafTarget[] }
---@alias RootTarget { containers: ContainerTarget[] }

---@alias LeafSource { id: string }
---@alias ContainerSource { items: LeafSource[] }
---@alias RootSource { containers: ContainerSource[] }

---@type RootTarget
local target

---@type RootSource
local source

target = source"#,
        );

        assert_that!(diagnostics.len(), eq(1));
        assert_that!(
            diagnostics[0].message,
            eq("Cannot assign `RootSource` to `RootTarget`.
  The types of field `containers.items.id` are incompatible.
    Type `string` is not assignable to type `number`.")
        );
    }

    #[gtest]
    fn table_generic_to_nested_array_reports_deepest_missing_field() {
        let mut ws = VirtualWorkspace::new();
        let diagnostics = ws.get_diagnostics(
            DiagnosticCode::AssignTypeMismatch,
            r#"---@alias SourceItem {}
---@alias TargetItem { id: number }

---@type table<integer, SourceItem[]>
local source

---@type TargetItem[][]
local target = source"#,
        );

        assert_that!(diagnostics.len(), eq(1));
        assert_that!(
            diagnostics[0].message,
            eq(
                "Cannot assign `table<integer,SourceItem[]>` to `TargetItem[][]`.\n  Type `{  }` is missing the `id` field from type `{ id: number }`."
            )
        );
    }

    #[gtest]
    fn keyed_source_to_nested_array_reports_deepest_missing_field() {
        let mut ws = VirtualWorkspace::new();
        let diagnostics = ws.get_diagnostics(
            DiagnosticCode::AssignTypeMismatch,
            r#"---@alias SourceItem {}
---@alias TargetItem { id: number }

---@class IndexedSource
---@field [integer] SourceItem[]

---@type IndexedSource
local source

---@type TargetItem[][]
local target = source"#,
        );

        assert_that!(diagnostics.len(), eq(1));
        assert_that!(
            diagnostics[0].message,
            eq(
                "Cannot assign `IndexedSource` to `TargetItem[][]`.\n  Type `{  }` is missing the `id` field from type `{ id: number }`."
            )
        );
    }

    #[gtest]
    fn nested_array_to_object_alias_reports_inner_array_type() {
        let mut ws = VirtualWorkspace::new();
        let diagnostics = ws.get_diagnostics(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@alias Item { foo: number }
            ---@type string[][][]
            local b
            ---@type Item[]
            local a = b
            "#,
        );

        assert_that!(diagnostics.len(), eq(1));
        assert_that!(
            diagnostics[0].message,
            eq(
                "Cannot assign `string[][][]` to `Item[]`.\n  Type `string[][]` is not assignable to type `{ foo: number }`."
            )
        );
    }

    #[gtest]
    fn nullable_array_element_reports_incompatible_non_nil_branch() {
        let mut ws = VirtualWorkspace::new();
        let source = r#"---@type string?
local value

---@type boolean[]
local target = {
    value,
}"#;

        assert_false!(ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, source));
    }

    #[gtest]
    fn nullable_union_mismatch_does_not_render_nil_branch() {
        let mut ws = VirtualWorkspace::new();
        let source = r#"local target ---@type boolean?
local source ---@type string?
target = source"#;

        let diagnostics = ws.get_diagnostics(DiagnosticCode::AssignTypeMismatch, source);

        assert_that!(diagnostics.len(), eq(1));
        assert_that!(
            diagnostics[0].message,
            eq("Type `string?` is not assignable to type `boolean?`.
  Type `string` is not assignable to type `boolean`.")
        );
    }

    #[gtest]
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

        let diagnostics = ws.get_diagnostics(DiagnosticCode::AssignTypeMismatch, source);

        assert_that!(diagnostics.len(), eq(1));
        assert_that!(diagnostics[0].range.start.line, eq(expected_line));
    }

    #[gtest]
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

        let diagnostics = ws.get_diagnostics(DiagnosticCode::AssignTypeMismatch, source);

        assert_that!(diagnostics.len(), eq(1));
        assert_that!(diagnostics[0].range.start.line, eq(expected_line));
    }

    #[gtest]
    fn union_table_mismatch_reports_deepest_field_mismatch() {
        let mut ws = VirtualWorkspace::new();
        let source = r#"---@class C
---@field type "one"

---@class D
---@field type "two"

---@param cd C | D
local function cd(cd) end

cd({ type = "test" })"#;

        let diagnostics = ws.get_diagnostics(DiagnosticCode::AssignTypeMismatch, source);
        assert_that!(diagnostics.len(), eq(1));
        assert_that!(
            diagnostics[0].message,
            eq("Type `\"test\"` is not assignable to type `(\"one\"|\"two\")`.")
        );
    }

    // 字段诊断被行级屏蔽时, 不能回退到整体诊断
    #[gtest]
    fn suppressed_leaf_field_silences_whole_assignment_fallback() {
        let mut ws = VirtualWorkspace::new();
        let source = r#"---@class SuppressWholeTarget
---@field a string
---@field b string
---@type SuppressWholeTarget
local target = {
    ---@diagnostic disable-next-line: assign-type-mismatch
    a = 123,
    b = "ok",
}"#;

        assert!(ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, source));
    }
}
