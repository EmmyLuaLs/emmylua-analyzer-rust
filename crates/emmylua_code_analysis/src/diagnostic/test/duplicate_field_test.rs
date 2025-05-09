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

    #[test]
    fn test_duplicate_function_2() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "1.lua",
            r#"
                ---@class D31.A
                local A = {}

                ---@param ... any
                ---@return any, any, any, any
                function A:execute(...)
                end

                return A
            "#,
        );
        // TODO: 这里应该报错, 但底层存在问题, 暂时不报错, issue: #430
        assert!(ws.check_code_for(
            DiagnosticCode::DuplicateSetField,
            r#"
            local A = require("1")

            ---@class D31.B
            local B = {}

            function B:__init()
                self.originalExecute = A.execute
                A.execute = function(trg, ...)
                    self.originalExecute(trg, ...)
                end
            end
        "#
        ));
    }
}
