use crate::{DiagnosticCode, VirtualWorkspace};
use googletest::prelude::*;
use lsp_types::Diagnostic;
use tokio_util::sync::CancellationToken;

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

fn mismatches(source: &str) -> Vec<Diagnostic> {
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
    ws.analysis
        .diagnostic
        .enable_only(DiagnosticCode::ParamTypeMismatch);
    let file = ws.def(&format!("{TYPES}\n{source}"));
    ws.analysis
        .diagnose_file(file, CancellationToken::new())
        .expect("parameter diagnostics")
}

#[gtest]
fn cast_owner_before_passing_optional_member() {
    let diagnostics = mismatches(
        r#"
        ---@param values string[]
        local function remove_value(values)
            local result = request_path()
            ---@cast result OneGoal
            ---@cast result.start_index uint32
            table.remove(values, result.start_index)
            if values[1] then values[2] = "updated" end
            table.remove(values, result.start_index)
        end
        "#,
    );
    expect_that!(diagnostics, is_empty());
}

#[gtest]
fn guard_and_assignment_narrow_member_owner() {
    let diagnostics = mismatches(
        r#"
        local result = request_path()
        if result.kind == "one" then
            ---@cast result.start_index uint32
            accept_index(result.start_index)
        end
        result = request_one()
        ---@cast result.start_index uint32
        accept_index(result.start_index)
        "#,
    );
    expect_that!(diagnostics, is_empty());
}

#[gtest]
fn narrowed_owner_union_checks_only_remaining_components() {
    let diagnostics = mismatches(
        r#"
        ---@class AnotherGoal
        ---@field start_index uint32
        ---@param result OneGoal|AllGoals|AnotherGoal
        local function narrowed_union(result)
            ---@cast result OneGoal|AnotherGoal
            table.remove({}, result.start_index)
        end
        "#,
    );
    expect_that!(diagnostics, is_empty());
}

#[gtest]
fn nested_owner_cast_is_respected() {
    let diagnostics = mismatches(
        r#"
        ---@class Wrapper
        ---@field result OneGoal|AllGoals
        ---@param wrapper Wrapper
        local function remove_value(wrapper)
            ---@cast wrapper.result OneGoal
            table.remove({}, wrapper.result.start_index)
        end
        "#,
    );
    expect_that!(diagnostics, is_empty());
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
    expect_that!(diagnostics[0].message, contains_substring("nil"));
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
    expect_that!(diagnostics.len(), eq(2));
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
    expect_that!(diagnostics[0].message, contains_substring("string"));
}
