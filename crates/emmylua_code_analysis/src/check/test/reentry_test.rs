use super::check_source;
use crate::DiagnosticCode;

#[test]
fn recursive_type_queries_keep_unrelated_assignment_error() {
    for definitions in [
        r#"
        ---@alias Input<T> Filter<Input<T[]>>
        ---@alias Filter<T> T extends string and T or T[]
        "#,
        r#"
        ---@class Owner<T>
        ---@field next Input<T[]>
        ---@alias Input<T> RawGet<Owner<T>, 'next'>
        "#,
        r#"
        ---@class Owner<T>
        ---@field next RawGet<Owner<T[]>, 'next'>
        ---@alias Input<T> RawGet<Owner<T>, 'next'>
        "#,
    ] {
        let diagnostics = check_source(&format!(
            r#"
            {definitions}
            ---@param value table<string, Input<string>>
            local function accept(value) return value end
            accept({{name = {{}}}})
            ---@type string
            local unrelated = 42
            return unrelated
            "#,
        ));
        assert!(
            diagnostics.iter().any(|d| {
                d.code == DiagnosticCode::AssignTypeMismatch
                    && d.message.contains("string")
                    && d.message.contains("42")
            }),
            "{definitions}: {diagnostics:?}"
        );
    }
}

#[test]
fn long_binary_expressions_keep_assignment_checks() {
    let expression = std::iter::repeat_n("'a'", 512)
        .collect::<Vec<_>>()
        .join(" .. ");
    for (target, expected) in [
        ("string", vec![]),
        ("integer", vec![DiagnosticCode::AssignTypeMismatch]),
    ] {
        let diagnostics = check_source(&format!(
            "---@type {target}\nlocal value = {expression}\nreturn value"
        ));
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            expected,
            "{diagnostics:?}"
        );
    }
}

#[test]
fn long_member_expressions_keep_assignment_checks() {
    let expression = format!("value{}", ".child".repeat(256));
    for (target, expected) in [
        ("Node", vec![]),
        ("integer", vec![DiagnosticCode::AssignTypeMismatch]),
    ] {
        let diagnostics = check_source(&format!(
            "---@class Node\n---@field child Node\n---@param value Node\nlocal function read(value)\n---@type {target}\nlocal result = {expression}\nreturn result\nend\nreturn read"
        ));
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            expected,
            "{diagnostics:?}"
        );
    }
}

#[test]
fn recursive_calls_keep_unrelated_assignment_error() {
    let diagnostics = check_source(
        r#"
        ---@param ... boolean
        local function recurse(...)
            if ... then return 1, 'done' end
            return 1, recurse(true)
        end
        local result = recurse(false)
        ---@type string
        local wrong = 42
        return result, wrong
    "#,
    );
    assert_eq!(
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
        [DiagnosticCode::AssignTypeMismatch],
        "{diagnostics:?}"
    );
}

#[test]
fn cyclic_fields_keep_independent_member_checks() {
    for (target, expected) in [
        ("string", vec![]),
        ("integer", vec![DiagnosticCode::ReturnTypeMismatch]),
    ] {
        let diagnostics = check_source(&format!(
            r#"
            ---@alias Cycle<T> Filter<Cycle<T[]>>
            ---@alias Filter<T> T extends string and T or T[]
            ---@alias Record<T> {{field: T, cycle: Cycle<T>}}
            ---@param value Record<string>
            ---@return {target}
            local function read(value)
                return value.field
            end
            return read
            "#,
        ));
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            expected,
            "{diagnostics:?}"
        );
    }
}

#[test]
fn long_alias_chains_keep_return_checks() {
    let aliases = (0..512)
        .map(|i| format!("---@alias Alias{i} Alias{}\n", i + 1))
        .collect::<String>();
    for (source, target, expected) in [
        ("Alias0", "string", vec![]),
        (
            "Alias0",
            "integer",
            vec![DiagnosticCode::ReturnTypeMismatch],
        ),
        ("string", "Alias0", vec![]),
        (
            "integer",
            "Alias0",
            vec![DiagnosticCode::ReturnTypeMismatch],
        ),
    ] {
        let diagnostics = check_source(&format!(
            "{aliases}---@alias Alias512 string\n---@param value {source}\n---@return {target}\nlocal function read(value)\nreturn value\nend\nreturn read"
        ));
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            expected,
            "{source} -> {target}: {diagnostics:?}"
        );
    }
}
