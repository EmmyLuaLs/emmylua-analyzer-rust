#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_closure_return() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        --- @generic T, U
        --- @param arr T[]
        --- @param op fun(item: T, index: integer): U
        --- @return U[]
        function map(arr, op)
        end
        "#,
        );

        let ty = ws.expr_ty(
            r#"
        map({ 1, 2, 3 }, function(item, i)
            return tostring(item)
        end)
        "#,
        );
        let expected = ws.ty("string[]");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_issue_140_1() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@class Object
        
        ---@class T
        local inject2class ---@type (Object| T)?
        if jsonClass then
            if inject2class then
                A = inject2class
            end
        end
        "#,
        );

        let ty = ws.expr_ty("A");
        let expected = ws.ty("Object | T");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_issue_140_2() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
        local msgBody ---@type { _hgQuiteMsg : 1 }?
        if not msgBody or not msgBody._hgQuiteMsg then
        end
        "#
        ));
    }

    #[test]
    fn test_issue_140_3() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
        local SELF ---@type unknown
        if SELF ~= nil then
            SELF:OnDestroy()
        end
        "#
        ));
    }

    #[test]
    fn test_issue_107() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
        ---@type {bar?: fun():string}
        local props
        if props.bar then
            local foo = props.bar()
        end

        if type(props.bar) == 'function' then
            local foo = props.bar() 
        end

        local foo = props.bar and props.bar() or nil 
        "#
        ));
    }
    
    #[test]
    fn test_issue_100() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
        local f = io.open('', 'wb')
        if not f then
            error("Could not open a file")
        end

        f:write('')
        "#
        ));
    }

    #[test]
    fn test_issue_93() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
        local text    --- @type string[]?
        if staged then
            local text1 --- @type string[]?
            text = text1
        else
            local text2 --- @type string[]?
            text = text2
        end

        if not text then
            return
        end

        --- @param _a string[]
        local function foo(_a) end

        foo(text)
        "#
        ));
    }
}
