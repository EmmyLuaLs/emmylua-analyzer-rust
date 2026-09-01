//! Tests for the unnecessary_assert / unnecessary_if / await_in_sync checkers.

use crate::DiagnosticCode;
use crate::Emmyrc;

use super::{check_source, check_source_with_emmyrc, count_by_code};

fn check_with_code(source: &str, code: DiagnosticCode) -> Vec<super::Diagnostic> {
    let mut emmyrc = Emmyrc::default();
    emmyrc.diagnostics.enables.push(code);
    check_source_with_emmyrc(source, emmyrc)
}

/// Definition-only `---@meta` files must not produce diagnostics.
#[test]
fn test_meta_file_has_no_diagnostics() {
    let diagnostics = check_source(
        r#"
        ---@meta
        local undefined_global = 1
        local x = 1
        x = "string"
        "#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "meta file must be definition-only, got: {:?}",
        diagnostics
    );
}

/// Library workspace files must not produce diagnostics.
#[test]
fn test_library_file_has_no_diagnostics() {
    use lsp_types::Uri;
    use std::str::FromStr;
    use std::sync::Arc;

    let emmyrc = Arc::new(Emmyrc::default());
    let mut db = crate::SalsaDatabase::new();
    db.update_config(emmyrc.clone());
    db.add_library_workspace(&crate::WorkspaceFolder::new(
        std::path::PathBuf::from("C:/libs/some-lib"),
        true,
    ));

    let uri = Uri::from_str("file:///C:/libs/some-lib/def.lua").unwrap();
    let fid = db.set_file_content(
        &uri,
        Some("local undefined_global = 1\nlocal x = 1\nx = \"string\"".to_string()),
    );

    let model = crate::SalsaSemanticModel::new(&db, fid).expect("model");
    let diagnostics =
        crate::check::check_file(&model, Arc::new(crate::check::CheckConfig::new(&emmyrc)));
    assert_eq!(
        diagnostics.len(),
        0,
        "library file must be definition-only, got: {:?}",
        diagnostics
    );
}

/// `assert(true)` is always true → reported.
#[test]
fn test_unnecessary_assert_truthy() {
    let diagnostics = check_source("local a = assert(true)");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::UnnecessaryAssert),
        1
    );
}

/// `assert(x)` variable condition → not reported.
#[test]
fn test_unnecessary_assert_ok() {
    let diagnostics = check_source("local a = assert(x)");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::UnnecessaryAssert),
        0
    );
}

/// `if 1 then` is always true → reported.
#[test]
fn test_unnecessary_if_truthy() {
    let diagnostics = check_source("if 1 then\n    print(1)\nend");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::UnnecessaryIf),
        1
    );
}

/// `if x then` → not reported.
#[test]
fn test_unnecessary_if_ok() {
    let diagnostics = check_source("if x then\n    print(1)\nend");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::UnnecessaryIf),
        0
    );
}

/// Calling an `---@async` function in a synchronous context → reported.
#[test]
fn test_await_in_sync_basic() {
    let diagnostics = check_with_code(
        "---@async\nlocal function f() end\nf()",
        DiagnosticCode::AwaitInSync,
    );
    assert_eq!(count_by_code(&diagnostics, DiagnosticCode::AwaitInSync), 1);
}

/// Calling it inside an async function → not reported.
#[test]
fn test_await_in_async_ok() {
    let diagnostics = check_with_code(
        "---@async\nlocal function f() end\n---@async\nlocal function g()\n    f()\nend",
        DiagnosticCode::AwaitInSync,
    );
    assert_eq!(count_by_code(&diagnostics, DiagnosticCode::AwaitInSync), 0);
}

/// Calling a non-async function → not reported.
#[test]
fn test_await_sync_call_ok() {
    let diagnostics = check_with_code("local function f() end\nf()", DiagnosticCode::AwaitInSync);
    assert_eq!(count_by_code(&diagnostics, DiagnosticCode::AwaitInSync), 0);
}

// ──────────────────────────────────────────────
// access_invisible / cast_type_mismatch
// ──────────────────────────────────────────────

/// Cross-file access to `---@field private` → reported.
#[test]
fn test_access_invisible_cross_file() {
    use lsp_types::Uri;
    use std::str::FromStr;
    use std::sync::Arc;

    use crate::semantic_model::SemanticModel;

    let emmyrc = Arc::new(Emmyrc::default());
    let mut db = crate::SalsaDatabase::new();
    db.update_config(emmyrc.clone());
    // Definition file: private field.
    let uri_b = Uri::from_str("file:///C:/ws/def.lua").unwrap();
    let fid_b = db.set_file_content(
        &uri_b,
        Some("---@class C\n---@field private secret number\nlocal C = {}".to_string()),
    );
    // Usage file: a type-annotated variable accesses a private field across files.
    let uri_a = Uri::from_str("file:///C:/ws/use.lua").unwrap();
    let fid = db.set_file_content(
        &uri_a,
        Some("---@type C\nlocal c\nlocal v = c.secret".to_string()),
    );
    db.update_main_root(std::path::PathBuf::from("C:/ws"));
    let _ = fid_b;
    let model = SemanticModel::new(&db, fid).expect("model");
    let diagnostics = crate::check::check_file(
        &model,
        Arc::new(crate::check::CheckConfig::new(&emmyrc.clone())),
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::AccessInvisible),
        1
    );
}

/// `---@cast` type-compatible: number → number not reported.
#[test]
fn test_cast_type_match_ok() {
    let diagnostics = check_source("local x = 1\n---@cast x +number\nprint(x)");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::CastTypeMismatch),
        0
    );
}

/// `---@cast` type-incompatible: number → string reported.
#[test]
fn test_cast_type_mismatch() {
    let diagnostics = check_source("local x = 1\n---@cast x string\nprint(x)");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::CastTypeMismatch),
        1
    );
}

// ──────────────────────────────────────────────
// call_non_callable / enum_value_mismatch
// ──────────────────────────────────────────────

/// Calling a number → reported.
#[test]
fn test_call_non_callable_basic() {
    let diagnostics = check_source("local x = 1\nlocal y = x()");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::CallNonCallable),
        1
    );
}

/// Calling a function → not reported.
#[test]
fn test_call_callable_ok() {
    let diagnostics = check_source("local function f() end\nf()");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::CallNonCallable),
        0
    );
}

/// Calling a union with a callable component but also a non-callable component → reported (mirrors old call_non_callable semantics).
#[test]
fn test_call_union_callable_ok() {
    let diagnostics =
        check_source("---@type integer|fun():string\nlocal i\ni = function() end\nlocal y = i()");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::CallNonCallable),
        1
    );
}

/// Comparing against an invalid enum value → reported.
#[test]
fn test_enum_value_mismatch() {
    let diagnostics = check_source(
        "---@enum Dir\n---@field Left 1\n---@field Right 2\n---@type Dir\nlocal d\nif d == 5 then\n    print('x')\nend",
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::EnumValueMismatch),
        1
    );
}

/// Comparing against a valid enum value → not reported.
#[test]
fn test_enum_value_match_ok() {
    let diagnostics = check_source(
        "---@enum Dir\n---@field Left 1\n---@field Right 2\n---@type Dir\nlocal d\nif d == 2 then\n    print('x')\nend",
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::EnumValueMismatch),
        0
    );
}

// ──────────────────────────────────────────────
// need_check_nil / unresolved_require
// ──────────────────────────────────────────────

/// Nullable prefix call → reported.
#[test]
fn test_need_check_nil_call() {
    let diagnostics = check_source("---@type fun()|nil\nlocal f\nlocal x = f()");
    assert_eq!(count_by_code(&diagnostics, DiagnosticCode::NeedCheckNil), 1);
}

/// Nullable prefix member access → reported.
#[test]
fn test_need_check_nil_index() {
    let diagnostics = check_source("---@type { x: number }?\nlocal t\nlocal v = t.x");
    assert_eq!(count_by_code(&diagnostics, DiagnosticCode::NeedCheckNil), 1);
}

/// Non-nullable → not reported.
#[test]
fn test_need_check_nil_ok() {
    let diagnostics = check_source("local t = { x = 1 }\nlocal v = t.x");
    assert_eq!(count_by_code(&diagnostics, DiagnosticCode::NeedCheckNil), 0);
}

/// Chained method calls (class `@field get fun(self: self, ...): T?`):
/// each nullable link in the chain is reported; `self` does not count as an argument, so MissingParameter is not reported.
#[test]
fn test_need_check_nil_chained_method_call() {
    let diagnostics = check_source(
        "---@class Cast1\n---@field get fun(self: self, a: number): Cast1?\n\n---@type Cast1\nlocal A\n\nlocal _a = A:get(1):get(2):get(3)",
    );
    assert_eq!(count_by_code(&diagnostics, DiagnosticCode::NeedCheckNil), 2);
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::MissingParameter),
        0
    );
}

/// Requiring a non-existent module → reported.
#[test]
fn test_unresolved_require() {
    let diagnostics = check_source("local m = require('not.exist.module')");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::UnresolvedRequire),
        1
    );
}

// ──────────────────────────────────────────────
// undefined_field
// ──────────────────────────────────────────────

/// Field absent on a named type → reported.
#[test]
fn test_undefined_field_named() {
    let diagnostics = check_source(
        "---@class C\n---@field x number\nlocal C = {}\n---@type C\nlocal c\nlocal v = c.missing",
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::UndefinedField),
        1
    );
}

/// Defined field → not reported.
#[test]
fn test_defined_field_ok() {
    let diagnostics = check_source(
        "---@class C\n---@field x number\nlocal C = {}\n---@type C\nlocal c\nlocal v = c.x",
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::UndefinedField),
        0
    );
}

/// Anonymous table fields → not reported (lenient).
#[test]
fn test_undefined_field_table_skipped() {
    let diagnostics = check_source("local t = { a = 1 }\nlocal v = t.missing");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::UndefinedField),
        0
    );
}

// ──────────────────────────────────────────────
// duplicate_field / duplicate_index / readonly
// ──────────────────────────────────────────────

/// Duplicate key in a table literal → reported.
#[test]
fn test_duplicate_index() {
    let diagnostics = check_source("local t = { a = 1, a = 2 }");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::DuplicateIndex),
        2
    );
}

/// Duplicate `@field` → DuplicateDocField.
#[test]
fn test_duplicate_field_doc() {
    let diagnostics =
        check_source("---@class C\n---@field x number\n---@field x string\nlocal C = {}");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::DuplicateDocField),
        2
    );
}

/// Reassigning a variable marked `---@readonly` → reported.
#[test]
fn test_readonly_reassign() {
    let diagnostics = check_source("---@readonly\nlocal x = 1\nx = 2");
    assert_eq!(count_by_code(&diagnostics, DiagnosticCode::ReadOnly), 1);
}

/// Not marked readonly → not reported.
#[test]
fn test_readonly_ok() {
    let diagnostics = check_source("local x = 1\nx = 2");
    assert_eq!(count_by_code(&diagnostics, DiagnosticCode::ReadOnly), 0);
}

// ──────────────────────────────────────────────
// check_field（MissingFields / InjectField）
// ──────────────────────────────────────────────

/// Table literal missing a required `@field` → MissingFields.
#[test]
fn test_missing_fields() {
    let diagnostics = check_source(
        "---@class C\n---@field x number\n---@field y number\nlocal C = {}\n---@type C\nlocal c = { x = 1 }",
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::MissingFields),
        1
    );
}

/// Nullable field is optional → not reported as missing.
#[test]
fn test_missing_fields_optional_ok() {
    let diagnostics = check_source(
        "---@class C\n---@field x number\n---@field y number?\nlocal C = {}\n---@type C\nlocal c = { x = 1 }",
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::MissingFields),
        0
    );
}

/// Table literal contains a field not on the target type → InjectField.
#[test]
fn test_inject_field() {
    let diagnostics = check_source(
        "---@class C\n---@field x number\nlocal C = {}\n---@type C\nlocal c = { x = 1, extra = 2 }",
    );
    assert_eq!(count_by_code(&diagnostics, DiagnosticCode::InjectField), 1);
}

/// All required fields present → not reported.
#[test]
fn test_check_field_ok() {
    let diagnostics = check_source(
        "---@class C\n---@field x number\nlocal C = {}\n---@type C\nlocal c = { x = 1 }",
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::MissingFields),
        0
    );
    assert_eq!(count_by_code(&diagnostics, DiagnosticCode::InjectField), 0);
}

// ──────────────────────────────────────────────
// discard_returns
// ──────────────────────────────────────────────

/// Discarding the return value of an `---@nodiscard` function call statement → reported.
#[test]
fn test_discard_returns() {
    let diagnostics = check_source("---@nodiscard\nlocal function f() return 1 end\nf()");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::DiscardReturns),
        1
    );
}

/// No `---@nodiscard` → not reported.
#[test]
fn test_discard_returns_ok() {
    let diagnostics = check_source("local function f() return 1 end\nf()");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::DiscardReturns),
        0
    );
}
