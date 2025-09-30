#[cfg(test)]
mod test {

    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_table() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
        table.concat({'', ''}, ' ')
        "#
        ));
    }

    #[test]
    fn test_sub_string() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_code_for(
            DiagnosticCode::MissingParameter,
            r#"
        local t = ("m2"):sub(1)
        "#
        ));
    }

    #[test]
    fn test_class_default_call() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@attribute class_ctor(name: string, strip_self: boolean?, return_self: boolean?)

            ---@generic T
            ---@param [class_ctor("__init")] name `T`
            ---@return T
            function meta(name)
            end
        "#,
        );

        ws.def(
            r#"
        ---@class MyClass
        local M = meta("MyClass")

        function M:__init()
        end

        A = M()
        "#,
        );

        let ty = ws.expr_ty("A");
        let expected = ws.ty("MyClass");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));
    }

    #[test]
    fn test_issue_770() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_code_for(
            DiagnosticCode::RedundantParameter,
            r#"
        local table = {1,2}
        if next(table, 2) == '2' then
            print('ok')
        end
        "#
        ));
    }
}
