#[cfg(test)]
mod tests {
    use crate::check::test::{check_source, count_by_code};
    use crate::{DiagnosticCode, VirtualWorkspace};

    fn assert_missing_return_ok(code: &str) {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(DiagnosticCode::MissingReturn, code));
    }

    fn assert_missing_return_error(code: &str) {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(DiagnosticCode::MissingReturn, code));
    }

    #[test]
    fn test_contextual_callback_return_count() {
        let source = r#"
            ---@class Completion2.A
            ---@field event fun(aaa)
            ---@field query fun(bbb): unknown

            ---@type Completion2.A
            local a = {
                event = function(aaa)
                    return aaa
                end,
                query = function(bbb)
                    return bbb
                end,
            }
        "#;
        let diagnostics = check_source(source);
        let extras: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == DiagnosticCode::RedundantReturnValue)
            .map(|diagnostic| &source[diagnostic.range])
            .collect();
        assert_eq!(extras, ["aaa"]);
    }

    #[test]
    fn test_2() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingReturnValue,
            r#"
            ---@return integer a
            ---@return integer b
            ---@return integer ...
            local function foo()
                return 1
            end
        "#
        ));
    }

    #[test]
    fn test_missing_return_value() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingReturnValue,
            r#"
            ---@return number
            local function test()
                return
            end
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturnValue,
            r#"
            ---@return number
            ---@return string
            local function test()
                return 1, "2"
            end
        "#
        ));
    }

    #[test]
    fn test_forwarded_missing_return_value_with_parameter_docs() {
        let diagnostics = check_source(
            r#"
            ---@param flag boolean
            local function no_result(flag) end

            ---@return string
            local function forward()
                return no_result(false)
            end
            return forward
            "#,
        );

        assert_eq!(
            count_by_code(&diagnostics, DiagnosticCode::MissingReturnValue),
            1
        );
        assert_eq!(
            count_by_code(&diagnostics, DiagnosticCode::ReturnTypeMismatch),
            0
        );
    }

    #[test]
    fn test_nonfinal_call_supplies_one_return_value() {
        let diagnostics = check_source(
            r#"
            ---@param flag boolean
            local function two(flag)
                return 'hello', 1
            end

            ---@param flag boolean
            local function none(flag) end

            ---@return string, integer
            local function forward_two()
                return two(false), 2
            end

            ---@return nil, integer
            local function forward_none()
                return none(false), 2
            end
            return forward_two, forward_none
            "#,
        );

        assert_eq!(
            count_by_code(&diagnostics, DiagnosticCode::RedundantReturnValue),
            0
        );
        assert_eq!(
            count_by_code(&diagnostics, DiagnosticCode::MissingReturnValue),
            0
        );
    }

    #[test]
    fn test_named_return_pack_count() {
        let source = r#"
            ---@alias Pair [string, integer]...

            ---@return Pair
            local function pair()
                return 'hello', 1
            end

            ---@return Pair
            local function forward()
                return pair()
            end

            ---@return Pair
            local function truncate()
                return pair(), 2
            end

            ---@return Pair
            local function short()
                return 'hello'
            end

            ---@return Pair
            local function long()
                return 'hello', 1, true
            end

            ---@return Pair
            local function wrong_type()
                return 'hello', 'oops'
            end

            ---@alias MaybePair ([string]...) | ([string, integer]...)
            ---@param more boolean
            ---@return MaybePair
            local function maybe_pair(more)
                if more then return 'hello', 1 end
                return 'hello'
            end

            ---@param more boolean
            ---@return string, integer?
            local function forward_maybe_pair(more)
                return maybe_pair(more)
            end

            return forward, truncate, short, long, wrong_type, forward_maybe_pair
        "#;
        let diagnostics = check_source(source);

        let snippets_for = |code: DiagnosticCode| {
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == code)
                .map(|diagnostic| &source[diagnostic.range])
                .collect::<Vec<_>>()
        };
        assert_eq!(
            snippets_for(DiagnosticCode::MissingReturnValue),
            ["return 'hello'"]
        );
        assert_eq!(snippets_for(DiagnosticCode::RedundantReturnValue), ["true"]);
        assert_eq!(snippets_for(DiagnosticCode::ReturnTypeMismatch), ["'oops'"]);
    }

    #[test]
    fn test_missing_return_value_variadic() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturnValue,
            r#"
            --- @return integer?
            --- @return integer?
            function bar()
                return
            end
        "#
        ));
    }

    #[test]
    fn test_return_expr_list_missing() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturnValue,
            r#"
            ---@return integer, integer
            local function foo()
            end

            ---@return integer, integer
            local function bar()
                return foo()
            end
        "#
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingReturnValue,
            r#"
            ---@return integer
            local function foo()
            end

            ---@return integer, integer
            local function bar()
                return foo()
            end
        "#
        ));
    }

    #[test]
    fn test_dots() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::RedundantReturnValue,
            r#"
            ---@return number, any...
            local function test()
                return 1, 2, 3
            end
        "#
        ));
    }

    #[test]
    fn test_redundant_return_value() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::RedundantReturnValue,
            r#"
            ---@return number
            local function test()
                return 1, 2
            end
        "#
        ));
    }

    #[test]
    fn test_assert_optional_return_is_not_redundant() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::RedundantReturnValue,
            r#"
            --- @return string
            function foo()
                local res --- @type string?
                return assert(res)
            end
        "#
        ));
    }

    #[test]
    fn test_not_return_anno() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturnValue,
            r#"
            local function baz()
                if true then
                    return
                end
                return 1
            end
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::RedundantReturnValue,
            r#"
            function bar(a)
                return tonumber(a)
            end
        "#
        ));
    }

    #[test]
    fn test_return_expr_list_redundant() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::RedundantReturnValue,
            r#"
            ---@return integer, integer
            local function foo()
            end

            ---@return integer, integer
            local function bar()
                return foo()
            end
        "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::RedundantReturnValue,
            r#"
            ---@return integer, integer, integer
            local function foo()
            end

            ---@return integer, integer
            local function bar()
                return foo()
            end
        "#
        ));
    }

    #[test]
    fn test_missing_return() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            local A
            ---@return number
            function F()
                while A do
                    if A then
                        return 1
                    end
                end
            end
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            ---@return number
            local function foo()
                while true do
                    return 1
                end
            end
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            local A
            ---@return number
            local function foo()
                if A then
                    A = false
                end

                while true do
                    return 1
                end
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            local A
            ---@return number
            local function foo()
                while true do
                    if A then
                        break
                    end

                    return 1
                end
            end
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            ---@return number
            local function foo(A)
                while A do
                end

                return 1
            end
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            local A
            ---@return number
            function F()
                while true do
                    if A then
                        return 1
                    end
                end
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            local A
            ---@return number
            local function foo()
                while true do
                    if A then
                        do
                            break
                        end
                    end

                    while true do
                        return 1
                    end
                end
            end
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            local A
            ---@return number
            local function foo()
                while true do
                    return 1

                    if A then
                        break
                    end
                end
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            local A
            ---@return number
            local function foo()
                while true do
                    if A then
                        break
                    end

                    while true do
                        return 1
                    end
                end
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            local A
            ---@return number
            function F()
                while A do
                    if A then
                        return 1
                    else
                        return 2
                    end
                end
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            ---@return number
            local function f()
            end
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"

            ---@return number?
            local function f()
            end
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            ---@return any ...
            local function f()
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            ---@return number
            function F()
                X = 1
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            local A
            ---@return number
            function F()
                if A then
                    return 1
                end
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            local A, B
            ---@return number
            function F()
                if A then
                    return 1
                elseif B then
                    return 2
                end
            end
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            local A, B
            ---@return number
            function F()
                if A then
                    return 1
                elseif B then
                    return 2
                else
                    return 3
                end
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            local A, B
            ---@return number
            function F()
                if A then
                elseif B then
                    return 2
                else
                    return 3
                end
            end
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            ---@return any
            function F()
                X = 1
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            ---@return any, number
            function F()
                X = 1
            end
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            ---@return any, any
            function F()
                X = 1
            end
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            local A
            ---@return number
            function F()
                for _ = 1, 10 do
                    if A then
                        return 1
                    end
                end
                error('should not be here')
            end
            "#
        ));
    }

    #[test]
    fn test_missing_return_accepts_truthy_loops() {
        for code in [
            r#"
            ---@return number
            local function foo()
                while (true) do
                    return 1
                end
            end
            "#,
            r#"
            ---@return number
            local function foo()
                while 1 == 1 do
                    return 1
                end
            end
            "#,
            r#"
            ---@return number
            local function foo()
                while 1 do
                    return 1
                end
            end
            "#,
            r#"
            ---@return number
            local function foo()
                while {} do
                    return 1
                end
            end
            "#,
            r#"
            ---@return number
            local function foo()
                repeat
                    return 1
                until true
            end
            "#,
            r#"
            ---@return number
            local function foo()
                repeat
                    return 1
                until 1 == 1
            end
            "#,
            r#"
            ---@return number
            local function foo()
                repeat
                    return 1
                until "done"
            end
            "#,
            r#"
            ---@return number
            local function foo()
                repeat
                    return 1
                until function() end
            end
            "#,
            r#"
            ---@return number
            local function foo(a)
                repeat
                    if a then
                        return 1
                    end
                until true

                return 2
            end
            "#,
        ] {
            assert_missing_return_ok(code);
        }
    }

    #[test]
    fn test_missing_return_accepts_truthy_ifs() {
        for code in [
            r#"
            ---@return number
            local function foo()
                if 1 == 1 then
                    return 1
                end
            end
            "#,
            r#"
            ---@return number
            local function foo()
                if 1 == 1 then
                    return 1
                else
                    while pred() do
                    end
                end
            end
            "#,
            r#"
            ---@return number
            local function foo()
                if {} then
                    return 1
                end
            end
            "#,
        ] {
            assert_missing_return_ok(code);
        }
    }

    #[test]
    fn test_missing_return_keeps_local_if_call_condition_dynamic() {
        assert_missing_return_error(
            r#"
            local should_take_branch = function()
                return true
            end

            ---@return number
            local function foo()
                if should_take_branch() then
                    return 1
                end
            end
            "#,
        );
    }

    #[test]
    fn test_missing_return_accepts_dynamic_parent_walk_before_return() {
        // This matches the current `MissingReturn` check: runtime-dependent
        // loops are allowed when the function still reaches a later `return`.
        assert_missing_return_ok(
            r#"
            ---@class Node
            local Node = {}

            ---@return Node?
            function Node:parent()
                return nil
            end

            ---@param node Node?
            ---@return integer
            local function get_indent(node)
                local indent = 0

                while node do
                    node = node:parent()
                end

                return indent
            end
            "#,
        );
    }

    #[test]
    fn test_missing_return_accepts_dynamic_repeat_before_return() {
        assert_missing_return_ok(
            r#"
            ---@return number
            local function foo(done)
                repeat
                until done

                return 1
            end
            "#,
        );
    }

    #[test]
    fn test_missing_return_accepts_truthy_while_with_break_before_return() {
        assert_missing_return_ok(
            r#"
            ---@return number
            local function foo(done)
                while true do
                    if done then
                        break
                    end
                end

                return 1
            end
            "#,
        );
    }

    #[test]
    fn test_missing_return_accepts_non_terminating_truthy_while() {
        assert_missing_return_ok(
            r#"
            --- @param ready boolean
            --- @return string
            function foo(ready)
                while true do
                    if ready then
                        return 'ready'
                    end
                end

                error('unreachable')
            end
            "#,
        );
    }

    #[test]
    fn test_missing_return_accepts_infinite_repeat_with_break_before_return() {
        assert_missing_return_ok(
            r#"
            ---@return number
            local function foo(done)
                repeat
                    if done then
                        break
                    end
                until false

                return 1
            end
            "#,
        );
    }

    #[test]
    fn test_missing_return_accepts_dynamic_while_with_infinite_body_before_return() {
        assert_missing_return_ok(
            r#"
            ---@return number
            local function foo(a)
                while a do
                    while true do
                    end
                end

                return 1
            end
            "#,
        );
    }

    #[test]
    fn test_missing_return_accepts_dynamic_while_with_break_or_infinite_body_before_return() {
        assert_missing_return_ok(
            r#"
            ---@return number
            local function foo(a, b)
                while a do
                    if b then
                        break
                    end

                    while true do
                    end
                end

                return 1
            end
            "#,
        );
    }

    #[test]
    fn test_missing_return_accepts_non_terminating_numeric_for_before_return() {
        assert_missing_return_ok(
            r#"
            ---@return number
            local function foo()
                for _ = 1, 10 do
                    while true do
                    end
                end

                return 1
            end
            "#,
        );
    }

    #[test]
    fn test_missing_return_accepts_non_terminating_generic_for_before_return() {
        assert_missing_return_ok(
            r#"
            local function iter(_, done)
                if done then
                    return nil
                end

                return true, true
            end

            ---@return number
            local function foo()
                for _ in iter, nil, nil do
                    while true do
                    end
                end

                return 1
            end
            "#,
        );
    }

    #[test]
    fn test_issue_236() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::MissingReturn,
            r#"
            --- @param a number
            --- @return integer
            function foo(a)
            if a == 0 then
                return 0
            end

            if a < 0 then
                return 0
            else
                return 0
            end
            end
            "#
        ));
    }

    #[test]
    fn test_miss_return_1() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@meta
        ---@class oslib
        os = {}
        ---@param code integer
        ---@param close? boolean
        ---@return integer
        function os.exit(code, close) end

        "#,
        );

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            local M = {}
            M.oldOsExit = os.exit

            os.exit = function(...)
            end
        "#,
        ));
    }

    #[test]
    fn test_miss_return_2() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
            os.exit = function(...)
            end
        "#,
        ));
    }

    #[test]
    fn test_miss_return_3() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
                ---@class Point

                ---@class Unit

                ---@class Player

                ---@class CreateData
                ---@field target Point|Unit
                ---@field owner? Unit|Player


                ---@param data CreateData
                ---@return string
                local function send(data)
                    if not data.owner then
                        data.owner = ""
                    end
                    if data.target then
                        return ""
                    else
                        return ""
                    end
                end
        "#,
        ));
    }

    #[test]
    fn test_pcall_missing_return() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
                pcall(function() end)
        "#,
        ));
    }

    #[test]
    fn test_missing_return_1() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingReturn,
            r#"
                ---@generic T
                ---@param field T
                ---@return T
                ---@return T
                local function test(field)
                end
        "#,
        ));
    }

    #[test]
    fn test_issue_567() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::RedundantReturnValue,
            r#"
                local function fnil()
                end

                local f --- @type fun(c: fun())
                f(function()
                    return fnil()
                end)
        "#,
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::RedundantReturnValue,
            r#"
                --- @return nil
                local function f1()
                    return nil
                end
        "#,
        ));
    }
}
