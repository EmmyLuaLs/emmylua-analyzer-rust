//! Tests for the param_count / check_return_count / incomplete_signature_doc checkers.

use crate::DiagnosticCode;
use crate::Emmyrc;
use crate::VirtualWorkspace;

use super::{Diagnostic, check_source, check_source_with_emmyrc, count_by_code};

fn check_with_codes(source: &str, codes: &[DiagnosticCode]) -> Vec<Diagnostic> {
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

#[test]
#[ignore = "Known bug before ef507415: argument counts and forwarded variadic arguments"]
fn test_assigned_function_overloads_keep_optional_and_variadic_counts() {
    for definition in [
        "local accept = function(first, second) return first, second end",
        "accept = function(first, second) return first, second end",
    ] {
        for (overload, arguments, expected) in [
            ("fun(value?: string)", "", None),
            ("fun(value?: string)", "'text'", None),
            ("fun(value?: string)", "1, 2", None),
            (
                "fun(value?: string)",
                "1, 2, 3",
                Some(DiagnosticCode::RedundantParameter),
            ),
            ("fun(...: string)", "'a', 'b', 'c'", None),
            (
                "fun(...: string)",
                "42, 'a', 'b'",
                Some(DiagnosticCode::ParamTypeMismatch),
            ),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@class Module
                local module = {{}}
                ---@param first number
                ---@param second number
                ---@overload {overload}
                {definition}
                accept({arguments})
                return module
                "#,
            ));
            let codes = diagnostics.iter().map(|d| d.code).collect::<Vec<_>>();
            assert_eq!(
                codes,
                expected.into_iter().collect::<Vec<_>>(),
                "{definition}, {overload}, {arguments}: {diagnostics:?}"
            );
        }
    }
}

#[test]
#[ignore = "Known bug before ef507415: argument counts and forwarded variadic arguments"]
fn test_declared_function_counts_take_precedence_over_implementation_overloads() {
    for (call, missing) in [
        ("accept()", true),
        ("accept(42)", false),
        ("alias()", true),
        ("alias(42)", false),
        ("pcall(accept)", true),
        ("pcall(accept, 42)", false),
    ] {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert_eq!(
            ws.has_no_diagnostic(
                DiagnosticCode::MissingParameter,
                &format!(
                    r#"
                    ---@type fun(value: number)
                    ---@overload fun()
                    local accept = function(value) end
                    local alias = accept
                    {call}
                    "#,
                ),
            ),
            !missing,
            "{call}"
        );
    }
}

#[test]
#[ignore = "Known bug before ef507415: argument counts and forwarded variadic arguments"]
fn test_forwarded_varargs_reject_impossible_overload_counts() {
    for (annotation, call, redundant) in [
        ("", "accept(...)", false),
        ("", "accept({value = 2}, ...)", false),
        ("", "accept({value = 2}, true, ...)", true),
        ("", "accept({value = 'text'}, true, ...)", true),
        ("", "accept(..., {value = 2})", true),
        ("", "accept({value = 2}, (...))", true),
        ("---@param ... [boolean, boolean]...", "accept(...)", true),
    ] {
        let diagnostics = check_source(&format!(
            r#"
            ---@type fun() | fun(fields: {{value: number}})
            local accept
            {annotation}
            local function forward(...)
                {call}
            end
            return forward
            "#,
        ));
        assert_eq!(
            count_by_code(&diagnostics, DiagnosticCode::RedundantParameter) > 0,
            redundant,
            "{annotation}, {call}: {diagnostics:?}"
        );
        if !redundant {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        }
    }
}

#[test]
#[ignore = "Known bug before ef507415: argument counts and forwarded variadic arguments"]
fn test_pcall_forwarded_varargs_reject_impossible_overload_counts() {
    for (annotation, call, redundant) in [
        ("", "pcall(accept, ...)", false),
        ("", "pcall(accept, {value = 2}, ...)", false),
        ("", "pcall(accept, {value = 2}, true, ...)", true),
        ("", "pcall(accept, {value = 'text'}, true, ...)", true),
        ("", "pcall(accept, ..., {value = 2})", true),
        ("", "pcall(accept, {value = 2}, (...))", true),
        (
            "---@param ... [boolean, boolean]...",
            "pcall(accept, ...)",
            true,
        ),
    ] {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert_eq!(
            ws.has_no_diagnostic(
                DiagnosticCode::RedundantParameter,
                &format!(
                    r#"
                    ---@type fun() | fun(fields: {{[string]: number}})
                    local accept
                    {annotation}
                    local function forward(...)
                        {call}
                    end
                    return forward
                    "#,
                ),
            ),
            !redundant,
            "{annotation}, {call}"
        );
    }
}

#[test]
#[ignore = "Known bug before ef507415: argument counts and forwarded variadic arguments"]
fn test_pcall_callback_counts_keep_method_receivers() {
    for (call, missing) in [
        ("pcall(mod.accept, mod, 42)", false),
        ("pcall(mod.accept, mod)", true),
    ] {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert_eq!(
            ws.has_no_diagnostic(
                DiagnosticCode::MissingParameter,
                &format!(
                    r#"
                    local mod = {{}}
                    ---@param value number
                    function mod:accept(value) return self, value end
                    {call}
                    "#,
                ),
            ),
            !missing,
            "{call}"
        );
    }
}

#[test]
fn test_local_pcall_uses_its_own_parameters() {
    let diagnostics = check_source(
        r#"
        local function pcall(callback, value) return callback, value end
        pcall(function() end, 42)
        "#,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
#[ignore = "Known bug before ef507415: argument counts and forwarded variadic arguments"]
fn test_variadic_tuple_tails_keep_unbounded_argument_counts() {
    for callee in ["accept(", "pcall(accept, "] {
        for (pack, values, redundant) in [
            ("[number, string...]...", "42", false),
            ("[number, string...]...", "42, 'a', 'b'", false),
            ("[number, string...]...", "42, 'a', 'b', 'c'", false),
            ("[number, string]...", "42, 'a'", false),
            ("[number, string]...", "42, 'a', 'b'", true),
        ] {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            assert_eq!(
                ws.has_no_diagnostic(
                    DiagnosticCode::RedundantParameter,
                    &format!(
                        r#"
                        ---@param ... {pack}
                        local function accept(...) return ... end
                        {callee}{values})
                        "#,
                    ),
                ),
                !redundant,
                "{pack}, {callee}{values})"
            );
        }
    }
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
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
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
