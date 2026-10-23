#[cfg(test)]
mod test {
    use std::{ops::Deref, sync::Arc};

    use lsp_types::{Diagnostic, NumberOrString};
    use tokio_util::sync::CancellationToken;

    use crate::check::test::{check_source, check_source_with_emmyrc};
    use crate::{DiagnosticCode, Emmyrc, VirtualWorkspace};

    #[test]
    #[ignore = "Known bug before ef507415: overload contracts and candidate selection"]
    fn test_overloaded_function_values_keep_each_supported_contract() {
        for (setup, value) in [
            ("", "identity"),
            ("local alias = identity", "alias"),
            ("local mod = {identity = identity}", "mod.identity"),
        ] {
            for (domain, argument, valid) in [
                ("number", "1", true),
                ("string", "'text'", true),
                ("boolean", "true", false),
            ] {
                for (context, code) in [
                    (
                        format!(
                            "---@type fun(value: {domain}): {domain}\nlocal handler = {value}\nreturn handler"
                        ),
                        DiagnosticCode::AssignTypeMismatch,
                    ),
                    (
                        format!(
                            "---@param callback fun(value: {domain}): {domain}\nlocal function use(callback) return callback({argument}) end\nreturn use({value})"
                        ),
                        DiagnosticCode::ParamTypeMismatch,
                    ),
                    (
                        format!(
                            "---@return fun(value: {domain}): {domain}\nlocal function factory() return {value} end\nreturn factory"
                        ),
                        DiagnosticCode::ReturnTypeMismatch,
                    ),
                    (
                        format!(
                            "---@param ... fun(value: {domain}): {domain}\nlocal function use(...) return ... end\nreturn use({value})"
                        ),
                        DiagnosticCode::ParamTypeMismatch,
                    ),
                ] {
                    let diagnostics = check_source(&format!(
                        "---@param value number\n---@return number\n---@overload fun(value: string): string\nlocal function identity(value) return value end\n{setup}\n{context}"
                    ));
                    let expected = if valid { vec![] } else { vec![code] };
                    assert_eq!(
                        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                        expected,
                        "{domain}, {context}: {diagnostics:?}"
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "Known bug before ef507415: overload contracts and candidate selection"]
    fn test_generic_wrappers_preserve_overloads_and_callback_results() {
        let source = r#"
            ---@param value number
            ---@return number
            ---@overload fun(value: string): string
            local function identity(value) return value end
            ---@generic T
            ---@param value T
            ---@return T
            local function preserve(value) return value end
            local alias = preserve(identity)
            ---@generic R
            ---@param callback fun(value: string): R
            ---@return R
            local function apply(callback) return callback('text') end
            local result = apply(alias)
            ---@type string
            local correct = result
            ---@type number
            local wrong = result
            return correct, wrong
        "#;
        let diagnostics = check_source(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, DiagnosticCode::AssignTypeMismatch);
        assert_eq!(
            usize::from(diagnostics[0].range.start()),
            source.rfind("result").unwrap()
        );
    }

    #[test]
    #[ignore = "Known bug before ef507415: overload contracts and candidate selection"]
    fn test_generic_callbacks_prefer_specific_overloads_to_broad_fallbacks() {
        for (params, fallback, definition) in [
            (
                "---@param value number",
                "any",
                "local function identity(value) return value end",
            ),
            (
                "---@param value number",
                "unknown",
                "local function identity(value) return value end",
            ),
            (
                "---@param value any",
                "any",
                "local function identity(value) return value end",
            ),
            (
                "---@param value number\n---@param extra? number",
                "any",
                "local function identity(value, extra) return extra or value end",
            ),
            (
                "---@param ... number",
                "any",
                "local function identity(...) return ... end",
            ),
        ] {
            for (setup, callback) in [
                ("", "identity"),
                ("local alias = identity", "alias"),
                ("local mod = {identity = identity}", "mod.identity"),
            ] {
                let source = format!(
                    r#"
                {params}
                ---@return {fallback}
                ---@overload fun(value: number): number
                {definition}
                {setup}
                ---@generic R
                ---@param callback fun(value: number): R
                ---@return R
                local function apply(callback) return callback(1) end
                local result = apply({callback})
                ---@type number
                local correct = result
                ---@type string
                local wrong = result
                return correct, wrong
                "#
                );
                let diagnostics = check_source(&source);
                assert_eq!(diagnostics.len(), 1, "{callback}: {diagnostics:?}");
                assert_eq!(diagnostics[0].code, DiagnosticCode::AssignTypeMismatch);
                assert_eq!(
                    usize::from(diagnostics[0].range.start()),
                    source.rfind("result").unwrap()
                );
            }
        }
    }

    #[test]
    #[ignore = "Known bug before ef507415: overload contracts and candidate selection"]
    fn test_callback_overloads_cannot_require_arguments_the_caller_omits() {
        for overload in [
            "fun(value: number, first: string): string",
            "fun(value: number, ...: [string, string]...): string",
        ] {
            for (setup, callback) in [
                ("", "choose"),
                ("local alias = choose", "alias"),
                ("local mod = {choose = choose}", "mod.choose"),
            ] {
                let source = format!(
                    r#"
                    ---@param value number
                    ---@param first? number
                    ---@param second? number
                    ---@return number
                    ---@overload {overload}
                    local function choose(value, first, second)
                        return first or second or value
                    end
                    {setup}
                    ---@generic R
                    ---@param callback fun(value: number): R
                    ---@return R
                    local function apply(callback) return callback(1) end
                    local result = apply({callback})
                    ---@type number
                    local correct = result
                    ---@type string
                    local wrong = result
                    return correct, wrong
                    "#
                );
                let diagnostics = check_source(&source);
                assert_eq!(
                    diagnostics.len(),
                    1,
                    "{overload}, {callback}: {diagnostics:?}"
                );
                assert_eq!(diagnostics[0].code, DiagnosticCode::AssignTypeMismatch);
                assert_eq!(
                    usize::from(diagnostics[0].range.start()),
                    source.rfind("result").unwrap()
                );
            }
        }
    }

    #[test]
    #[ignore = "Known bug before ef507415: overload contracts and candidate selection"]
    fn test_replacing_an_overloaded_function_requires_every_contract() {
        for (replacement, valid) in [
            (
                "---@param value number\n---@return number\nlocal function replacement(value) return value end",
                false,
            ),
            (
                "---@param value string\n---@return string\n---@overload fun(value: number): number\nlocal function replacement(value) return value end",
                true,
            ),
        ] {
            let diagnostics = check_source(&format!(
                "---@param value number\n---@return number\n---@overload fun(value: string): string\nlocal function identity(value) return value end\n{replacement}\nidentity = replacement\nreturn identity"
            ));
            let expected = if valid {
                vec![]
            } else {
                vec![DiagnosticCode::AssignTypeMismatch]
            };
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                expected,
                "{replacement}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_function_union_must_match_for_every_possible_value() {
        let diagnostics = check_source(
            r#"
            ---@param value number
            ---@return number
            local function numeric(value) return value end
            ---@param value string
            ---@return string
            local function text(value) return value end
            ---@param callback fun(value: number): number
            local function use(callback) return callback(1) end
            ---@param choose boolean
            local function run(choose)
                local callback = choose and numeric or text
                return use(callback)
            end
            return run
            "#,
        );
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec![DiagnosticCode::ParamTypeMismatch],
            "{diagnostics:?}"
        );
    }

    #[test]
    #[ignore = "Known bug before ef507415: overload contracts and candidate selection"]
    fn test_overload_count_and_types_must_match_one_candidate() {
        for definition in [
            "local function accept(first, second) return first, second end",
            "local accept = function(first, second) return first, second end",
        ] {
            for callee in ["accept", "(accept)", "alias", "mod.accept"] {
                for protected in [false, true] {
                    for (arguments, valid) in
                        [("1, 2", true), ("'a', 'b', 'c'", true), ("42, 1, 2", false)]
                    {
                        let call = if protected {
                            format!("pcall({callee}, {arguments})")
                        } else {
                            format!("{callee}({arguments})")
                        };
                        let mut ws = VirtualWorkspace::new_with_init_std_lib();
                        let file_id = ws.def(&format!(
                            "---@param first number\n---@param second number\n---@overload fun(...: string)\n{definition}\nlocal alias = accept\nlocal mod = {{accept = accept}}\n{call}\nreturn alias, mod"
                        ));
                        let model = ws.analysis.semantic_model(file_id);
                        let diagnostics = crate::check::check_file(
                            &model,
                            Arc::new(crate::CheckConfig::new(&ws.get_emmyrc())),
                        );
                        assert_eq!(
                            diagnostics.is_empty(),
                            valid,
                            "{definition}, {call}: {diagnostics:?}"
                        );
                        assert!(
                            diagnostics
                                .iter()
                                .all(|d| d.code == DiagnosticCode::ParamTypeMismatch),
                            "{call}: {diagnostics:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_reassigned_function_does_not_keep_previous_overloads() {
        let diagnostics = check_source(
            r#"
            ---@param first number
            ---@param second number
            ---@overload fun(...: string)
            local accept = function(first, second) return first, second end
            ---@param value boolean
            accept = function(value) return value end
            accept('text')
            "#,
        );
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec![
                DiagnosticCode::ParamTypeMismatch,
                DiagnosticCode::AssignTypeMismatch
            ],
            "{diagnostics:?}"
        );
    }

    #[test]
    #[ignore = "Known bug before ef507415: dictionary key and value validation"]
    fn test_dictionary_overload_requires_matching_keys_values_and_mode() {
        for (call, valid) in [
            ("accept({'a'}, 'a')", true),
            ("accept({count = 1}, 1)", true),
            ("accept({count = 1}, 'a')", false),
            ("accept({'a'}, 1)", false),
            ("accept({[key] = 1}, 1)", true),
            ("accept({[key] = 1}, 'a')", false),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                local key = 'count'
                ---@param values table<integer, string>
                ---@param mode string
                ---@overload fun(values: table<string, integer>, mode: integer)
                local function accept(values, mode) return values, mode end
                {call}
                return key
                "#,
            ));
            assert_eq!(diagnostics.is_empty(), valid, "{call}: {diagnostics:?}");
            assert!(
                diagnostics
                    .iter()
                    .all(|d| d.code == DiagnosticCode::ParamTypeMismatch),
                "{call}: {diagnostics:?}"
            );
        }
    }

    #[test]
    #[ignore = "Known bug before ef507415: dictionary key and value validation"]
    fn test_dictionary_arguments_use_actual_computed_key_types() {
        for (key, domain, valid) in [
            ("1", "integer", true),
            ("1", "string", false),
            ("'entry'", "string", true),
            ("'entry'", "integer", false),
        ] {
            let diagnostics = check_source(&format!(
                "local key = {key}\n---@param values table<{domain}, string>\nlocal function accept(values) return values end\naccept({{[key] = 'ok'}})"
            ));
            assert_eq!(
                diagnostics.is_empty(),
                valid,
                "{domain}, {key}: {diagnostics:?}"
            );
            assert!(
                diagnostics
                    .iter()
                    .all(|d| d.code == DiagnosticCode::ParamTypeMismatch),
                "{diagnostics:?}"
            );
        }
    }

    #[test]
    #[ignore = "Known bug before ef507415: dictionary key and value validation"]
    fn test_dictionary_arguments_check_the_final_value_of_repeated_keys() {
        for (constructor, valid) in [
            ("{entry = 42, [key] = 'ok'}", true),
            ("{[key] = 42, entry = 'ok'}", true),
            ("{entry = 'ok', [key] = 42}", false),
            ("{entry = 42, [key] = nil}", true),
        ] {
            let diagnostics = check_source(&format!(
                "local key = 'entry'\n---@param values table<string, string>\nlocal function accept(values) return values end\naccept({constructor})\nreturn key"
            ));
            assert_eq!(
                diagnostics.is_empty(),
                valid,
                "{constructor}: {diagnostics:?}"
            );
            assert!(
                diagnostics
                    .iter()
                    .all(|d| d.code == DiagnosticCode::ParamTypeMismatch),
                "{diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_dictionary_argument_checks_do_not_read_later_mutations() {
        let diagnostics = check_source(
            "---@param values table<string, string>\nlocal function accept(values) return values end\nlocal values = {}\naccept(values)\nvalues.count = 1\nreturn values",
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    #[ignore = "Known bug before ef507415: static object fields lose precedence over index signatures"]
    fn test_table_fields_use_static_members_before_index_signatures() {
        for target in ["{[1]: number, [integer]: string}", "Record"] {
            for key in ["1", "1.0", "key"] {
                for (value, valid) in [("42", true), ("'text'", false)] {
                    for use_value in [
                        format!("---@type {target}\nlocal value = {{[{key}] = {value}}}"),
                        format!("accept({{[{key}] = {value}}})"),
                    ] {
                        let mut ws = VirtualWorkspace::new();
                        assert_eq!(
                            ws.has_no_diagnostic(
                                DiagnosticCode::AssignTypeMismatch,
                                &format!(
                                    r#"
                                    ---@class Record
                                    ---@field [1] number
                                    ---@field [integer] string
                                    local Record = {{}}
                                    local key = 1
                                    ---@type fun(value: {target})
                                    local accept
                                    {use_value}
                                    "#,
                                ),
                            ),
                            valid,
                            "{target}, {use_value}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_table_argument_fields_follow_call_generic_bindings() {
        for (generic, call, valid) in [
            ("T", "accept(1, {value = 2})", true),
            ("T", "accept('hello', {value = 'world'})", true),
            ("const T", "accept(1, {value = 1})", true),
            ("const T", "accept(1, {value = 2})", false),
            ("T", "accept--[[@<number>]](1, {value = 2})", true),
            ("T", "accept--[[@<number>]](1, {value = 'text'})", false),
        ] {
            for fields in ["{value: T}", "Fields<T>", "{[string]: T}"] {
                let diagnostics = check_source(&format!(
                    r#"
                    ---@alias Fields<T> {{value: T}}
                    ---@generic {generic}
                    ---@param item T
                    ---@param values {fields}
                    local function accept(item, values) return item, values end
                    {call}
                    "#,
                ));
                assert_eq!(
                    diagnostics.is_empty(),
                    valid,
                    "{fields}, {generic}, {call}: {diagnostics:?}"
                );
                if !valid {
                    assert!(
                        diagnostics.iter().any(|d| matches!(
                            d.code,
                            DiagnosticCode::ParamTypeMismatch | DiagnosticCode::AssignTypeMismatch
                        )),
                        "{diagnostics:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_table_argument_fields_use_method_parameter_positions() {
        for (call, valid) in [
            ("mod:accept({value = 2})", true),
            ("mod.accept(mod, {value = 2})", true),
            ("mod:accept({value = 'text'})", false),
            ("mod.accept(mod, {value = 'text'})", false),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@class Module
                ---@field value string
                local mod = {{value = 'text'}}
                ---@generic T
                ---@param receiver T
                ---@param fields {{value: number}}
                function mod.accept(receiver, fields) return receiver, fields end
                {call}
                "#,
            ));
            assert_eq!(diagnostics.is_empty(), valid, "{call}: {diagnostics:?}");
            if !valid {
                assert!(diagnostics.iter().any(|d| matches!(
                    d.code,
                    DiagnosticCode::ParamTypeMismatch | DiagnosticCode::AssignTypeMismatch
                )));
            }
        }
    }

    #[test]
    fn test_table_argument_fields_accept_any_matching_overload() {
        for (generic_fields, number_fields) in [
            ("{value: T}", "{value: number}"),
            ("{[string]: T}", "{[string]: number}"),
        ] {
            for (call, valid) in [
                ("accept('hello', {value = 2})", true),
                ("accept('hello', {value = 'world'})", true),
                ("accept('hello', {value = false})", false),
            ] {
                let diagnostics = check_source(&format!(
                    r#"
                    ---@type fun<T>(seed: T, fields: {generic_fields}) | fun(seed: string, fields: {number_fields})
                    local accept
                    {call}
                    "#,
                ));
                assert_eq!(
                    diagnostics.is_empty(),
                    valid,
                    "{generic_fields}, {call}: {diagnostics:?}"
                );
                if !valid {
                    assert!(diagnostics.iter().any(|d| matches!(
                        d.code,
                        DiagnosticCode::ParamTypeMismatch | DiagnosticCode::AssignTypeMismatch
                    )));
                }
            }
        }
    }

    #[test]
    fn test_table_argument_fields_use_count_matching_overloads() {
        for (signature, good, bad) in [
            (
                "fun() | fun(fields: {value: number})",
                "accept({value = 2})",
                "accept({value = 'text'})",
            ),
            (
                "fun(seed: number) | fun(seed: number, fields: {[string]: number})",
                "accept(1, {value = 2})",
                "accept(1, {value = 'text'})",
            ),
        ] {
            for (call, valid) in [(good, true), (bad, false)] {
                let diagnostics =
                    check_source(&format!("---@type {signature}\nlocal accept\n{call}"));
                assert_eq!(
                    diagnostics.is_empty(),
                    valid,
                    "{signature}, {call}: {diagnostics:?}"
                );
                if !valid {
                    assert!(diagnostics.iter().any(|d| matches!(
                        d.code,
                        DiagnosticCode::ParamTypeMismatch | DiagnosticCode::AssignTypeMismatch
                    )));
                }
            }
        }
    }

    #[test]
    fn test_table_argument_fields_keep_fixed_arguments_with_varargs() {
        for (signature, call) in [
            (
                "fun() | fun(fields: {value: number}, ...: any)",
                "accept({value = VALUE}, ...)",
            ),
            (
                "fun() | fun(fields: {[string]: number}, ...: any)",
                "accept({value = VALUE}, ...)",
            ),
            (
                "fun(seed: any) | fun(seed: any, fields: {value: number})",
                "accept(..., {value = VALUE})",
            ),
            (
                "fun(fields: {value: string}) | fun(fields: {value: number}, tail: any)",
                "accept({value = VALUE}, (...))",
            ),
        ] {
            for (value, valid) in [("2", true), ("'text'", false)] {
                let call = call.replace("VALUE", value);
                let diagnostics = check_source(&format!(
                    r#"
                    ---@type {signature}
                    local accept
                    local function forward(...)
                        {call}
                    end
                    return forward
                    "#,
                ));
                assert_eq!(
                    diagnostics.is_empty(),
                    valid,
                    "{signature}, {call}: {diagnostics:?}"
                );
                if !valid {
                    assert!(diagnostics.iter().any(|d| matches!(
                        d.code,
                        DiagnosticCode::ParamTypeMismatch | DiagnosticCode::AssignTypeMismatch
                    )));
                }
            }
        }
    }

    #[test]
    fn test_forwarded_varargs_preserve_possible_argument_counts() {
        for (annotation, call, valid) in [
            ("", "accept(...)", true),
            ("", "accept({value = 'text'}, ...)", true),
            (
                "---@param ... [boolean]...",
                "accept({value = 2}, ...)",
                true,
            ),
            (
                "---@param ... [boolean]...",
                "accept({value = 'text'}, ...)",
                false,
            ),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@type fun() | fun(fields: {{value: string}}) | fun(fields: {{value: number}}, tail: boolean)
                local accept
                {annotation}
                local function forward(...)
                    {call}
                end
                return forward
                "#,
            ));
            assert_eq!(
                diagnostics.is_empty(),
                valid,
                "{annotation}, {call}: {diagnostics:?}"
            );
            if !valid {
                assert!(diagnostics.iter().any(|d| matches!(
                    d.code,
                    DiagnosticCode::ParamTypeMismatch | DiagnosticCode::AssignTypeMismatch
                )));
            }
        }
    }

    #[test]
    #[ignore = "Known bug before ef507415: overload contracts and candidate selection"]
    fn test_protected_calls_use_count_matching_overloads() {
        for (fields, valid) in [("{value = 2}", true), ("{value = 'text'}", false)] {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            assert_eq!(
                ws.has_no_diagnostic(
                    DiagnosticCode::AssignTypeMismatch,
                    &format!(
                        r#"
                        ---@type fun() | fun(fields: {{[string]: number}})
                        local accept
                        pcall(accept, {fields})
                        "#,
                    ),
                ),
                valid,
                "{fields}"
            );
        }
    }

    #[test]
    #[ignore = "Known bug before ef507415: protected calls select incompatible overloads"]
    fn test_protected_calls_keep_fixed_arguments_with_varargs() {
        for (fields, valid) in [("{value = 2}", true), ("{value = 'text'}", false)] {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            assert_eq!(
                ws.has_no_diagnostic(
                    DiagnosticCode::AssignTypeMismatch,
                    &format!(
                        r#"
                        ---@type fun() | fun(fields: {{[string]: number}}, ...: any)
                        local accept
                        local function forward(...)
                            pcall(accept, {fields}, ...)
                        end
                        return forward
                        "#,
                    ),
                ),
                valid,
                "{fields}"
            );
        }
    }

    #[test]
    #[ignore = "Known bug before ef507415: table union members are not checked as complete alternatives"]
    fn test_table_union_arguments_accept_each_matching_member() {
        for callee in ["accept(", "pcall(accept, "] {
            for fields in [
                "{value: number}|{value: string}",
                "{[string]: number}|{[string]: string}",
                "Fields",
            ] {
                for (value, valid) in [("2", true), ("'text'", true), ("false", false)] {
                    let mut ws = VirtualWorkspace::new_with_init_std_lib();
                    let file_id = ws.def(&format!(
                        r#"
                        ---@alias Fields {{value: number}}|{{value: string}}
                        ---@type fun(fields: {fields})
                        local accept
                        {callee}{{value = {value}}})
                        "#,
                    ));
                    let diagnostics = ws
                        .analysis
                        .diagnose_file(file_id, CancellationToken::new())
                        .unwrap();
                    assert_eq!(
                        diagnostics.is_empty(),
                        valid,
                        "{callee}, {fields}, {value}: {diagnostics:?}"
                    );
                    if !valid {
                        assert!(diagnostics.iter().any(|d| matches!(
                            &d.code,
                            Some(NumberOrString::String(code))
                                if code == DiagnosticCode::AssignTypeMismatch.get_name()
                        )));
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "Known bug before ef507415: callable class generic and optional parameters"]
    fn test_callable_classes_keep_generic_and_optional_parameters() {
        for (call, valid) in [
            ("factory()", true),
            ("factory(1)", true),
            ("factory('text')", false),
            ("pcall(factory)", true),
            ("pcall(factory, 1)", true),
            ("pcall(factory, 'text')", false),
        ] {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            let file = ws.def(&format!(
                r#"
                ---@class Factory<T>
                ---@overload fun(value?: T): T
                ---@type Factory<number>
                local factory
                {call}
                "#,
            ));
            let diagnostics = ws
                .analysis
                .diagnose_file(file, CancellationToken::new())
                .unwrap();
            assert_eq!(diagnostics.is_empty(), valid, "{call}: {diagnostics:?}");
            if !valid {
                assert!(diagnostics.iter().any(|diagnostic| matches!(
                    &diagnostic.code,
                    Some(NumberOrString::String(code))
                        if code == DiagnosticCode::ParamTypeMismatch.get_name()
                )));
            }
        }
    }

    #[test]
    fn test_table_unions_use_assignment_rules_for_each_member() {
        for allow_base in [true, false] {
            for fields in ["Options", "Options|nil", "Options|{state: 'off'}"] {
                for callee in ["accept(", "pcall(accept, "] {
                    let mut ws = VirtualWorkspace::new_with_init_std_lib();
                    let mut config = ws.analysis.get_emmyrc().deref().clone();
                    config.strict.doc_base_const_match_base_type = allow_base;
                    ws.analysis.update_config(Arc::new(config));
                    let file = ws.def(&format!(
                        r#"
                        ---@alias Options {{state: 'on'}}
                        ---@type fun(options: {fields})
                        local accept
                        ---@param state string
                        local function forward(state)
                            {callee}{{state = state}})
                        end
                        return forward
                        "#,
                    ));
                    let diagnostics = ws
                        .analysis
                        .diagnose_file(file, CancellationToken::new())
                        .unwrap();
                    assert_eq!(
                        diagnostics.is_empty(),
                        allow_base,
                        "{allow_base}, {fields}, {callee}: {diagnostics:?}"
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "Known bug before ef507415: table union members are not checked as complete alternatives"]
    fn test_table_union_arguments_require_one_complete_member() {
        for (fields, value, valid) in [
            ("{left: number}|{right: string}", "{left = 2}", true),
            ("{left: number}|{right: string}", "{right = 'text'}", true),
            ("{left: number}|{right: string}", "{left = false}", false),
            (
                "{a: number, b: number}|{a: string, b: string}",
                "{a = 2, b = 'text'}",
                false,
            ),
            ("table|{value: number}", "{value = 'text'}", true),
            ("{value: number}|nil", "{value = 'text'}", false),
            ("{value: number}|fun()", "{value = 'text'}", false),
            (
                "{nested: {left: number}}|{nested: {right: string}}",
                "{nested = {right = 'text'}}",
                true,
            ),
            (
                "{nested: {left: number}}|{nested: {right: string}}",
                "{nested = {left = false}}",
                false,
            ),
        ] {
            for callee in ["accept(", "pcall(accept, "] {
                let mut ws = VirtualWorkspace::new_with_init_std_lib();
                let file_id = ws.def(&format!(
                    "---@type fun(fields: {fields})\nlocal accept\n{callee}{value})",
                ));
                let diagnostics = ws
                    .analysis
                    .diagnose_file(file_id, CancellationToken::new())
                    .unwrap();
                assert_eq!(
                    diagnostics.is_empty(),
                    valid,
                    "{callee}, {fields}, {value}: {diagnostics:?}"
                );
            }
        }
    }

    #[test]
    #[ignore = "Known bug before ef507415: pcall aliases do not share forwarding checks"]
    fn test_pcall_aliases_keep_callback_field_checks() {
        for (alias, callee) in [
            ("local pcall = pcall", "pcall"),
            ("local protected = pcall", "protected"),
            (
                "local protected = (pcall); local pcall = protected",
                "pcall",
            ),
            ("", "(pcall)"),
        ] {
            for (value, valid) in [("2", true), ("'text'", false)] {
                let mut ws = VirtualWorkspace::new_with_init_std_lib();
                let file_id = ws.def(&format!(
                    r#"
                    {alias}
                    ---@type fun(fields: {{value: number}})
                    local accept
                    {callee}(accept, {{value = {value}}})
                    "#,
                ));
                let diagnostics = ws
                    .analysis
                    .diagnose_file(file_id, CancellationToken::new())
                    .unwrap();
                assert_eq!(
                    diagnostics.is_empty(),
                    valid,
                    "{alias}, {callee}, {value}: {diagnostics:?}"
                );
                if !valid {
                    assert!(diagnostics.iter().any(|d| matches!(
                        &d.code,
                        Some(NumberOrString::String(code))
                            if code == DiagnosticCode::AssignTypeMismatch.get_name()
                    )));
                }
            }
        }
    }

    #[test]
    #[ignore = "Known bug before ef507415: pcall aliases do not share forwarding checks"]
    fn test_pcall_aliases_use_the_value_at_each_read() {
        for (alias, callee, later) in [
            ("local pcall = pcall", "pcall", "pcall = wrap"),
            ("local pcall; pcall = original_pcall", "pcall", ""),
            ("local pcall = pcall; pcall = pcall", "pcall", ""),
            ("local pcall = pcall", "pcall", "pcall = pcall"),
            (
                "local source = pcall; local protected = source; source = wrap",
                "protected",
                "",
            ),
            (
                "local protected; if flag then protected = pcall else protected = original_pcall end",
                "protected",
                "",
            ),
        ] {
            for (value, valid) in [("2", true), ("'text'", false)] {
                let mut ws = VirtualWorkspace::new_with_init_std_lib();
                let file_id = ws.def(&format!(
                    r#"
                    local original_pcall = pcall
                    local function wrap(callback, ...)
                        return original_pcall(callback, ...)
                    end
                    ---@type boolean
                    local flag
                    {alias}
                    ---@type fun(fields: {{value: number}})
                    local accept
                    {callee}(accept, {{value = {value}}})
                    {later}
                    return wrap, flag, {callee}
                    "#,
                ));
                let diagnostics = ws
                    .analysis
                    .diagnose_file(file_id, CancellationToken::new())
                    .unwrap();
                assert_eq!(
                    diagnostics.is_empty(),
                    valid,
                    "{alias}, {later}, {value}: {diagnostics:?}"
                );
                if !valid {
                    assert!(diagnostics.iter().any(|d| matches!(
                        &d.code,
                        Some(NumberOrString::String(code))
                            if code == DiagnosticCode::AssignTypeMismatch.get_name()
                    )));
                }
            }
        }
    }

    #[test]
    #[ignore = "Known bug before ef507415: function replacement keeps stale builtin identity"]
    fn test_function_definition_replaces_builtin_parameters() {
        for (value, valid) in [("'text'", true), ("42", false)] {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            let file_id = ws.def(&format!(
                r#"
                local protected = pcall
                ---@type fun(fields: {{value: number}})
                local accept
                protected(accept, {{value = 2}})
                ---@param value string
                function protected(callback, value) return callback, value end
                protected(accept, {value})
                "#,
            ));
            let diagnostics = ws
                .analysis
                .diagnose_file(file_id, CancellationToken::new())
                .unwrap();
            assert_eq!(diagnostics.is_empty(), valid, "{value}: {diagnostics:?}");
            if !valid {
                assert!(diagnostics.iter().any(|d| matches!(
                    &d.code,
                    Some(NumberOrString::String(code))
                        if code == DiagnosticCode::ParamTypeMismatch.get_name()
                )));
            }
        }
    }

    #[test]
    #[ignore = "Known bug before ef507415: replaced pcall still uses builtin forwarding"]
    fn test_replaced_pcall_uses_its_new_parameters() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            local pcall = pcall
            ---@type fun(fields: {value: number})
            local accept
            pcall(accept, {value = 2})
            pcall = function(callback, value) return callback, value end
            pcall(accept, {value = 'text'})
            "#,
        );
        let diagnostics = ws
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    #[ignore = "Known bug before ef507415: tuple parameter tails do not check every argument"]
    fn test_variadic_tuple_tails_check_each_argument() {
        for callee in ["accept(", "pcall(accept, "] {
            for (values, valid) in [
                ("42, 'a', 'b'", true),
                ("42, 'a', false", false),
                ("42, 'a', 'b', false", false),
            ] {
                let mut ws = VirtualWorkspace::new_with_init_std_lib();
                assert_eq!(
                    ws.has_no_diagnostic(
                        DiagnosticCode::ParamTypeMismatch,
                        &format!(
                            r#"
                            ---@param ... [number, string...]...
                            local function accept(...) return ... end
                            {callee}{values})
                            "#,
                        ),
                    ),
                    valid,
                    "{callee}{values})"
                );
            }
        }
    }

    #[test]
    #[ignore = "Known bug before ef507415: indexed mapped keys do not fully evaluate"]
    fn test_mapped_parameter_evaluates_indexed_keys() {
        for (keys, good, bad) in [
            ("({keys: 'x'})['keys']", "{x = 'text'}", "{x = 42}"),
            ("Source['keys']", "{x = 'text'}", "{x = 42}"),
            ("Keys", "{x = 'text'}", "{x = 42}"),
            (
                "({keys: 'x' | 'y'})['keys']",
                "{x = 'text', y = 'text'}",
                "{x = 42, y = 'text'}",
            ),
        ] {
            for (value, valid) in [(good, true), (bad, false)] {
                let diagnostics = check_source(&format!(
                    r#"
                    ---@class Source: {{keys: 'x'}}
                    local Source = {{}}
                    ---@alias Keys Source['keys']

                    ---@param fields {{[K in {keys}]: string}}
                    local function accept(fields) return fields end
                    accept({value})
                    return Source
                    "#,
                ));
                assert_eq!(
                    diagnostics.is_empty(),
                    valid,
                    "{keys}, {value}: {diagnostics:?}"
                );
                if !valid {
                    assert!(diagnostics.iter().any(|d| matches!(
                        d.code,
                        DiagnosticCode::ParamTypeMismatch | DiagnosticCode::AssignTypeMismatch
                    )));
                }
            }
        }
    }

    #[test]
    fn test_table_argument_fields_check_each_variadic_argument() {
        for (call, valid) in [
            ("accept(42, {x = 1}, {y = 2})", true),
            ("accept(42, {x = 1}, {y = 'text'})", false),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@generic T
                ---@param seed T
                ---@param ... {{[string]: T}}
                local function accept(seed, ...) return seed, ... end
                {call}
                "#,
            ));
            assert_eq!(diagnostics.is_empty(), valid, "{call}: {diagnostics:?}");
            if !valid {
                assert!(diagnostics.iter().any(|d| matches!(
                    d.code,
                    DiagnosticCode::ParamTypeMismatch | DiagnosticCode::AssignTypeMismatch
                )));
            }
        }
    }

    #[test]
    fn test_table_argument_fields_keep_assignment_diagnostic_settings() {
        for (disabled, expected) in [
            (
                DiagnosticCode::ParamTypeMismatch,
                vec![DiagnosticCode::AssignTypeMismatch],
            ),
            (DiagnosticCode::AssignTypeMismatch, vec![]),
        ] {
            let mut emmyrc = Emmyrc::default();
            emmyrc.diagnostics.disable.push(disabled);
            let diagnostics = check_source_with_emmyrc(
                r#"
                ---@generic T
                ---@param seed T
                ---@param fields {[string]: T}
                local function accept(seed, fields) return seed, fields end
                accept(42, {value = 'text'})
                "#,
                emmyrc,
            );
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                expected,
                "{disabled:?}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_mapped_parameter_keeps_function_generic_values() {
        for (mapped, value, valid) in [
            ("{[K in 'x']: T}", "{x = 42}", true),
            ("{[K in 'x']: T}", "{x = 'x'}", false),
            ("{[K in 'x']: {[P in 'y']: T}}", "{x = {y = 42}}", true),
            ("{[K in 'x']: {[P in 'y']: T}}", "{x = {y = 'x'}}", false),
            ("{[T in 'x']: T}", "{x = 'x'}", true),
            ("{[T in 'x']: T}", "{x = 42}", false),
            (
                "{[K in 'x']: (string extends infer K and K or never)}",
                "{x = 'text'}",
                true,
            ),
            (
                "{[K in 'x']: (string extends infer K and K or never)}",
                "{x = 42}",
                false,
            ),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@generic T: number
                ---@param item T
                ---@param values {mapped}
                local function accept(item, values) return item, values end
                accept(42, {value})
                "#,
            ));
            assert_eq!(
                diagnostics.is_empty(),
                valid,
                "{mapped}, {value}: {diagnostics:?}"
            );
            if !valid {
                assert!(
                    diagnostics.iter().any(|d| matches!(
                        d.code,
                        DiagnosticCode::ParamTypeMismatch | DiagnosticCode::AssignTypeMismatch
                    )),
                    "{diagnostics:?}"
                );
            }
        }
    }

    #[test]
    fn test_function_argument_accepts_any_parameter() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@param value any
            local function accept(value) print(value) end
            ---@param f function
            local function use(f) accept(f) end
            use(function() end)
            "#,
        );
        let diagnostics = ws
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    }

    #[test]
    fn test_function_argument_preserves_narrow_parameter_checks() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@param value number
            local function accept_number(value) print(value) end
            ---@param callback fun(value: string): string
            local function accept_callback(callback) print(callback('hello')) end
            ---@param f function
            ---@param callback fun(value: number): number
            local function use(f, callback)
                accept_number(f)
                accept_callback(callback)
            end
            use(function() end, function(value) return value end)
            "#,
        );
        let diagnostics = ws
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .unwrap();
        let codes: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.clone())
            .collect();
        assert_eq!(
            codes,
            vec![Some(NumberOrString::String("param-type-mismatch".into())); 2],
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn test_tuple_argument_accepts_valid_elements() {
        for (target, value) in [
            ("[integer, integer]", "{1, 2}"),
            ("[integer, integer]?", "{1, 2}"),
            ("Pair", "{1, 2}"),
            ("Pair?", "{1, 2}"),
            ("Pair?", "nil"),
            ("[integer, integer?]", "{1}"),
            ("[integer, integer?]", "{1, 2}"),
            ("[integer, integer|string]", "{1, \"text\"}"),
            ("Pair", "{1, 2, 3}"),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@alias Pair [integer, integer]
                ---@param pair {target}
                local function use_pair(pair) return pair end
                use_pair({value})
                "#,
            ));
            assert!(diagnostics.is_empty(), "{target}, {value}: {diagnostics:?}");
        }
    }

    #[test]
    fn test_tuple_argument_rejects_invalid_elements() {
        for (target, value) in [
            ("[integer, integer]", "{1, \"bad\"}"),
            ("[integer, integer]?", "{1, \"bad\"}"),
            ("Pair", "{1, \"bad\"}"),
            ("Pair?", "{1, \"bad\"}"),
            ("[integer, integer?]", "{1, \"bad\"}"),
            ("[integer, integer|boolean]", "{1, \"bad\"}"),
            ("Pair", "{1}"),
            ("Pair | [string, string] | boolean", "{1, \"bad\"}"),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@alias Pair [integer, integer]
                ---@param pair {target}
                local function use_pair(pair) return pair end
                use_pair({value})
                "#,
            ));
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                vec![DiagnosticCode::ParamTypeMismatch],
                "{target}, {value}: {diagnostics:?}"
            );
        }
    }

    fn param_type_diagnostics(ws: &mut VirtualWorkspace, block_str: &str) -> Vec<Diagnostic> {
        ws.analysis
            .diagnostic
            .enable_only(DiagnosticCode::ParamTypeMismatch);
        let file_id = ws.def(block_str);
        let code = Some(NumberOrString::String(
            DiagnosticCode::ParamTypeMismatch.get_name().to_string(),
        ));
        ws.analysis
            .diagnose_file(file_id, CancellationToken::new())
            .unwrap_or_default()
            .into_iter()
            .filter(|diagnostic| diagnostic.code == code)
            .collect()
    }

    #[test]
    fn test_any_unknown_accept_any_target() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@param x number
            local function f(x) end

            ---@type any
            local a
            f(a)

            ---@type unknown
            local b
            f(b)
            "#
        ));
    }

    #[test]
    fn test_lambda_callback_no_mismatch() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@param callback fun(msg: string): boolean
                local function f(callback)
                end

                f(function(msg)
                    return true
                end)
            "#
        ));
    }

    #[test]
    fn test_param_type_mismatch_still_runs_when_count_diagnostics_disabled() {
        let mut ws = VirtualWorkspace::new();
        let diagnostics = param_type_diagnostics(
            &mut ws,
            r#"
                ---@param a string
                ---@param b string
                local function test(a, b)
                end

                test(1)
        "#,
        );

        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].message.contains("string"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_issue_216() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@alias F1 fun(x: integer):integer
            do
                ---@type F1
                local test = function(x) return x + 1 end

                test("wrong type")
            end
        "#
        ));
    }

    #[test]
    fn test_issue_82() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@generic F: function
            ---@param _a F|integer
            ---@param _b? F
            ---@return F
            function foo(_a, _b)
                return _a
            end
            foo(function() end)
        "#
        ));
    }

    #[test]
    fn test_overload_param_type_mismatch_unions_failed_position() {
        let mut ws = VirtualWorkspace::new();
        let diagnostics = param_type_diagnostics(
            &mut ws,
            r#"
            ---@type fun(name: "游戏-初始化") | fun(name: "游戏-开始")
            local event
            local bad ---@type boolean

            event(bad)
        "#,
        );

        assert_eq!(diagnostics.len(), 1);
        let message = &diagnostics[0].message;
        assert!(message.contains("boolean"), "{message}");
        assert!(message.contains("游戏-初始化"), "{message}");
        assert!(message.contains("游戏-开始"), "{message}");
    }

    #[test]
    fn test_issue_75() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            local a, b = pcall(string.rep, "a", "w")
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            local a, b = pcall(string.rep, "a", 10000)
        "#
        ));
    }

    #[test]
    fn test_issue_85() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
        ---@param a table | nil
        local function foo(a)
            a = a or {}
            _ = a.b
        end

        ---@param a table?
        local function _bar(a)
            a = a or {}
            _ = a.b
        end
        "#
        ));
    }

    #[test]
    fn test_issue_84() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
        ---@param _a string[]
        local function bar(_a) end

        ---@param a? string[]?
        local function _foo(a)
            if not a then
                a = {}
            end

            bar(a)

            if not a then
                a = {}
            end

            bar(a)
        end
        "#
        ));
    }

    #[test]
    fn test_issue_83() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
        ---@param _t table<any, string>
        local function foo(_t) end

        foo({})
        foo({'a'})
        foo({'a', 'b'})

        local a ---@type string[]
        foo(a)
        "#
        ));
    }

    #[test]
    fn test_issue_113() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@enum Baz
            local asd = {
                Foo = 0,
                Bar = 1,
                Baz = 2,
            }

            ---@param bob {a: Baz}
            function Foo(bob)
                return Bar(bob)
            end

            ---@param bob {a: Baz}
            function Bar(bob)
            end
        "#
        ));
    }

    #[test]
    fn test_issue_111() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
        local Table = {}

        ---@param target table
        ---@param ... table
        ---@return table
        function Table.mergeInto(target, ...)
            -- Stuff
        end

        ---@param ... table
        ---@return table
        function Table.merge(...)
            return Table.mergeInto({}, ...)
        end
        "#
        ));
    }

    #[test]
    fn test_var_param_check() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@param target table
        ---@param ... table
        ---@return table
        function mergeInto(target, ...)
            -- Stuff
        end
        "#,
        );

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
        mergeInto({}, 1)
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
        mergeInto({}, {}, {})
        "#
        ));
    }

    #[test]
    fn test_issue_102() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
        ---@param _kind '' | 'Nr' | 'Ln' | 'Cul'
        function foo(_kind) end

        for _, kind in ipairs({ '', 'Nr', 'Ln', 'Cul' }) do
            foo(kind)
        end
        "#
        ));
    }

    #[test]
    fn test_issue_95() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
        local range ---@type { [1]: integer, [2]: integer }

        table.sort(range)
        "#
        ));
    }

    #[test]
    fn test_issue_135() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
        ---@alias A
        ---| "number" # A number

        ---@param a A
        local function f(a)
        end

        f("number")
        "#
        ));
    }

    #[test]
    fn test_colon_call_and_not_colon_define() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@class Test
            local Test = {}

            ---@param a string
            function Test.name(a)
            end

            Test:name()
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@class Test
            local Test = {}

            ---@param ... any
            function Test.dots(...)
            end

            Test:dots("a", "b", "c")
        "#
        ));
    }

    #[test]
    fn test_issue_148() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            local a = (''):format()
        "#
        ));
    }

    #[test]
    fn test_generic_dots_param() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            local d = select(1, 1, 2, 3)
        "#
        ));
    }

    #[test]
    fn test_bool_as_type() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
        --- @param _x string|true
        function foo(_x) end

        foo(true)
        "#
        ));
    }

    #[test]
    fn test_function() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@param sorter function
            ---@return string[]
            local function getTableKeys(sorter)
                local keys = {}
                table.sort(keys, sorter)
                return keys
            end
        "#
        ));
    }

    #[test]
    fn test_table_array() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@generic K, V
                ---@param t table<K, V>
                ---@return table<V, K>
                local function revertMap(t)
                end

                ---@param arr any[]
                local function sortCallbackOfIndex(arr)
                    ---@type table<any, integer>
                    local indexMap = revertMap(arr)
                end
        "#
        ));
    }

    #[test]
    fn test_table_class() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@param t table
                local function bar(t)
                end

                ---@class D11.A

                ---@type D11.A|any
                local a

                bar(a)
        "#
        ));
    }

    #[test]
    fn test_table_1() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@param t table[]
                local function bar(t)
                end

                ---@type table|any
                local a

                bar(a)
        "#
        ));
    }

    #[test]
    fn test_pairs() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@diagnostic disable: missing-return
                ---@generic K, V
                ---@param t table<K, V> | V[] | {[K]: V}
                ---@return fun(tbl: any):K, std.NotNull<V>
                local function _pairs(t) end

                ---@class D10.A

                ---@type {[string]: D10.A, _id: D10.A}
                local a

                for k, v in _pairs(a) do
                end
        "#
        ));
    }

    #[test]
    fn test_issue_278() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
        local a --- @type type|'callable'
        error(a)  -- expected `string` but found `(type|"callable")`
        "#
        ))
    }

    #[test]
    fn test_issue_696() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
        local ty --- @type type|fun(x:any): boolean

        --- @param _ty fun(v:any):boolean
        local function validate(_ty) end

        if type(ty) == 'function' then
          validate(ty)
        end
        "#
        ));
    }

    #[test]
    fn test_4() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@class D13.Meta
            ---@field __defineGet fun(self: self, key: string, f: fun(self: self): any)

            ---@class D13.Impl: D13.Meta
            local impl = {}

            impl:__defineGet("value", function(self)
                return 1
            end)

            "#
        ));
    }

    #[test]
    fn test_issue_286() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                local a --- @type boolean
                local b --- @type integer?
                local c = a and b or nil
                -- type of c is (nil|true), should be (integer|nil)
                local d = a and b
                -- type of d is (boolean|nil), should be (false|integer|nil)

                ---@param p integer?
                local function f1(p)
                end
                f1(c)

                ---@param p false|integer|nil
                local function f2(p)
                end
                f2(d)
        "#
        ));
    }

    #[test]
    fn test_issue_287() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@param a table
                local function f(a)
                end

                ---@type table?
                local a
                a = a or {}

                f(a)
        "#
        ));
    }

    #[test]
    fn test_issue_336() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            --- @param b string
            --- @param c? boolean
            --- @param d? string
            --- @overload fun(b: string, d: string)
            function foo(b, c, d) end

            foo('number', true)
        "#
        ));
    }

    #[test]
    fn test_issue_348() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                local a --- @type type|'a'
                string.len(a)
        "#
        ));
    }

    #[test]
    fn test_super() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@class py.ETypeMeta

                ---@class py.Vector3: py.ETypeMeta
                ---@class py.FVector3: py.ETypeMeta

                ---@alias Point.HandleType py.FVector3

                ---@class py.Point: py.Vector3

                ---@param point py.Point
                local function test(point)
                end

                ---@type Point.HandleType
                local handle

                test(handle)
        "#
        ));
    }

    #[test]
    fn test_union_type() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@class py.Area
            ---@class py.RecArea: py.Area
            ---@class py.CirArea: py.Area

            ---@param a py.Area
            local function test(a)
            end

            ---@type py.RecArea | py.CirArea
            local a

            test(a)
        "#
        ));
    }

    #[test]
    fn test_super_1() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@class py.SlotType: integer

                ---@param a py.SlotType
                local function test(a)
                end

                ---@type 0|1
                local a

                test(a)
        "#
        ));
    }

    #[test]
    fn test_alias_union_enum() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@alias EventType
                ---| GlobalEventType
                ---| UIEventType

                ---@enum UIEventType
                local UIEventType = {
                    ['UI_CREATE'] = "ET_UI_PREFAB_CREATE_EVENT",
                    ['UI_DELETE'] = "ET_UI_PREFAB_DEL_EVENT",
                }

                ---@enum GlobalEventType
                local GlobalEventType = {
                    ['GAME_INIT'] = "ET_GAME_INIT",
                    ['GAME_PAUSE'] = "ET_GAME_PAUSE",
                }

                ---@param event_name string
                local function get_py_event_name(event_name)
                end

                ---@param a EventType
                local function test(a)
                    get_py_event_name(a)
                end

        "#
        ));
    }

    #[test]
    fn test_alias_union_enum_2() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@alias EventType
                ---| GlobalEventType
                ---| UIEventType

                ---@enum UIEventType
                local UIEventType = {
                    ['UI_CREATE'] = "ET_UI_PREFAB_CREATE_EVENT",
                    ['UI_DELETE'] = "ET_UI_PREFAB_DEL_EVENT",
                }

                ---@enum GlobalEventType
                local GlobalEventType = {
                    ['GAME_INIT'] = 1,
                    ['GAME_PAUSE'] = "ET_GAME_PAUSE",
                }

                ---@param event_name string
                local function get_py_event_name(event_name)
                end

                ---@param a EventType
                local function test(a)
                    get_py_event_name(a)
                end

        "#
        ));
    }

    #[test]
    fn test_empty_class() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@class D4.A: table<integer, string>

                ---@param lua_conf D4.A
                local function enable_global_lua_trigger(lua_conf) end

                ---@return { on_event: fun(trigger: table,  actor, data), [integer]: string }
                function new_global_trigger() end

                local a = new_global_trigger()

                enable_global_lua_trigger(a)
        "#
        ));
    }

    #[test]
    fn test_super_and_enum_1() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@enum AbilityType
                local AbilityType = {
                    HIDE   = 0,
                    NORMAL = 1,
                    COMMON = 2,
                    HERO   = 3,
                }
                ---@class py.AbilityType: integer

                ---@param ability_type py.AbilityType
                local function a(ability_type) end

                ---@param type AbilityType
                local function get(type)
                    local py_list = a(type)
                end
        "#
        ));
    }

    #[test]
    fn test_super_and_enum_2() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@enum AbilityType
                local AbilityType = {
                    HIDE   = "a",
                    NORMAL = 1,
                    COMMON = 2,
                    HERO   = 3,
                }
                ---@class py.AbilityType: integer

                ---@param ability_type py.AbilityType
                local function a(ability_type) end

                ---@param type AbilityType
                local function get(type)
                    local py_list = a(type)
                end
        "#
        ));
    }

    #[test]
    fn test_generic_array() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@class LocalTimer

            ---@generic V
            ---@param list V[]
            ---@return integer
            local function sort(list) end

            ---@type { need_sort: true?, [integer]: LocalTimer }
            local queue = {}
            sort(queue)
        "#
        ));
    }

    #[test]
    fn test_function_union() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@class (partial) D21.A
                local M

                ---@alias EventType
                ---| GlobalEventType
                ---| UIEventType

                ---@enum UIEventType
                local UIEventType = {
                    ['UI_CREATE'] = "ET_UI_PREFAB_CREATE_EVENT",
                }
                ---@enum GlobalEventType
                local GlobalEventType = {
                    ['GAME_INIT'] = "ET_GAME_INIT",
                }

                ---@param event_type EventType
                function M:event(event_type)
                end

                ---@class (partial) D21.A
                ---@field event fun(self: self, event: "游戏-初始化")
                ---@field event fun(self: self, event: "游戏-开始")

                ---@param p boolean
                local function test(p)
                    M:event(p)
                end
        "#
        ));
    }

    #[test]
    fn test_function_union_2() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@class (partial) D21.A
                local M

                ---@alias EventType
                ---| GlobalEventType
                ---| UIEventType

                ---@enum UIEventType
                local UIEventType = {
                    ['UI_CREATE'] = "ET_UI_PREFAB_CREATE_EVENT",
                }
                ---@enum GlobalEventType
                local GlobalEventType = {
                    ['GAME_INIT'] = "ET_GAME_INIT",
                }

                ---@param event_type EventType
                function M:event(event_type)
                end

                ---@class (partial) D21.A
                ---@field event fun(self: self, event: "游戏-初始化")

                ---@param p EventType
                local function test(p)
                    M:event(p)
                end
        "#
        ));
    }

    #[test]
    fn test_function_union_meta() {
        let mut ws = VirtualWorkspace::new();
        let mut emmyrc = ws.analysis.emmyrc.as_ref().clone();
        emmyrc.strict.meta_override_file_define = false;
        ws.analysis.update_config(Arc::new(emmyrc));

        ws.def(
            r#"
                ---@meta
                ---@class (partial) D21.A
                ---@field event fun(self: self, event: "游戏-初始化")
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@class (partial) D21.A
                local M

                ---@alias EventType
                ---| GlobalEventType
                ---| UIEventType

                ---@enum UIEventType
                local UIEventType = {
                    ['UI_CREATE'] = "ET_UI_PREFAB_CREATE_EVENT",
                }
                ---@enum GlobalEventType
                local GlobalEventType = {
                    ['GAME_INIT'] = "ET_GAME_INIT",
                }

                ---@param event_type EventType
                function M:event(event_type)
                end

                ---@param p EventType
                local function test(p)
                    M:event(p)
                end
        "#
        ));
    }

    #[test]
    fn test_function_self() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@class A

                ---@param init fun(self: self)
                local function test(init)
                end

                ---@param superInit fun(self: A)
                local function declare(superInit)
                    test(superInit)
                end
        "#
        ));
    }

    #[test]
    fn test_self_1() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "1.lua",
            r#"
                ---@class D31.A
                local A = {}

                ---@param ... any
                ---@return any, any, any, any
                function A:execute(...)
                end

                return A
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            local A = require("1")

            ---@class D31.B
            local B = {}

            function B:__init()
                self.originalExecute = A.execute
                A.execute = function(trg, ...)
                    self.originalExecute(trg, ...)
                end
            end
        "#
        ));
    }

    #[test]
    fn test_recursive_variadic_inference_preserves_other_diagnostics() {
        let source = r#"
            ---@param ... boolean
            local function recurse(...)
                if ... then return 1, 'done' end
                return 1, recurse(true)
            end
            local result = recurse(false)
            ---@type string
            local wrong = 42
            return result, wrong
        "#;
        let diagnostics = check_source(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert_eq!(diagnostics[0].code, DiagnosticCode::AssignTypeMismatch);
        assert_eq!(
            usize::from(diagnostics[0].range.start()),
            source.find("wrong = 42").unwrap() + "wrong = ".len()
        );
    }

    #[test]
    fn test_generic_callback_keeps_prefixed_variadic_results() {
        let source = r#"
            ---@generic First, Rest
            ---@param callback fun(): First, Rest...
            ---@return First, Rest...
            local function wrap(callback) return callback() end
            ---@param ... integer
            local function values(...) return true, ... end
            local packed = {wrap(function() return values(1, 2, 3) end)}
            ---@type boolean
            local first = packed[1]
            ---@type integer
            local valid = packed[3]
            ---@type string?
            local wrong = packed[3]
            return first, valid, wrong
        "#;
        let diagnostics = check_source(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert_eq!(diagnostics[0].code, DiagnosticCode::AssignTypeMismatch);
        assert_eq!(
            usize::from(diagnostics[0].range.start()),
            source.rfind("packed[3]").unwrap()
        );
    }

    #[test]
    fn test_long_argument_expression_keeps_type_checks() {
        let expression = vec!["1"; 1500].join(" + ");
        let wrong_number = format!("text_value({expression})");
        let wrong_string = "number_value(1 --[[@as string]])";
        let source = format!(
            r#"
            ---@param value number
            local function number_value(value) return value end
            ---@param value string
            local function text_value(value) return value end
            local unrelated = 1 --[[@as integer]]
            number_value(unrelated)
            number_value({expression})
            {wrong_number}
            text_value(1 --[[@as string]])
            {wrong_string}
        "#
        );
        let diagnostics = check_source(&source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:#?}");
        let actual: Vec<_> = diagnostics
            .iter()
            .map(|d| (d.code, usize::from(d.range.start())))
            .collect();
        assert_eq!(
            actual,
            vec![
                (
                    DiagnosticCode::ParamTypeMismatch,
                    source.find(&wrong_number).unwrap() + "text_value(".len(),
                ),
                (
                    DiagnosticCode::ParamTypeMismatch,
                    source.find(wrong_string).unwrap() + "number_value(".len(),
                ),
            ]
        );
    }

    #[test]
    fn test_guarded_member_tracks_each_access_and_owner_assignment() {
        let source = r#"
            ---@class A
            ---@field type 'point'
            ---@field handle string?
            ---@class B
            ---@field type 'unit'
            ---@field handle integer?
            ---@param text string
            local function consume(text) return text end
            ---@param target A | B
            ---@param other A
            local function check(target, other)
                if target.handle then
                    if target.type == 'point' then
                        consume(target.handle)
                        consume(other.handle)
                        target.handle = nil
                        consume(target.handle)
                    end
                    target = other
                    if target.type == 'point' then
                        consume(target.handle)
                    end
                end
            end
            return check
        "#;
        let diagnostics = check_source(source);
        let mut actual = diagnostics
            .iter()
            .filter(|d| d.code == DiagnosticCode::ParamTypeMismatch)
            .map(|d| usize::from(d.range.start()))
            .collect::<Vec<_>>();
        actual.sort_unstable();
        let expected = [
            "consume(other.handle)",
            "consume(target.handle)\n                    end",
            "consume(target.handle)\n                    end\n                end",
        ]
        .map(|text| source.find(text).unwrap() + "consume(".len());
        assert_eq!(actual, expected, "{diagnostics:#?}");
    }

    #[test]
    fn test_flow_alias() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let mut emmyrc = ws.analysis.get_emmyrc().deref().clone();
        emmyrc.strict.array_index = false;
        ws.analysis.update_config(Arc::new(emmyrc));

        // With no arguments, callback stays nil; event(callback) must warn.
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@class Trigger
                ---@alias Trigger.CallBack fun(trg: Trigger, ...): any, any, any, any

                ---@param callback Trigger.CallBack
                local function event(callback)
                end

                local function core_subscribe(...)
                    ---@type Trigger.CallBack
                    local callback
                    local nargs = select('#', ...)
                    if nargs == 1 then
                        callback = ...
                    elseif nargs > 1 then
                        extra_args = { ... }
                        callback = extra_args[nargs]
                    end

                    local b = event(callback)
                end
        "#
        ));
    }

    #[test]
    fn test_issue_487() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@param start string
            ---@return boolean
            function string:startswith(start)
                return self:sub(1, #start) == start
            end
        "#
        ));
    }

    #[test]
    fn test_inferred_integer_returns_keep_argument_and_return_checks() {
        for (body, target, accepted) in [
            ("return 1", "integer", true),
            ("local value = 1; return value", "integer", true),
            ("return 7 // 3", "integer", true),
            ("return 7 % 3", "integer", true),
            ("return 1 + 2", "integer", true),
            (
                "local value = 1; if _flag then return value end; return nil",
                "integer?",
                true,
            ),
            (
                "local value = 1; if _flag then return value end; return",
                "integer?",
                true,
            ),
            (
                "local value = 1; if _flag then return value end; return",
                "integer",
                false,
            ),
            (
                "local value = 1; if _flag then return value end; return 'text'",
                "integer | string",
                true,
            ),
            (
                "if _flag then return 1 / 2 end; return nil",
                "integer?",
                false,
            ),
            (
                "if _flag then return 1 / 2 end; return 'text'",
                "integer | string",
                false,
            ),
            (
                "---@return number\nlocal function number() return 1 end\nreturn number()",
                "integer",
                false,
            ),
            ("return 1 / 2", "integer", false),
            ("return 9223372036854775807 + (1 + 0)", "integer", false),
            ("return 9223372036854775807 + (-1)", "integer", true),
        ] {
            let source = format!(
                r#"
                ---@param _flag boolean
                local function inferred(_flag) {body} end
                ---@param value {target}
                local function consume(value) return value end
                consume(inferred(true))
                ---@return {target}
                local function forward() return inferred(true) end
                return forward
                "#
            );
            let diagnostics = check_source(&source);
            let mut actual = diagnostics
                .iter()
                .map(|d| (d.code, usize::from(d.range.start())))
                .collect::<Vec<_>>();
            actual.sort_by_key(|(_, offset)| *offset);
            let expected = if accepted {
                vec![]
            } else {
                vec![
                    (
                        DiagnosticCode::ParamTypeMismatch,
                        source.find("inferred(true)").unwrap(),
                    ),
                    (
                        DiagnosticCode::ReturnTypeMismatch,
                        source.rfind("inferred(true)").unwrap(),
                    ),
                ]
            };
            assert_eq!(actual, expected, "{source}\n{diagnostics:#?}");
        }
    }

    #[test]
    fn test_division_constants_use_floating_point_precision() {
        for (expression, expected, accepted) in [
            ("9007199254740993 / 1", "9007199254740992", true),
            ("9007199254740993 / 1", "9007199254740993", false),
            ("9007199254740993 / 3", "integer", false),
            ("9223372036854775807 / 1", "9223372036854775807", false),
            ("45 / 3", "15", true),
            ("-6 / 2", "-3", true),
        ] {
            let mut ws = VirtualWorkspace::new();
            let source = format!(
                r#"
                ---@param value {expected}
                local function consume(value) end
                consume({expression})
                "#
            );
            assert_eq!(
                ws.has_no_diagnostic(DiagnosticCode::ParamTypeMismatch, &source),
                accepted,
                "{source}"
            );
        }
    }

    #[test]
    fn test_int() {
        let source = r#"
            ---@param count integer
            local function loop_count(count) return count end
            loop_count(45 / 3)
            loop_count(45 / 2)
        "#;
        let diagnostics = check_source(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert_eq!(diagnostics[0].code, DiagnosticCode::ParamTypeMismatch);
        assert_eq!(
            usize::from(diagnostics[0].range.start()),
            source.find("45 / 2").unwrap()
        );
    }

    #[test]
    fn test_int_to_alias() {
        let mut ws = VirtualWorkspace::new();
        let mut emmyrc = ws.analysis.get_emmyrc().deref().clone();
        emmyrc.strict.doc_base_const_match_base_type = true;
        ws.analysis.update_config(Arc::new(emmyrc));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@alias IdAlias
                ---| 311000001
                ---| 311000002

                ---@param id IdAlias
                local function f(id)
                end

                ---@type integer
                local a
                f(a)
        "#
        ));
    }

    #[test]
    fn test_enum_value_matching() {
        let mut ws = VirtualWorkspace::new();
        let mut emmyrc = ws.analysis.get_emmyrc().deref().clone();
        emmyrc.strict.doc_base_const_match_base_type = true;
        ws.analysis.update_config(Arc::new(emmyrc));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@enum SlotType
                local SlotType = {
                    bag = 0,
                    item = 1,
                }

                ---@enum ConstSlotType
                local ConstSlotType = {
                    ['NOT_IN_BAG'] = -1,
                    ['PKG'] = 0,
                    ['BAR'] = 1,
                }

                ---@param type ConstSlotType
                local function get_item_by_slot(type)
                end

                ---@param field SlotType
                local function bind_unit_slot(field)
                    get_item_by_slot(field)
                end"#
        ));
    }

    #[test]
    fn test_enum_value_matching_2() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@enum DamageType
                local DamageType = {
                    ['物理'] = "物理",
                }
                for _, damageType in pairs(DamageType) do end
            "#
        ));
    }

    #[test]
    fn test_super_type_match() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@class UnitKey: integer

                ---@alias IdAlias
                ---| 10101

                ---@param key IdAlias
                local function get(key)
                end

                ---@type UnitKey
                local key

                get(key)
            "#
        ));
    }

    #[test]
    fn test_self() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@class test
                local A

                function A:stop()
                end

                local stop = A.stop
                stop(A)
            "#
        ));
    }

    #[test]
    fn test_generic_type() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@class ObserverParams<T>
                ---@field next fun(value: T)
                ---@field errorResume? fun(error: any)


                ---@class Observer<T>
                local Observer = {}

                ---@param observer ObserverParams<T>
                function Observer:subscribe(observer)
                end
        "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type Observer<number>
            local observer

            observer:subscribe({
                next = function(value)
                    print(value)
                end
            })
            "#
        ));
    }

    #[test]
    fn test_issue_573() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                --- @class A
                --- @field [integer] string
                --- @field data any

                --- @param a string[]
                function takesArray(a) end
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                takesArray({} --[[@as A]])
            "#
        ));
    }

    #[test]
    fn test_issue_574() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                --- @param x { y: integer } & { z: string }
                function foo(x) end
            "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
               foo({y = "", z = ""})
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
               foo({y = 1, z = ""})
            "#
        ));
    }

    #[test]
    fn test_meta_pairs() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
                ---@class RingBufferSpan<T>
                local RingBufferSpan

                ---@return fun(): integer, T
                function RingBufferSpan:__pairs()
                end
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            local pairs = pairs

            ---@type RingBufferSpan
            local span

            for k, v in pairs(span) do
            end
            "#
        ));

        // Test generics.
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type RingBufferSpan<number>
            local span

            for k, v in pairs(span) do
                A = v
            end
            "#
        ));
        let a_ty = ws.expr_ty("A");
        let expected = ws.ty("number");
        assert_eq!(a_ty, expected);
    }

    #[test]
    fn test_generic_union_type() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@class Params<T>
                ---@field next fun(value: T)
                ---@field error? fun(error: any)

                ---@generic T
                ---@param params fun(value: T) | Params<T>
                function test(params)
                end

            "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                test({})
            "#
        ));
    }

    #[test]
    fn test_alias_branch_label_flow() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "test.lua",
            r#"
            ---@alias EditorAttrTypeAlias
            ---| 'ATTR_BASE'
            ---| 'ATTR_BASE_RATIO'
            ---| 'ATTR_ALL_RATIO'

            ---@param attr_element string
            function test(attr_element) end
        "#,
        );

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@param attr_type EditorAttrTypeAlias
            function add_attr(attr_type)
                if attr_type ~= 'ATTR_BASE' then
                end
                test(attr_type)
            end
        "#
        ));
    }

    #[test]
    fn test_self_contain_tpl() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "test.lua",
            r#"
                ---@class Observable<T>
                Observable = {}

                ---@param ... Observable<any>
                function zip(...)
                end

        "#,
        );

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            function Observable:test()
                zip(self)
            end
        "#
        ));
    }

    #[test]
    fn test_issue_841() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
        --- @class B
        --- @field cmd string

        --- @class A: B
        --- @field cmd? string

        --- @param x A
        local function foo(x)
        end

        foo({})
        "#,
        ));
    }

    #[test]
    fn test_fix_issue_844() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@alias Tester fun(customTesters: Tester[]): boolean?

        ---@generic V
        ---@param t V[]
        ---@return fun(tbl: any):int, V
        function ipairs(t) end
        "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@param newTesters Tester[]
            local function addMatchers(newTesters)
                for _, tester in ipairs(newTesters) do
                end
            end
        "#
        ));
    }

    #[test]
    fn test_pairs_1() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@param value string
        function aaaa(value)
        end

        ---@generic K, V
        ---@param t {[K]: V} | V[]
        ---@return fun(tbl: any):K, V
        function pairs(t) end
        "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type {[string]: number}
            local matchers = {}
            for _, matcher in pairs(matchers) do
                aaaa(matcher)
            end
        "#
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@alias MatchersObject {[string]: number}
            ---@type MatchersObject
            local matchers = {}
            for _, matcher in pairs(matchers) do
                aaaa(matcher)
            end
        "#
        ));
    }

    #[test]
    fn test_keyof() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class SuiteHooks
            ---@field beforeAll string
            ---@field afterAll string

            ---@param name keyof SuiteHooks
            function test(name)
            end
        "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            test("a")
        "#,
        ));
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            test("beforeAll")
        "#,
        ));
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type keyof SuiteHooks
            local name
            test(name)
        "#,
        ));
    }

    #[test]
    fn test_index_access_with_keyof_alias() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class A
            ---@field one 1
            ---@class B
            ---@field one string
            ---@alias Key 'one'
            ---@alias KeyofA keyof A
            ---@alias KeyofB keyof B
            "#,
        );
        for indexed in [
            "A[KeyofA]",
            "A[Key]",
            "A[KeyofB]",
            "RawGet<A, Key>",
            "RawGet<A, keyof B>",
            "[1, string][0]",
            "RawGet<[1, string], 0>",
        ] {
            for (parameter, accepted) in [("1", true), ("string", false)] {
                assert_eq!(
                    ws.has_no_diagnostic(
                        DiagnosticCode::ParamTypeMismatch,
                        &format!(
                            "---@type {indexed}\nlocal value\n---@param v {parameter}\nlocal function accept(v) end\naccept(value)"
                        ),
                    ),
                    accepted,
                    "{indexed} -> {parameter}"
                );
            }
        }
    }

    #[test]
    fn test_origin_self() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Runner
            ---@field onCollectStart? fun(self:self)

            ---@type Runner
            runner = {}

        "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            local originOnCollectStart = runner.onCollectStart
            runner.onCollectStart = function(self)
                originOnCollectStart(self)
            end
        "#,
        ));
    }

    #[test]
    fn test_issue_896() {
        let mut ws = VirtualWorkspace::new();
        let mut emmyrc = ws.get_emmyrc();
        emmyrc.strict.array_index = false;
        ws.update_emmyrc(emmyrc);

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@alias MyFnA fun(): number
            ---@alias MyFnB<T> fun(): T
            ---@alias MyFnC<T> fun(): number

            local aaa ---@type MyFnA[]
            ---@param aaa MyFnA[]
            local function AAA(aaa) return aaa end
            local _ = AAA(aaa)

            local bbb1 ---@type (MyFnB<number>)[]
            local bbb2 = { function() return 1 end } ---@type (MyFnB<number>)[]
            ---@param bbb (MyFnB<number>)[]
            local function BBB(bbb) return bbb end
            local _ = BBB(bbb1)
            local _ = BBB(bbb2)

            local ccc1 ---@type (MyFnC<number>)[]
            local ccc2 = { function() return 1 end } ---@type (MyFnC<number>)[]
            ---@param ccc (MyFnC<number>)[]
            local function CCC(ccc) return ccc end
            local _ = CCC(ccc1)
            local _ = CCC(ccc2)
        "#,
        ));
    }

    #[test]
    fn test_call_operator_implicit_self_param_type() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@class Iter

                ---@class IterMod
                ---@operator call: Iter

                ---@type string[]
                local paths = { "a" }

                ---@type IterMod
                local iter

                local value = iter(paths)
        "#,
        ));
    }

    #[test]
    fn test_colon_call_param_check_uses_receiver_when_member_is_callable() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@class Owner
                ---@field run CallableMethod

                ---@class CallableMethod
                ---@overload fun(owner: Owner)

                ---@type Owner
                local owner

                owner:run()
        "#,
        ));
    }

    #[test]
    fn test_generic_infer_function_2() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T: table
            ---@param table T
            ---@param metatable table|nil
            ---@return T
            function setmetatable(table, metatable) end

            ---@alias PartialFunction<T> { [P in keyof T]: T[P] extends function and T[P]? or T[P]; }

            ---@class MockContext<T>

            ---@class Mock<T>
            ---@field mock MockContext<T>
            Mock = {}
            "#,
        );

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type PartialFunction<Mock>
            local mock

            setmetatable(mock, Mock)
        "#,
        ));
    }

    #[test]
    fn test_generic_variability() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
                ---@generic T
                ---@param ... T...
                function test(...) end
            "#,
        );

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@type string?
                local message
                test(message)
                test(message, message)
        "#,
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            local filename = 'flag.text'
            local d = io.open(filename, 'r')
            assert(d)
        "#,
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            local filename = 'flag.text'
            assert(io.open(filename, 'r'))
            "#,
        ));
    }

    #[test]
    fn test_generic_constraint_arg_to_incompatible_param() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@class Animal
            ---@field name string

            ---@param value string
            local function takeString(value)
            end

            ---@generic T: Animal
            ---@param animal T
            local function checkAnimal(animal)
                takeString(animal)
            end
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@class Animal
            ---@field name string

            ---@param value Animal
            local function takeAnimal(value)
            end

            ---@generic T: Animal
            ---@param animal T
            local function checkAnimal(animal)
                takeAnimal(animal)
            end
        "#
        ));
    }

    #[test]
    fn test_generic_callback_parameter_contravariance() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class A
            ---@class B
            ---@class C
            ---@class D

            ---@param a A | B | C
            ---@return boolean
            function condition(a)
                return true
            end

            ---@generic T
            ---@param a T[]
            ---@param b fun(a: T): boolean
            function test(a, b) end
            "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type (A | B | C | D)[]
            local ABCD
            test(ABCD, condition)
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type (A | B)[]
            local AB
            test(AB, condition)
            "#
        ));
    }

    #[test]
    fn test_unknown_alias_param_accepts() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@alias Anything unknown
                ---@param s string
                local function f(s) end

                ---@type Anything
                local a
                f(a)
            "#
        ));
    }

    #[test]
    fn test_dot_call_colon_defined_method_uses_correct_arg_type() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@class Foo
                ---@field value number
                local Foo = {}

                ---@param x number
                function Foo:method(x) end

                ---@type Foo
                local foo
                Foo.method(foo, 1)
            "#
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@class Foo
                ---@field value number
                local Foo = {}

                ---@param x Foo
                function Foo:method(x) end

                ---@type Foo
                local foo
                Foo.method(foo, 1)
            "#
        ));
    }

    /// Fewer arguments than the variadic slot position: the variadic parameter
    /// consumes an empty slice instead of panicking on `args[index..]`.
    #[test]
    fn test_variadic_slot_beyond_args_no_panic() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@param a string
                ---@param b string
                ---@param ... number
                local function f(a, b, ...) end
                f("x")
            "#
        ));
    }

    /// Same for a generic variadic slot (`...: T...`), which has its own early path.
    #[test]
    fn test_generic_variadic_slot_beyond_args_no_panic() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@generic T
                ---@param a string
                ---@param ... T...
                local function f(a, ...) end
                f("x")
            "#
        ));
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@generic T
                ---@param a string
                ---@param ... T...
                local function f(a, ...) end
                f()
            "#
        ));
    }

    /// No arguments at all, so the variadic slot index is past the end of `args`.
    #[test]
    fn test_variadic_slot_no_args_no_panic() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@param a string
                ---@param ... number
                local function f(a, ...) end
                f()
            "#
        ));
    }

    /// The variadic slot still reports mismatches for the arguments it does receive.
    #[test]
    fn test_variadic_slot_still_checks_provided_args() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@param a string
                ---@param b string
                ---@param ... number
                local function f(a, b, ...) end
                f("x", "y", "z")
            "#
        ));
    }

    /// Full default checker pipeline (the path behind `textDocument/diagnostic`) on the
    /// crashing input: no panic, and the missing named argument is still reported.
    #[test]
    fn test_variadic_slot_beyond_args_full_check_pipeline() {
        let diagnostics = check_source(
            r#"
                ---@param a string
                ---@param b string
                ---@param ... number
                local function f(a, b, ...) end
                f("x")
            "#,
        );
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == DiagnosticCode::MissingParameter)
                .count(),
            1,
            "diagnostics: {:?}",
            diagnostics
        );
    }
}
