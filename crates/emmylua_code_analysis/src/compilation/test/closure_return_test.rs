#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    fn assert_inferred_return_without_nil(code: &str) {
        let mut ws = VirtualWorkspace::new();

        ws.def(code);

        let ty = ws.expr_ty("result");
        let expected = ws.ty("integer");
        let nil = ws.ty("nil");
        assert!(ws.check_type(&ty, &expected));
        assert!(!ws.check_type(&ty, &nil));
    }

    #[test]
    fn test_flow() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        --- @return string[] stdout
        --- @return string? stderr
        local function foo() end

        --- @param _a string[]
        local function bar(_a) end

        local a = {}

        a = foo()

        b = a
        "#,
        );
        let ty = ws.expr_ty("b");
        let expected = ws.ty("string[]");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_issue_265() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
        local function bar()
            return ''
        end

        --- @return integer
        function foo()
            return bar() --[[@as integer]]
        end

        "#,
        ));
    }

    #[test]
    fn test_issue_464() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.check_code_for_namespace(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                ---@class D31
                ---@field func? fun(a:number, b:string):number

                ---@type D31
                local f = {
                    func = function(a, b)
                        return "a"
                    end,
                }
        "#,
        ));

        assert!(ws.check_code_for_namespace(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                ---@class D31
                ---@field func? fun(a:number, b:string):number

                ---@type D31
                local f = {
                    func = function(a, b)
                        return a
                    end,
                }
        "#,
        ));
    }

    #[test]
    fn test_inferred_return_preserves_never() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@return { y: number } & { y: string }
        local function impossible() end

        local function f()
            return impossible().y
        end

        result = f()
        "#,
        );

        assert_eq!(ws.expr_ty("result"), ws.ty("never"));
    }

    #[test]
    fn test_member_doc_return_preserves_never() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@return { y: number } & { y: string }
        local function impossible() end

        ---@class ClosureTest
        ---@field e fun(): never
        ---@field e fun(): never
        local Test

        function Test.e()
            return impossible().y
        end

        result = Test.e()
        "#,
        );

        assert_eq!(ws.expr_ty("result"), ws.ty("never"));
    }

    #[test]
    fn test_inferred_return_from_truthy_loops() {
        for code in [
            r#"
        local function f()
            while true do
                return 1
            end
        end

        result = f()
        "#,
            r#"
        local function f()
            while (true) do
                return 1
            end
        end

        result = f()
        "#,
            r#"
        local function f()
            while 1 == 1 do
                return 1
            end
        end

        result = f()
        "#,
            r#"
        local function f()
            while 1 do
                return 1
            end
        end

        result = f()
        "#,
            r#"
        local function f()
            while {} do
                return 1
            end
        end

        result = f()
        "#,
            r#"
        local function f()
            repeat
                return 1
            until true
        end

        result = f()
        "#,
            r#"
        local function f()
            repeat
                return 1
            until 1 == 1
        end

        result = f()
        "#,
            r#"
        local function f()
            repeat
                return 1
            until "done"
        end

        result = f()
        "#,
            r#"
        local function f()
            repeat
                return 1
            until function() end
        end

        result = f()
        "#,
            r#"
        local function f(a)
            repeat
                if a then
                    return 1
                end
            until true

            return 2
        end

        result = f()
        "#,
        ] {
            assert_inferred_return_without_nil(code);
        }
    }

    #[test]
    fn test_inferred_return_from_truthy_ifs() {
        for code in [
            r#"
        local function f()
            if 1 == 1 then
                return 1
            end
        end

        result = f()
        "#,
            r#"
        local function f()
            if 1 then
                return 1
            end
        end

        result = f()
        "#,
            r#"
        local function f()
            if {} then
                return 1
            end
        end

        result = f()
        "#,
            r#"
        local function f()
            if "done" then
                return 1
            end
        end

        result = f()
        "#,
        ] {
            assert_inferred_return_without_nil(code);
        }
    }

    #[test]
    fn test_inferred_return_from_infinite_repeat_does_not_assume_nil() {
        assert_inferred_return_without_nil(
            r#"
        local function f(a)
            repeat
                if a then
                    return 1
                end
            until false
        end

        result = f()
        "#,
        );
    }

    #[test]
    fn test_inferred_return_from_truthy_while_with_break_before_return() {
        assert_inferred_return_without_nil(
            r#"
        local function f(done)
            while true do
                if done then
                    break
                end
            end

            return 1
        end

        result = f()
        "#,
        );
    }

    #[test]
    fn test_inferred_return_from_infinite_repeat_with_break_before_return() {
        assert_inferred_return_without_nil(
            r#"
        local function f(done)
            repeat
                if done then
                    break
                end
            until false

            return 1
        end

        result = f()
        "#,
        );
    }

    #[test]
    fn test_return_flow_keeps_local_while_call_condition_dynamic() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        local should_enter_loop = function()
            return true
        end

        local function f()
            while should_enter_loop() do
                return 1
            end

            return ""
        end

        result = f()
        "#,
        );

        let ty = ws.expr_ty("result");
        let expected = ws.ty("integer|string");
        let nil = ws.ty("nil");
        assert!(ws.check_type(&ty, &expected));
        assert!(!ws.check_type(&ty, &nil));
    }

    #[test]
    fn test_return_flow_keeps_global_if_call_condition_dynamic() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        function should_take_branch()
            return true
        end

        local function f()
            if should_take_branch() then
                return 1
            else
                return ""
            end
        end

        result = f()
        "#,
        );

        let ty = ws.expr_ty("result");
        let expected = ws.ty("integer|string");
        let nil = ws.ty("nil");
        assert!(ws.check_type(&ty, &expected));
        assert!(!ws.check_type(&ty, &nil));
    }
}
