use super::check_source;
use crate::DiagnosticCode;

// Source annotation, expression, target annotation, expected compatibility.
const CASES: &[(&str, &str, &str, bool)] = &[
    ("true | false", "value", "boolean", true),
    ("boolean", "value", "true | false", true),
    ("Choice", "value", "Bool", true),
    ("Bool", "value", "Choice", true),
    ("Switch", "value", "Choice", true),
    ("boolean", "value", "Truth | Falsity | string", true),
    ("boolean", "value", "Choice | string", true),
    ("boolean", "value", "boolean | nil", true),
    ("boolean", "true", "boolean", true),
    ("boolean", "false", "boolean", true),
    ("boolean", "true", "Truth", true),
    ("boolean", "false", "Falsity", true),
    ("true", "value", "boolean", true),
    ("false", "value", "boolean", true),
    ("true", "value", "Truth | string", true),
    ("false", "value", "Falsity | nil", true),
    ("boolean", "value", "true", false),
    ("boolean", "value", "false", false),
    ("boolean", "value", "Truth | string", false),
    ("boolean", "value", "Falsity | nil", false),
    ("Bool", "value", "Truth", false),
    ("Switch", "value", "Truth", false),
    ("Bool", "value", "Falsity | nil", false),
    ("Truth", "value", "Falsity", false),
    ("Falsity", "value", "Truth | string", false),
    ("boolean", "false", "true", false),
    ("boolean", "true", "false", false),
    ("boolean", "true", "'true'", false),
    ("boolean", "false", "'false'", false),
    ("boolean", "false", "Truth | string", false),
    ("boolean", "true", "Falsity | nil", false),
    ("false", "value", "true", false),
    ("true", "value", "false", false),
    ("false", "value", "Truth | string", false),
    ("true", "value", "Falsity | nil", false),
    ("boolean", "value", "string", false),
    ("boolean", "value", "number", false),
    ("boolean", "value", "nil", false),
    ("boolean", "'text'", "string", true),
    ("boolean", "1", "number", true),
    ("boolean", "nil", "nil", true),
    ("boolean", "'text'", "boolean", false),
    ("boolean", "1", "boolean", false),
    ("boolean", "nil", "boolean", false),
];

fn check_cases(code: DiagnosticCode, make_source: impl Fn(&str, &str, &str) -> String) {
    let mut failures = Vec::new();
    for &(source_type, expression, target_type, accepted) in CASES {
        let source = format!(
            r#"
            ---@diagnostic disable: unused
            ---@alias Truth true
            ---@alias Falsity false
            ---@alias Choice Truth | Falsity
            ---@alias Bool boolean
            ---@alias Switch Bool
            {}
            "#,
            make_source(source_type, expression, target_type)
        );
        let diagnostics = check_source(&source);
        let mut expected = if accepted { vec![] } else { vec![code] };
        // The return-count checker treats an annotated nil return as zero values.
        if code == DiagnosticCode::ReturnTypeMismatch && target_type == "nil" && !accepted {
            expected.push(DiagnosticCode::RedundantReturnValue);
        }
        let actual: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
        if actual != expected {
            failures.push(format!(
                "{source_type}: {expression} -> {target_type}, accepted={accepted}: {diagnostics:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn boolean_return_compatibility() {
    check_cases(
        DiagnosticCode::ReturnTypeMismatch,
        |source, value, target| {
            format!(
                r#"
            ---@param value {source}
            ---@return {target}
            local function test(value)
                return {value}
            end
            return test
            "#
            )
        },
    );
}

#[test]
fn boolean_argument_compatibility() {
    check_cases(
        DiagnosticCode::ParamTypeMismatch,
        |source, value, target| {
            format!(
                r#"
            ---@param value {target}
            local function accept(value) end
            ---@param value {source}
            local function test(value)
                accept({value})
            end
            return test
            "#
            )
        },
    );
}

#[test]
fn boolean_enum_arguments_match_member_values() {
    for value in [true, false] {
        for target in ["Flag", "FlagAlias", "FlagAlias | number"] {
            for (argument, accepted) in [
                ("Flag.Value".to_string(), true),
                (value.to_string(), true),
                ((!value).to_string(), false),
            ] {
                let diagnostics = check_source(&format!(
                    r#"
                    ---@diagnostic disable: unused
                    ---@enum Flag
                    local Flag = {{
                        Value = {value},
                        Label = "{opposite}",
                        ["{opposite}"] = 0,
                    }}
                    ---@alias FlagAlias Flag
                    ---@param value {target}
                    local function accept(value) end
                    accept({argument})
                    "#,
                    opposite = !value,
                ));
                let actual: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
                let expected = if accepted {
                    vec![]
                } else {
                    vec![DiagnosticCode::ParamTypeMismatch]
                };
                assert_eq!(actual, expected, "{argument} -> {target}, member={value}");
            }
        }
    }
}

#[test]
fn boolean_local_initialization_compatibility() {
    check_cases(
        DiagnosticCode::AssignTypeMismatch,
        |source, value, target| {
            format!(
                r#"
            ---@param value {source}
            local function test(value)
                ---@type {target}
                local result = {value}
                return result
            end
            return test
            "#
            )
        },
    );
}

#[test]
fn boolean_reassignment_compatibility() {
    check_cases(
        DiagnosticCode::AssignTypeMismatch,
        |source, value, target| {
            format!(
                r#"
            ---@param value {source}
            local function test(value)
                ---@type {target}
                local result
                result = {value}
                return result
            end
            return test
            "#
            )
        },
    );
}
