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
            (
                "{[1] = 'ok'}",
                vec![
                    DiagnosticCode::AssignTypeMismatch,
                    DiagnosticCode::MissingFields,
                ],
            ),
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

        use DiagnosticCode::ParamTypeMismatch;

        fn assert_diagnostics(source: &str, expected: &[DiagnosticCode]) {
            let diagnostics = check_source(source);
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                expected,
                "{source}\n{diagnostics:?}"
            );
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
                return value
                "#,
            ));
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                vec![DiagnosticCode::AssignTypeMismatch],
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

#[cfg(test)]
mod test_issue_1318 {
    use crate::{DiagnosticCode, VirtualWorkspace};

    const LINTER: &str = r#"
        ---@class Linter
        ---@field cmd string the command to run
        ---@field stdin boolean pass content via stdin
    "#;

    /// #1318: a literal typed `Partial<T>` (or a class inheriting it) may set any field of `T`.
    #[test]
    fn test_partial_literal_accepts_fields_of_t() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def_file("linter.lua", LINTER);
        for code in [
            "---@type Partial<Linter>\nlocal b = { cmd = 'x' }",
            "---@class SubPartial: Partial<Linter>\n---@field extra? integer\n\n---@type SubPartial\nlocal a = { cmd = 'x' }",
        ] {
            assert!(
                ws.has_no_diagnostic(DiagnosticCode::InjectField, code),
                "{code}"
            );
            assert!(
                ws.has_no_diagnostic(DiagnosticCode::MissingFields, code),
                "{code}"
            );
        }
    }
}
