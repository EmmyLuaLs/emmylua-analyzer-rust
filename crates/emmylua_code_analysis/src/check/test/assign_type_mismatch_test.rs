#[cfg(test)]
mod tests {
    use crate::check::test::check_source;
    use crate::{DiagnosticCode, VirtualWorkspace};

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
                    return initialized, reassigned
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
        for (source, target) in [
            // Reject boolean, even though string and integer are accepted.
            ("string|integer|boolean", "string|number"),
            // Reject nil because the target does not accept it.
            ("true?", "boolean|string"),
            // Reject number because it can contain non-integer values.
            ("string|number", "string|integer"),
            // Reject integer when the target accepts only strings.
            ("string|integer", "string"),
        ] {
            let diagnostics = check_source(&format!(
                r#"
                ---@param value {source}
                local function use(value)
                    ---@type {target}
                    local target = value
                    return target
                end
                return use
                "#,
            ));
            assert_eq!(
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                vec![DiagnosticCode::AssignTypeMismatch],
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

        // `a` is already A's class table; `setmetatable({}, a)` yields a table with A as its metatable,
        // so it cannot directly serve as the runtime table for subclass B (B's table needs B's members/metatable).
        assert!(!ws.has_no_diagnostic_in_namespace(
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
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
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

                    if ok == false then
                        error(result)
                    end

                    if not ok then
                        error(result)
                    end

                    ---@type integer
                    local narrowed = result
                end
            "#,
        ));
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

            ---@param a A | B | C
            ---@return boolean
            function condition(a)
                return true
            end
            "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@type fun(a: A | B | C | D): boolean
            local tmp = condition
            "#,
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@type fun(a: A | B): boolean
            local tmp = condition
            "#,
        ));
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
}
