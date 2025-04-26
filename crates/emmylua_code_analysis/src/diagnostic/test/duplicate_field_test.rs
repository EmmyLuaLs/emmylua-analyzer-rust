#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_duplicate_field() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for_namespace(
            DiagnosticCode::DuplicateDocField,
            r#"
            ---@class Test
            ---@field name string
            ---@field name string
            local Test = {}

            Test.name = 1
            "#
        ));

        assert!(ws.check_code_for_namespace(
            DiagnosticCode::DuplicateDocField,
            r#"
            ---@class Test
            ---@field name string
            ---@field age number
            local Test = {}
            "#
        ));

        assert!(!ws.check_code_for_namespace(
            DiagnosticCode::DuplicateDocField,
            r#"
            ---@class Test
            ---@field name string
            ---@field name number
            local Test = {}
            "#
        ));

        assert!(ws.check_code_for_namespace(
            DiagnosticCode::DuplicateDocField,
            r#"
            ---@class Test1
            ---@field name string

            ---@class Test2
            ---@field name string
            "#
        ));
    }

    #[test]
    fn test_duplicate_function_1() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for_namespace(
            DiagnosticCode::DuplicateDocField,
            r#"
            ---@class Test
            ---@field a fun()
            local Test = {}

            function Test.a()
            end
            "#
        ));

        assert!(ws.check_code_for_namespace(
            DiagnosticCode::DuplicateDocField,
            r#"
            ---@class Test
            ---@field a fun()
            ---@field a fun()
            local Test = {}

            function Test.a()
            end
            "#
        ));

        assert!(!ws.check_code_for_namespace(
            DiagnosticCode::DuplicateSetField,
            r#"
            ---@class Test
            ---@field a fun()
            local Test = {}

            function Test.a()
            end
            
            function Test.a()
            end
            "#
        ));
    }
}
