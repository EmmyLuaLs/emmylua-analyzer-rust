//! Tests for the param_count / check_return_count / incomplete_signature_doc checkers.

use crate::DiagnosticCode;
use crate::Emmyrc;

use super::{check_source, check_source_with_emmyrc, count_by_code};

fn check_with_codes(source: &str, codes: &[DiagnosticCode]) -> Vec<super::Diagnostic> {
    let mut emmyrc = Emmyrc::default();
    for code in codes {
        emmyrc.diagnostics.enables.push(*code);
    }
    check_source_with_emmyrc(source, emmyrc)
}

/// Fewer arguments than required parameters → MissingParameter.
#[test]
fn test_param_count_missing() {
    let diagnostics =
        check_source("---@param a number\n---@param b number\nlocal function f(a, b) end\nf(1)");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::MissingParameter),
        1
    );
}

/// More arguments than parameters → RedundantParameter.
#[test]
fn test_param_count_redundant() {
    let diagnostics = check_source("---@param a number\nlocal function f(a) end\nf(1, 2)");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::RedundantParameter),
        1
    );
}

/// Count matches: not reported.
#[test]
fn test_param_count_ok() {
    let diagnostics =
        check_source("---@param a number\n---@param b number\nlocal function f(a, b) end\nf(1, 2)");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::MissingParameter),
        0
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::RedundantParameter),
        0
    );
}

/// Colon method call: the `self` slot is implicitly supplied by the receiver and does not count as an argument → not reported.
#[test]
fn test_param_count_colon_call_self_slot() {
    let diagnostics = check_source(
        "---@class C\n---@field get fun(self: self, a: number): C?\n---@type C\nlocal c\nlocal _x = c:get(1)",
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::MissingParameter),
        0
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::RedundantParameter),
        0
    );
}

/// Colon method call missing argument (other than self) → still reports MissingParameter.
#[test]
fn test_param_count_colon_call_missing() {
    let diagnostics = check_source(
        "---@class C\n---@field get fun(self: self, a: number, b: string): C?\n---@type C\nlocal c\nlocal _x = c:get(1)",
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::MissingParameter),
        1
    );
}

/// Colon-defined method called with colon: the implicit self must not be counted as an extra argument.
#[test]
fn test_param_count_colon_define_colon_call_ok() {
    let diagnostics = check_source(
        "---@class C
local C = {}
---@param a number
function C:name(a) end
C:name(1)",
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::RedundantParameter),
        0
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::MissingParameter),
        0
    );
}

/// Colon-defined method called with too many args should still report redundant.
#[test]
fn test_param_count_colon_define_colon_call_redundant() {
    let diagnostics = check_source(
        "---@class C
local C = {}
---@param a number
function C:name(a) end
C:name(1, 2)",
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::RedundantParameter),
        1
    );
}

/// Colon-defined method called with dot and explicit self should be valid.
#[test]
fn test_param_count_colon_define_dot_call_with_self_ok() {
    let diagnostics = check_source(
        "---@class C
local C = {}
---@param a number
function C:name(a) end
C.name(C, 1)",
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::RedundantParameter),
        0
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::MissingParameter),
        0
    );
}

#[test]
fn test_table_insert_overload_accepts_both_forms() {
    let mut ws = crate::VirtualWorkspace::new_with_init_std_lib();
    assert!(ws.has_no_diagnostic(
        DiagnosticCode::RedundantParameter,
        r#"
        local t = {}
        table.insert(t, 1)
        table.insert(t, 1, 2)
        "#
    ));
}

/// Colon-defined method called with dot but missing the explicit self should report missing.
#[test]
fn test_param_count_colon_define_dot_call_missing_self() {
    let diagnostics = check_source(
        "---@class C
local C = {}
---@param a number
function C:name(a) end
C.name(1)",
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::MissingParameter),
        1
    );
}

/// `@return` annotates 2 values but only 1 is returned → MissingReturnValue.
#[test]
fn test_return_count_missing_value() {
    let diagnostics =
        check_source("---@return number\n---@return string\nlocal function f()\n    return 1\nend");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::MissingReturnValue),
        1
    );
}

/// Annotates 1 value but returns 2 → RedundantReturnValue.
#[test]
fn test_return_count_redundant_value() {
    let diagnostics = check_source("---@return number\nlocal function f()\n    return 1, 's'\nend");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::RedundantReturnValue),
        1
    );
}

/// No return statement but the annotation requires a return value → MissingReturn.
#[test]
fn test_return_count_missing_return() {
    let diagnostics = check_source("---@return number\nlocal function f()\n    local x = 1\nend");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::MissingReturn),
        1
    );
}

/// Function missing doc comment → IncompleteSignatureDoc (disabled by default, explicitly enabled).
#[test]
fn test_incomplete_signature_doc_missing_comment() {
    let diagnostics = check_with_codes(
        "local function f(a)\nend",
        &[DiagnosticCode::IncompleteSignatureDoc],
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::IncompleteSignatureDoc),
        1
    );
}

/// Parameter missing `@param` annotation → reported.
#[test]
fn test_incomplete_signature_doc_missing_param() {
    let diagnostics = check_with_codes(
        "---@return number\nlocal function f(a)\n    return 1\nend",
        &[DiagnosticCode::IncompleteSignatureDoc],
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::IncompleteSignatureDoc),
        1
    );
}
