use super::check_source;
use crate::DiagnosticCode;

#[test]
#[ignore = "Known bug before ef507415: self-indexed class bases remain unresolved"]
fn object_class_base_can_index_its_own_fields() {
    for (target, expected) in [
        ("string", vec![]),
        ("integer", vec![DiagnosticCode::ReturnTypeMismatch]),
    ] {
        let diagnostics = check_source(&format!(
            r#"
            ---@class Config: {{value: string, mirror: Config['value']}}
            ---@param config Config
            ---@return {target}
            local function read(config) return config.mirror end
            return read
            "#,
        ));
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            expected,
            "{target}: {diagnostics:?}"
        );
    }
}

#[test]
fn recursive_type_queries_keep_unrelated_assignment_error() {
    for definitions in [
        r#"
        ---@alias Lookup<T> RawGet<T, 'next'>
        ---@class Owner<T>
        ---@field next Lookup<Owner<T[]>>
        ---@alias Input<T> Lookup<Owner<T>>
        "#,
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
            diagnostics
                .iter()
                .any(|d| d.code == DiagnosticCode::TypeComplexity),
            "{definitions}: {diagnostics:?}"
        );
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
    for record in [
        "---@alias Record<T> {field: T, cycle: Cycle<T>}",
        "---@alias Record<T> {field: T, next?: Record<T[]>}",
        "---@alias Record<T> First<T, Cycle<T>>",
        "---@alias Record<T> Second<Cycle<T>, T>",
        "---@alias Record<T> Forward<T, Cycle<T>>",
        "---@alias Record<T> ForwardSecond<Cycle<T>, T>",
        "---@alias Record<T> Forward<Identity<Identity<T>>, Cycle<T>>",
    ] {
        for (target, expected) in [
            ("string", vec![]),
            ("integer", vec![DiagnosticCode::ReturnTypeMismatch]),
        ] {
            let diagnostics = check_source(&format!(
                r#"
            ---@alias Cycle<T> Filter<Cycle<T[]>>
            ---@alias Filter<T> T extends string and T or T[]
            ---@alias First<A, B> {{field: A}}
            ---@alias Second<A, B> {{field: B}}
            ---@alias Forward<A, B> First<A, B>
            ---@alias ForwardSecond<A, B> Second<A, B>
            ---@alias Identity<T> T
            {record}
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

#[test]
fn conditional_inference_resolves_lazy_alias_fields() {
    for (value, expected) in [
        ("42", vec![]),
        ("true", vec![DiagnosticCode::AssignTypeMismatch]),
    ] {
        let diagnostics = check_source(&format!(
            r#"
            ---@alias Items<T> T[]
            ---@alias Wrapped<T> {{items: Items<T>}}
            ---@alias Element<T> T extends {{items: (infer U)[]}} and integer or boolean
            ---@type Element<Wrapped<string>>
            local result = {value}
            return result
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
fn lazy_callable_fields_keep_argument_checks() {
    for (field, nullable) in [
        ("Choose<string>", false),
        ("Identity<Choose<string>>", false),
        ("Maybe<string> | nil", true),
        ("Wrapped<string>", true),
    ] {
        for (argument, expected) in [
            ("42", vec![]),
            ("true", vec![DiagnosticCode::ParamTypeMismatch]),
            ("", vec![DiagnosticCode::MissingParameter]),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@alias Choose<T> T extends string and fun(value: integer): string or boolean
                ---@alias Identity<T> T
                ---@alias Maybe<T> Choose<T> | nil
                ---@alias Wrapped<T> Maybe<T> | nil
                ---@class Box
                ---@field callback {field}
                ---@param box Box
                local function call(box)
                    return box.callback({argument})
                end
                return call
                "#,
            ));
            let mut expected = expected;
            if nullable {
                expected.push(DiagnosticCode::NeedCheckNil);
            }
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                expected,
                "{field}, {argument}: {diagnostics:?}"
            );
        }
    }
}

#[test]
fn lazy_union_fields_keep_nil_checks() {
    for field in ["Wrapped<Value>", "Nested<Value>"] {
        let diagnostics = check_source(&format!(
            r#"
            ---@alias Nullable<T> T | nil
            ---@alias Wrapped<T> Nullable<T> | T
            ---@alias Nested<T> Wrapped<T> | T
            ---@class Value
            ---@field name string
            ---@class Box
            ---@field value {field}
            ---@param box Box
            ---@return string
            local function read(box)
                return box.value.name
            end
            return read
            "#,
        ));
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            [DiagnosticCode::NeedCheckNil],
            "{field}: {diagnostics:?}"
        );
    }
}

#[test]
fn lazy_callable_fields_keep_function_generic_scope() {
    for field in ["fun<U>(value: U): U", "Identity<fun<U>(value: U): U>"] {
        for (target, expected) in [
            ("string", vec![]),
            ("integer", vec![DiagnosticCode::AssignTypeMismatch]),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@alias Identity<T> T
                ---@class Box<T>
                ---@field callback {field}
                ---@param box Box<integer>
                local function call(box)
                    local result = box.callback('hello')
                    ---@type {target}
                    local checked = result
                    return checked
                end
                return call
                "#,
            ));
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                expected,
                "{field}, {target}: {diagnostics:?}"
            );
        }
    }
}

#[test]
fn lazy_optional_fields_keep_nil_in_assignments() {
    for (target, expected) in [
        ("integer?", vec![]),
        ("integer", vec![DiagnosticCode::AssignTypeMismatch]),
    ] {
        let diagnostics = check_source(&format!(
            r#"
            ---@alias Identity<T> T
            ---@class Box
            ---@field value? Identity<integer>
            ---@param box Box
            local function read(box)
                ---@type {target}
                local result = box.value
                return result
            end
            return read
            "#,
        ));
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            expected,
            "{target}: {diagnostics:?}"
        );
    }
}

#[test]
fn lazy_alias_fields_keep_omission_checks_in_intersections() {
    for (field, expected) in [
        ("Maybe<integer>", vec![]),
        ("Maybe<integer> | boolean", vec![]),
        (
            "Identity<integer>",
            vec![DiagnosticCode::AssignTypeMismatch],
        ),
    ] {
        let diagnostics = check_source(&format!(
            r#"
            ---@alias Maybe<T> T | nil
            ---@alias Identity<T> T
            ---@class Empty
            ---@class Extra
            ---@class Part
            ---@field value {field}
            ---@param value Empty
            local function copy(value)
                ---@type Part & Extra
                local result = value
                return result
            end
            return copy
            "#,
        ));
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            expected,
            "{field}: {diagnostics:?}"
        );
    }
}
