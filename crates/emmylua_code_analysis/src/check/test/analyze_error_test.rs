//! Tests for the analyze_error checker (mirrors old `compilation/test/annotation_test.rs` /
//! TypeNotFound / MissingTypeArgument cases disabled by M2 in `generic_test.rs`).

use super::{check_source, count_by_code};
use crate::DiagnosticCode;

/// `---@class AnonymousObserver<T>: Observer<T>`: parent type Observer is undefined → TypeNotFound.
#[test]
fn test_type_not_found_in_class_super() {
    let diags = check_source("---@class AnonymousObserver<T>: Observer<T>");
    assert_eq!(count_by_code(&diags, DiagnosticCode::TypeNotFound), 1);
}

/// `---@type NotFound` → TypeNotFound.
#[test]
fn test_type_not_found_in_doc_type() {
    let diags = check_source("---@type NotDefinedType\nlocal value");
    assert_eq!(count_by_code(&diags, DiagnosticCode::TypeNotFound), 1);
}

/// Required generic argument missing (bare name) → MissingTypeArgument.
#[test]
fn test_missing_type_argument_bare_name() {
    let diags = check_source(
        r#"
        ---@class Foo<T>
        local Foo = {}
        ---@type Foo
        local value
        "#,
    );
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::MissingTypeArgument),
        1,
        "diagnostics: {:?}",
        diags
    );
}

/// Explicit arguments are insufficient and the missing parameter has no default → MissingTypeArgument.
#[test]
fn test_missing_type_argument_explicit() {
    let diags = check_source(
        r#"
        ---@class Foo<T, U>
        local Foo = {}
        ---@type Foo<string>
        local value
        "#,
    );
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::MissingTypeArgument),
        1,
        "diagnostics: {:?}",
        diags
    );
}

/// All generic parameters have defaults → not reported.
#[test]
fn test_generic_defaults_no_error() {
    let diags = check_source(
        r#"
        ---@class Foo<T = string>
        local Foo = {}
        ---@type Foo
        local value
        "#,
    );
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::MissingTypeArgument),
        0
    );
    assert_eq!(count_by_code(&diags, DiagnosticCode::TypeNotFound), 0);
}

/// Builtin types and file-local generic parameters do not report TypeNotFound.
#[test]
fn test_builtin_and_generic_params_ok() {
    let diags = check_source(
        r#"
        ---@generic T
        ---@param value T
        ---@return string?
        local function identity(value) return value end
        "#,
    );
    assert_eq!(count_by_code(&diags, DiagnosticCode::TypeNotFound), 0);
}

/// Local generics in `fun<T>(x: T): T` are not reported.
#[test]
fn test_func_local_generic_ok() {
    let diags = check_source(
        r#"
        ---@type fun<T>(value: T): T
        local fn
        "#,
    );
    assert_eq!(count_by_code(&diags, DiagnosticCode::TypeNotFound), 0);
}

/// `@field` without an `@class` context → AnnotationUsageError.
#[test]
fn test_field_without_class() {
    let diags = check_source(
        r#"
        ---@field x number
        local value
        "#,
    );
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::AnnotationUsageError),
        1,
        "diagnostics: {:?}",
        diags
    );
}

/// `@field` under `@class` is valid.
#[test]
fn test_field_under_class_ok() {
    let diags = check_source(
        r#"
        ---@class C
        ---@field x number
        local C = {}
        "#,
    );
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::AnnotationUsageError),
        0
    );
}
