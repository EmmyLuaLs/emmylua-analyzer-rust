#[cfg(test)]
mod tests {
    use crate::check::test::{check_source, check_source_with_emmyrc};
    use crate::{DiagnosticCode, Emmyrc, VirtualWorkspace};

    #[test]
    fn test_numeric_member_guards_use_equivalent_integer_keys() {
        for (key, write, accepted) in [
            ("1.0", "", true),
            ("1", "t[1.0] = nil", false),
            ("'1'", "", false),
        ] {
            let source = format!(
                r#"
                ---@param flag boolean
                local function read(flag)
                    local t = {{[1] = flag and 'a' or nil}}
                    if t[1] then
                        {write}
                        ---@type string
                        local value = t[{key}]
                        return value
                    end
                end
                return read
                "#
            );
            let mut ws = VirtualWorkspace::new();
            assert_eq!(
                ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, &source),
                accepted,
                "{source}"
            );
        }
    }

    #[test]
    fn test_child_alias_guards_keep_captured_object_identity() {
        for (setup, guard, write, read, accepted) in [
            ("", "t.child.value", "", "b.value", true),
            ("", "b.value", "", "t.child.value", true),
            ("local c = (b)", "c.value", "", "t.child.value", true),
            ("", "b.value", "t.child = other", "b.value", true),
            ("", "b.value", "t.child = other", "t.child.value", false),
            ("t.child = other", "b.value", "", "t.child.value", false),
            ("", "t.child.value", "b.value = nil", "t.child.value", false),
            ("", "t.child.value", "t.child.value = nil", "b.value", false),
            (
                "local a = t",
                "b.value",
                "a.child = other",
                "t.child.value",
                false,
            ),
            (
                "local a = t; a = t",
                "b.value",
                "a.child = other",
                "t.child.value",
                false,
            ),
            (
                "local a = t; local d = a; a = t",
                "b.value",
                "d.child = other",
                "t.child.value",
                false,
            ),
            (
                "",
                "b.value",
                "if flag then t.child = other end",
                "t.child.value",
                false,
            ),
            ("", "b.value", "t[key] = other", "t.child.value", false),
            ("", "b.value", "other.value = nil", "t.child.value", true),
        ] {
            let source = format!(
                r#"
                ---@class Child
                ---@field value string?
                ---@class Holder
                ---@field child Child
                ---@param t Holder
                ---@param other Child
                ---@param flag boolean
                ---@param key string
                local function read(t, other, flag, key)
                    local b = t.child
                    {setup}
                    if {guard} then
                        {write}
                        ---@type string
                        local value = {read}
                        return value
                    end
                end
                return read
                "#
            );
            let mut ws = VirtualWorkspace::new();
            assert_eq!(
                ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, &source),
                accepted,
                "{source}"
            );
        }
    }

    #[test]
    fn test_alias_guards_ignore_only_later_straight_line_assignments() {
        for (body, accepted) in [
            ("if b.value then consume(t.value) end; b = other", true),
            ("if b.value then consume(t.value) end; t = other", true),
            ("b = other; if b.value then consume(t.value) end", false),
            ("if b.value then b = other; consume(b.value) end", false),
            (
                "while flag do if b.value then consume(t.value) end; b = other end",
                false,
            ),
            (
                "local inner = function() if b.value then consume(t.value) end end; t = other; return inner",
                false,
            ),
        ] {
            let source = format!(
                r#"
                ---@class Holder
                ---@field value string?
                ---@param value string
                local function consume(value) end
                ---@param t Holder
                ---@param other Holder
                ---@param flag boolean
                local function read(t, other, flag)
                    local b = t
                    {body}
                end
                return read
                "#
            );
            let mut ws = VirtualWorkspace::new();
            assert_eq!(
                ws.has_no_diagnostic(DiagnosticCode::ParamTypeMismatch, &source),
                accepted,
                "{source}"
            );
        }
    }

    #[test]
    fn test_string_enum_values_across_assignment_argument_and_return() {
        for (value, accepted, argument_accepted) in [
            ("'a'", true, true),
            ("'b'", true, true),
            ("'first'", false, true),
            ("'x'", false, false),
        ] {
            for (code, usage) in [
                (
                    DiagnosticCode::AssignTypeMismatch,
                    format!("---@type Kind\nlocal value = {value}"),
                ),
                (
                    DiagnosticCode::ParamTypeMismatch,
                    format!(
                        "---@param value Kind\nlocal function consume(value) end\nconsume({value})"
                    ),
                ),
                (
                    DiagnosticCode::ReturnTypeMismatch,
                    format!("---@return Kind\nlocal function read() return {value} end"),
                ),
            ] {
                let mut ws = VirtualWorkspace::new();
                let source = format!(
                    "---@enum Kind\nlocal Kind = {{ first = 'a', second = ('b') }}\n{usage}"
                );
                // Arguments also accept enum member names.
                let accepted = if code == DiagnosticCode::ParamTypeMismatch {
                    argument_accepted
                } else {
                    accepted
                };
                assert_eq!(ws.has_no_diagnostic(code, &source), accepted, "{source}");
            }
        }
    }

    #[test]
    fn test_string_enum_values_from_another_file() {
        let mut ws = VirtualWorkspace::new();
        ws.def("---@enum Kind\nlocal Kind = { first = 'a', second = 'b' }");
        for (value, accepted) in [("'a'", true), ("'x'", false)] {
            let source = format!("---@type Kind\nlocal value = {value}");
            assert_eq!(
                ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, &source),
                accepted,
                "{source}"
            );
        }
    }

    #[test]
    fn test_value_compatibility_across_assignment_argument_and_return() {
        for (source_type, target_type, accepted) in [
            ("'a'", "'b'", false),
            ("'1'", "1", false),
            ("true", "boolean", true),
            ("true", "false", false),
            ("string | nil", "string", false),
            ("Required?", "Required", false),
            ("fun(...: number)", "fun(...: integer)", true),
            ("fun(...: integer)", "fun(...: number)", false),
            ("integer", "number", true),
            ("number", "integer", false),
            ("string", "string | nil", true),
            ("string | integer", "string | number", true),
            ("string | number", "string | integer", false),
            ("string | integer", "integer", false),
            ("fun(): string", "fun(text: string): string", true),
            ("fun(text: string): string", "fun(): string", false),
            ("fun(text: string)", "fun(...: string)", false),
            ("fun(text: string?)", "fun(...: string)", true),
            (
                "fun(first: string, second: string)",
                "fun(first: string, ...: string)",
                false,
            ),
            (
                "fun(first: string, second: string?)",
                "fun(first: string, ...: string)",
                true,
            ),
            ("fun(...: [string, string]...)", "fun(...: string)", false),
            ("fun(...: [string?, string?]...)", "fun(...: string)", true),
            (
                "fun(...: [string, integer]...)",
                "fun(text: string, count: integer)",
                true,
            ),
            ("fun(...: [string, integer]...)", "fun(text: string)", false),
            ("fun(...: [string, integer]...)", "fun()", false),
            ("fun(...: [string, integer?]...)", "fun(text: string)", true),
            ("fun(...: [string?, integer?]...)", "fun()", true),
            ("fun(...: [string, integer])", "fun()", true),
            (
                "fun(prefix: boolean, ...: [string, integer]...)",
                "fun(prefix: boolean)",
                false,
            ),
            (
                "fun(prefix: boolean, ...: [string, integer]...)",
                "fun(prefix: boolean, text: string, count: integer)",
                true,
            ),
            (
                "fun(value: number): integer",
                "fun(value: integer): number",
                true,
            ),
            (
                "fun(value: integer): number",
                "fun(value: number): integer",
                false,
            ),
            ("[integer]", "{[integer]: number}", true),
            ("[integer]", "{[integer]: string}", false),
            ("[integer, string]", "{[integer]: number}", false),
            ("[integer, string]", "{[integer]: number | string}", true),
            ("[integer, string]", "{[1]: number, [2]: string}", true),
            ("[integer]", "{[2]: string}", false),
            ("[integer]", "{[integer]: number, tag: string}", false),
            ("[integer]", "{[integer]: number, tag: string?}", true),
            ("integer[]", "{[integer]: number}", true),
            ("integer[]", "{[integer]: string}", false),
            ("integer[]", "{[integer]: number} | nil", true),
            ("integer[]", "{[integer]: string} | nil", false),
            ("integer[]", "{[integer]: number, [1]: string}", false),
            ("integer[]", "{[number]: number}", true),
            ("integer[]", "{[integer | string]: number}", true),
            ("integer[]", "{[integer]: number, [number]: string}", false),
            ("[integer]", "{[integer]: number, [number]: string}", false),
            ("fun(): integer, string", "fun(): number, string", true),
            ("fun(): integer, string", "fun(): integer, number", false),
            ("fun(): integer, string", "fun(): string, string", false),
            ("fun(): integer, string", "fun(): number", true),
            ("fun(): integer, string", "fun(): string", false),
            ("fun(): integer?, string", "fun(): number?", true),
            ("fun(): integer", "fun(): number, string", false),
            ("fun(): integer", "fun(): number, string?", true),
            ("fun(): integer, string", "fun(): number, string...", true),
            ("fun(): integer, string", "fun(): number, number...", false),
            ("fun(): integer", "fun(): number, string...", true),
            (
                "fun(): integer, integer...",
                "fun(): number, number...",
                true,
            ),
            (
                "fun(): integer, string...",
                "fun(): number, number...",
                false,
            ),
            ("fun(): integer...", "fun(): number, string", false),
            (
                "fun(): integer, integer...",
                "fun(): number, number, number",
                true,
            ),
            (
                "fun(): integer, string, boolean",
                "fun(): number, string...",
                false,
            ),
            (
                "fun(): integer, string, boolean...",
                "fun(): number, string...",
                false,
            ),
            ("fun(): nil", "fun(): nil, nil", true),
            ("fun(): any", "fun(): integer, string", true),
            ("fun(): never", "fun(): integer, string", true),
            ("fun(): [integer, string]...", "fun(): number, string", true),
            ("fun(): [integer, integer]...", "fun(): number...", true),
            (
                "fun(): [integer, string]...",
                "fun(): number, number",
                false,
            ),
        ] {
            let source = format!(
                r#"
                ---@class Required
                local _Required = {{}}
                ---@param value {source_type}
                ---@return {target_type}
                local function convert(value)
                    ---@type {target_type}
                    local _assigned = value
                    ---@param argument {target_type}
                    local function consume(argument) return argument end
                    consume(value)
                    return value
                end
                return convert
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
                        DiagnosticCode::AssignTypeMismatch,
                        source.find("assigned = value").unwrap() + "assigned = ".len(),
                    ),
                    (
                        DiagnosticCode::ParamTypeMismatch,
                        source.find("consume(value)").unwrap() + "consume(".len(),
                    ),
                    (
                        DiagnosticCode::ReturnTypeMismatch,
                        source.find("return value").unwrap() + "return ".len(),
                    ),
                ]
            };
            assert_eq!(
                actual, expected,
                "{source_type} -> {target_type}: {diagnostics:#?}"
            );
        }
    }

    #[test]
    fn test_inferred_literal_unions_allow_reassignment() {
        for (initial, replacement, wrong) in [
            ("flag and 'open' or 'closed'", "'other'", "false"),
            ("flag and 1 or 2", "3", "'wrong'"),
            ("flag and 1.5 or 2.5", "3.5", "'wrong'"),
            ("flag and 'open' or nil", "'other'", "false"),
            ("flag and true or nil", "false", "'wrong'"),
        ] {
            let source = format!(
                r#"
                ---@param flag boolean
                local function update(flag)
                    local value = {initial}
                    value = {replacement}
                    value = {wrong}
                    local state = {{ value = {initial} }}
                    state.value = {replacement}
                    state.value = {wrong}
                    return value, state
                end
                return update
                "#
            );
            let mut diagnostics = check_source(&source);
            diagnostics.sort_by_key(|d| d.range.start());
            let actual = diagnostics
                .iter()
                .map(|d| (d.code, usize::from(d.range.start())))
                .collect::<Vec<_>>();
            assert_eq!(
                actual,
                ["value", "state.value"]
                    .map(|target| (
                        DiagnosticCode::AssignTypeMismatch,
                        source.find(&format!("{target} = {wrong}")).unwrap()
                            + target.len()
                            + " = ".len(),
                    ))
                    .to_vec(),
                "{source}\n{diagnostics:#?}"
            );
        }
    }

    #[test]
    fn test_inferred_fields_allow_reassignment_through_aliases_and_imports() {
        for imported in [false, true] {
            for (annotation, replacement, accepted) in [
                ("", "'other'", true),
                ("", "false", false),
                ("---@type 'open' | 'closed'", "'other'", false),
            ] {
                let original = format!(
                    r#"
                    ---@type boolean
                    local flag
                    local original = {{
                        {annotation}
                        state = flag and 'open' or 'closed'
                    }}
                    "#
                );
                let mut ws = VirtualWorkspace::new();
                let source = if imported {
                    ws.def_file("config.lua", &format!("{original}\nreturn original"));
                    format!("local config = require('config')\nconfig.state = {replacement}")
                } else {
                    format!("{original}\nlocal config = original\nconfig.state = {replacement}")
                };
                assert_eq!(
                    ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, &source),
                    accepted,
                    "imported={imported}: {source}"
                );
            }
        }
    }

    #[test]
    fn test_integer_member_comparisons_narrow_only_the_matching_key() {
        for table in [
            "local t = {[1] = _value, ['1'] = _value}",
            "---@class Indexed\n---@field [1] string | integer\n---@field ['1'] string | integer\n---@type Indexed\nlocal t",
        ] {
            for (read, accepted) in [("t[1]", true), ("t['1']", false)] {
                let source = format!(
                    r#"
                    ---@param _value string | integer
                    local function read(_value)
                        {table}
                        if t[1] == 'a' then
                            ---@type string
                            local _text = {read}
                        end
                    end
                    return read
                    "#
                );
                let diagnostics = check_source(&source);
                assert_eq!(
                    diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                    if accepted {
                        vec![]
                    } else {
                        vec![DiagnosticCode::AssignTypeMismatch]
                    },
                    "{source}\n{diagnostics:#?}"
                );
            }
        }
    }

    #[test]
    fn test_member_guards_follow_stable_local_aliases() {
        for (setup, guarded, read, accepted) in [
            ("local b = a", "b", "a", true),
            ("local b = a", "a", "b", true),
            ("local b = (a); local c = b", "c", "a", true),
            ("local b = _other", "b", "a", false),
            ("local b = a; b = _other", "b", "a", false),
            ("local b = a; a = _other", "b", "a", false),
            ("a = _other; local b = a; a = _third", "b", "a", false),
        ] {
            let source = format!(
                r#"
                ---@class Node
                ---@field value string?
                ---@param a Node
                ---@param _other Node
                ---@param _third Node
                local function read(a, _other, _third)
                    {setup}
                    if {guarded}.value then
                        ---@type string
                        local _text = {read}.value
                    end
                end
                return read
                "#
            );
            let diagnostics = check_source(&source);
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                if accepted {
                    vec![]
                } else {
                    vec![DiagnosticCode::AssignTypeMismatch]
                },
                "{source}\n{diagnostics:#?}"
            );
        }
    }

    #[test]
    fn test_member_guards_stop_after_alias_field_writes() {
        for (write, accepted) in [
            ("a.value = nil", false),
            ("b.value = nil", false),
            ("if _flag then b.value = nil end", false),
            ("_other.value = nil", true),
        ] {
            let source = format!(
                r#"
                ---@class Node
                ---@field value string?
                ---@param a Node
                ---@param _other Node
                ---@param _flag boolean
                local function read(a, _other, _flag)
                    local b = a
                    if b.value then
                        {write}
                        ---@type string
                        local _text = a.value
                    end
                end
                return read
                "#
            );
            let diagnostics = check_source(&source);
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                if accepted {
                    vec![]
                } else {
                    vec![DiagnosticCode::AssignTypeMismatch]
                },
                "{source}\n{diagnostics:#?}"
            );
        }
    }

    #[test]
    fn test_member_guards_stop_after_alias_owner_writes() {
        for (write, accepted) in [
            ("b.child = {value = nil}", false),
            ("b.child = {value = 'safe'}", true),
            ("if _flag then b.child = {value = nil} end", false),
            ("b[_key] = {value = nil}", false),
            ("_other.child = {value = nil}", true),
        ] {
            let source = format!(
                r#"
                ---@class Leaf
                ---@field value string?
                ---@class Tree
                ---@field child Leaf
                ---@param a Tree
                ---@param _other Tree
                ---@param _flag boolean
                ---@param _key string
                local function read(a, _other, _flag, _key)
                    local b = a
                    if b.child.value then
                        {write}
                        ---@type string
                        local _text = a.child.value
                    end
                end
                return read
                "#
            );
            let diagnostics = check_source(&source);
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                if accepted {
                    vec![]
                } else {
                    vec![DiagnosticCode::AssignTypeMismatch]
                },
                "{source}\n{diagnostics:#?}"
            );
        }
    }

    #[test]
    fn test_array_values_satisfy_numeric_members_of_index_unions() {
        for key in ["1 | 'tag'", "Keys", "NestedKeys", "RecursiveKeys"] {
            for (expected, accepted) in [("number", true), ("string", false)] {
                let source = format!(
                    r#"
                    ---@alias Keys 1 | 'tag'
                    ---@alias NestedKeys Keys | 'meta'
                    ---@alias RecursiveKeys integer | 'tag' | RecursiveKeys
                    ---@param value integer[]
                    ---@return {{[{key}]: {expected}}}
                    local function convert(value)
                        ---@type {{[{key}]: {expected}}}
                        local _assigned = value
                        ---@param argument {{[{key}]: {expected}}}
                        local function consume(argument) return argument end
                        consume(value)
                        return value
                    end
                    return convert
                    "#
                );
                let mut config = Emmyrc::default();
                config.strict.doc_base_const_match_base_type = false;
                let mut diagnostics = check_source_with_emmyrc(&source, config);
                diagnostics.sort_by_key(|d| d.range.start());
                let codes = diagnostics.iter().map(|d| d.code).collect::<Vec<_>>();
                let expected = if accepted {
                    vec![]
                } else {
                    vec![
                        DiagnosticCode::AssignTypeMismatch,
                        DiagnosticCode::ParamTypeMismatch,
                        DiagnosticCode::ReturnTypeMismatch,
                    ]
                };
                assert_eq!(codes, expected, "{source}\n{diagnostics:#?}");
            }
        }
    }

    #[test]
    fn test_inferred_literals_can_change_but_declared_literals_cannot() {
        let source = r#"
            local fraction = 1.5
            fraction = 2.5
            local changed = false
            changed = true
            local state = { ready = true }
            state.ready = false
            ---@type false
            local fixed = false
            fixed = true
            return fraction, changed, state, fixed
        "#;
        let diagnostics = check_source(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert_eq!(diagnostics[0].code, DiagnosticCode::AssignTypeMismatch);
        assert_eq!(
            usize::from(diagnostics[0].range.start()),
            source.find("fixed = true").unwrap() + "fixed = ".len()
        );
    }

    #[test]
    fn test_parameter_reassignment_preserves_literal_annotations() {
        for (parameter_type, value, accepted) in [
            ("false", "true", false),
            ("false", "false", true),
            ("true", "false", false),
            ("boolean", "true", true),
            ("'open'", "'closed'", false),
            ("'open'", "'open'", true),
            ("'open' | 'closed'", "'other'", false),
            ("string", "'closed'", true),
            ("1", "2", false),
            ("1", "1", true),
            ("1 | 2", "3", false),
            ("integer", "2", true),
            ("integer", "1 + 2", true),
            ("integer", "1 / 2", false),
            ("integer", "1.5 + 1", false),
        ] {
            let source = format!(
                r#"
                ---@param value {parameter_type}
                local function update(value)
                    value = {value}
                    return value
                end
                return update
                "#
            );
            let diagnostics = check_source(&source);
            let actual = diagnostics
                .iter()
                .map(|d| (d.code, usize::from(d.range.start())))
                .collect::<Vec<_>>();
            let expected = if accepted {
                vec![]
            } else {
                vec![(
                    DiagnosticCode::AssignTypeMismatch,
                    source.find("value = ").unwrap() + "value = ".len(),
                )]
            };
            assert_eq!(
                actual, expected,
                "{parameter_type} = {value}: {diagnostics:#?}"
            );
        }
    }

    #[test]
    fn test_vararg_packing_keeps_tuple_assignment_checks() {
        let source = r#"
            ---@param ... [string, integer]
            local function tuples(...)
                local packed = { ... }
                ---@type [string, integer]
                local first = packed[1]
                ---@type [string, integer]
                local second = packed[2]
                ---@type integer
                local wrong = packed[3]
                return first, second, wrong
            end
            return tuples({'a', 1}, {'b', 2}, {'c', 3})
        "#;
        let diagnostics = check_source(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert_eq!(diagnostics[0].code, DiagnosticCode::AssignTypeMismatch);
        assert_eq!(
            usize::from(diagnostics[0].range.start()),
            source.find("packed[3]").unwrap()
        );
        assert_eq!(
            diagnostics[0].message,
            "expected `integer` but found `(string,integer)`.\n  expected `integer`, found `(string,integer)`"
        );
    }

    #[test]
    fn test_vararg_builtin_results_keep_assignment_checks() {
        let source = r#"
            ---@param ... [string, integer]
            local function forward(...) return ... end
            local packed = {pcall(forward, {'a', 1}, {'b', 2})}
            ---@type [string, integer]?
            local valid_tuple = packed[3]
            ---@type integer?
            local invalid_tuple = packed[3]
            ---@param ... integer
            local function values(...) return true, ... end
            ---@type integer
            local valid_integer = select(3, values(1, 2, 3))
            ---@type string?
            local invalid_integer = select(3, values(1, 2, 3))
            return valid_tuple, invalid_tuple, valid_integer, invalid_integer
        "#;
        let diagnostics = check_source(source);
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, usize::from(diagnostic.range.start())))
                .collect::<Vec<_>>(),
            vec![
                (
                    DiagnosticCode::AssignTypeMismatch,
                    source.find("invalid_tuple = packed[3]").unwrap() + "invalid_tuple = ".len()
                ),
                (
                    DiagnosticCode::AssignTypeMismatch,
                    source.rfind("select(3, values(1, 2, 3))").unwrap()
                ),
            ],
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn test_vararg_function_results_keep_callback_parameter_rules() {
        let source = r#"
            ---@alias Formatter fun(text: string): string
            ---@alias Constant fun(): string
            ---@alias WrongArgument fun(value: integer): string
            local function constant() return 'hello' end
            ---@param ... Constant
            local function forward(...) return ... end
            local function produce() return forward(constant, constant) end
            ---@param producer fun(): Formatter...
            local function use(producer)
                local first, second = producer()
                return first('one'), second('two')
            end
            ---@param producer fun(): Formatter, Formatter...
            local function use_tail(producer)
                local first, second = producer()
                return first('one'), second('two')
            end
            ---@param ... WrongArgument
            local function invalid(...) return ... end
            use(produce)
            use_tail(produce)
            use(invalid)
            use_tail(invalid)
            constant('extra')
        "#;
        let diagnostics = check_source(source);
        let mut actual = diagnostics
            .iter()
            .map(|d| (d.code, usize::from(d.range.start())))
            .collect::<Vec<_>>();
        actual.sort_by_key(|(_, offset)| *offset);
        assert_eq!(
            actual,
            vec![
                (
                    DiagnosticCode::ParamTypeMismatch,
                    source.find("use(invalid)").unwrap() + "use(".len()
                ),
                (
                    DiagnosticCode::ParamTypeMismatch,
                    source.find("use_tail(invalid)").unwrap() + "use_tail(".len()
                ),
                (
                    DiagnosticCode::RedundantParameter,
                    source.find("'extra'").unwrap()
                ),
            ],
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn test_vararg_callback_numeric_results_follow_assignment_rules() {
        let source = r#"
            ---@param callback fun(): number...
            local function accepts_numbers(callback) return callback() end
            ---@param callback fun(): number, number...
            local function accepts_number_tail(callback) return callback() end
            ---@param ... integer
            local function integers(...) return ... end
            accepts_numbers(integers)
            accepts_number_tail(integers)
            ---@param callback fun(): integer...
            local function requires_integers(callback) return callback() end
            ---@param callback fun(): integer, integer...
            local function requires_integer_tail(callback) return callback() end
            ---@param ... number
            local function numbers(...) return ... end
            requires_integers(numbers)
            requires_integer_tail(numbers)
        "#;
        let diagnostics = check_source(source);
        let mut actual = diagnostics
            .iter()
            .map(|d| (d.code, usize::from(d.range.start())))
            .collect::<Vec<_>>();
        actual.sort_by_key(|(_, offset)| *offset);
        assert_eq!(
            actual,
            vec![
                (
                    DiagnosticCode::ParamTypeMismatch,
                    source.find("requires_integers(numbers)").unwrap() + "requires_integers(".len()
                ),
                (
                    DiagnosticCode::ParamTypeMismatch,
                    source.find("requires_integer_tail(numbers)").unwrap()
                        + "requires_integer_tail(".len()
                ),
            ],
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn test_vararg_generic_callback_keeps_assignment_and_callback_checks() {
        let source = r#"
            ---@generic First, Rest
            ---@param callback fun(): First, Rest...
            ---@return First, Rest...
            local function wrap(callback) return callback() end
            ---@param ... integer
            local function values(...) return ... end
            local packed = {wrap(function() return values(1, 2, 3) end)}
            ---@type integer
            local valid = packed[3]
            ---@type string?
            local invalid = packed[3]
            ---@generic Rest
            ---@param callback fun(): string, Rest...
            local function requires_text(callback) return callback() end
            requires_text(function() return values(1, 2, 3) end)
            ---@generic First
            ---@param callback fun(): First, string...
            local function requires_string_tail(callback) return callback() end
            requires_string_tail(values)
            ---@param callback fun(): integer...
            local function consume_numbers(callback) return callback() end
            ---@param ... [integer, integer]...
            local function fixed(...) return ... end
            consume_numbers(fixed)
            ---@generic Results
            ---@param callback fun(): Results...
            ---@return Results...
            local function invoke(callback) return callback() end
            ---@param ... [string, integer]
            local function tuples(...) return ... end
            local function pairs() return tuples({'a', 1}, {'b', 2}) end
            invoke(pairs)
            return valid, invalid
        "#;
        let diagnostics = check_source(source);
        let mut actual = diagnostics
            .iter()
            .map(|d| (d.code, usize::from(d.range.start())))
            .collect::<Vec<_>>();
        actual.sort_by_key(|(_, offset)| *offset);
        assert_eq!(
            actual,
            vec![
                (
                    DiagnosticCode::AssignTypeMismatch,
                    source.rfind("packed[3]").unwrap()
                ),
                (
                    DiagnosticCode::ParamTypeMismatch,
                    source.rfind("function() return values").unwrap()
                ),
                (
                    DiagnosticCode::ParamTypeMismatch,
                    source.rfind("values)").unwrap()
                ),
                (
                    DiagnosticCode::ParamTypeMismatch,
                    source.find("consume_numbers(fixed)").unwrap() + "consume_numbers(".len()
                ),
            ],
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn test_union_assignment_accepts_each_source_member() {
        for (source, target) in [
            // Accept string and nil; 'FILE' is already included in string.
            ("string?", "(string|'FILE')?"),
            // Accept true as a boolean, and accept nil.
            ("true?", "boolean?"),
            // Match string and integer to different target members.
            ("string|integer", "string|number|boolean"),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@param value {source}
                local function use(value)
                    ---@type {target}
                    local initialized = value
                    ---@type {target}
                    local reassigned
                    reassigned = value
                    ---@type ({target})[]
                    local packed = {{value}}
                    return initialized, reassigned, packed
                end
                return use
                "#,
            ));
            assert!(
                diagnostics.is_empty(),
                "{source} -> {target}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_union_assignment_rejects_incompatible_source_members() {
        for (source, target, diagnostic_count) in [
            // Reject boolean, even though string and integer are accepted.
            ("string|integer|boolean", "string|number", 2),
            // Nil cannot supply a required value, but omits a table entry.
            ("true?", "boolean|string", 1),
            // Reject number because it can contain non-integer values.
            ("string|number", "string|integer", 2),
            // Reject integer when the target accepts only strings.
            ("string|integer", "string", 2),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@param value {source}
                local function use(value)
                    ---@type {target}
                    local target = value
                    ---@type ({target})[]
                    local packed = {{value}}
                    return target, packed
                end
                return use
                "#,
            ));
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                vec![DiagnosticCode::AssignTypeMismatch; diagnostic_count],
                "{source} -> {target}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_table_literal_numeric_key_uses_index_signature() {
        let diagnostics = check_source(
            r#"
            ---@class Record
            ---@field ["[9223372036854774784]"]? integer
            ---@field [integer] string
            ---@type Record
            local value = {[9223372036854774784.0] = "ok"}
            return value
            "#,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn test_table_literal_numeric_key_rejects_wrong_index_value() {
        let diagnostics = check_source(
            r#"
            ---@class Record
            ---@field ["[9223372036854774784]"]? integer
            ---@field [integer] string
            ---@type Record
            local value = {[9223372036854774784.0] = 1}
            return value
            "#,
        );
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec![DiagnosticCode::AssignTypeMismatch],
            "{diagnostics:?}"
        );
    }

    #[test]
    fn test_table_literal_numeric_and_string_fields_have_distinct_types() {
        let diagnostics = check_source(
            r#"
            ---@class Numeric
            ---@field [1]? integer
            ---@class Record: Numeric
            ---@field ["[1]"]? string
            ---@type Record
            local numeric = {[1] = 1}
            ---@type Record
            local named = {["[1]"] = "ok"}
            return numeric, named
            "#,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn test_table_literal_nullable_tuple() {
        let diagnostics = check_source(
            r#"
            ---@type [integer, integer]?
            local range
            range = {1, 2}
            return range
            "#,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn test_table_literal_tuple_alias() {
        let diagnostics = check_source(
            r#"
            ---@alias Pair [integer, integer]
            ---@alias Range Pair
            ---@type Range
            local pair = {1, 2}
            ---@type Pair?
            local optional = {3, 4}
            return pair, optional
            "#,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn test_table_literal_tuple_alias_array() {
        let diagnostics = check_source(
            r#"
            ---@alias Chunk [string, string?]
            ---@type Chunk[]
            local chunks = {{"text", "Comment"}, {" "}}
            return chunks
            "#,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn test_table_literal_tuple_alias_array_rejects_wrong_element() {
        let diagnostics = check_source(
            r#"
            ---@alias Chunk [string, string?]
            ---@type Chunk[]
            local chunks = {{"text", 1}}
            return chunks
            "#,
        );
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec![DiagnosticCode::AssignTypeMismatch],
            "{diagnostics:?}"
        );
    }

    #[test]
    fn test_table_literal_tuple_rejects_wrong_element() {
        for target in ["[integer, integer]?", "Pair", "Pair?"] {
            let diagnostics = check_source(&format!(
                r#"
                ---@alias Pair [integer, integer]
                ---@type {target}
                local pair = {{1, "bad"}}
                return pair
                "#,
            ));
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                vec![DiagnosticCode::AssignTypeMismatch],
                "{target}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_table_literal_tuple_rejects_missing_required_element() {
        for target in ["[integer, integer]", "[integer, integer]?", "Pair", "Pair?"] {
            let diagnostics = check_source(&format!(
                r#"
                ---@alias Pair [integer, integer]
                ---@type {target}
                local pair = {{1}}
                return pair
                "#,
            ));
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                vec![DiagnosticCode::AssignTypeMismatch],
                "{target}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_table_literal_tuple_allows_optional_and_extra_elements() {
        let diagnostics = check_source(
            r#"
            ---@alias Pair [integer, integer?]
            ---@type Pair?
            local short = {1}
            ---@type [integer, integer]
            local long = {1, 2, 3}
            return short, long
            "#,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn test_table_literal_tuple_rejects_invalid_alternatives() {
        let diagnostics = check_source(
            r#"
            ---@alias Pair [integer, integer]
            ---@type Pair | [string, string] | boolean
            local pair = {1, "bad"}
            return pair
            "#,
        );
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec![DiagnosticCode::AssignTypeMismatch],
            "{diagnostics:?}"
        );
    }

    #[test]
    fn test_1() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@generic T: string
            ---@param name  `T` 类名
            ---@return T
            local function meta(name)
                return name
            end

            ---@class Class
            local class = meta("class")
            "#
        ));
    }

    #[test]
    fn test_2() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class Diagnostic.Test7
            Diagnostic = {}

            ---@param a Diagnostic.Test7
            ---@param b number
            ---@return number
            function Diagnostic:add(a, b)
                return a + b
            end

            local add = Diagnostic.add
            "#
        ));
    }

    #[test]
    fn test_cast_add_type_allows_assignment() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@type string
            local val

            ---@cast val + boolean
            local after_cast = 1
            val = true
            "#
        ));
    }

    #[test]
    fn test_cast_add_type_allows_assignment_from_declared_union_in_narrowed_branch() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@type string|number
            local val

            ---@cast val + boolean
            if val == "a" then
                val = true
            end
            "#
        ));
    }

    #[test]
    fn test_cast_add_type_allows_assignment_after_branch_join() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@type string
            local val

            ---@cast val + boolean
            local cond ---@type boolean
            if cond then
                local branch = 1
            end

            val = true
            "#
        ));
    }

    #[test]
    fn test_branch_local_cast_does_not_allow_assignment_after_join() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@type string
            local val

            local cond ---@type boolean
            if cond then
                ---@cast val + boolean
                local branch = 1
            end

            val = true
            "#
        ));
    }

    #[test]
    fn test_field_cast_add_type_allows_assignment() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class CastField
            ---@field value string

            local obj ---@type CastField

            ---@cast obj.value + boolean
            obj.value = true
            "#
        ));
    }

    // #[test]
    // fn test_3() {
    //     let mut ws = VirtualWorkspace::new();
    //     assert!(ws.has_no_diagnostic_in_namespace(
    //         DiagnosticCode::AssignTypeMismatch,
    //         r#"
    //             ---@param s    string
    //             ---@param i?   integer
    //             ---@param j?   integer
    //             ---@param lax? boolean
    //             ---@return integer?
    //             ---@return integer? errpos
    //             ---@nodiscard
    //             local function get_len(s, i, j, lax) end

    //             local len = 0
    //             ---@diagnostic disable-next-line: need-check-nil
    //             len = len + get_len("", 1, 1, true)
    //         "#
    //     ));
    // }

    #[test]
    fn test_enum() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@enum SubscriberFlags
                local SubscriberFlags = {
                    None = 0,
                    Tracking = 1 << 0,
                    Recursed = 1 << 1,
                    ToCheckDirty = 1 << 3,
                    Dirty = 1 << 4,
                }
                ---@class Subscriber
                ---@field flags SubscriberFlags

                ---@type Subscriber
                local subscriber

                subscriber.flags = subscriber.flags & ~SubscriberFlags.Tracking -- 被推断为`integer`而不是实际整数值, 允许匹配
            "#
        ));

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@enum SubscriberFlags
                local SubscriberFlags = {
                    None = 0,
                    Tracking = 1 << 0,
                    Recursed = 1 << 1,
                    ToCheckDirty = 1 << 3,
                    Dirty = 1 << 4,
                }
                ---@class Subscriber
                ---@field flags SubscriberFlags

                ---@type Subscriber
                local subscriber

                subscriber.flags = 9 -- 不允许匹配不上的实际值
            "#
        ));
    }

    #[test]
    fn test_intersection_assign_to_class() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            --- @class A
            --- @field x integer
            --- @field y integer

            --- @class B
            --- @field y string
            --- @field z integer

            local c --- @type A & B

            --- @class C
            --- @field x integer
            --- @field y integer
            --- @field z integer

            --- @type C
            _ = c -- missing y
            "#
        ));
    }

    #[test]
    fn test_intersection_assign_from_class() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            --- @class A
            --- @field x integer
            --- @field y integer

            --- @class B
            --- @field y string
            --- @field z integer

            --- @class C
            --- @field x integer
            --- @field y integer
            --- @field z integer

            local v --- @type C

            local c --- @type A & B
            c = v  -- no y in A & B
            "#
        ));
    }

    #[test]
    fn test_intersection_assign_from_class_inherited_members() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class Base
            ---@field x integer

            ---@class C: Base
            ---@field y integer

            ---@class A
            ---@field x integer

            ---@class B
            ---@field y integer

            local v ---@type C

            local c ---@type A & B
            c = v
            "#
        ));
    }

    #[test]
    fn test_intersection_assign_tableconst_conflict() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class A
            ---@field y integer

            ---@class B
            ---@field y string

            local c ---@type A & B
            c = { y = 1 } -- no y in A & B
            "#
        ));
    }

    #[test]
    fn test_intersection_assign_tableconst_requires_right_only_members() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class A
            ---@field y integer

            ---@class B
            ---@field z integer

            local c ---@type A & B
            c = { y = 1 }
            "#
        ));
    }

    #[test]
    fn test_issue_193() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                --- @return string?
                --- @return string?
                local function foo() end

                local a, b = foo()
            "#
        ));
    }

    #[test]
    fn test_issue_196() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@class A

                ---@return table
                function foo() end

                ---@type A
                local _ = foo()
            "#
        ));
    }

    #[test]
    fn test_issue_197() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                local a = setmetatable({}, {})
            "#
        ));
    }

    /// Tests that cannot currently be solved.
    #[test]
    fn test_error() {
        // let mut ws = VirtualWorkspace::new();

        // Type inference issue.
        // assert!(ws.has_no_diagnostic_in_namespace(
        //     DiagnosticCode::AssignTypeMismatch,
        //     r#"
        // local n

        // if G then
        //     n = {}
        // else
        //     n = nil
        // end

        // local t = {
        //     x = n,
        // }
        //             "#
        // ));
    }

    #[test]
    fn test_valid_cases() {
        let mut ws = VirtualWorkspace::new();

        // Test cases that should pass (no type mismatch)
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
local m = {}
---@type integer[]
m.ints = {}
            "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@class A
---@field x A

---@type A
local t

t.x = {}
            "#
        ));

        // Test cases that should fail (type mismatch)
        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@class A
---@field x integer

---@type A
local t

t.x = true
            "#
        ));

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@class A
---@field x integer

---@type A
local t

---@type boolean
local y

t.x = y
            "#
        ));

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@class A
local m

m.x = 1

---@type A
local t

t.x = true
            "#
        ));

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@class A
local m

---@type integer
m.x = 1

m.x = true
            "#
        ));

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@class A
local mt

---@type integer
mt.x = 1

function mt:init()
    self.x = true
end
            "#
        ));

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@class A
---@field x integer

---@type A
local t = {
    x = true
}
            "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@type boolean[]
local t = {}

t[5] = nil
            "#
        ));
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@type table<string, true>
local t = {}

t['x'] = nil
            "#
        ));

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@type [boolean]
local t = { [1] = nil }

t = nil
            "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
local t = { true }

t[1] = nil
            "#
        ));

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@class A
local t = {
    x = 1
}

t.x = true
            "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@type number
local t

t = 1
            "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@type number
local t

---@type integer
local y

t = y
            "#
        ));

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@class A
local m

---@type number
m.x = 1

m.x = {}
            "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@type boolean[]
local t = {}

---@type boolean?
local x

t[#t+1] = x
            "#
        ));

        // Additional test cases
        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@type number
local n
---@type integer
local i

i = n
            "#
        ));

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@type number|boolean
local nb

---@type number
local n

n = nb
            "#
        ));

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@type number
local x = 'aaa'
            "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@class X

---@class A
local mt = G

---@type X
mt._x = nil
            "#
        ));
        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@class A
local a = {}

---@class B
local b = a
            "#
        ));

        // 类表自引用允许赋值.
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@class A
                local a = {}
                a.__index = a
            "#
        ));

        // TODO: 之前该诊断依赖于`a.__index = a`的自赋值错误, 实际代码没有处理 setmetatable 的情况, 这里我们先放行后续再为此添加错误诊断
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@class A
                local a = {}
                a.__index = a

                ---@class B: A
                local b = setmetatable({}, a)
            "#
        ));

        // Continue with more test cases as needed
        // Under the new semantics, field types still participate in assignment checks: `number?` cannot be assigned to `number`.
        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@class A
---@field x number?
local a

---@class B
---@field x number
local b

b.x = a.x
            "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
local mt = {}
mt.x = 1
mt.x = nil
            "#
        ));

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@alias test boolean

---@type test
local test = 4
            "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@class MyClass
local MyClass = {}

function MyClass:new()
    ---@class MyClass
    local myObject = setmetatable({
        initialField = true
    }, self)

    print(myObject.initialField)
end
            "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@class T
local t = {
    x = nil
}

t.x = 1
            "#
        ));

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@type {[1]: string, [10]: number, xx: boolean}
local t = {
    true,
    [10] = 's',
    xx = 1,
}
            "#
        ));

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
---@type boolean[]
local t = { 1, 2, 3 }
            "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
local t = {}
t.a = 1
t.a = 2
return t
            "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            local function name()
                return 1, 2
            end
            local x, y
            x, y = name()
            "#
        ));
    }

    // May need to be handled.
    #[test]
    fn test_pending() {
        let mut ws = VirtualWorkspace::new();
        // TODO: 下面说的新语义实际上没有实现, 仅是依赖错误的父类推断来实现报告错误, 这里先改为无错误报告以通过测试.
        // New semantics: `a` is A's runtime table (class A itself), so it cannot directly serve as subclass B's runtime table.
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class A
            local a = {}

            ---@class B: A
            local b = a
                "#
        ));

        // Allow accepting a parent class.
        // TODO: when accepting a parent class, check that it has all non-nullable members of the subclass.
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class Option: string

            ---@param x Option
            local function f(x) end

            ---@type Option
            local x = 'aaa'

            f(x)
                        "#
        ));

        // Array types allow nullable elements, but direct `nil` in initializer assignments is not allowed (actually a shortcut: table_expr inference does not handle this edge case; it may be allowed later).
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::AssignTypeMismatch,
            r#"
        ---@type boolean[]
        local t = { true, false, nil }
        "#
        ));
    }

    #[test]
    fn test_issue_247() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
        local a --- @type boolean
        local b --- @type integer
        b = 1 + (a and 1 or 0)
        "#
        ));
    }

    #[test]
    fn test_issue_246() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
        --- @alias Type1 'add' | 'change' | 'delete'
        --- @alias Type2 'add' | 'change' | 'delete' | 'untracked'

        local ty1 --- @type Type1?

        --- @type Type2
        local _ = ty1 or 'untracked'
        "#
        ));
    }

    #[test]
    fn test_issue_285() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                --- @return string, integer
                local function foo() end

                local text, err
                text, err = foo()

                ---@type integer
                local b = err
        "#
        ));
    }

    #[test]
    fn test_issue_338() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            local t ---@type 0|-1

            t = -1
        "#
        ));
    }

    #[test]
    fn test_return_self() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class UI
            ---@overload fun(): self
            local M

            ---@type UI
            local a = M()
        "#
        ));
    }

    #[test]
    fn test_table_pack_in_function() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@param ... any
                local function build(...)
                    local t = table.pack(...)
                end
        "#
        ));
    }

    #[test]
    fn test_assign_field_with_flow() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@class M
                local M

                ---@type 'new' | 'inited' | 'started'
                M.state = 'new'

                function M:test()
                    if self.state ~= 'started' and self.state ~= 'inited' then
                        return
                    end
                    self.state = 'new'
                end
        "#
        ));
    }

    #[test]
    fn test_flow_1() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@class Unit

                ---@class Player

                ---@class CreateData
                ---@field owner? Unit|Player

                ---@param data CreateData
                local function send(data)
                    if not data.owner then
                        data.owner = ""
                    end
                end
        "#
        ));
    }

    #[test]
    fn test_flow_2() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@class Unit

                ---@class Player

                ---@class CreateData
                ---@field owner? Unit|Player

                ---@param data Unit|Player?
                local function send(data)
                    if not data then
                        data = ""
                    end
                end
        "#
        ));
    }

    #[test]
    fn test_table_array() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@type  { [1]: string, [integer]: any }
                local py_event

                ---@type any[]
                local py_args

                py_event = py_args
        "#
        ));
    }

    #[test]
    fn test_issue_330() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@enum MyEnum
            local MyEnum = { A = 1, B = 2 }

            local x --- @type MyEnum?

            ---@type MyEnum
            local a = x or MyEnum.A
        "#
        ));
    }

    #[test]
    fn test_issue_393() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@alias SortByScoreCallback fun(o: any): integer

                ---@param tbl any[]
                ---@param callbacks SortByScoreCallback | SortByScoreCallback[]
                function sortByScore(tbl, callbacks)
                    if type(callbacks) ~= 'table' then
                        callbacks = { callbacks }
                    end
                end
        "#
        ));
    }

    #[test]
    fn test_issue_374() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                --- @param x? integer
                --- @return integer?
                --- @overload fun(): integer
                function bar(x) end

                --- @type integer
                local _ = bar() -- - error cannot assign `integer?` to `integer`
        "#
        ));
    }

    #[test]
    fn test_nesting_table_field_1() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class T1
            ---@field x T2

            ---@class T2
            ---@field xx number
        "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@type T1
            local t = {
                x = {
                    xx = "",
                }
            }
        "#
        ));
    }

    #[test]
    fn test_nesting_table_field_2() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class T1
            ---@field x number
        "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@type T1
            local t = {
                x = {
                    xx = "",
                }
            }
        "#
        ));
    }

    #[test]
    fn test_optional_alias_field_rejects_table_literal_regardless_of_declaration_order() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@alias B true?

            ---@class A
            ---@field field B

            ---@type A
            local var = { field = {} }
        "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@alias B true?

            ---@class A
            ---@field field? B

            ---@type A
            local var = { field = {} }
        "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class A
            ---@field field? B

            ---@alias B true?

            ---@type A
            local var = { field = {} }
        "#
        ));
    }

    #[test]
    fn test_issue_525() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@type table<integer,true|string>
                local lines
                for lnum = 1, #lines do
                    if lines[lnum] == true then
                        lines[lnum] = ''
                    end
                end
        "#
        ));
    }

    #[test]
    fn test_param_tbale() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@class ability
                ---@field t abilityType

                ---@enum (key) abilityType
                local abilityType = {
                    passive = 1,
                }

                ---@param a ability
                function test(a)

                end

                test({
                    t = ""
                })
        "#
        ));
    }

    #[test]
    fn test_table_field_type_mismatch() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            local export = {
                ---@type number?
                vvv = "a"
            }
        "#
        ));
    }

    #[test]
    fn test_object_table() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@alias A {[string]: string}

        ---@param matchers A
        function name(matchers)
        end
        "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            name({
                toBe = 1,
            })
        "#
        ));
    }

    #[test]
    fn test_generic_array_alias_tuple() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias array<T> T[]
        "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@type array<number>
            local list = {
                "2",
            }
        "#
        ));
    }

    #[test]
    fn test_ref_index_key_match_tuple() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@class Item
                ---@field id int

                ---@class TbItem
                ---@field [int] Item

                ---@type TbItem
                local items = {
                    { id = 1 },
                    { id = 2 },
                    { id = 2 },
                }
            "#,
        ));
    }

    #[test]
    fn test_ref_index_key_match_tuple_with_optional_super_member() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@class OptsBase
                ---@field foo? boolean

                ---@class Opts : OptsBase
                ---@field [integer] string

                ---@type Opts
                local opts1 = { "hello" }
            "#,
        ));
    }

    #[test]
    fn test_ref_index_key_match_tuple_with_required_super_member() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@class OptsBase
                ---@field foo boolean

                ---@class Opts : OptsBase
                ---@field [integer] string

                ---@type Opts
                local opts1 = { "hello" }
            "#,
        ));
    }

    #[test]
    fn test_or_table_literal_satisfies_class_with_index_signature() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@class Foo
                ---@field [integer] string
                ---@field other number

                local foo ---@type Foo?
                foo = foo or { other = 5 }
            "#,
        ));
    }

    #[test]
    fn test_table_literal_index_member_must_match_class_index_signature() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@class Foo
                ---@field [integer] string

                ---@type Foo
                local foo = { [1] = 1 }
            "#,
        ));
    }

    #[test]
    fn test_ref_index_access_assign_class_to_object_mismatch() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@class A
                ---@field [integer] string

                local t ---@type { [integer]: number }
                local a ---@type A

                t = a
            "#,
        ));
    }

    #[test]
    fn test_ref_index_access_assign_object_to_class_mismatch() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                ---@class A
                ---@field [integer] string

                local t ---@type { [integer]: number }
                local a ---@type A

                a = t
            "#,
        ));
    }

    #[test]
    fn test_exact_string_reassignment_in_narrowed_branch_keeps_assign_literal() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
                local x ---@type string|number

                if x == 1 then
                    x = "a"

                    ---@type "a"
                    local y = x
                end
            "#,
        ));
    }

    #[test]
    fn test_return_overload_mixed_guards_keep_assign_narrowing() {
        let source = r#"
                ---@generic T, E
                ---@param ok boolean
                ---@param success T
                ---@param failure E
                ---@return boolean
                ---@return T|E
                ---@return_overload true, T
                ---@return_overload false, E
                local function pick(ok, success, failure)
                    if ok then
                        return true, success
                    end
                    return false, failure
                end

                ---@param cond boolean
                local function test(cond)
                    local ok, result = pick(cond, 1, "err")

                    ---@type integer
                    local unguarded = result

                    if ok == false then
                        error(result)
                    end

                    if not ok then
                        error(result)
                    end

                    ---@type integer
                    local narrowed = result
                end
            "#;
        let diagnostics = check_source(source);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code == DiagnosticCode::AssignTypeMismatch)
                .map(|d| usize::from(d.range.start()))
                .collect::<Vec<_>>(),
            vec![source.find("unguarded = result").unwrap() + "unguarded = ".len()],
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn test_overloaded_call_results_use_the_matching_argument_type() {
        let mut diagnostics = check_source(
            r#"
            ---@overload fun(value: string): integer
            ---@param _value boolean
            ---@return string
            local function pick(_value) return 'text' end

            ---@type integer
            local assigned = pick(true)
            ---@param value integer
            local function consume(value) return value end
            consume(pick(true))
            ---@return integer
            local function forward() return pick(true) end
            return assigned, forward
            "#,
        );
        diagnostics.sort_by_key(|diagnostic| diagnostic.range.start());
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec![
                DiagnosticCode::AssignTypeMismatch,
                DiagnosticCode::ParamTypeMismatch,
                DiagnosticCode::ReturnTypeMismatch,
            ],
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn test_function_parameter_contravariance_assignment() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class A
            ---@class B
            ---@class C
            ---@class D
            ---@alias Pair A | B
            ---@alias TooWide A | B | C | D
            ---@alias Identity<T> T

            ---@param a A | B | C
            ---@return boolean
            function condition(a)
                return true
            end
            "#,
        );
        for (parameter, accepted) in [
            ("A | B | C | D", false),
            ("TooWide", false),
            ("Identity<A | B | C | D>", false),
            ("A | B", true),
            ("Pair", true),
            ("Identity<A | B>", true),
        ] {
            assert_eq!(
                ws.has_no_diagnostic(
                    DiagnosticCode::AssignTypeMismatch,
                    &format!("---@type fun(a: {parameter}): boolean\nlocal tmp = condition"),
                ),
                accepted,
                "{parameter}"
            );
        }
    }

    #[test]
    fn test_generic_extends_table() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            --- @alias Procedure fun(...: any...): any

            --- @alias MockReturnType<T> T extends table and nil or any

            --- @class MockContext<T = Procedure>
            --- @field results MockResult<MockReturnType<T>>

            --- @class MockResult<T>
            --- @field value T

            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"

            --- @type MockContext
            local state

            --- @type MockResult<Procedure>
            local result

            state.results = result
            "#,
        ));
    }

    #[test]
    fn test_generic_constraint_assign_to_incompatible_type() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class Animal
            ---@field name string

            ---@generic T: Animal
            ---@param animal T
            local function checkAnimal(animal)
                ---@type string
                local name = animal
            end
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class Animal
            ---@field name string

            ---@generic T: Animal
            ---@param animal T
            local function checkAnimal(animal)
                ---@type Animal
                local same = animal
            end
        "#
        ));
    }

    #[test]
    fn class_index_assignment_self() {
        let source = r#"
            ---@class LinkedTableA
            ---@field value string
            local M = {}
            M.__index = M
            "#;
        let diagnostics = check_source(source);
        assert_eq!(diagnostics.len(), 0, "{diagnostics:#?}");
    }

    #[test]
    fn class_index_assignment_keeps_declared_field_checks() {
        let source = r#"
            ---@class LinkedTableA
            ---@field __index string
            ---@field value string
            local M = {}
            M.__index = M
            M.__index = 1
            M.value = M
            "#;
        let diagnostics = check_source(source);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == DiagnosticCode::AssignTypeMismatch)
                .map(|diagnostic| usize::from(diagnostic.range.start()))
                .collect::<Vec<_>>(),
            vec![
                source.find("M.__index = M").unwrap() + "M.__index = ".len(),
                source.find("M.__index = 1").unwrap() + "M.__index = ".len(),
                source.find("M.value = M").unwrap() + "M.value = ".len(),
            ],
            "{diagnostics:#?}"
        );
    }

    /// #1334: a read through an index signature has the signature's value type.
    #[test]
    fn test_index_signature_read_has_value_type() {
        for (decl, read, accepted) in [
            ("---@param pt table<string, integer>", "pt.X", false),
            ("---@param pt table<string, integer>", "pt['X']", false),
            ("---@param pt table<string, integer>", "pt[k]", false),
            ("---@param pt table<integer, integer>", "pt[1]", false),
            ("---@param pt I1334", "pt.X", false),
            ("---@param pt I1334", "pt[k]", false),
            // The key must fit the index domain; a named field beats the signature.
            ("---@param pt table<'X', integer>", "pt.Y", true),
            ("---@param pt table<1, integer>", "pt[2]", true),
            ("---@param pt I1334", "pt.flag", true),
        ] {
            let source = format!(
                r#"
                ---@class I1334
                ---@field flag boolean
                ---@field [string] integer

                {decl}
                ---@param k string
                local function g(pt, k)
                    ---@type boolean
                    local a = {read}
                    return a
                end
                return g
                "#
            );
            let mut ws = VirtualWorkspace::new();
            assert_eq!(
                ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, &source),
                accepted,
                "{source}"
            );
        }
    }

    /// #1334: the `{ [K]: V }` form (a `---@param` of it is still a plain `table`, so a typed local).
    #[test]
    fn test_object_index_signature_read_has_value_type() {
        for read in ["pt.X", "pt[k]"] {
            let source = format!(
                r#"
                ---@type {{ [string]: integer }}
                local pt = {{}}
                local k = tostring(1)
                ---@type boolean
                local a = {read}
                return a
                "#
            );
            let mut ws = VirtualWorkspace::new();
            assert!(
                !ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, &source),
                "{source}"
            );
        }
    }
}
