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

/// `tostring` is a library function, not an operator: `@operator tostring`
/// must be reported instead of being silently ignored, and the diagnostic
/// must point at the name.
#[test]
fn test_unknown_operator() {
    let source = "---@class Complex\n---@operator tostring: string\nlocal Complex = {}";
    let diags = check_source(source);
    let diag = diags
        .iter()
        .find(|diag| diag.code == DiagnosticCode::AnnotationUsageError)
        .unwrap_or_else(|| panic!("unknown operator is not reported: {diags:?}"));
    assert!(
        diag.message.contains("unknown operator `tostring`"),
        "{}",
        diag.message
    );
    assert_eq!(&source[diag.range], "tostring");
}

/// Operator names are case-sensitive, and names that are not operators are
/// rejected: `pairs` is handled through the `__pairs` metatable, not through
/// `@operator`.
#[test]
fn test_unknown_operator_names() {
    for name in ["Add", "pairs"] {
        let source = format!("---@class C\n---@operator {name}: string\nlocal C = {{}}");
        let diags = check_source(&source);
        let diag = diags
            .iter()
            .find(|diag| diag.code == DiagnosticCode::AnnotationUsageError)
            .unwrap_or_else(|| panic!("`{name}` is not reported: {diags:?}"));
        assert_eq!(&source.as_str()[diag.range], name);
    }
}

/// A missing operator name is a doc syntax error, not an annotation usage
/// error, and must not panic.
#[test]
fn test_operator_without_name() {
    let diags = check_source("---@class C\n---@operator\nlocal C = {}");
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::AnnotationUsageError),
        0,
        "diagnostics: {:?}",
        diags
    );
}

/// Accepted operator names are not reported, including names whose overloads
/// are not evaluated yet.
#[test]
fn test_known_operators_ok() {
    let diags = check_source(
        r#"
        ---@class Complex
        ---@operator add(Complex): Complex
        ---@operator band(Complex): Complex
        ---@operator eq(Complex): boolean
        ---@operator len: integer
        ---@operator call: string
        local Complex = {}
        "#,
    );
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::AnnotationUsageError),
        0,
        "diagnostics: {:?}",
        diags
    );
}
