#[cfg(test)]
mod tests {
    use crate::check::test::check_source;
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_object_parent_field_uses_child_optionality() {
        for (parent, child, expected) in [
            ("integer", "?", vec![]),
            ("integer?", "", vec![DiagnosticCode::MissingFields]),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@class Record: {{foo: {parent}}}
                ---@field foo{child} integer
                ---@type Record
                local value = {{}}
                return value
                "#,
            ));
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                expected,
                "{parent}, {child}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_missing_fields() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class test
            ---@field a number

            ---@type test
            local test = {}
        "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class test1
            ---@field a number

            ---@class test2: test1

            ---@type test
            local test = {}
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class test3
            ---@field a number

            ---@class test4: test3
            ---@field b number

            ---@type test
            local test = {
                a = 1,
                b = 2,
            }
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class test5
            ---@field a? number

            ---@class test6: test5
            ---@field b number

            ---@type test5
            local test = {
                b = 2,
            }
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class test7
            ---@field a number

            local test = {}
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class test8
            ---@field a number
            ---@type test8
            local test
        "#
        ));
    }

    #[test]
    fn test_override_optional() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class test1
            ---@field a? number

            ---@class test2: test1
            ---@field a number

            ---@type test2
            local test = {
            }
        "#
        ));
    }

    #[test]
    fn test_generic() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class test1<T>
            ---@field a number

            ---@type test1<string>
            local test = {
            }
        "#
        ));
    }

    #[test]
    fn test_object_type() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class test1: { a: number }

            ---@type test1
            local test = {
            }
        "#
        ));
    }

    #[test]
    fn test_issue_262() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
--- @class D11.Opts
--- @field field? any

--- @param opts D11.Opts
local function foo(opts) end

foo({})
        "#
        ));
    }

    #[test]
    fn test_1() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
                ---@type table
                local a = {}

                print(a[1])
        "#
        ));
    }

    #[test]
    fn test_issue_296() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::UndefinedField,
            r#"
                ---@generic T
                ---@param table table
                ---@param metatable {__index: T}
                ---@return T
                local function abc(table, metatable) end

                ---@class B
                local B

                --- @return B
                function newB()
                    local self = abc({}, { __index = B })
                    self:notmethod()
                    return self
                end
        "#
        ));
    }

    #[test]
    fn test_issue_302() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
                ---@class data
                data = {}
                data.raw = {}
                data.is_demo = false

                --- @param _self data
                function data.extend(_self, _otherdata)
                -- Impl
                end

                data:extend({
                {
                    type = "item",
                    name = "my-item",
                },
                })
        "#
        ));
    }

    #[test]
    fn test_issue_449() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class D31.A
            ---@field public a string

            ---@class D31.B
            ---@field public b string


            ---@param ab D31.A & D31.B
            local function f(ab)
            end

            f({})
        "#
        ));
    }

    #[test]
    fn test_union_table_generic() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@class RingBuffer<T>
        ---@field a number

        ---@class LiveList<T>
        ---@field list table<integer, T> | RingBuffer<T>
        "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@type LiveList
            local LiveList

            LiveList.list = {}
        "#
        ));
    }

    #[test]
    fn test_union_enum_array_does_not_report_missing_fields() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@enum NiceEnum
            local GOODGUYS = {
                superman = 1
            }

            ---@alias Evil string | NiceEnum

            ---@param evils Evil | (Evil[])
            local function do_evil(evils) end

            do_evil({ "hi", "dead" })
        "#
        ));
    }

    #[test]
    fn test_union_array_named_table_still_reports_missing_fields() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class Foo
            ---@field name string

            ---@param foo Foo | Foo[]
            local function use_foo(foo) end

            use_foo({ typo = 1 })
        "#
        ));
    }

    #[test]
    fn test_union_array_empty_table_does_not_report_missing_fields() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class Foo
            ---@field name string

            ---@param foo Foo | Foo[]
            local function use_foo(foo) end

            use_foo({})
        "#
        ));
    }

    #[test]
    fn test_alias_field_optionality() {
        for (field_type, optional) in [
            ("PersonAge", true),
            ("Maybe<integer>", true),
            ("Wrapped<integer>", true),
            ("Identity<unknown>", true),
            ("Optional<string>", true),
            ("Identity<integer>", false),
            ("Optional<integer>", false),
            ("Identity<Node>", false),
        ] {
            let mut ws = VirtualWorkspace::new();
            assert_eq!(
                ws.has_no_diagnostic(
                    DiagnosticCode::MissingFields,
                    &format!(
                        r#"
            ---@alias PersonAge
            --- | integer
            --- | nil
            ---@alias Maybe<T> T | nil
            ---@alias Wrapped<T> Maybe<T> | T
            ---@alias Identity<T> T
            ---@alias Optional<T> T extends string and nil or T

            ---@class Node
            ---@field next Node

            ---@class Person
            ---@field name string
            ---@field age {field_type}

            ---@type Person
            local person = {{ name = "123" }}
        "#
                    )
                ),
                optional,
                "field type: {field_type}"
            );
        }
    }
}
