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
            ---@attribute constructor(name: string, strip_self: boolean?, return_self: boolean?)

            ---@generic T
            ---@param [constructor("__init")] name `T`
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

        ws.check_code_for(
            DiagnosticCode::AssignTypeMismatch,
            r#"
        ---@[skip_diagnostic("table_field")]
        local config = {}
        "#,
        );
    }
}
