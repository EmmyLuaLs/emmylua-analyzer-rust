//! Tests for the param_type_check / return_type_mismatch checkers.

use crate::{DiagnosticCode, VirtualWorkspace};

use super::{check_source, count_by_code};

/// Argument type mismatch: f expects string, got 1.
#[test]
fn test_param_type_mismatch_basic() {
    let diagnostics = check_source("---@param s string\nlocal function f(s) end\nf(1)");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::ParamTypeMismatch),
        1
    );
}

/// Argument types match: not reported.
#[test]
fn test_param_type_match_ok() {
    let diagnostics = check_source(
        "---@param s string\nlocal function f(s) end\nf('x')\nlocal n = 1\n---@param x number\nlocal function g(x) end\ng(n)",
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::ParamTypeMismatch),
        0
    );
}

/// Named type parameter: passing number where Ref(C) is expected is reported (empty table {} matches any table type, so not reported).
#[test]
fn test_param_type_mismatch_named() {
    let diagnostics =
        check_source("---@class C\nlocal C = {}\n---@param c C\nlocal function f(c) end\nf(1)");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::ParamTypeMismatch),
        1
    );
}

/// Undocumented function: skipped (nothing to check).
#[test]
fn test_param_type_no_doc_skipped() {
    let diagnostics = check_source("local function f(s) end\nf(1)");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::ParamTypeMismatch),
        0
    );
}

/// Actual return type does not match the `---@return` annotation.
#[test]
fn test_return_type_mismatch_basic() {
    let diagnostics = check_source("---@return string\nlocal function f()\n    return 1\nend");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::ReturnTypeMismatch),
        1
    );
}

/// Return types match: not reported.
#[test]
fn test_return_type_match_ok() {
    let diagnostics = check_source("---@return string\nlocal function f()\n    return 'x'\nend");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::ReturnTypeMismatch),
        0
    );
}

/// No `@return` annotation: skipped.
#[test]
fn test_return_type_no_doc_skipped() {
    let diagnostics = check_source("local function f()\n    return 1\nend");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::ReturnTypeMismatch),
        0
    );
}

/// `---@type number` annotation does not match the initializer.
#[test]
fn test_assign_type_mismatch_doc_type() {
    let diagnostics = check_source("---@type number\nlocal x = 's'");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::AssignTypeMismatch),
        1
    );
}

/// Annotation matches: not reported.
#[test]
fn test_assign_type_doc_type_ok() {
    let diagnostics = check_source("---@type number\nlocal x = 1");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::AssignTypeMismatch),
        0
    );
}

/// Reassignment does not match the declared type.
#[test]
fn test_assign_type_mismatch_reassign() {
    let diagnostics = check_source("local x = 1\nx = 's'");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::AssignTypeMismatch),
        1
    );
}

/// Reassignment matches: not reported.
#[test]
fn test_assign_type_reassign_ok() {
    let diagnostics = check_source("local x = 1\nx = 2");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::AssignTypeMismatch),
        0
    );
}

/// Undocumented local declaration initializer: nothing to compare, not reported.
#[test]
fn test_assign_type_no_doc_skipped() {
    let diagnostics = check_source("local x = 's'");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::AssignTypeMismatch),
        0
    );
}

// ──────────────────────────────────────────────
// generic_constraint_mismatch
// ──────────────────────────────────────────────

/// `---@generic T: string` + number argument → T violates the constraint.
#[test]
fn test_generic_constraint_mismatch_basic() {
    let diagnostics =
        check_source("---@generic T: string\n---@param x T\nlocal function f(x) end\nf(1)");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::GenericConstraintMismatch),
        1
    );
}

/// Argument satisfies the constraint: not reported.
#[test]
fn test_generic_constraint_match_ok() {
    let diagnostics =
        check_source("---@generic T: string\n---@param x T\nlocal function f(x) end\nf('s')");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::GenericConstraintMismatch),
        0
    );
}

/// Unconstrained generic: not reported.
#[test]
fn test_generic_no_constraint_skipped() {
    let diagnostics = check_source("---@generic T\n---@param x T\nlocal function f(x) end\nf(1)");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::GenericConstraintMismatch),
        0
    );
}

/// Named type constraint: `T: Base` with number passed.
#[test]
fn test_generic_constraint_named() {
    let diagnostics = check_source(
        "---@class Base\nlocal Base = {}\n---@generic T: Base\n---@param x T\nlocal function f(x) end\nf(1)",
    );
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::GenericConstraintMismatch),
        1
    );
}

// ──────────────────────────────────────────────
// Alias expansion.
// ──────────────────────────────────────────────

/// `---@alias Dir -1|1` + argument 1: literal union collapses to number, not reported.
#[test]
fn test_alias_param_accepts_member() {
    let diagnostics =
        check_source("---@alias Dir -1|1\n---@param d Dir\nlocal function foo(d) end\nfoo(1)");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::ParamTypeMismatch),
        0
    );
}

/// Alias target mismatch: Dir=number, string passed → reported.
#[test]
fn test_alias_param_rejects_mismatch() {
    let diagnostics =
        check_source("---@alias Dir number\n---@param d Dir\nlocal function foo(d) end\nfoo('s')");
    assert_eq!(
        count_by_code(&diagnostics, DiagnosticCode::ParamTypeMismatch),
        1
    );
}

/// Recursive alias (A = B, B = A): guard prevents cycles, no crash.
#[test]
fn test_alias_recursive_no_crash() {
    let diagnostics = check_source(
        "---@alias A B\n---@alias B A\n---@param a A\nlocal function foo(a) end\nfoo(1)",
    );
    // Recursive alias cannot be decided: just ensure no crash (diagnostic count is not important).
    let _ = diagnostics;
}

// A declared value type is the contract of every key, as it was in 0.25.1: without it a write
// through a key passed, and `ints.a = 'x'` even made that key read as a string.
#[test]
fn test_write_through_an_index_signature() {
    let prefix = "---@class L\n---@field cmd string\n\n\
                  ---@type table<string, integer>\nlocal ints = {}\n\
                  ---@type table<string, L>\nlocal lins = {}\n\
                  ---@type L[]\nlocal list = {}\n\
                  ---@type { [string]: integer }\nlocal obj = {}\n\
                  ---@class Holder\n---@field map table<string, integer>\n\
                  ---@type Holder\nlocal holder\n\
                  local key = 'k'\n";
    let cases: &[(&str, bool)] = &[
        ("ints.a = 'x'", false),
        ("ints['a'] = 'x'", false),
        ("ints[key] = 'x'", false),
        ("ints.a = 1", true),
        ("ints['a'] = 1", true),
        ("ints.a = 1\nints.a = 2", true),
        // Removing a key is not a type error.
        ("ints.a = nil", true),
        ("list[1] = nil", true),
        ("list[#list + 1] = { cmd = 'x' }", true),
        ("list[#list + 1] = 1", false),
        ("ints.a = 1\nints.a = 'x'", false),
        ("lins.a = 1", false),
        ("lins.a = { cmd = 'x' }", true),
        ("lins.a = { cmd = 1 }", false),
        ("---@type integer?\nlocal maybe\nints.a = maybe", true),
        ("list[1] = 1", false),
        ("list[1] = { cmd = 'x' }", true),
        ("obj.a = 'x'", false),
        ("obj.a = 1", true),
        ("holder.map.a = 'x'", false),
        ("holder.map['a'] = 'x'", false),
        ("holder.map.a = 1", true),
        (
            "---@param m table<string, integer>\nlocal function f(m) m.a = 'x' end",
            false,
        ),
        (
            "---@param m table<string, integer>\nlocal function f(m) m.a = 1 end",
            true,
        ),
        // No declared value type: nothing to hold the write to.
        ("local open = {}\nopen.a = 'x'\nopen.b = 1", true),
        // The class table declares its members: a write there is not held to `[string]`.
        (
            "---@class Box\n---@field [string] integer\nlocal Box = {}\nBox.a = 'x'",
            true,
        ),
        // A named field beats the index signature.
        (
            "---@class Mix\n---@field [string] integer\n---@field name string\n\n\
              ---@type Mix\nlocal mix\nmix.name = 'x'",
            true,
        ),
        (
            "---@class Mix\n---@field [string] integer\n---@field name string\n\n\
              ---@type Mix\nlocal mix\nmix.other = 'x'",
            false,
        ),
    ];
    for (write, clean) in cases {
        let mut ws = VirtualWorkspace::new();
        let source = format!("{prefix}{write}");
        assert_eq!(
            ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, &source),
            *clean,
            "{write}"
        );
    }
}

#[test]
fn test_write_through_an_index_signature_of_a_class_in_another_file() {
    let declaring = "---@class Mix\n---@field [string] integer\n---@field name string\n\
                     local Mix = {}\nfunction Mix:go() end\nMix.plain = true\n";
    for (write, clean) in [
        ("---@type Mix\nlocal mix\nmix.other = 'x'", false),
        ("---@type Mix\nlocal mix\nmix.other = 1", true),
        ("---@type Mix\nlocal mix\nmix.name = 'x'", true),
        (
            "---@param p Mix\nlocal function f(p) p.other = 'x' end",
            false,
        ),
        // Members the class table was given keep their own type, not `[string]`'s.
        ("---@type Mix\nlocal mix\nmix.go = function() end", true),
        ("---@type Mix\nlocal mix\nmix.plain = false", true),
    ] {
        let mut ws = VirtualWorkspace::new();
        ws.def_file("decl.lua", declaring);
        assert_eq!(
            ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, write),
            clean,
            "{write}"
        );
    }
}
