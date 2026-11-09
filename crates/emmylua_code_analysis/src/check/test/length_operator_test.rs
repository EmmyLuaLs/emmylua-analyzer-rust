#[cfg(test)]
mod tests {
    use crate::check::test::{check_source, count_by_code};
    use crate::{DiagnosticCode, LuaType, VirtualWorkspace};

    fn assert_diagnostic_count(source: &str, code: DiagnosticCode, expected: usize) {
        let diagnostics = check_source(source);
        assert_eq!(
            count_by_code(&diagnostics, DiagnosticCode::SyntaxError),
            0,
            "{source}\n{diagnostics:?}"
        );
        assert_eq!(
            count_by_code(&diagnostics, code),
            expected,
            "{source}\n{diagnostics:?}"
        );
    }

    #[test]
    fn test_builtin_and_aliased_lengths_are_integer() {
        for operand in [
            "string",
            "string[]",
            "{name:string}",
            "string|string[]",
            "Builtin",
            "List<string>",
            "Identity<List<string>>",
            "Identity<Identity<string>>",
            "Identity<string>|Identity<string[]>",
            "TextOnly<string|integer>",
            "TextOrList<string>",
            "TextOrList<integer>",
            "TextOrList<string|integer>",
            "TextOrList<Input>",
            "TextOrList<Identity<Input>>",
            "Select<integer[], Input>",
        ] {
            let mut ws = VirtualWorkspace::new();
            ws.def(&format!(
                r#"
                ---@alias Builtin string|string[]
                ---@alias Input string|integer
                ---@alias List<T> T[]
                ---@alias Identity<T> T
                ---@alias TextOnly<T> T extends string and T or never
                ---@alias TextOrList<T> T extends string and T or T[]
                ---@alias Select<U,T> T extends string and T or U
                ---@type {operand}
                local value
                length = #value
                "#
            ));
            assert_eq!(ws.expr_ty("length"), LuaType::Integer, "{operand}");
        }
        let mut ws = VirtualWorkspace::new();
        assert_eq!(ws.expr_ty("#'abc'"), LuaType::Integer);
        assert_eq!(ws.expr_ty("#{'a', 'b'}"), LuaType::Integer);
    }

    #[test]
    fn test_array_length_works_in_returns_arguments_and_indexes() {
        let diagnostics = check_source(
            r#"
            ---@param n integer
            local function consume(n) return n end
            ---@param values string[]
            ---@return integer
            local function count(values)
                consume(#values)
                local last = values[#values]
                return #values
            end
            count({'a'})
            "#,
        );
        for code in [
            DiagnosticCode::ReturnTypeMismatch,
            DiagnosticCode::ParamTypeMismatch,
            DiagnosticCode::UndefinedField,
        ] {
            assert_eq!(count_by_code(&diagnostics, code), 0, "{diagnostics:?}");
        }
    }

    #[test]
    fn test_conditional_alias_length_returns_integer() {
        assert_diagnostic_count(
            r#"
            ---@alias Input string|integer
            ---@alias TextOrList<T> T extends string and T or T[]
            ---@param value TextOrList<Input>
            ---@return integer
            local function length(value)
                return #value -- The alias resolves to string|integer[].
            end
            return length
            "#,
            DiagnosticCode::ReturnTypeMismatch,
            0,
        );
    }

    #[test]
    fn test_length_preserves_custom_operator_results() {
        for (operand, expected) in [
            ("Sized", "string"),
            ("Sized|string[]", "string|integer"),
            ("LengthInput", "string|integer"),
            ("Identity<Sized>|Identity<string[]>", "string|integer"),
            ("Select<Sized, string|integer>", "string|integer"),
        ] {
            let mut ws = VirtualWorkspace::new();
            ws.def(&format!(
                r#"
                ---@class Sized
                ---@operator len: string
                local Sized = {{}}
                ---@alias LengthInput Sized|string[]
                ---@alias Identity<T> T
                ---@alias Select<U,T> T extends string and T or U
                ---@type {operand}
                local value
                length = #value
                "#
            ));
            let expected = ws.ty(expected);
            assert_eq!(ws.expr_ty("length"), expected, "{operand}");
        }
    }

    #[test]
    fn test_length_preserves_metatable_class_operator() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@class Sized
            ---@operator len: string
            local Sized = {}
            local value = setmetatable({}, { __index = Sized })
            length = #value
            "#,
        );
        assert_eq!(ws.expr_ty("length"), LuaType::String);
    }

    #[test]
    fn test_length_reports_invalid_operands_and_return_types() {
        for (operand, result) in [
            ("boolean", "integer"),
            ("string|boolean", "integer"),
            ("string[]", "string"),
        ] {
            let source = format!(
                r#"
                ---@param value {operand}
                ---@return {result}
                local function count(value) return #value end
                return count
                "#
            );
            assert_diagnostic_count(&source, DiagnosticCode::ReturnTypeMismatch, 1);
        }
    }

    #[test]
    #[ntest::timeout(5000)]
    fn test_recursive_alias_lengths_keep_operand_type() {
        for (aliases, operand) in [
            ("---@alias Input Input", "Input"),
            ("---@alias Input Other\n---@alias Other Input", "Input"),
            ("---@alias Input<T> Input<T[]>", "Input<string>"),
            ("---@alias Input<T> string|Input<T[]>", "Input<string>"),
            ("---@alias Input<T> Input<T[]>|string", "Input<string>"),
            (
                "---@alias Input<T> T extends string and Input<T[]> or Input<T>",
                "Input<string>",
            ),
            (
                "---@alias Input<T> Other<T[]>\n---@alias Other<T> Input<T[]>",
                "Input<string>",
            ),
            (
                "---@alias Input<T> Input<{left:T,right:T}>",
                "Input<string>",
            ),
            (
                "---@alias Input<T> Filter<Input<T[]>>\n---@alias Filter<T> T extends string and T or T[]",
                "Input<string>",
            ),
            (
                "---@alias Input<T> Filter<Input<T>>\n---@alias Filter<T> T extends string and T or T[]",
                "Input<string>",
            ),
            (
                "---@alias Input<T> Match<{outer:Input<T[]>}>\n---@alias Match<T> T extends {outer:{inner:infer U}} and U or boolean",
                "Input<string>",
            ),
        ] {
            let mut ws = VirtualWorkspace::new();
            ws.def(&format!(
                r#"
                {aliases}
                ---@type {operand}
                local value
                length = #value
                "#
            ));
            let expected = ws.ty(operand);
            assert_eq!(ws.expr_ty("length"), expected, "{aliases}");
        }
    }

    #[test]
    #[ntest::timeout(5000)]
    fn test_length_resolves_long_finite_alias_chains() {
        let mut source = (0..512)
            .map(|index| format!("---@alias Alias{index} Alias{}\n", index + 1))
            .collect::<String>();
        source.push_str("---@alias Alias512 string\n---@type Alias0\nlocal value\nlength = #value");
        let mut ws = VirtualWorkspace::new();
        ws.def(&source);
        assert_eq!(ws.expr_ty("length"), LuaType::Integer);
    }

    #[test]
    fn test_stable_local_array_has_a_last_element_after_a_nonempty_guard() {
        for guard in [
            "if not a[1] then return end",
            "if a[1] == nil then return end",
            "if nil == (a)[1] then return end",
            "if not (a[1] ~= nil) then return end",
            "if #a <= 0 then return end",
            "if #a < 1 then return end",
            "if #a ~= 1 then return end",
            "if #a == 0 then return end",
            "if 0 >= #a then return end",
            "if not (#(a) > 0) then return end",
            "if not a[1] then return end; if #a > 10 then return end",
            "if #a > 10 then return end; if not a[1] then return end",
            "if check then if not a[1] then return end else if #a < 1 then return end end",
        ] {
            let source = format!(
                r#"
                ---@param check boolean
                local function last(check)
                    ---@type string[]
                    local a = {{'first'}}
                    {guard}
                    ---@type string
                    local value = ((a))[(#(a))]
                    return value
                end
                return last
                "#
            );
            assert_diagnostic_count(&source, DiagnosticCode::AssignTypeMismatch, 0);
        }
    }

    #[test]
    fn test_stable_last_element_preserves_the_element_type() {
        for (array, element) in [
            ("(string|false)[]", "string|false"),
            ("string[]|integer[]", "string|integer"),
        ] {
            let mut ws = VirtualWorkspace::new();
            ws.def(&format!(
                r#"
                ---@type {array}
                local a = {{'first'}}
                if a[1] == nil then return end
                last = a[#a]
                "#,
            ));
            assert_eq!(ws.expr_ty("last"), ws.ty(element));
        }
    }

    #[test]
    fn test_last_element_proof_preserves_existing_cast_types() {
        assert_diagnostic_count(
            r#"
            ---@type string[]
            local a = {'first'}
            if not a[1] then return end
            ---@cast a integer[]
            ---@type string
            local value = a[#a]
            "#,
            DiagnosticCode::AssignTypeMismatch,
            1,
        );
    }

    #[test]
    fn test_last_element_requires_a_nonempty_guard_on_every_path() {
        for guard in [
            "",
            "if #a <= 1 then else return end", // An upper bound allows zero.
            "if a[1] ~= nil then return end",  // The remaining path has no first element.
            "if not a[2] then return end",     // A later element does not exclude a hole at 1.
            "if not other[1] then return end",
            "if check then if not a[1] then return end end",
        ] {
            let source = format!(
                r#"
                ---@param check boolean
                local function last(check)
                    ---@type string[]
                    local a = {{'first'}}
                    ---@type string[]
                    local other = {{'other'}}
                    {guard}
                    ---@type string
                    local value = a[#a]
                    return value
                end
                return last
                "#
            );
            assert_diagnostic_count(&source, DiagnosticCode::AssignTypeMismatch, 1);
        }
    }

    #[test]
    fn test_any_write_alias_capture_or_escape_disables_the_last_element_proof() {
        for use_array in [
            "a = {}",
            "a[1] = nil",
            "(a)[2] = 'changed'",
            "a[i] = nil",
            "mutate(a)",
            "a:clear()",
            "a.clear(a)",
            "local alias = a; alias[1] = nil",
            "local stored = {values = a}",
            "local edit = function() a[1] = nil end",
            "local equal = a == peer",
        ] {
            for before_read in [true, false] {
                let read = "---@type string\nlocal value = a[#a];";
                let body = if before_read {
                    format!("{use_array}\n{read}")
                } else {
                    format!("{read}\n{use_array}")
                };
                let source = format!(
                    r#"
                    ---@param mutate fun(values:string[])
                    ---@param peer table
                    ---@param i integer
                    local function last(mutate, peer, i)
                        ---@type string[]
                        local a = {{'first'}}
                        if not a[1] then return end
                        {body}
                        return value
                    end
                    return last
                    "#
                );
                assert_diagnostic_count(&source, DiagnosticCode::AssignTypeMismatch, 1);
            }
        }
    }

    #[test]
    fn test_unrelated_operations_do_not_affect_a_stable_local_array() {
        assert_diagnostic_count(
            r#"
            ---@param notify fun()
            ---@param n integer
            local function last(notify, n)
                ---@type string[]
                local a = {'first'}
                if not a[1] then return end
                notify()
                local next = n + 1
                local first = a[1]
                if a == nil then return end
                ---@type string
                local value = a[#a]
                return value, first, next
            end
            return last
            "#,
            DiagnosticCode::AssignTypeMismatch,
            0,
        );
    }

    #[test]
    fn test_external_tables_and_metatables_keep_nullable_last_elements() {
        for declaration in [
            "", // The parameter has no ownership or metatable guarantee.
            "---@type string[]\nlocal original = {'first'}; local a = original",
            "---@type string[]\nlocal a = setmetatable({'first'}, {__len = function() return 0 end})",
        ] {
            let source = format!(
                r#"
                ---@param a string[]
                local function last(a)
                    {declaration}
                    if not a[1] then return end
                    ---@type string
                    local value = a[#a]
                    return value
                end
                return last
                "#
            );
            assert_diagnostic_count(&source, DiagnosticCode::AssignTypeMismatch, 1);
        }
    }

    #[test]
    fn test_last_element_proof_does_not_cross_a_function_boundary() {
        assert_diagnostic_count(
            r#"
            ---@type string[]
            local a = {'first'}
            if not a[1] then return end
            return function()
                ---@type string
                local value = a[#a]
                return value
            end
            "#,
            DiagnosticCode::AssignTypeMismatch,
            1,
        );
    }

    #[test]
    fn test_last_element_proof_requires_the_same_array_and_exact_length() {
        for index in ["#other", "#a + 1"] {
            let source = format!(
                r#"
                ---@type string[]
                local a = {{'first'}}
                ---@type string[]
                local other = {{'other'}}
                if not a[1] then return end
                ---@type string
                local value = a[{index}]
                "#
            );
            assert_diagnostic_count(&source, DiagnosticCode::AssignTypeMismatch, 1);
        }
    }

    #[test]
    fn test_upper_bounds_do_not_prove_constant_indexes_exist() {
        for (condition, expected) in [
            ("#a <= 1", 1),
            ("1 >= #a", 1),
            ("#a < 2", 1),
            ("2 > #a", 1),
            ("not (#a > 1)", 1),
            ("#a == 1", 0),
            ("1 == #a", 0),
            ("#a >= 1", 0),
            ("1 <= #a", 0),
            ("#a > 0", 0),
            ("0 < #a", 0),
            ("not (#a < 1)", 0),
        ] {
            let source = format!(
                r#"
                ---@param a string[]
                local function first(a)
                    if {condition} then
                        ---@type string
                        local value = a[1]
                        return value
                    end
                end
                return first
                "#
            );
            assert_diagnostic_count(&source, DiagnosticCode::AssignTypeMismatch, expected);
        }
    }
}
