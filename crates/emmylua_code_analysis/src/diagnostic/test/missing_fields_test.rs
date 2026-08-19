#[cfg(test)]
mod tests {

    use lsp_types::NumberOrString;
    use tokio_util::sync::CancellationToken;

    use crate::{DiagnosticCode, VirtualWorkspace};

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
    fn test_nested_tables_are_checked_from_outermost_table() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class MissingFieldsNestedInner
            ---@field required string

            ---@class MissingFieldsNestedOuter
            ---@field child MissingFieldsNestedInner

            ---@param value MissingFieldsNestedOuter
            local function consume(value) end

            consume({ child = {} })
            "#,
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
    fn test_union_alias_does_not_report_other_branch_fields() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class MissingFieldsAliasA
            ---@field a string

            ---@class MissingFieldsAliasB
            ---@field b string

            ---@alias MissingFieldsAliasUnion MissingFieldsAliasA | MissingFieldsAliasB

            ---@type MissingFieldsAliasUnion
            local value = { a = "a" }
            "#,
        ));
    }

    #[test]
    fn test_generic_union_alias_does_not_report_other_branch_fields() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class MissingFieldsGenericAliasA<T>
            ---@field a T

            ---@class MissingFieldsGenericAliasB<T>
            ---@field b T

            ---@alias MissingFieldsGenericAliasUnion<T> MissingFieldsGenericAliasA<T> | MissingFieldsGenericAliasB<T>

            ---@type MissingFieldsGenericAliasUnion<string>
            local value = { a = "a" }
            "#,
        ));
    }

    #[test]
    fn test_multiline_union_alias_does_not_report_other_branch_fields() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class MissingFieldsMultilineAliasA
            ---@field a string

            ---@class MissingFieldsMultilineAliasB
            ---@field b string

            ---@alias MissingFieldsMultilineAliasUnion
            ---| MissingFieldsMultilineAliasA
            ---| MissingFieldsMultilineAliasB

            ---@type MissingFieldsMultilineAliasUnion
            local value = { a = "a" }
            "#,
        ));
    }

    #[test]
    fn test_multiline_union_nil_field_is_optional() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@alias PersonAge
            --- | integer
            --- | nil

            ---@class Person
            ---@field name string
            ---@field age PersonAge

            ---@type Person
            local person = { name = "123" }
        "#
        ));
    }

    #[test]
    fn test_generic_inherited_fields_are_instantiated() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class MissingFieldsBase<T>
            ---@field value T

            ---@class MissingFieldsChild<T>: MissingFieldsBase<T>
            ---@field own T
            "#,
        );

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@type MissingFieldsChild<string>
            local value = {}
            "#,
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@type MissingFieldsChild<string>
            local value = {
                value = "value",
                own = "own",
            }
            "#,
        ));
    }

    #[test]
    fn test_intersection_required_field_wins_over_optional() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class MissingFieldsOptional
            ---@field value? string

            ---@class MissingFieldsRequired
            ---@field value string

            ---@param value MissingFieldsOptional & MissingFieldsRequired
            local function consume(value) end

            consume({})
            "#,
        ));
    }

    #[test]
    fn test_most_specific_optional_field_overrides_required_parent() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class MissingFieldsRequiredParent
            ---@field value string

            ---@class MissingFieldsOptionalChild: MissingFieldsRequiredParent
            ---@field value? string

            ---@type MissingFieldsOptionalChild
            local value = {}
            "#,
        ));
    }

    #[test]
    fn test_generic_alias_nil_field_is_optional() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@alias MissingFieldsMaybe<T> T | nil

            ---@class MissingFieldsBox<T>
            ---@field value MissingFieldsMaybe<T>

            ---@type MissingFieldsBox<string>
            local value = {}
            "#,
        ));
    }

    #[test]
    fn test_index_only_type_does_not_report_missing_fields() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class MissingFieldsIndexOnly
            ---@field [string] number

            ---@type MissingFieldsIndexOnly
            local value = {}
            "#,
        ));
    }

    #[test]
    fn test_cyclic_inheritance_does_not_overflow_missing_fields_walk() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingFields,
            r#"
            ---@class MissingFieldsCycleA: MissingFieldsCycleB
            ---@class MissingFieldsCycleB: MissingFieldsCycleA

            ---@type MissingFieldsCycleA
            local value = {}
            "#,
        ));
    }

    #[test]
    fn test_call_argument_comment_does_not_shift_missing_fields_range() {
        let mut ws = VirtualWorkspace::new();
        ws.analysis
            .diagnostic
            .enable_only(DiagnosticCode::MissingFields);
        let file_id = ws.def(
            r#"---@class A
---@field a 1
---@class B
---@field b 2
---@class C
---@field c 3

---@param a A
---@param b B
---@param c C
local function test(a, b, c) end

test(
    -- What
    {},
    {},
    {}
)"#,
        );
        let code = Some(NumberOrString::String(
            DiagnosticCode::MissingFields.get_name().to_string(),
        ));
        let diagnostics = ws
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .unwrap_or_default()
            .into_iter()
            .filter(|diagnostic| diagnostic.code == code)
            .collect::<Vec<_>>();

        assert_eq!(diagnostics.len(), 3, "{diagnostics:#?}");
        assert_eq!(diagnostics[0].range.start.line, 14, "{diagnostics:#?}");
        assert!(diagnostics[0].message.contains("`a`"), "{diagnostics:#?}");
        assert_eq!(diagnostics[1].range.start.line, 15, "{diagnostics:#?}");
        assert!(diagnostics[1].message.contains("`b`"), "{diagnostics:#?}");
        assert_eq!(diagnostics[2].range.start.line, 16, "{diagnostics:#?}");
        assert!(diagnostics[2].message.contains("`c`"), "{diagnostics:#?}");
    }
}
