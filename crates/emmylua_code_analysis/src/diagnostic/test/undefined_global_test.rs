#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_issue_250() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedGlobal,
            r#"
            --- @class A
            --- @field field any
            local A = {}

            function A:method()
            pcall(function()
                return self.field
            end)
            end
            "#
        ));
    }

    #[test]
    fn test_return_cast_self_no_undefined_global() {
        let mut ws = VirtualWorkspace::new();
        // @return_cast self should not produce undefined-global error in methods
        assert!(!ws.check_code_for(
            DiagnosticCode::UndefinedGlobal,
            r#"
            ---@class MyClass
            local MyClass = {}

            ---@return_cast self MyClass
            function MyClass:check1()
                return true
            end
            "#
        ));
    }

    #[test]
    fn test_return_cast_self_field_no_undefined_global() {
        let mut ws = VirtualWorkspace::new();
        // @return_cast self.field should not produce undefined-global error in methods
        assert!(!ws.check_code_for(
            DiagnosticCode::UndefinedGlobal,
            r#"
            ---@class MyClass
            ---@field value string|number
            local MyClass = {}

            ---@return_cast self.value string
            function MyClass:check_string()
                return type(self.value) == "string"
            end
            "#
        ));
    }
}
