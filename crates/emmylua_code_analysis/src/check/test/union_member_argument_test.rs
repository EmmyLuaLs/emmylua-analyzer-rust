use crate::{DiagnosticCode, VirtualWorkspace};
use googletest::prelude::*;
use lsp_types::{Diagnostic, NumberOrString};
use tokio_util::sync::CancellationToken;

// Each test gets a fresh workspace; these shared classes are defined exactly once.
const TYPES: &str = r#"
---@alias uint32 integer
---@class OneGoal
---@field kind "one"
---@field start_index? uint32
---@class AllGoals
---@field kind "all"
---@field goals integer[]
---@return OneGoal|AllGoals
local function request_path() end
---@return OneGoal
local function request_one() end
---@param index integer
local function accept_index(index) end
"#;

// Negative cases inspect diagnostic counts and messages, not just their presence.
fn mismatches(source: &str) -> Vec<Diagnostic> {
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
    ws.analysis
        .diagnostic
        .enable_only(DiagnosticCode::ParamTypeMismatch);
    let file = ws.def(&format!("{TYPES}\n{source}"));
    let diagnostics = ws
        .analysis
        .diagnose_file(file, CancellationToken::new())
        .expect("parameter diagnostics");
    let code = Some(NumberOrString::String(
        DiagnosticCode::ParamTypeMismatch.get_name().to_owned(),
    ));
    assert!(diagnostics.iter().all(|diagnostic| diagnostic.code == code));
    diagnostics
}

#[gtest]
fn cast_owner_before_passing_optional_member() {
    let source = r#"
        ---@param values string[]
        local function remove_value(values)
            local result = request_path()
            ---@cast result OneGoal
            ---@cast result.start_index uint32
            table.remove(values, result.start_index)
            if values[1] then values[2] = "updated" end
            table.remove(values, result.start_index)
        end
        "#;
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
    assert!(ws.has_no_diagnostic(
        DiagnosticCode::ParamTypeMismatch,
        &format!("{TYPES}\n{source}"),
    ));
}

#[gtest]
fn guard_and_assignment_narrow_member_owner() {
    let source = r#"
        local result = request_path()
        if result.kind == "one" then
            ---@cast result.start_index uint32
            accept_index(result.start_index)
        end
        result = request_one()
        ---@cast result.start_index uint32
        accept_index(result.start_index)
        "#;
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
    assert!(ws.has_no_diagnostic(
        DiagnosticCode::ParamTypeMismatch,
        &format!("{TYPES}\n{source}"),
    ));
}

#[gtest]
fn narrowed_owner_union_checks_only_remaining_components() {
    let source = r#"
        ---@class AnotherGoal
        ---@field start_index uint32
        ---@param result OneGoal|AllGoals|AnotherGoal
        ---@param values string[]
        local function narrowed_union(result, values)
            ---@cast result OneGoal|AnotherGoal
            table.remove(values, result.start_index)
        end
        "#;
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
    assert!(ws.has_no_diagnostic(
        DiagnosticCode::ParamTypeMismatch,
        &format!("{TYPES}\n{source}"),
    ));
}

#[gtest]
fn nested_owner_cast_is_respected() {
    let source = r#"
        ---@class Wrapper
        ---@field result OneGoal|AllGoals
        ---@param wrapper Wrapper
        ---@param values string[]
        local function remove_value(wrapper, values)
            ---@cast wrapper.result OneGoal
            table.remove(values, wrapper.result.start_index)
        end
        "#;
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
    assert!(ws.has_no_diagnostic(
        DiagnosticCode::ParamTypeMismatch,
        &format!("{TYPES}\n{source}"),
    ));
}

#[gtest]
fn unnarrowed_union_still_reports_missing_member() {
    let diagnostics = mismatches(
        r#"
        local result = request_path()
        accept_index(result.start_index)
        "#,
    );
    assert_that!(diagnostics.len(), eq(1));
    assert_that!(diagnostics[0].message, contains_substring("nil"));
}

#[gtest]
fn branch_guard_does_not_hide_missing_members_on_other_paths() {
    let diagnostics = mismatches(
        r#"
        local result = request_path()
        if result.kind == "one" then
            ---@cast result.start_index uint32
            accept_index(result.start_index)
        else
            accept_index(result.start_index)
        end
        accept_index(result.start_index)
        "#,
    );
    assert_that!(diagnostics.len(), eq(2));
}

#[gtest]
fn narrowed_owner_still_checks_the_actual_field_type() {
    let diagnostics = mismatches(
        r#"
        ---@param text string
        local function accept_text(text) end
        local result = request_path()
        ---@cast result OneGoal
        ---@cast result.start_index uint32
        accept_text(result.start_index)
        "#,
    );
    assert_that!(diagnostics.len(), eq(1));
    assert_that!(diagnostics[0].message, contains_substring("string"));
}

#[gtest]
fn cast_can_widen_non_union_owner_to_union_with_missing_member() {
    let diagnostics = mismatches(
        r#"
        ---@param result OneGoal
        local function widened_owner(result)
            ---@cast result OneGoal|AllGoals
            accept_index(result.start_index)
        end
        "#,
    );
    assert_that!(diagnostics.len(), eq(1));
    assert_that!(diagnostics[0].message, contains_substring("nil"));
}

#[gtest]
fn cast_to_unrelated_owner_with_incompatible_field_is_rejected() {
    let diagnostics = mismatches(
        r#"
        ---@class TextGoal
        ---@field start_index string
        local result = request_path()
        ---@cast result TextGoal
        accept_index(result.start_index)
        "#,
    );
    assert_that!(diagnostics.len(), eq(1));
}

#[gtest]
fn wide_union_cast_keeps_only_selected_owner() {
    let mut source = String::new();
    let mut types = vec!["OneGoal".to_owned()];
    for index in 0..16 {
        source.push_str(&format!(
            "---@class OtherGoal{index}\n---@field other{index} boolean\n"
        ));
        types.push(format!("OtherGoal{index}"));
    }
    source.push_str(&format!(
        r#"
        ---@param result {}
        ---@param values string[]
        local function wide_union(result, values)
            ---@cast result OneGoal
            table.remove(values, result.start_index)
        end
        "#,
        types.join("|")
    ));
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
    assert!(ws.has_no_diagnostic(
        DiagnosticCode::ParamTypeMismatch,
        &format!("{TYPES}\n{source}"),
    ));
}

#[test]
#[ntest::timeout(5000)]
fn recursive_class_union_member_queries_terminate() {
    let prefix = format!("node{}", ".next".repeat(16));
    let source = format!(
        r#"
        ---@class WithIndex
        ---@field next WithIndex|WithoutIndex
        ---@field start_index integer
        ---@class WithoutIndex
        ---@field next WithIndex|WithoutIndex
        ---@field other boolean
        ---@param node WithIndex|WithoutIndex
        local function recursive_owner(node)
            ---@cast {prefix} WithIndex
            accept_index({prefix}.start_index)
        end
        "#
    );
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
    assert!(ws.has_no_diagnostic(
        DiagnosticCode::ParamTypeMismatch,
        &format!("{TYPES}\n{source}"),
    ));
}
