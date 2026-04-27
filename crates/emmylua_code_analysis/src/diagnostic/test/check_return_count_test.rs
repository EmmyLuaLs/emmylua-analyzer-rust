#[cfg(test)]
mod tests {
    use crate::{DiagnosticCode, VirtualWorkspace};

    fn assert_missing_return_ok(code: &str) {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(DiagnosticCode::MissingReturn, code));
    }

    fn assert_missing_return_error(code: &str) {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.check_code_for(DiagnosticCode::MissingReturn, code));
    }

    #[test]
    fn test_1() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::RedundantReturnValue,
            r#"
            ---@class Completion2.A
            ---@field event fun(aaa)

            ---@type Completion2.A
            local a = {
                event = function(aaa)
                    return aaa
                end,
            }
        "#
        ));
    }

    #[test]
    fn test_2() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
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

        assert!(!ws.check_code_for(
            DiagnosticCode::MissingReturnValue,
            r#"
            ---@return number
            local function test()
                return
            end
        "#
        ));

        assert!(ws.check_code_for(
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
    fn test_missing_return_value_variadic() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
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

        assert!(ws.check_code_for(
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
        assert!(!ws.check_code_for(
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

        assert!(ws.check_code_for(
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

        assert!(!ws.check_code_for(
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
    fn test_not_return_anno() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
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

        assert!(ws.check_code_for(
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

        assert!(ws.check_code_for(
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

        assert!(!ws.check_code_for(
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

        assert!(!ws.check_code_for(
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

        assert!(ws.check_code_for(
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

        assert!(ws.check_code_for(
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

        assert!(!ws.check_code_for(
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

        assert!(ws.check_code_for(
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

        assert!(!ws.check_code_for(
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

        assert!(!ws.check_code_for(
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

        assert!(ws.check_code_for(
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

        assert!(!ws.check_code_for(
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

        assert!(!ws.check_code_for(
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

        assert!(!ws.check_code_for(
            DiagnosticCode::MissingReturn,
            r#"
            ---@return number
            local function f()
            end
            "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::MissingReturn,
            r#"

            ---@return number?
            local function f()
            end
            "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::MissingReturn,
            r#"
            ---@return any ...
            local function f()
            end
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::MissingReturn,
            r#"
            ---@return number
            function F()
                X = 1
            end
            "#
        ));

        assert!(!ws.check_code_for(
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

        assert!(!ws.check_code_for(
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

        assert!(ws.check_code_for(
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

        assert!(!ws.check_code_for(
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

        assert!(ws.check_code_for(
            DiagnosticCode::MissingReturn,
            r#"
            ---@return any
            function F()
                X = 1
            end
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::MissingReturn,
            r#"
            ---@return any, number
            function F()
                X = 1
            end
            "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::MissingReturn,
            r#"
            ---@return any, any
            function F()
                X = 1
            end
            "#
        ));

        assert!(ws.check_code_for(
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
    fn test_missing_return_rejects_dynamic_while_with_infinite_body_before_return() {
        assert_missing_return_error(
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
    fn test_missing_return_rejects_dynamic_while_with_break_or_infinite_body_before_return() {
        assert_missing_return_error(
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
    fn test_missing_return_rejects_stalling_numeric_for_before_return() {
        assert_missing_return_error(
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
    fn test_missing_return_rejects_stalling_generic_for_before_return() {
        assert_missing_return_error(
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
        assert!(ws.check_code_for_namespace(
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

        assert!(ws.check_code_for(
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

        assert!(ws.check_code_for(
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

        assert!(ws.check_code_for(
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

        assert!(ws.check_code_for(
            DiagnosticCode::MissingReturn,
            r#"
                pcall(function() end)
        "#,
        ));
    }

    #[test]
    fn test_missing_return_1() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(!ws.check_code_for(
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
        assert!(ws.check_code_for(
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

        assert!(ws.check_code_for(
            DiagnosticCode::RedundantReturnValue,
            r#"
                --- @return nil
                local function f1()
                    return nil
                end
        "#,
        ));
    }

    #[test]
    fn test_call_arg_closure_expected_missing_return() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::MissingReturn,
            r#"
                ---@param cb fun(): string
                local function call(cb)
                end

                call(function()
                end)
        "#,
        ));
    }
}
