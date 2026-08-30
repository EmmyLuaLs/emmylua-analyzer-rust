#[cfg(test)]
mod tests {

    use crate::{DiagnosticCode, VirtualWorkspace};
    use googletest::prelude::*;

    #[test]
    fn test_missing_fields() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class test
            ---@field a number

            ---@type test
            local test = {}
        "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class test1
            ---@field a number

            ---@class test2: test1

            ---@type test
            local test = {}
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class test7
            ---@field a number

            local test = {}
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
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

    // 嵌套子表已验证的字段不得满足父层同名字段的缺失判断.
    #[test]
    fn test_verified_fields_are_isolated_between_nesting_levels() {
        let mut ws = VirtualWorkspace::new();

        // 子表提供了 `name`, 但父层的 `name` 仍然缺失.
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@class IsolatedChild
            ---@field name string

            ---@class IsolatedParent
            ---@field name string
            ---@field child IsolatedChild

            ---@param value IsolatedParent
            local function consume(value) end

            consume({ child = { name = "n" } })
            "#,
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@class IsolatedChild
            ---@field name string

            ---@class IsolatedParent
            ---@field name string
            ---@field child IsolatedChild

            ---@param value IsolatedParent
            local function consume(value) end

            consume({ name = "n", child = { name = "n" } })
            "#,
        ));
    }

    // 同层级的兄弟子表之间, 已验证字段同样互不影响.
    #[test]
    fn test_verified_fields_are_isolated_between_sibling_tables() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class SiblingItem
            ---@field name string

            ---@class SiblingHolder
            ---@field first SiblingItem
            ---@field second SiblingItem

            ---@param value SiblingHolder
            local function consume(value) end

            consume({ first = { name = "a" }, second = {} })
            "#,
        ));
    }

    // 变参展开路径验证通过的位置同样应计入已提供字段.
    #[test]
    fn test_variadic_expansion_marks_index_fields_verified() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class VariadicTuple
            ---@field [1] string
            ---@field [2] string

            ---@return string, string
            local function two() return "a", "b" end

            ---@type VariadicTuple
            local value = { two() }
            "#,
        ));
    }

    #[test]
    fn test_expr_const_key_matches_canonical_member_name() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class ExprKeyTarget
            ---@field [16] string

            local KEY = 16

            ---@type ExprKeyTarget
            local value = { [KEY] = "a" }
            "#,
        ));
    }

    // 泛型别名包裹的类应支持字段级检查与缺失检查.
    #[test]
    fn test_generic_alias_to_class_supports_missing_fields() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class AliasBoxTarget<T>
            ---@field value T

            ---@alias AliasBoxWrap<T> AliasBoxTarget<T>

            ---@type AliasBoxWrap<string>
            local value = {}
            "#,
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@type AliasBoxWrap<string>
            local value = { value = "a" }
            "#,
        ));
    }

    #[test]
    fn test_issue_262() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::ParamTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@type LiveList<string>
            local LiveList

            LiveList.list = {}
        "#
        ));
    }

    #[test]
    fn test_union_enum_array_does_not_report_missing_fields() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::ParamTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
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
    fn test_union_reports_missing_when_no_branch_satisfied() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"

            ---@class UnionBranchA
            ---@field x number

            ---@class UnionBranchB
            ---@field y number

            ---@param value UnionBranchA | UnionBranchB
            local function consume(value) end

            consume({})
            "#,
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class UnionBranchA
            ---@field x number

            ---@class UnionBranchB
            ---@field y number

            ---@param value UnionBranchA | UnionBranchB
            local function consume(value) end

            consume({ x = 1 })
            "#,
        ));
    }

    #[test]
    fn test_union_cross_branch_field_conflict_falls_back_to_type_diagnostic() {
        let mut ws = VirtualWorkspace::new();

        let source = r#"
            ---@class CrossConflictA
            ---@field x number
            ---@field y string

            ---@class CrossConflictB
            ---@field x string
            ---@field y number

            ---@param value CrossConflictA | CrossConflictB
            local function consume(value) end

            consume({ x = "s", y = "s" })
        "#;
        assert!(!ws.has_no_diagnostic(DiagnosticCode::ParamTypeMismatch, source));
        assert!(ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, source));
    }

    #[test]
    fn test_union_alias_does_not_report_other_branch_fields() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@type MissingFieldsChild<string>
            local value = {}
            "#,
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::ParamTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
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
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class MissingFieldsCycleA: MissingFieldsCycleB
            ---@class MissingFieldsCycleB: MissingFieldsCycleA

            ---@type MissingFieldsCycleA
            local value = {}
            "#,
        ));
    }

    #[gtest]
    fn test_call_argument_comment_does_not_shift_missing_fields_range() {
        let mut ws = VirtualWorkspace::new();
        let diagnostics = ws.get_diagnostics(
            DiagnosticCode::ParamTypeMismatch,
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

        assert_that!(diagnostics.len(), eq(1));
        assert_that!(diagnostics[0].range.start.line, eq(14));
        assert_that!(
            diagnostics[0].message,
            eq(
                "Argument of type `table` is not assignable to parameter of type `A`.\n  Type `table` is missing the `a` field from type `A`."
            )
        );
    }

    #[gtest]
    fn test_union_target_selects_best_matching_branch_missing_fields() {
        let mut ws = VirtualWorkspace::new();
        let diagnostics = ws.get_diagnostics(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class UnionA
            ---@field a string
            ---@field extra_a string

            ---@class UnionB
            ---@field b string
            ---@field extra_b string

            ---@type UnionA|UnionB
            local t = { a = "hello" }
            "#,
        );
        assert_that!(diagnostics.len(), eq(1));
        assert_that!(
            diagnostics[0].message,
            eq("Cannot assign `{ a = \"hello\" }` to `(UnionA|UnionB)`.
  Type `{ a = \"hello\" }` is missing the `extra_a` field from type `UnionA`.")
        );
    }

    // 与分支仅共享可选字段时, 仍按该分支上报缺失的必填字段.
    #[gtest]
    fn test_union_shared_optional_field_still_reports_missing_required() {
        let mut ws = VirtualWorkspace::new();
        let diagnostics = ws.get_diagnostics(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class OptSharedA
            ---@field a string
            ---@field extra? number

            ---@class OptSharedB
            ---@field b string

            ---@type OptSharedB|OptSharedA
            local t = { extra = 1 }
            "#,
        );
        assert_that!(diagnostics.len(), eq(1));
        assert_that!(
            diagnostics[0].message,
            eq("Cannot assign `{ extra = 1 }` to `(OptSharedB|OptSharedA)`.
  Type `{ extra = 1 }` is missing the `a` field from type `OptSharedA`.")
        );
    }

    // 缺失字段超过 4 个时只显示前 4 个属性.
    #[gtest]
    fn test_missing_fields_limits_to_four_properties() {
        let mut ws = VirtualWorkspace::new();
        let diagnostics = ws.get_diagnostics(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class ManyFields
            ---@field a string
            ---@field b string
            ---@field c string
            ---@field d string
            ---@field e string
            ---@field f string

            ---@type ManyFields
            local t = {}
            "#,
        );
        assert_that!(diagnostics.len(), eq(1));
        assert_that!(
            diagnostics[0].message,
            eq(
                "Cannot assign `table` to `ManyFields`.\n  Type `table` is missing the following fields from type `ManyFields`: a, b, c, d, and 2 more."
            )
        );
    }
}
