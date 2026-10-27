#[cfg(test)]
mod test {
    use crate::check::test::check_source;
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_array_parent_accepts_empty_and_valid_elements() {
        for parent in ["Item[]", "ItemArray", "ItemArrayAlias", "ArrayBase<Item>"] {
            for value in ["{}", "{{name = 'ok'}}"] {
                let diagnostics = check_source(&format!(
                    r#"
---@class Item
---@field name string
---@class ItemArray: Item[]
---@alias ItemArrayAlias Item[]
---@class ArrayBase<T>: T[]
---@class Items: {parent}
---@type Items
local items = {value}
---@param values Items
local function accept(values) return values end
accept({value})
return items
"#
                ));
                assert!(diagnostics.is_empty(), "{parent}, {value}: {diagnostics:?}");
            }
        }
    }

    #[test]
    fn test_array_parent_checks_container_fields_separately() {
        for (value, expected) in [
            ("{name = 1, label = 'items', {name = 'ok'}}", vec![]),
            ("{name = 1, label = 'items'}", vec![]),
            ("{label = 'items'}", vec![DiagnosticCode::MissingFields]),
            ("{name = 1}", vec![DiagnosticCode::MissingFields]),
        ] {
            let diagnostics = check_source(&format!(
                r#"
---@class Item
---@field name string
---@class Metadata
---@field label string
---@class Items: Item[], Metadata
---@field name number
---@type Items
local items = {value}
return items
"#
            ));
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                expected,
                "{value}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_array_parent_reports_missing_field_on_element() {
        for (parent, value) in [
            ("Item[]", "{{}}"),
            ("Item[]", "{{name = 'ok'}, {}}"),
            ("Item[][]", "{{{}}}"),
        ] {
            let source = format!(
                r#"
---@class Item
---@field name string
---@class Items: {parent}
---@type Items
local items = {value}
return items
"#
            );
            let diagnostics = check_source(&source);
            assert_eq!(diagnostics.len(), 1, "{value}: {diagnostics:?}");
            let diagnostic = &diagnostics[0];
            assert_eq!(diagnostic.code, DiagnosticCode::MissingFields);
            assert_eq!(diagnostic.message, "Missing required fields: `name`");
            assert_eq!(&source[diagnostic.range], "{}");
        }
    }

    #[test]
    fn test_array_parent_checks_elements_in_calls_and_reassignments() {
        for statement in ["accept({{}})", "items = {{}}"] {
            let source = format!(
                r#"
---@class Item
---@field name string
---@class Items: Item[]
---@type Items
local items
---@param values Items
local function accept(values) return values end
{statement}
return items, accept
"#
            );
            let diagnostics = check_source(&source);
            assert_eq!(diagnostics.len(), 1, "{statement}: {diagnostics:?}");
            assert_eq!(diagnostics[0].code, DiagnosticCode::MissingFields);
            assert_eq!(&source[diagnostics[0].range], "{}");
        }
    }

    #[test]
    fn test_array_parent_rejects_invalid_element_value() {
        let diagnostics = check_source(
            r#"
---@class Item
---@field name string
---@class Items: Item[]
---@type Items
local items = {{name = 123}}
return items
"#,
        );
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec![DiagnosticCode::AssignTypeMismatch],
            "{diagnostics:?}"
        );
    }

    #[test]
    fn test_generic_index_signature_contracts() {
        for (value, expected) in [
            ("{integer = true, [1] = 'ok'}", vec![]),
            (
                "{integer = true, [1] = false}",
                vec![DiagnosticCode::AssignTypeMismatch],
            ),
            (
                "{integer = true, ['1'] = 'ok'}",
                vec![DiagnosticCode::InjectField],
            ),
            ("{[1] = 'ok'}", vec![DiagnosticCode::MissingFields]),
        ] {
            let diagnostics = check_source(&format!(
                "
                ---@class Base<K, V>
                ---@field [K] V

                ---@class Record: Base<integer, string>
                ---@field integer boolean
                ---@type Record
                local value = {value}
                return value"
            ));
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                expected,
                "{value}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_generic_index_signature_on_call_argument() {
        let diagnostics = check_source(
            "---@class Dictionary<K, V>
            ---@field [K] V
            ---@param value Dictionary<string, boolean>
            local function accept(value) return value end
            accept({name = true})
            accept({name = 'bad'})",
        );
        assert!(
            diagnostics.iter().any(|d| matches!(
                d.code,
                DiagnosticCode::AssignTypeMismatch | DiagnosticCode::ParamTypeMismatch
            )),
            "{diagnostics:?}"
        );
        assert!(
            !diagnostics.iter().any(|d| matches!(
                d.code,
                DiagnosticCode::InjectField | DiagnosticCode::MissingFields
            )),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn test_index_signature_float_keys_use_numeric_domain() {
        for (domain, expected) in [
            ("integer", vec![DiagnosticCode::InjectField]),
            ("number", vec![]),
        ] {
            let diagnostics = check_source(&format!(
                "---@class Record
                ---@field [{domain}] string
                ---@type Record
                local value = {{[1.5] = 'ok'}}
                return value"
            ));
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                expected,
                "{domain}: {diagnostics:?}"
            );
        }
    }

    mod generic_tables {
        use super::*;

        use DiagnosticCode::{AssignTypeMismatch, ParamTypeMismatch};

        fn assert_diagnostics(source: &str, expected: &[DiagnosticCode]) {
            let diagnostics = check_source(source);
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                expected,
                "{source}\n{diagnostics:?}"
            );
        }

        fn assert_workspace_diagnostics(
            ws: &mut VirtualWorkspace,
            source: &str,
            expected: &[(DiagnosticCode, u32)],
        ) {
            let file = ws.def(source);
            let diagnostics = ws
                .analysis
                .diagnose_file(file, tokio_util::sync::CancellationToken::new())
                .unwrap();
            let expected: Vec<_> = expected
                .iter()
                .map(|(code, line)| {
                    (
                        Some(lsp_types::NumberOrString::String(code.get_name().into())),
                        *line,
                    )
                })
                .collect();
            assert_eq!(
                diagnostics
                    .iter()
                    .map(|d| (d.code.clone(), d.range.start.line))
                    .collect::<Vec<_>>(),
                expected,
                "{source}\n{diagnostics:?}"
            );
        }

        #[test]
        fn test_disabled_field_errors_preserve_type_errors() {
            use crate::Emmyrc;
            use crate::check::test::check_source_with_emmyrc;

            for (code, declaration, target, argument) in [
                (
                    DiagnosticCode::MissingFields,
                    "---@param opts {label: string, required: string}",
                    "{label: string, required: string}",
                    "{label = 1}",
                ),
                (
                    DiagnosticCode::InjectField,
                    "---@class Options\n---@field label string\nlocal _Options = {}\n---@param opts Options",
                    "Options",
                    "{label = 1, extra = true}",
                ),
            ] {
                let source = format!(
                    "{declaration}\nlocal function accept(opts) return opts end\naccept({argument})\npcall(accept, {argument})\n---@type {target}\nlocal assignment = {argument}\nreturn assignment"
                );
                let mut config = Emmyrc::default();
                config.diagnostics.disable.push(code);
                let diagnostics = check_source_with_emmyrc(&source, config);
                assert_eq!(
                    diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                    [ParamTypeMismatch, ParamTypeMismatch, AssignTypeMismatch],
                    "{source}\n{diagnostics:?}"
                );
                assert_diagnostics(
                    &format!("---@diagnostic disable: {}\n{source}", code.get_name()),
                    &[ParamTypeMismatch, ParamTypeMismatch, AssignTypeMismatch],
                );
            }
            assert_diagnostics(
                r#"
                ---@diagnostic disable: missing-fields
                ---@type {label: string}
                local value = {}
                return value
                "#,
                &[AssignTypeMismatch],
            );
            assert_diagnostics(
                r#"
                ---@param opts {label: string}
                ---@overload fun(opts: string)
                local function accept(opts) return opts end
                accept({})
                "#,
                &[DiagnosticCode::MissingFields],
            );
        }

        #[test]
        fn test_class_index_signatures_use_projected_key_and_value_types() {
            for (target, valid_key, invalid_key) in [
                ("Dict<integer, string>", "1", "'extra'"),
                ("Derived<string>", "1", "'extra'"),
                ("Dict<NumericKey, string>", "1", "'extra'"),
                ("ObjectBase<string>", "1", "'extra'"),
                ("MixedBase<string>", "1", "true"),
                ("Dict<LabelKey, string>", "'left'", "'extra'"),
            ] {
                let source = format!(
                    r#"
                    ---@alias NumericKey integer
                    ---@alias LabelKey 'left'|'right'
                    ---@class Dict<K, V>
                    ---@field [K] V
                    ---@field label? V
                    local _Dict = {{}}
                    ---@class Derived<V>: Dict<integer, V>
                    local _Derived = {{}}
                    ---@class ObjectBase<V>: {{[integer]: V}}
                    ---@field label? V
                    local _ObjectBase = {{}}
                    ---@class MixedBase<V>: {{[integer]: V, [string]: V}}
                    local _MixedBase = {{}}
                    ---@param values {target}
                    local function accept(values) return values end
                    accept({{[{valid_key}] = 'ok', label = 'ok'}})
                    accept({{[{invalid_key}] = 'ok'}})
                    accept({{[{valid_key}] = true}})
                    ---@type {target}
                    local valid = {{[{valid_key}] = 'ok'}}
                    ---@type {target}
                    local invalid_key = {{[{invalid_key}] = 'ok'}}
                    ---@type {target}
                    local invalid_value = {{[{valid_key}] = true}}
                    return valid, invalid_key, invalid_value
                    "#
                );
                assert_diagnostics(
                    &source,
                    &[
                        DiagnosticCode::InjectField,
                        ParamTypeMismatch,
                        DiagnosticCode::InjectField,
                        AssignTypeMismatch,
                    ],
                );
            }
        }

        #[test]
        fn test_array_assignments_check_keys_and_expanded_results() {
            assert_diagnostics(
                r#"
                ---@return string, integer
                local function pair() return 'ok', 1 end
                ---@param values string[]
                local function accept(values) return values end
                accept({pair()})
                accept({name = 'ok'})
                ---@type string[]
                local values = {(pair()), pair(), 'last'}
                values = {[1] = pair()}
                values = {pair()}
                values = {name = 'ok'}
                ---@type string[]
                local invalid_results = {pair()}
                ---@type string[]
                local invalid_key = {name = 'ok'}
                ---@type {values: string[]}
                local nested = {values = {pair()}}
                return values, invalid_results, invalid_key, nested
                "#,
                &[
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                    AssignTypeMismatch,
                    AssignTypeMismatch,
                    AssignTypeMismatch,
                    AssignTypeMismatch,
                    AssignTypeMismatch,
                ],
            );
        }

        #[test]
        fn test_dictionary_values_keep_referenced_table_types() {
            assert_diagnostics(
                r#"
                local valid = {'ok'}
                local invalid = {name = 1}
                local holder = {child = invalid}
                local function child() return {name = 1} end
                ---@param items table<integer, table<integer, string>>
                local function accept(items) return items end
                accept({valid})
                accept({invalid})
                accept({holder.child})
                accept({child()})
                ---@type table<integer, table<integer, string>>
                local assignment = {invalid}
                return assignment
                "#,
                &[
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                    AssignTypeMismatch,
                ],
            );
        }

        #[test]
        #[ntest::timeout(10000)]
        fn test_recursive_dictionary_references_check_invalid_leaves() {
            for (alias, target) in [
                ("Tree table<string, string|Tree>", "Tree"),
                ("Tree<T> table<string, string|Tree<T[]>>", "Tree<string>"),
            ] {
                let mut ws = VirtualWorkspace::new_with_init_std_lib();
                ws.def_files(vec![
                    ("a.lua", "return {child = require('b'), label = 1}"),
                    ("b.lua", "return {child = require('a')}"),
                ]);
                assert_workspace_diagnostics(
                    &mut ws,
                    &format!(
                        "---@alias {alias}\n---@param values {target}\nlocal function accept(values) return values end\naccept({{child = require('a')}})\naccept({{child = require('b')}})"
                    ),
                    &[(ParamTypeMismatch, 3), (ParamTypeMismatch, 4)],
                );
            }
        }

        #[test]
        #[ntest::timeout(10000)]
        fn test_recursive_dictionary_references_keep_used_generic_arguments() {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            ws.def_files(vec![
                ("a.lua", "return {child = require('b'), label = 'ok'}"),
                ("b.lua", "return {child = require('a')}"),
            ]);
            assert_workspace_diagnostics(
                &mut ws,
                "---@alias Tree<T> table<string, T|Tree<T[]>>\n---@param values Tree<string>\nlocal function accept(values) return values end\naccept({child = require('a')})\naccept({child = require('b')})",
                &[(ParamTypeMismatch, 3), (ParamTypeMismatch, 4)],
            );
        }

        #[test]
        fn test_record_union_selection_checks_required_fields() {
            assert_diagnostics(
                r#"
                ---@alias Options {left: string}|{right: integer}
                ---@param opts Options
                local function accept(opts) return opts end
                accept({left = 'ok'})
                accept({right = 1})
                accept({})
                ---@type Options
                local valid = {right = 1}
                return valid
                "#,
                &[DiagnosticCode::MissingFields],
            );
            assert_diagnostics(
                r#"
                ---@class Record
                ---@field label string
                local _Record = {}
                ---@param opts Record|table<string, integer|string>
                local function accept(opts) return opts end
                accept({label = 'ok', extra = 1})
                "#,
                &[],
            );
        }

        #[test]
        fn test_nil_values_do_not_create_dictionary_entries() {
            assert_diagnostics(
                r#"
                ---@param values table<integer, string>
                local function accept(values) return values end
                accept({name = nil})
                accept({[false] = nil})
                accept({[1] = nil})
                accept({name = 'bad'})
                accept({1})
                ---@type table<integer, string>
                local empty = {name = nil}
                return empty
                "#,
                &[ParamTypeMismatch, ParamTypeMismatch],
            );
        }

        #[test]
        fn test_nullable_dictionary_values_check_all_present_alternatives() {
            assert_diagnostics(
                r#"
                ---@alias MaybeText 'left'|'right'|nil

                ---@class Texts
                ---@field [string] string
                local _Texts = {}
                ---@param values table<integer, string>
                local function accept(values) return values end
                ---@param values Texts
                local function named(values) return values end
                ---@param text MaybeText
                ---@param invalid string|integer|nil
                local function build(text, invalid)
                    accept({text})
                    accept({[1] = text})
                    named({name = text})
                    ---@type table<integer, string>
                    local valid = {text}
                    accept({name = text})
                    accept({invalid})
                    named({name = invalid})
                    ---@type table<integer, string>
                    local wrong_key = {name = text}
                    ---@type table<integer, string>
                    local wrong_value = {invalid}
                    return valid, wrong_key, wrong_value
                end
                return build
                "#,
                &[
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                    AssignTypeMismatch,
                    AssignTypeMismatch,
                ],
            );
        }

        #[test]
        #[ntest::timeout(10000)]
        fn test_recursive_dictionaries_ignore_only_unused_arguments() {
            for (alias, target) in [
                (
                    "Tree<T> table<string, string|Tree<T[]>>|boolean",
                    "Tree<string>",
                ),
                (
                    "Tree<T, U> table<string, T|Tree<T, U[]>>",
                    "Tree<string, integer>",
                ),
            ] {
                for (leaf, expected) in [
                    ("'ok'", &[][..]),
                    (
                        "1",
                        &[
                            (ParamTypeMismatch, 5),
                            (ParamTypeMismatch, 6),
                            (AssignTypeMismatch, 8),
                        ][..],
                    ),
                ] {
                    let mut ws = VirtualWorkspace::new_with_init_std_lib();
                    ws.def_files(vec![
                        (
                            "a.lua",
                            &format!("return {{child = require('b'), label = {leaf}}}"),
                        ),
                        ("b.lua", "return {child = require('a')}"),
                    ]);
                    assert_workspace_diagnostics(
                        &mut ws,
                        &format!(
                            "---@alias {alias}\nlocal a = require('a')\nlocal b = require('b')\n---@param values {target}\nlocal function accept(values) return values end\naccept({{child = a}})\npcall(accept, {{child = b}})\n---@type {target}\nlocal root = {{child = a}}\nreturn root"
                        ),
                        expected,
                    );
                }
            }
        }

        #[test]
        #[ntest::timeout(10000)]
        fn test_recursive_arguments_keep_indirect_field_dependencies() {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            ws.def_files(vec![
                ("a.lua", "return {label = 'ok', child = require('b')}"),
                ("b.lua", "return {label = 1, child = require('a')}"),
            ]);
            assert_workspace_diagnostics(
                &mut ws,
                "---@alias Tree<T, U, V> {label: T, child?: Tree<U, T, V[]>}\nlocal root = require('a')\n---@param value Tree<string, integer, boolean>\nlocal function accept(value) return value end\naccept(root)\n---@param value Tree<string, string, boolean>\nlocal function reject(value) return value end\nreject(root)",
                &[(ParamTypeMismatch, 7)],
            );
        }

        #[test]
        fn test_dictionary_constructors_expand_only_the_final_unkeyed_field() {
            assert_diagnostics(
                r#"
                ---@return string, integer
                local function pair() return 'ok', 1 end
                ---@return string, string
                local function texts() return 'one', 'two' end
                local function nothing() end
                ---@param values table<integer, string>
                local function accept(values) return values end
                accept({pair()})
                accept({(pair())})
                accept({[1] = pair()})
                accept({pair(), 'last'})
                accept({texts()})
                accept({nothing()})
                ---@class TextList
                ---@field [integer] string
                local _TextList = {}
                ---@param values TextList
                local function named(values) return values end
                named({texts()})
                named({pair()})
                ---@param values table<1, string>
                local function single(values) return values end
                single({texts()})
                ---@type table<integer, string>
                local invalid = {pair()}
                return invalid
                "#,
                &[
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                    AssignTypeMismatch,
                ],
            );
        }

        #[test]
        fn test_dictionary_constructors_check_repeated_result_values_and_keys() {
            assert_diagnostics(
                r#"
                ---@param values table<integer, string>
                local function accept(values) return values end
                ---@param values table<1, string>
                local function single(values) return values end
                ---@param ... string
                local function text_values(...)
                    accept({...})
                    accept({(...), 'last'})
                    single({...})
                end
                ---@param ... string|integer
                local function mixed_values(...)
                    accept({...})
                end
                text_values('ok')
                mixed_values('ok', 1)
                "#,
                &[ParamTypeMismatch, ParamTypeMismatch],
            );
        }

        #[test]
        fn test_dictionary_constructors_check_all_return_alternatives() {
            assert_diagnostics(
                r#"
                ---@param flag boolean
                local function alternatives(flag)
                    if flag then return 'one', 'two', 3 end
                    return 'one', 'two'
                end
                ---@param values table<integer, string>
                local function accept(values) return values end
                accept({alternatives(true)})
                accept({(alternatives(true))})
                local function prefixed(flag) return 'prefix', alternatives(flag) end
                accept({prefixed(true)})
                accept({(prefixed(true))})
                "#,
                &[ParamTypeMismatch, ParamTypeMismatch],
            );
        }

        #[test]
        fn test_call_field_errors_use_one_bound_candidate() {
            let valid = r#"
                ---@param opts {left: string}
                ---@overload fun(opts: {right: integer})
                local function accept(opts) return opts end
                accept({left = 'ok'})
                accept({right = 1})

                local receiver = {}
                ---@param opts {child: {label: string}}
                function receiver:accept(opts) return self, opts end
                receiver.accept({}, {child = {label = 'ok'}})
                receiver:accept({child = {label = 'ok'}})
            "#;
            assert_diagnostics(valid, &[]);
            let mut ws = VirtualWorkspace::new();
            assert!(ws.has_no_diagnostic(DiagnosticCode::MissingFields, valid));
            let invalid = r#"
                local receiver = {}
                ---@param opts {child: {label: string}}
                function receiver:accept(opts) return self, opts end
                receiver.accept(receiver, {child = {}})
                receiver:accept({child = {}})

                ---@generic T
                ---@param opts {child: T}
                local function accept(opts) return opts end
                accept--[[@<{label: string}>]]({child = {label = 'ok'}})
                accept--[[@<{label: string}>]]({child = {}})

                ---@param ... {label: string}
                local function many(...) return ... end
                many({label = 'ok'}, {})
                pcall(many, {label = 'ok'}, {})
                "#;
            assert_diagnostics(
                invalid,
                &[
                    DiagnosticCode::MissingFields,
                    DiagnosticCode::MissingFields,
                    DiagnosticCode::MissingFields,
                    DiagnosticCode::MissingFields,
                    DiagnosticCode::MissingFields,
                ],
            );
            assert!(!ws.has_no_diagnostic(DiagnosticCode::MissingFields, invalid));
        }

        #[test]
        fn test_nested_table_arguments_use_receiver_and_generic_bindings() {
            assert_diagnostics(
                r#"
                local receiver = {}
                ---@param opts {label: string, values?: table<integer, string>}
                function receiver:accept(opts) return self, opts end
                receiver.accept(receiver, {label = 'ok', values = {'ok'}})
                receiver.accept(receiver, {label = {}})
                receiver.accept(receiver, {label = 'ok', values = {1}})
                receiver:accept({label = 'ok', values = {1}})

                ---@generic T
                ---@param opts {values: table<integer, T>}
                local function accept(opts) return opts end
                accept--[[@<string>]]({values = {'ok'}})
                accept--[[@<string>]]({values = {1}})

                ---@generic T
                ---@param value T
                ---@param opts {values: table<integer, T>}
                local function inferred(value, opts) return value, opts end
                inferred('ok', {values = {'ok'}})
                inferred('ok', {values = {1}})
                "#,
                &[
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                ],
            );
        }

        #[test]
        fn test_cross_file_record_parameters_keep_explicit_generics() {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            ws.def_file(
                "accept.lua",
                r#"
                ---@generic T
                ---@param opts {values: table<integer, T>, [integer]: T}
                function accept(opts) return opts end
            "#,
            );
            assert_workspace_diagnostics(
                &mut ws,
                "accept--[[@<string>]]({values = {'ok'}, 'ok'})\naccept--[[@<string>]]({values = {1}, 'ok'})\naccept--[[@<string>]]({values = {'ok'}, 1})",
                &[(ParamTypeMismatch, 1), (ParamTypeMismatch, 2)],
            );
        }

        #[test]
        fn test_record_unions_and_overloads_accept_a_complete_alternative() {
            assert_diagnostics(
                r#"
                ---@alias Options {values: table<integer, string>}|{values: table<integer, integer>}
                ---@param opts Options
                local function accept(opts) return opts end
                accept({values = {'ok'}})
                accept({values = {1}})
                accept({values = {true}})
                ---@type Options
                local valid = {values = {1}}
                ---@type Options
                local invalid = {values = {true}}

                ---@param opts {values: table<integer, string>}
                ---@param mode string
                ---@overload fun(opts: {values: table<integer, integer>}, mode: integer)
                local function overloaded(opts, mode) return opts, mode end
                overloaded({values = {'ok'}}, 'text')
                overloaded({values = {1}}, 1)
                overloaded({values = {1}}, 'text')
                return valid, invalid
                "#,
                &[ParamTypeMismatch, ParamTypeMismatch, AssignTypeMismatch],
            );
        }

        #[test]
        fn test_dictionary_record_alternatives_check_fields_and_indexes() {
            assert_diagnostics(
                r#"
                ---@class Entry
                ---@field label string
                ---@field [integer] 'ok'
                local _Entry = {}
                ---@param values table<string, integer>|Entry
                local function accept(values) return values end
                accept({count = 1})
                accept({label = 'ok'})
                accept({label = false})
                accept({label = 'ok', 'ok'})
                accept({label = 'ok', 1})
                accept({label = 'ok', 'bad'})
                ---@type table<string, Entry>
                local valid = {child = {label = 'ok', 'ok'}}
                ---@type table<string, Entry>
                local invalid = {child = {label = 'ok', 1}}
                return valid, invalid
                "#,
                &[
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                    AssignTypeMismatch,
                ],
            );
        }

        #[test]
        fn test_parentheses_preserve_nested_dictionary_literal_checks() {
            for value in ["{'ok'}", "({'ok'})", "(({'ok'}))"] {
                assert_diagnostics(
                    &format!(
                        "---@type table<string, table<integer, string>>\nlocal value = {{child = {value}}}\nreturn value"
                    ),
                    &[],
                );
            }
            for value in ["{1}", "({1})", "(({1}))"] {
                assert_diagnostics(
                    &format!(
                        r#"
                    ---@type table<string, table<integer, string>>
                    local value = {{child = {value}}}
                    ---@param values table<string, table<integer, string>>
                    local function accept(values) return values end
                    accept({{child = {value}}})
                    return value
                    "#
                    ),
                    &[ParamTypeMismatch, AssignTypeMismatch],
                );
            }
        }

        #[test]
        #[ntest::timeout(10000)]
        fn test_recursive_scalar_alias_rejects_table_arguments() {
            for value in ["{}", "{name = 1}"] {
                assert_diagnostics(
                    &format!(
                        r#"
                    ---@alias A A|string
                    ---@param value A
                    local function accept(value) return value end
                    accept({value})
                    "#,
                    ),
                    &[ParamTypeMismatch],
                );
            }
        }

        #[test]
        #[ntest::timeout(10000)]
        fn test_branching_recursive_aliases_do_not_repeat_union_checks() {
            assert_diagnostics(
                r#"
                ---@alias A B|C
                ---@alias B A|string
                ---@alias C A|table<integer, string>
                ---@param value A
                local function accept(value) return value end
                accept({'ok'})
                accept({true})
                "#,
                &[ParamTypeMismatch],
            );
        }

        #[test]
        #[ntest::timeout(10000)]
        fn test_recursive_record_aliases_check_the_concrete_alternative() {
            assert_diagnostics(
                r#"
                ---@class Entry
                ---@field label string
                ---@alias Record Record|Entry
                ---@type Record
                local valid = {label = 'ok'}
                ---@type Record
                local invalid = {label = 1}
                ---@type Record
                local missing = {}
                return valid, invalid, missing
                "#,
                &[AssignTypeMismatch, DiagnosticCode::MissingFields],
            );
        }

        #[test]
        #[ntest::timeout(10000)]
        fn test_growing_recursive_aliases_check_other_alternatives() {
            assert_diagnostics(
                r#"
                ---@alias Values<T> Values<T[]>|table<integer, string>
                ---@alias Key<T> Key<T[]>|integer
                ---@param values Values<string>
                ---@param keyed table<Key<string>, string>
                local function accept(values, keyed) return values, keyed end
                accept({'ok'}, {[1] = 'ok'})
                accept({true}, {[1] = 'ok'})
                accept({'ok'}, {name = 'bad'})
                accept({name = 'bad'}, {[1] = 'ok'})
                "#,
                &[
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                    DiagnosticCode::TypeComplexity,
                    DiagnosticCode::TypeComplexity,
                ],
            );
        }

        #[test]
        fn test_record_constructors_resolve_constant_expression_keys() {
            let source = r#"
                local key = 'text'
                ---@param opts {text: string}
                local function accept(opts) return opts end
                accept({[key] = 'ok'})
                pcall(accept, {[key] = 'ok'})
                ---@type {text: string}
                local value = {[key] = 'ok'}
                value = {[key] = 'updated'}
                accept(value)
                return value
            "#;
            assert_diagnostics(source, &[]);
            assert_diagnostics(
                &format!("---@diagnostic disable: missing-fields\n{source}"),
                &[],
            );
            assert_diagnostics(
                r#"
                local key = 'text'
                ---@param opts {text: string}
                local function accept(opts) return opts end
                accept({[key] = 1})
                ---@type {text: string}
                local value = {[key] = 1}
                return value
                "#,
                &[ParamTypeMismatch, AssignTypeMismatch],
            );
        }

        #[test]
        fn test_record_constructors_allow_absent_nullable_alias_fields() {
            assert_diagnostics(
                r#"
                ---@alias OptionalText string|nil
                ---@param opts {text: OptionalText}
                local function accept(opts) return opts end
                accept({})
                pcall(accept, {})
                ---@type {text: OptionalText}
                local value = {}
                value = {}
                accept({text = 1})
                ---@type {text: OptionalText}
                local invalid = {text = 1}
                ---@type {text: string}
                local missing = {}
                return value, invalid, missing
                "#,
                &[
                    ParamTypeMismatch,
                    AssignTypeMismatch,
                    DiagnosticCode::MissingFields,
                ],
            );
        }

        #[test]
        fn test_constructor_keys_use_the_first_call_result() {
            assert_diagnostics(
                r#"
                ---@return 'text', integer
                local function name() return 'text', 1 end
                local function index() return 1.0, 'unused' end
                ---@param opts {text: string}
                local function accept(opts) return opts end
                accept({[name()] = 'ok'})
                ---@type {text: string}
                local record = {[name()] = 'ok'}
                ---@type table<integer, string>
                local dictionary = {[index()] = 'ok'}
                ---@type table<integer, string>
                local invalid = {[index()] = false}
                return record, dictionary, invalid
                "#,
                &[AssignTypeMismatch],
            );
        }

        #[test]
        fn test_record_constructors_resolve_integral_float_keys() {
            assert_diagnostics(
                r#"
                local key = 1.0
                ---@class NumericRecord
                ---@field [1] string
                ---@type NumericRecord
                local named = {[key] = 'ok'}
                ---@type {[1]: string}
                local record = {[key] = 'ok'}
                ---@type {[1]: string}
                local invalid = {[key] = false}
                return named, record, invalid
                "#,
                &[AssignTypeMismatch],
            );
        }

        #[test]
        fn test_required_record_fields_include_final_call_results() {
            assert_diagnostics(
                r#"
                local function pair() return 'ok', 1 end
                ---@class Pair
                ---@field [1] string
                ---@field [2] integer
                ---@type Pair
                local named = {pair()}
                ---@type {[1]: string, [2]: integer}
                local record = {pair()}
                ---@param opts {[1]: string, [2]: integer}
                local function accept(opts) return opts end
                accept({pair()})
                accept({pair(), false})
                ---@type {[1]: string, [2]: integer}
                local missing = {(pair())}
                return named, record, missing
                "#,
                &[ParamTypeMismatch, DiagnosticCode::MissingFields],
            );
        }

        #[test]
        fn test_whole_table_arguments_use_stable_fields() {
            assert_diagnostics(
                r#"
                ---@class Record
                ---@field text string
                local _Record = {}
                ---@param opts Record
                local function record(opts) return opts end
                ---@param opts {text: 'ok'}
                local function literal(opts) return opts end
                ---@param opts table<string, 'ok'>
                local function dictionary(opts) return opts end

                local value = {}
                value.text = 'ok'
                record(value)
                pcall(record, value)
                literal(value)
                pcall(literal, value)
                dictionary(value)
                literal({text = value.text})
                dictionary({text = value.text})
                "#,
                &[ParamTypeMismatch, ParamTypeMismatch, ParamTypeMismatch],
            );
        }

        #[test]
        fn test_optional_fields_stay_optional_in_arguments_and_returns() {
            for declaration in [
                "---@type {text?: string}\nlocal value = {}",
                "local value = {text = nil}",
            ] {
                assert_diagnostics(
                    &format!(
                        r#"
                ---@param record {{text: string}}
                local function record(record) return record end
                ---@return {{text: string}}
                local function run()
                    {declaration}
                    value.text = 'hello'
                    record({{text = value.text}})
                    record(value)
                    pcall(record, value)
                    return value
                end
                return run
                "#
                    ),
                    &[
                        ParamTypeMismatch,
                        ParamTypeMismatch,
                        DiagnosticCode::ReturnTypeMismatch,
                    ],
                );
            }
        }

        #[test]
        fn test_whole_table_assignment_updates_the_variable_flow_type() {
            assert_diagnostics(
                r#"
                ---@param opts {text: string}
                local function accept(opts) return opts end
                local value = {text = 'initial'}
                value = {text = 1}
                accept(value)
                return value
                "#,
                &[ParamTypeMismatch],
            );
        }

        #[test]
        fn test_record_references_use_metatable_members() {
            assert_diagnostics(
                r#"
                ---@param opts {text: string}
                local function accept(opts) return opts end
                local valid = setmetatable({}, {__index = {text = 'ok'}})
                accept(valid)
                pcall(accept, valid)
                ---@type {text: string}
                local copy = valid
                local invalid = setmetatable({}, {__index = {text = 1}})
                accept(invalid)
                pcall(accept, invalid)
                ---@type {text: string}
                local wrong = invalid
                return copy, wrong
                "#,
                &[ParamTypeMismatch, ParamTypeMismatch, AssignTypeMismatch],
            );
        }

        #[test]
        fn test_inline_metatables_supply_record_fields() {
            assert_diagnostics(
                r#"
                ---@param opts {text: string}
                local function accept(opts) return opts end
                accept(setmetatable({}, {__index = {text = 'ok'}}))
                pcall(accept, setmetatable({}, {__index = {text = 'ok'}}))
                ---@type {text: string}
                local copy = (setmetatable({}, {__index = {text = 'ok'}}))
                ---@type {item: {text: string}}
                local nested = {item = setmetatable({}, {__index = {text = 'ok'}})}
                ---@return {text: string}
                local function make()
                    return setmetatable({}, {__index = {text = 'ok'}})
                end
                accept(setmetatable({}, {__index = {text = 1}}))
                ---@type {text: string}
                local wrong = setmetatable({}, {__index = {text = 1}})
                return copy, nested, make, wrong
                "#,
                &[ParamTypeMismatch, AssignTypeMismatch],
            );
        }

        #[test]
        fn test_record_constructors_check_values_before_later_writes() {
            assert_diagnostics(
                r#"
                ---@diagnostic disable: missing-fields
                ---@type {text: string}
                local later = {}
                later.text = 'ok'

                local key = 'text'
                ---@type {text: string}
                local overwritten = {text = 'ok', [key] = 1}
                return later, overwritten
                "#,
                &[AssignTypeMismatch, AssignTypeMismatch],
            );
        }

        #[test]
        fn test_record_constructors_keep_absent_return_alternatives() {
            assert_diagnostics(
                r#"
                local function pair(full)
                    if full then return 'ok', 1 end
                    return 'ok'
                end
                ---@param opts {[1]: string, [2]: integer?}
                local function optional(opts) return opts end
                ---@param opts {[1]: string, [2]: integer}
                local function required(opts) return opts end
                optional({pair(true)})
                required({pair(true)})
                "#,
                &[ParamTypeMismatch],
            );
        }

        #[test]
        fn test_absent_fields_require_nullable_intersection_components() {
            assert_diagnostics(
                r#"
                ---@alias OptionalText (string|nil)&(integer|nil)
                ---@alias RequiredText (string|nil)&integer

                ---@class OptionalRecord
                ---@field text OptionalText

                ---@type OptionalRecord
                local named = {}
                ---@type {text: OptionalText}
                local record = {}
                ---@param opts {text: OptionalText}
                local function accept(opts) return opts end
                accept({})
                ---@type {text: RequiredText}
                local missing = {}
                return named, record, missing
                "#,
                &[DiagnosticCode::MissingFields],
            );
        }

        #[test]
        fn test_dictionary_records_require_declared_fields() {
            for (target, valid, missing) in [
                (
                    "{label: string, values: table<integer, string>}",
                    "{label = 'ok', values = {'ok'}}",
                    "{values = {'ok'}}",
                ),
                (
                    "{child: {label: string}, values: table<integer, string>}",
                    "{child = {label = 'ok'}, values = {'ok'}}",
                    "{child = {}, values = {'ok'}}",
                ),
            ] {
                assert_diagnostics(
                    &format!(
                        r#"
                        ---@param opts {target}
                        local function accept(opts) return opts end
                        accept({valid})
                        accept({missing})
                    "#
                    ),
                    &[DiagnosticCode::MissingFields],
                );
            }
        }

        #[test]
        fn test_generic_dictionary_records_preserve_optional_fields() {
            for alias in [
                "Options<T> {label: string, values?: table<string, T>}",
                "Options<T> T extends string and {label: string, values?: table<string, T>} or never",
            ] {
                assert_diagnostics(
                    &format!(
                        r#"
                        ---@alias {alias}
                        ---@param opts Options<string>
                        local function accept(opts) return opts end
                        accept({{label = 'ok'}})
                        accept({{label = 'ok', values = {{name = 'ok'}}}})
                        accept({{values = {{name = 'ok'}}}})
                        accept({{label = 'ok', values = {{name = 1}}}})
                    "#
                    ),
                    &[DiagnosticCode::MissingFields, ParamTypeMismatch],
                );
            }
        }

        #[test]
        fn test_dictionary_checks_each_variadic_argument() {
            assert_diagnostics(
                r#"
                ---@param ... table<integer, string>
                local function accept(...) return ... end
                accept({'a'}, {'b'})
                accept({'a'}, {name = 'b'})
                "#,
                &[ParamTypeMismatch],
            );
        }

        #[test]
        #[ntest::timeout(10000)]
        fn test_recursive_dictionary_keeps_checking_other_fields() {
            assert_diagnostics(
                r#"
                ---@alias Tree table<string, string|Tree>
                local root = {}
                root.child = root
                ---@param values Tree
                local function accept(values) return values end
                accept({child = root, label = 'ok'})
                accept({child = root, label = 1})
                accept({child = {label = 'ok'}})
                "#,
                &[ParamTypeMismatch],
            );
        }

        #[test]
        fn test_dictionary_values_can_call_generic_functions() {
            assert_diagnostics(
                r#"
                ---@generic T
                ---@param _seed T
                ---@param values table<string, T>
                ---@return table<string, T>
                local function identity(_seed, values) return values end
                ---@type table<string, table<string, string>>
                local valid = {child = identity('ok', {label = 'ok'})}
                ---@type table<string, table<string, string>>
                local invalid = {child = identity(1, {label = 1})}
                return valid, invalid
                "#,
                &[AssignTypeMismatch],
            );
        }

        #[test]
        fn test_dictionary_calls_use_explicit_and_inferred_generics() {
            assert_diagnostics(
                r#"
                ---@generic T
                ---@param values table<integer, T>
                local function explicit(values) return values end
                explicit--[[@<integer>]]({1})
                explicit--[[@<integer>]]({'a'})

                ---@generic T
                ---@param seed T
                ---@param values table<integer, T>
                local function inferred(seed, values) return seed, values end
                inferred(1, {2})
                inferred(1, {'a'})
                inferred--[[@<integer>]](1, {2})
                inferred--[[@<integer>]](1, {'a'})

                ---@generic T
                ---@param value T
                ---@param other T
                local function same(value, other) return value, other end
                same({}, {})
                same({}, 1)
                "#,
                &[
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                    ParamTypeMismatch,
                ],
            );
        }

        #[test]
        fn test_generic_class_arguments_preserve_field_constraints() {
            for (target, valid, invalid) in [
                ("Box<T>", "{value = 1}", "{value = 1, extra = true}"),
                ("Box<T>|nil", "{value = 1}", "{value = 1, extra = true}"),
                ("Dict<T>", "{1}", "{name = 1}"),
                ("Dict<T>|nil", "{1}", "{name = 1}"),
            ] {
                assert_diagnostics(
                    &format!(
                        r#"
                        ---@class Box<T>
                        ---@field value T
                        local _Box = {{}}
                        ---@class Dict<T>
                        ---@field [integer] T
                        local _Dict = {{}}

                        ---@generic T
                        ---@param values {target}
                        local function accept(values) return values end
                        accept({valid})
                        accept({invalid})
                        pcall(accept, {valid})
                        pcall(accept, {invalid})
                        "#
                    ),
                    &[DiagnosticCode::InjectField, DiagnosticCode::InjectField],
                );
            }
        }

        #[test]
        fn test_generic_table_method_arguments_use_the_declared_parameter() {
            assert_diagnostics(
                r#"
                local receiver = {}
                ---@param values table<integer, string>
                function receiver:accept(values) return self, values end
                receiver:accept({'a'})
                receiver.accept(receiver, {'a'})
                receiver.accept(receiver, {name = 'a'})
                receiver:accept({1})
                "#,
                &[ParamTypeMismatch, ParamTypeMismatch],
            );
        }

        #[test]
        fn test_generic_aliases_preserve_table_constraints() {
            for (alias, target, valid, wrong_value, wrong_key) in [
                (
                    "Values<T> table<integer, T>",
                    "Values<string>",
                    "{'ok'}",
                    "{true}",
                    "{name = 'bad'}",
                ),
                (
                    "Values<T> table<integer, T>",
                    "table<string, Values<string>>",
                    "{items = {'a'}}",
                    "{items = {1}}",
                    "{items = {name = 'a'}}",
                ),
                (
                    "Identity<T> T",
                    "Identity<Identity<table<integer, string>>>",
                    "{'ok'}",
                    "{true}",
                    "{name = 'bad'}",
                ),
                (
                    "Values<T> T extends string and table<integer, T> or table<string, T>",
                    "Values<string>",
                    "{'ok'}",
                    "{true}",
                    "{name = 'bad'}",
                ),
                (
                    "Options<T> {[K in 'values']: table<integer, T>}",
                    "Options<string>",
                    "{values = {'ok'}}",
                    "{values = {true}}",
                    "{values = {name = 'bad'}}",
                ),
                (
                    "Options<T> {[K in 'values']?: table<integer, T>}",
                    "Options<string>",
                    "{}",
                    "{values = {true}}",
                    "{values = {name = 'bad'}}",
                ),
            ] {
                assert_diagnostics(
                    &format!(
                        r#"
                    ---@alias {alias}
                    ---@type {target}
                    local _valid = {valid}
                    ---@type {target}
                    local _wrong_value = {wrong_value}
                    ---@type {target}
                    local _wrong_key = {wrong_key}
                    ---@param values {target}
                    local function accept(values) return values end
                    accept({valid})
                    accept({wrong_value})
                    accept({wrong_key})
                "#
                    ),
                    &[
                        ParamTypeMismatch,
                        ParamTypeMismatch,
                        AssignTypeMismatch,
                        AssignTypeMismatch,
                    ],
                );
            }
        }

        #[test]
        fn test_table_alternatives_must_accept_the_whole_literal() {
            for target in [
                "Values",
                "table<integer, string>|table<integer, integer>",
                "table<integer, integer>|table<integer, string>",
                "table<integer, string>|integer[]",
                "integer[]|table<integer, string>",
            ] {
                assert_diagnostics(
                    &format!(
                        r#"
                        ---@alias Values table<integer, string>|table<integer, integer>
                        ---@type {target}
                        local strings = {{'a'}}
                        ---@type {target}
                        local numbers = {{1}}
                        ---@type {target}
                        local mixed = {{'a', 1}}
                        ---@param values {target}
                        local function accept(values) return values end
                        accept({{'a'}})
                        accept({{1}})
                        accept({{'a', 1}})
                        return strings, numbers, mixed
                        "#
                    ),
                    &[ParamTypeMismatch, AssignTypeMismatch],
                );
            }
            assert_diagnostics(
                r#"
                ---@param values table<integer, integer>
                ---@overload fun(values: table<integer, string>)
                local function overloaded(values) return values end
                overloaded({'a'})
                overloaded({1, 'a'})
                ---@param values table<integer, table<integer, integer>|table<integer, string>>
                local function nested(values) return values end
                nested({{'a'}})
                nested({{1, 'a'}})
                "#,
                &[ParamTypeMismatch, ParamTypeMismatch],
            );
        }

        #[test]
        fn test_dictionary_constructors_check_keys_and_values() {
            for (target, valid, invalid) in [
                ("table<integer, string>", "{nil}", "{[1.5] = 'a'}"),
                (
                    "table<integer, string>",
                    "{[1.0] = 'a', [-1.0] = 'b', [(-2e0)] = 'c'}",
                    "{[1.5] = 'a'}",
                ),
                ("table<integer, string>", "{[-1.0] = 'a'}", "{[-1.5] = 'a'}"),
                ("table<number, string>", "{[1.5] = 'a'}", "{name = 'a'}"),
                (
                    "table<'allowed', string>",
                    "{allowed = 'a'}",
                    "{other = 'a'}",
                ),
                ("table<1, string>", "{[1.0] = 'a'}", "{[2] = 'a'}"),
                (
                    "table<'left'|'right', string>",
                    "{right = 'a'}",
                    "{other = 'a'}",
                ),
                (
                    "table<integer, table<integer, string>>",
                    "{{'a'}}",
                    "{{name = 'a'}}",
                ),
            ] {
                assert_diagnostics(
                    &format!(
                        r#"
                    ---@param values {target}
                    local function accept(values) return values end
                    accept({valid})
                    accept({invalid})
                    ---@type {target}
                    local valid = {valid}
                    ---@type {target}
                    local invalid = {invalid}
                    return valid, invalid
                "#
                    ),
                    &[ParamTypeMismatch, AssignTypeMismatch],
                );
            }
        }

        #[test]
        fn test_dictionary_literal_keys_require_known_values() {
            assert_diagnostics(
                r#"
                ---@param name string
                ---@param index integer
                ---@param known 'left'|'right'
                local function build(name, index, known)
                    ---@type table<'left'|'right', string>
                    local valid = {[known] = 'ok'}
                    ---@type table<'left'|'right', string>
                    local invalid_name = {[name] = 'bad'}
                    ---@type table<1, string>
                    local invalid_index = {[index] = 'bad'}
                    return valid, invalid_name, invalid_index
                end
                return build
                "#,
                &[AssignTypeMismatch, AssignTypeMismatch],
            );
        }

        #[test]
        #[ntest::timeout(10000)]
        fn test_recursive_literals_check_deep_leaves() {
            for (depth, leaf, expected) in [
                (20, "{label = 'ok'}", &[][..]),
                (20, "{label = true}", &[ParamTypeMismatch][..]),
                (120, "{label = 'ok'}", &[][..]),
                (120, "{label = true}", &[ParamTypeMismatch][..]),
            ] {
                let nested = format!("{}{}{}", "{child = ".repeat(depth), leaf, "}".repeat(depth));
                assert_diagnostics(
                    &format!(
                        r#"
                    ---@alias Tree<T> table<string, string|Tree<T[]>>
                    ---@param values Tree<string>
                    local function accept(values) return values end
                    accept({nested})
                "#
                    ),
                    expected,
                );
            }
        }

        #[test]
        #[ntest::timeout(10000)]
        fn test_cyclic_modules_keep_checking_other_fields() {
            for (alias, target) in [
                ("Tree table<string, string|Tree>", "Tree"),
                ("Tree<T> table<string, string|Tree<T[]>>", "Tree<string>"),
            ] {
                let mut ws = VirtualWorkspace::new_with_init_std_lib();
                ws.def_files(vec![
                    ("a.lua", "return {child = require('b')}"),
                    ("b.lua", "return {child = require('a')}"),
                    ("valid.lua", "return {label = 'ok'}"),
                    ("invalid.lua", "return false"),
                ]);
                assert_workspace_diagnostics(
                    &mut ws,
                    &format!(
                        r#"
                    ---@alias {alias}
                    ---@param values {target}
                    local function accept(values) return values end
                    local child = require('a')
                    accept({{child = child, label = 'ok'}})
                    accept({{child = child, label = 1}})
                    accept({{child = child, label = 'ok'}})
                    accept({{child = require('valid')}})
                    accept({{child = require('invalid')}})
                "#
                    ),
                    &[(ParamTypeMismatch, 6), (ParamTypeMismatch, 9)],
                );
            }
        }

        #[test]
        fn test_standard_table_declaration_keeps_dictionary_permissions() {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            let source = r#"
                ---@param values table<any, any>
                local function any_keys(values) return values end
                any_keys({'reset', 'file.txt'})
                any_keys({command = 'reset', [true] = 1})
                ---@param values table<integer, string>
                local function integer_keys(values) return values end
                integer_keys({'ok'})
                integer_keys({name = 'bad'})
                integer_keys({1})
                ---@type table<integer, string>
                local wrong_key = {name = 'bad'}
                ---@type table<integer, string>
                local wrong_value = {1}
                return wrong_key, wrong_value
            "#;
            let expected = [
                (ParamTypeMismatch, 8),
                (ParamTypeMismatch, 9),
                (AssignTypeMismatch, 11),
                (AssignTypeMismatch, 13),
            ];
            assert_workspace_diagnostics(&mut ws, source, &expected);
            for code in [
                AssignTypeMismatch,
                ParamTypeMismatch,
                DiagnosticCode::InjectField,
            ] {
                ws.analysis.diagnostic.enable_only(code);
                let expected: Vec<_> = expected
                    .iter()
                    .copied()
                    .filter(|(expected, _)| *expected == code)
                    .collect();
                assert_workspace_diagnostics(&mut ws, source, &expected);
            }
        }

        #[test]
        fn test_record_constructors_ignore_overwritten_nested_values() {
            for constructor in [
                "{item = {}, [key] = {required = 'ok'}}",
                "({item = {}, [key] = ({required = 'ok'})})",
            ] {
                assert_diagnostics(
                    &format!(
                        "local key = 'item'\n---@param opts {{item: {{required: string}}}}\nlocal function accept(opts) return opts end\naccept({constructor})\npcall(accept, {constructor})\n---@type {{item: {{required: string}}}}\nlocal value = {constructor}\nreturn value"
                    ),
                    &[],
                );
            }
        }

        #[test]
        fn test_object_index_signatures_allow_nil_entries() {
            assert_diagnostics(
                r#"
                ---@param opts {values: table<string, string>, [string]: string}
                local function accept(opts) return opts end
                accept({values = {}, label = nil})
                "#,
                &[],
            );
        }

        #[test]
        fn test_generic_table_overload_requires_the_whole_call_to_match() {
            assert_diagnostics(
                r#"
                ---@param values table<integer, string>
                ---@param mode string
                ---@overload fun(values: table<string, integer>, mode: integer)
                local function accept(values, mode) return values, mode end
                accept({'a'}, 'a')
                accept({count = 1}, 1)
                accept({count = 1}, 'a')
                "#,
                &[ParamTypeMismatch],
            );
        }
    }

    #[test]
    fn test_class_method_self_field_diagnostic() {
        let diagnostics = check_source(
            r#"
            ---@class LinkedTableA
            local M

            function M:init()
                self.a = 1
            end
            "#,
        );
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != DiagnosticCode::InjectField),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn test_class_method_self_field_reference_is_rejected() {
        let diagnostics = check_source(
            r#"
            ---@class LinkedTableA

            ---@type LinkedTableA
            local M

            function M:init()
                self.a = 1
            end
            "#,
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InjectField),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn test_table_literal_integral_float_keys() {
        let diagnostics = check_source(
            r#"
            ---@class Numeric
            ---@field [0] integer
            ---@field [1] integer
            ---@type Numeric
            local decimal = {[0.0] = 0, [1.0] = 1}
            ---@type Numeric
            local exponent = {[0e0] = 0, [1e0] = 1}

            ---@class LargeNumeric
            ---@field [9223372036854774784] integer
            ---@type LargeNumeric
            local large = {[9223372036854774784.0] = 1}
            ---@param value Numeric
            local function accept(value) return value end
            accept({[0.0] = 0, [1e0] = 1})
            return decimal, exponent, large
            "#,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn test_table_literal_integral_float_key_rejects_wrong_value() {
        for key in ["1", "9223372036854774784"] {
            let diagnostics = check_source(&format!(
                r#"
                ---@class Numeric
                ---@field [{key}]? integer
                ---@type Numeric
                local value = {{[{key}.0] = "bad"}}
                ---@param record Numeric
                local function accept(record) return record end
                accept({{[{key}.0] = "bad"}})
                return value
                "#,
            ));
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                vec![
                    DiagnosticCode::ParamTypeMismatch,
                    DiagnosticCode::AssignTypeMismatch
                ],
                "{key}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_table_literal_float_key_must_match_integer_key() {
        for (field, key) in [
            ("1", "1.5"),
            ("9223372036854775807", "9223372036854775808.0"),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@class Numeric
                ---@field [{field}]? integer
                ---@type Numeric
                local value = {{[{key}] = 1}}
                return value
                "#,
            ));
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                vec![DiagnosticCode::InjectField],
                "{key}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_table_literal_numeric_and_string_keys_are_distinct() {
        for (field, key) in [("1", "\"[1]\""), ("\"[1]\"", "1")] {
            let diagnostics = check_source(&format!(
                r#"
                ---@class Record
                ---@field [{field}] integer
                ---@type Record
                local value = {{[{key}] = 1}}
                return value
                "#,
            ));
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                vec![DiagnosticCode::InjectField, DiagnosticCode::MissingFields],
                "{field}, {key}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_table_literal_numeric_and_string_keys_inherited() {
        let diagnostics = check_source(
            r#"
            ---@class Numeric
            ---@field [1] integer
            ---@class Mixed: Numeric
            ---@field ["[1]"] integer
            ---@type Mixed
            local t = {["[1]"] = 1}
            return t
            "#,
        );
        assert_eq!(
            diagnostics
                .iter()
                .map(|d| (d.code, d.message.as_str()))
                .collect::<Vec<_>>(),
            vec![(
                DiagnosticCode::MissingFields,
                "Missing required fields: `[1]`"
            )],
            "{diagnostics:?}"
        );
    }

    #[test]
    fn test_table_literal_dictionary_index() {
        let diagnostics = check_source(
            r#"
            ---@class Dict
            ---@field [string] integer
            ---@type Dict
            local t = {foo = 1}
            ---@type Dict
            local empty = {}
            return t.foo, empty
            "#,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn test_table_literal_dictionary_index_inherited() {
        let diagnostics = check_source(
            r#"
            ---@class Dict
            ---@field [string] integer
            ---@class NamedDict: Dict
            ---@field required integer
            ---@type NamedDict
            local t = {required = 1, foo = 2}
            return t
            "#,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn test_table_literal_dictionary_index_rejects_wrong_key() {
        let diagnostics = check_source(
            r#"
            ---@class Dict
            ---@field [string] integer
            ---@type Dict
            local t = {[1] = 1}
            return t
            "#,
        );
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec![DiagnosticCode::InjectField],
            "{diagnostics:?}"
        );
    }

    #[test]
    fn test_table_literal_dictionary_index_rejects_wrong_value() {
        let diagnostics = check_source(
            r#"
            ---@class Dict
            ---@field [string] integer
            ---@type Dict
            local t = {foo = "bad"}
            return t
            "#,
        );
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec![DiagnosticCode::AssignTypeMismatch],
            "{diagnostics:?}"
        );
    }

    #[test]
    fn test_table_literal_dictionary_index_requires_named_field() {
        let diagnostics = check_source(
            r#"
            ---@class Dict
            ---@field [string] integer
            ---@field required integer
            ---@type Dict
            local t = {foo = 1}
            return t
            "#,
        );
        assert_eq!(
            diagnostics
                .iter()
                .map(|d| (d.code, d.message.as_str()))
                .collect::<Vec<_>>(),
            vec![(
                DiagnosticCode::MissingFields,
                "Missing required fields: `required`"
            )],
            "{diagnostics:?}"
        );
    }

    #[test]
    fn test_issue_195() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
            local ret = {} --- @type string[]
            local b
            ret[#ret + 1] = b
            ret[#ret + 1] = 'a'
        "#
        ));
    }

    #[test]
    fn test_inject_field() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
            ---@class test1

            ---@type test1
            local test
            test.a = 1

        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
            ---@class test2
            ---@field a number

            ---@type test2
            local test
            test.a = 1

        "#
        ));
    }

    #[test]
    fn test_class_def_dynamic_key_in_method_is_allowed() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
            ---@class TestBind
            Bind = {}

            ---@class TestFunc
            ---@field call_name string
            local M = {}

            function M:call()
                Bind[self.call_name] = 1
            end
        "#
        ));
    }

    #[test]
    fn test_super_table() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
            ---@class test1<T>: {[string]: number }, table<string, string>

            ---@type test1<string>
            local test

            test.a = "1"
        "#
        ));
    }

    #[test]
    fn test_object() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
            ---@type { [number]: number }
            local test2 = {
            }
            test2.a = 1
        "#
        ));
    }

    #[test]
    fn test_self() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
            ---@class Diagnostic.8_1
            ---@field a number
            local Test = {}

            function Test:name()
                self.a = 1
            end
        "#
        ));
    }

    #[test]
    fn test_any_key() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
            ---@type { [number]: number }
            local t

            t[any] = 1
        "#
        ));
    }

    #[test]
    fn test_issue_264() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
                local a = { 'a' }
                a[#a + 1] = 'b'

                ---@type string[]
                local b = { 'a' }
                b[#b + 1] = 'b'

                ---@type table<integer, string>
                local c = { 'a' }
                c[#c + 1] = 'b'

                ---@type { [integer]: string }
                local d = { 'a' }
                d[#d + 1] = 'b'
        "#
        ));
    }

    #[test]
    fn test_tuple() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
                local a = { 'a' }
                a[#a + 1] = 'b'
        "#
        ));

        // assert!(!ws.has_no_diagnostic(
        //     DiagnosticCode::InjectField,
        //     r#"
        //         ---@type [ 'a' ]
        //         local a = { 'a' }
        //         a[#a + 1] = 'b'
        // "#
        // ));
    }

    #[test]
    fn test_export() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "a.lua",
            r#"
            local export = {}

            export.a = 1

            return export
            "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
            local a = require("a")
            a.newField = 1
            "#,
        ));
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
            local a = require("a")
            a.a = 2
            "#,
        ));
    }

    #[test]
    fn test_export_2() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "a.lua",
            r#"
            return {
                a = 1
            }
            "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
            local a = require("a")
            a.newField = 1
            "#,
        ));
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
            local a = require("a")
            a.a = 2
            "#,
        ));
    }

    #[test]
    fn test_issue_660() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "a.lua",
            r#"
            --- @class (file) vim.var_accessor
            --- @field [string] any
            --- @field [integer] vim.var_accessor

            vim = {}

            ---@type vim.var_accessor
            vim.g = {}
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
            if vim.g.aaa then
                return
            end

            vim.g.aaa = true
            "#,
        ));
    }

    #[test]
    fn test_alias_unknown_allows_field_injection() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
                ---@alias Anything unknown

                ---@type Anything
                local a
                a.foo = 1
            "#
        ));
    }

    #[test]
    fn test_alias_object_and_unknown_table_initializers() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
                ---@alias Anything unknown

                ---@type Anything
                local a = { foo = 1 }
            "#
        ));
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::InjectField,
            r#"
                ---@alias Obj { x: number }

                ---@type Obj
                local a = { x = 1 }
            "#
        ));
    }
    #[test]
    fn test_partial_class_redeclaration_keeps_self_field() {
        let diagnostics = check_source(
            r#"
            ---@class (partial) Develop.AttrA
            local M = {}

            ---@class (partial) Develop.AttrA

            function M:__init()
                self.a = 1
            end
            "#,
        );
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != DiagnosticCode::InjectField),
            "{diagnostics:?}"
        );
    }
    #[test]
    fn test_cross_file_redeclaration_keeps_self_field() {
        let meta = "---@meta

---@class Ability
";
        let good = r#"
            ---@class (partial) Ability
            local M = {}

            function M:__init(id)
                self.id = id
            end
            "#;
        let mut ws = VirtualWorkspace::new();
        ws.def_file("aaa_meta.lua", meta);
        let good_id = ws.def_file("zzz_good.lua", good);

        let inject_count = |ws: &VirtualWorkspace| {
            ws.analysis
                .diagnose_file(good_id, tokio_util::sync::CancellationToken::new())
                .unwrap()
                .into_iter()
                .filter(|d| {
                    matches!(&d.code, Some(lsp_types::NumberOrString::String(c)) if c == "inject-field")
                })
                .count()
        };
        assert_eq!(inject_count(&ws), 0);

        // 重算结果应保持一致.
        let uri = ws.analysis.get_uri(good_id).expect("uri");
        ws.analysis.update_file_by_uri(&uri, Some(good.to_string()));
        assert_eq!(inject_count(&ws), 0);
    }

    #[test]
    fn test_record_fields_distinguish_optional_and_required_values() {
        let source = r#"
            ---@param opts {label?: string, values?: table<string, string>}
            local function optional(opts) return opts end
            ---@type table<string, string>?
            local values
            optional({})
            optional({label = nil, values = nil})
            optional({values = values})
            optional({values = {name = 'ok'}})

            ---@param opts {label: string, values: table<string, string>}
            local function required(opts) return opts end
            required({label = 'ok', values = {}})
            required({label = nil, values = {}})
            required({label = 'ok', values = nil})
            "#;
        let diagnostics = check_source(source);
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            [
                DiagnosticCode::ParamTypeMismatch,
                DiagnosticCode::ParamTypeMismatch,
            ],
            "{source}\n{diagnostics:?}"
        );
    }
}

// These tests load the standard library because its `table` declaration is
// what made dictionaries incorrectly look like closed records.

mod dictionary_constructor {
    use crate::{DiagnosticCode, VirtualWorkspace};
    use googletest::prelude::*;

    #[googletest::gtest]
    fn dictionary_constructor_accepts_keys_with_standard_library() {
        for declaration in ["local signals", "signals"] {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            let source = format!(
                r#"
            ---@class Signal
            ---@field name string
            KEY = 'depot'
            ---@type table<string, Signal>
            {declaration} = {{
                [KEY] = {{name = KEY}},
                literal = {{name = 'literal'}},
                ['bracket'] = {{name = 'bracket'}},
            }}
        "#
            );
            assert_that!(
                ws.has_no_diagnostic(DiagnosticCode::InjectField, &source),
                eq(true),
                "{declaration}"
            );
        }
    }

    #[googletest::gtest]
    fn dictionary_constructor_alias_and_call_argument() {
        for target in ["table<integer, string>", "Dictionary"] {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            ws.def("---@alias Dictionary table<integer, string>");
            let source = format!(
                r#"
            ---@param values {target}
            local function consume(values) end
            consume({{[1] = 'one', [2] = 'two'}})
            ---@type {target}
            local values = {{}}
            values = {{[3] = 'three'}}
        "#
            );
            assert_that!(
                ws.has_no_diagnostic(DiagnosticCode::InjectField, &source),
                eq(true),
                "{target}"
            );
        }
    }

    #[googletest::gtest]
    fn dictionary_constructor_does_not_disable_class_field_checks() {
        for target in ["Record", "Box<string>"] {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            ws.def(
                r#"
            ---@class Record
            ---@field name string
            ---@class Box<T>
            ---@field name T
        "#,
            );
            assert_that!(
                ws.has_no_diagnostic(
                    DiagnosticCode::InjectField,
                    &format!("---@type {target}\nlocal value = {{name = 'ok', extra = true}}")
                ),
                eq(false),
                "{target}"
            );
            assert_that!(
                ws.has_no_diagnostic(
                    DiagnosticCode::MissingFields,
                    &format!("---@type {target}\nlocal value = {{}}")
                ),
                eq(false),
                "{target}"
            );
        }
    }

    #[googletest::gtest]
    fn dictionary_constructor_preserves_user_defined_table_types() {
        for private in [false, true] {
            let mut ws = if private {
                VirtualWorkspace::new_with_init_std_lib()
            } else {
                VirtualWorkspace::new()
            };
            let modifier = if private { "(private) " } else { "" };
            let source = format!(
                "---@class {modifier}table<T>\n---@field name T\n---@type table<string>\nlocal value = {{name = 'ok', extra = true}}"
            );
            // has_no_diagnostic enables and matches exactly the requested code.
            assert_that!(
                ws.has_no_diagnostic(DiagnosticCode::InjectField, &source),
                eq(false),
                "{modifier}"
            );
        }
    }

    #[googletest::gtest]
    fn dictionary_constructor_compound_targets_keep_record_checks() {
        for target in [
            "table<string, integer> & Record",
            "Record & table<string, integer>",
        ] {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            ws.def(&format!(
                "---@class Record\n---@field name string\n---@alias Combined {target}"
            ));
            assert_that!(
                ws.has_no_diagnostic(
                    DiagnosticCode::MissingFields,
                    "---@type Combined\nlocal value = {}"
                ),
                eq(false),
                "{target}"
            );
            assert_that!(
                ws.has_no_diagnostic(
                    DiagnosticCode::InjectField,
                    "---@type Combined\nlocal value = {name = 'ok', extra = true}"
                ),
                eq(false),
                "{target}"
            );
        }
        for target in [
            "table<string, integer> | nil",
            "nil | table<string, integer>",
        ] {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            assert_that!(
                ws.has_no_diagnostic(
                    DiagnosticCode::InjectField,
                    &format!("---@type {target}\nlocal value = {{answer = 42}}")
                ),
                eq(true),
                "{target}"
            );
        }
    }
}
