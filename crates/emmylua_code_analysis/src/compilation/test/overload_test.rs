#[cfg(test)]
mod test {
    use std::{ops::Deref, sync::Arc};

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
        let mut emmyrc = ws.analysis.emmyrc.deref().clone();
        emmyrc.runtime.class_default_call.function_name = "__init".to_string();
        emmyrc.runtime.class_default_call.force_non_colon = true;
        emmyrc.runtime.class_default_call.force_return_self = true;
        ws.analysis.update_config(Arc::new(emmyrc));

        ws.def(
            r#"
        ---@class MyClass
        local M = {}

        function M:__init(a)
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

    #[test]
    fn test_meta_global_overload() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        --- @meta

        --- @param a string
        --- @return string
        function foo(a) end

        --- @param a integer
        --- @param b integer
        --- @return integer
        function foo(a, b) end
        "#,
        );

        assert_eq!(ws.expr_ty("foo('a')"), ws.ty("string"));
        assert_eq!(ws.expr_ty("foo('a', 'b')"), ws.ty("unknown"));
        assert_eq!(ws.expr_ty("foo(1, 2)"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("foo(1)"), ws.ty("unknown"));
    }
}
