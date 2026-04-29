#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_constructor() {
        let mut ws = VirtualWorkspace::new();

        ws.def_files(vec![
            (
                "init.lua",
                r#"
                A = meta("A")
                "#,
            ),
            (
                "meta.lua",
                r#"
            ---@attribute constructor(name: string, root_class: string?, strip_self: boolean?, return_self: boolean?)

            ---@generic T
            ---@[constructor("__init")]
            ---@param name `T`
            ---@return T
            function meta(name)
            end
                "#,
            ),
        ]);
    }

    #[test]
    fn test_def_attribute() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
        ---@[lsp_optimization("check_table_field")]
        local config = {}
        "#,
        );
    }

    #[test]
    fn test_delayed_definition() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@[lsp_optimization("delayed_definition")]
        local config

        function func()
            A = config
        end

        config = 1
        "#,
        );

        let ty = ws.expr_ty("A");
        let ty_desc = ws.humanize_type(ty);
        assert_eq!(ty_desc, "integer");
    }
}
