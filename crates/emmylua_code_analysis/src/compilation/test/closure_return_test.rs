#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

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
}
