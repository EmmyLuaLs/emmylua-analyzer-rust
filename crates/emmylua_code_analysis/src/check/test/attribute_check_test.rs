//! Tests for the attribute_check checker (mirrors the old `compilation/test/attribute_test.rs`
//! `test_attribute_overload_uses_arg_type_for_diagnostic` + parameter count checks).

use super::{check_source, count_by_code};
use crate::DiagnosticCode;

const ATTRIBUTE_HEADER: &str = r#"
---@class Attribute
"#;

/// Overload covers the argument type → no type mismatch reported.
#[test]
fn test_attribute_overload_accepts_matching_arg() {
    let diags = check_source(&format!(
        r#"{ATTRIBUTE_HEADER}
---@class custom_attribute: Attribute
---@overload fun(value: string)
---@overload fun(value: integer)

---@[custom_attribute(1)]
local value
"#
    ));
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::AttributeParamTypeMismatch),
        0,
        "diagnostics: {:?}",
        diags
    );
}

/// Argument type does not match the only overload → AttributeParamTypeMismatch.
#[test]
fn test_attribute_param_type_mismatch() {
    let diags = check_source(&format!(
        r#"{ATTRIBUTE_HEADER}
---@class string_attribute: Attribute
---@overload fun(value: string)

---@[string_attribute(1)]
local value
"#
    ));
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::AttributeParamTypeMismatch),
        1,
        "diagnostics: {:?}",
        diags
    );
}

/// Required parameter missing → AttributeMissingParameter.
#[test]
fn test_attribute_missing_parameter() {
    let diags = check_source(&format!(
        r#"{ATTRIBUTE_HEADER}
---@class two_arg_attribute: Attribute
---@overload fun(first: string, second: integer)

---@[two_arg_attribute("x")]
local value
"#
    ));
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::AttributeMissingParameter),
        1,
        "diagnostics: {:?}",
        diags
    );
}

/// Redundant parameter → AttributeRedundantParameter.
#[test]
fn test_attribute_redundant_parameter() {
    let diags = check_source(&format!(
        r#"{ATTRIBUTE_HEADER}
---@class one_arg_attribute: Attribute
---@overload fun(value: string)

---@[one_arg_attribute("x", "y")]
local value
"#
    ));
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::AttributeRedundantParameter),
        1,
        "diagnostics: {:?}",
        diags
    );
}

/// Annotations on classes not derived from Attribute are not checked.
#[test]
fn test_non_attribute_class_ignored() {
    let diags = check_source(&format!(
        r#"{ATTRIBUTE_HEADER}
---@class plain
---@overload fun(value: string)

---@[plain(1)]
local value
"#
    ));
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::AttributeParamTypeMismatch),
        0
    );
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::AttributeMissingParameter),
        0
    );
}
