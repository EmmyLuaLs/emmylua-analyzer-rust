//! Tests for the code_style checker (the codes are disabled by default and explicitly enabled in tests).

use crate::DiagnosticCode;
use crate::Emmyrc;

use super::{check_source_with_emmyrc, count_by_code};

fn check_with_code(source: &str, code: DiagnosticCode) -> Vec<super::Diagnostic> {
    let mut emmyrc = Emmyrc::default();
    emmyrc.diagnostics.enables.push(code);
    check_source_with_emmyrc(source, emmyrc)
}

/// Early-return `if` pattern: suggests inverting.
#[test]
fn test_invert_if_suggests() {
    let diagnostics = check_with_code(
        r#"
function f(cond)
    if cond then
        local a = 1
        local b = 2
        local c = 3
    else
        return
    end
    print(a)
end
"#,
        DiagnosticCode::InvertIf,
    );
    assert_eq!(count_by_code(&diagnostics, DiagnosticCode::InvertIf), 1);
}

/// Both branches exit → not reported.
#[test]
fn test_invert_if_both_exit_skipped() {
    let diagnostics = check_with_code(
        "function f(cond)\n    if cond then\n        return 1\n    else\n        return\n    end\nend",
        DiagnosticCode::InvertIf,
    );
    assert_eq!(count_by_code(&diagnostics, DiagnosticCode::InvertIf), 0);
}

/// No `else` → not reported.
#[test]
fn test_invert_if_no_else_skipped() {
    let diagnostics = check_with_code(
        "function f(cond)\n    if cond then\n        print(1)\n    end\nend",
        DiagnosticCode::InvertIf,
    );
    assert_eq!(count_by_code(&diagnostics, DiagnosticCode::InvertIf), 0);
}

/// Non-literal assert message (call expression) → reported.
#[test]
fn test_assert_non_literal() {
    let diagnostics = check_with_code(
        "local function msg() return 'x' end\nlocal a = assert(true, msg())",
        DiagnosticCode::NonLiteralExpressionsInAssert,
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::NonLiteralExpressionsInAssert),
        1
    );
}

/// Literal assert message → not reported.
#[test]
fn test_assert_literal_ok() {
    let diagnostics = check_with_code(
        "local a = assert(true, 'msg')",
        DiagnosticCode::NonLiteralExpressionsInAssert,
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::NonLiteralExpressionsInAssert),
        0
    );
}

/// Local alias: after `local alias = t.a`, continuing to write `t.a` suggests using the alias.
#[test]
fn test_preferred_local_alias() {
    let diagnostics = check_with_code(
        "local t = { a = 1 }\nlocal alias = t.a\nprint(t.a)",
        DiagnosticCode::PreferredLocalAlias,
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::PreferredLocalAlias),
        1
    );
}

/// Using the alias → not reported.
#[test]
fn test_preferred_local_alias_used() {
    let diagnostics = check_with_code(
        "local t = { a = 1 }\nlocal alias = t.a\nprint(alias)",
        DiagnosticCode::PreferredLocalAlias,
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::PreferredLocalAlias),
        0
    );
}
