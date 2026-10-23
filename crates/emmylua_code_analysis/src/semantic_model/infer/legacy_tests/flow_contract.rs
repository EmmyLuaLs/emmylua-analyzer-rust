use crate::{DiagnosticCode, LuaType, test_lib::VirtualWorkspace};
use lsp_types::NumberOrString;
use tokio_util::sync::CancellationToken;

fn check_contract(code: &str) -> Vec<String> {
    check_contract_in(&mut VirtualWorkspace::new_with_init_std_lib(), code)
}

fn check_contract_in(ws: &mut VirtualWorkspace, code: &str) -> Vec<String> {
    let mut failures = Vec::new();
    ws.enable_check(DiagnosticCode::NeedCheckNil);
    let file = ws.def(code);
    let diagnostics = ws
        .analysis
        .diagnose_file(file, CancellationToken::new())
        .unwrap_or_default();
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| {
            [
                DiagnosticCode::AssignTypeMismatch,
                DiagnosticCode::ParamTypeMismatch,
                DiagnosticCode::ReturnTypeMismatch,
                DiagnosticCode::UndefinedField,
                DiagnosticCode::MissingFields,
                DiagnosticCode::NeedCheckNil,
                DiagnosticCode::InjectField,
            ]
            .iter()
            .any(|code| diagnostic.code == Some(NumberOrString::String(code.get_name().to_owned())))
        })
        .collect();
    for (line, text) in code.lines().enumerate() {
        let expected = text.contains("-- Error:") || text.contains("-- inject-field:");
        let actual: Vec<_> = errors
            .iter()
            .filter(|diagnostic| diagnostic.range.start.line as usize == line)
            .collect();
        if expected != !actual.is_empty() {
            failures.push(format!(
                "line {}: {text}\n  expected error: {expected}; {actual:?}",
                line + 1
            ));
        }
    }
    failures
}

fn assert_contract(code: &str) {
    let failures = check_contract(code);
    assert!(failures.is_empty(), "{code}\n{}", failures.join("\n"));
}

#[test]
#[ignore = "Known bug before ef507415: function assignments lose declared call contracts"]
fn declared_function_contracts_survive_all_call_forms() {
    for annotation in [
        "fun(value: number): number",
        "Callback",
        "Callback|nil",
        "TypedCallback<number>",
    ] {
        for definition in [
            "local accept = function(value) return value end",
            "local accept; accept = function(value) return value end",
            "local accept; function accept(value) return value end",
        ] {
            assert_contract(&format!(
                r#"
                ---@alias Callback fun(value: number): number
                ---@alias TypedCallback<T> fun(value: T): T
                ---@type {annotation}
                {definition}
                local alias = accept
                accept(1);
                (accept)(1);
                alias(1)
                pcall(accept, 1)
                accept('text'); -- Error: the declared parameter is a number.
                (accept)('text'); -- Error: parentheses preserve the contract.
                alias('text') -- Error: aliases preserve the contract.
                pcall(accept, 'text') -- Error: protected calls preserve the contract.
                ---@type string
                local text = alias(1) -- Error: the declared return is a number.
                "#,
            ));
        }
    }
}

#[test]
#[ignore = "Known bug before ef507415: function assignments lose declared call contracts"]
fn implementation_docs_do_not_replace_declared_call_returns() {
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
    ws.def(
        r#"
        ---@type fun(value: number): number
        ---@param value number
        local make = function(value) return 'text' end
        DirectResult = make(1)
        local alias = make
        AliasResult = alias(1)
        local _, result = pcall(make, 1)
        ProtectedResult = result
        "#,
    );
    for result in ["DirectResult", "AliasResult"] {
        assert_eq!(ws.expr_ty(result), LuaType::Number, "{result}");
    }
    // A protected call can also return the error message.
    let expected = ws.ty("number|string");
    assert_eq!(ws.expr_ty("ProtectedResult"), expected);
}

#[test]
fn function_assignments_keep_all_declared_overloads() {
    assert_contract(
        r#"
        ---@type (fun(value: number): number) | (fun(value: string): string)
        local accept
        function accept(value) return value end
        local alias = accept
        alias(1)
        alias('text')
        pcall(accept, 'text')
        alias(false) -- Error: neither overload accepts a boolean.
        pcall(accept, false) -- Error: neither overload accepts a boolean.
        ---@type string
        local text = alias(1) -- Error: this overload returns a number.
        "#,
    );
}

#[test]
#[ignore = "Known bug before ef507415: function assignments lose declared call contracts"]
fn nullable_function_contracts_still_follow_nil_assignments() {
    assert_contract(
        r#"
        ---@type (fun(value: number): number)?
        local accept
        function accept(value) return value end
        local alias = accept
        accept = nil
        alias(1)
        alias('text') -- Error: a saved function keeps its contract.
        accept(1) -- Error: the current value is nil.
        "#,
    );
}

#[test]
fn error_paths_do_not_make_constructed_fields_optional() {
    assert_contract(
        r#"
        ---@param stop boolean
        local function build(stop)
            local value = {}
            if stop then error('stop') else value.text = 'hello' end
            ---@type {text: string}
            local record = value
            return record
        end
        "#,
    );
}

#[test]
fn literal_true_loops_use_only_break_exits() {
    assert_contract(
        r#"
        local value = {}
        while true do
            value.text = 'hello'
            break
        end
        ---@type {text: string}
        local record = value

        ---@param done boolean
        local function build(done)
            local value = {}
            while (true) do
                if done then
                    value.text = 'hello'
                    break
                end
            end
            ---@type {text: string}
            local record = value
            return record
        end
        "#,
    );
}

#[test]
fn field_assertions_do_not_disable_inferred_table() {
    assert_contract(
        r#"
        local value = {count = 1 --[[@as integer]]}
        ---@type {text: string}
        local record = value -- The completed interface includes the later definition.
        value.text = 'late' -- An earlier use does not restrict definitions.

        local built = {text = 'hello' --[[@as string]]}
        built.count = 1
        ---@type {text: string, count: integer}
        local record = built
        built.extra = 1
        "#,
    );
}

#[test]
fn tuple_writes_check_the_declared_slot_type() {
    assert_contract(
        r#"
        ---@type [integer, string]
        local value = {1, 'hello'}
        value[1] = 2
        value[2] = 'updated'
        value[1] = 'bad' -- Error: the first slot accepts integers.
        value[2] = 3 -- Error: the second slot accepts strings.
        value[1] = nil
        "#,
    );
}
