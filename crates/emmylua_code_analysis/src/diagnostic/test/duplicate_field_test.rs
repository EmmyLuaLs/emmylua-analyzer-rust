#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_duplicate_field() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::DuplicateDocField,
            r#"
            ---@class Test
            ---@field name string
            ---@field name string
            local Test = {}

            Test.name = 1
            "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::DuplicateDocField,
            r#"
            ---@class Test
            ---@field name string
            ---@field age number
            local Test = {}
            "#
        ));

        assert!(!ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::DuplicateDocField,
            r#"
            ---@class Test
            ---@field name string
            ---@field name number
            local Test = {}
            "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
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

        assert!(ws.has_no_diagnostic_in_namespace(
            DiagnosticCode::DuplicateDocField,
            r#"
            ---@class Test
            ---@field a fun()
            local Test = {}

            function Test.a()
            end
            "#
        ));

        assert!(ws.has_no_diagnostic_in_namespace(
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

        assert!(!ws.has_no_diagnostic_in_namespace(
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

    // remove this test
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
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::DuplicateSetField,
            r#"
            local A = require("1")

            A.execute = function(trg, ...)
            end
        "#
        ));
    }

    #[test]
    fn test_duplicate_function_3() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::DuplicateSetField,
            r#"
                ---@class D31.A
                local A = {}
                A.a = function() end

                function A:init()
                    self.a = function()
                    end
                end
        "#
        ));
    }

    #[test]
    fn test_duplicate_function_4() {
        let mut ws = VirtualWorkspace::new();
        // 如果是 .member = 参数, 则不报错
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::DuplicateSetField,
            r#"
                ---@class D31.A
                local A = {}
                A.a = function() end

                ---@param a fun()
                function A:init(a)
                    self.a = a
                end
        "#
        ));
    }

    #[test]
    fn test_return_self() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::DuplicateSetField,
            r#"
                ---@class test
                local A

                ---@return self
                function A.new()
                end

                function A:stop()
                end

                local a = A.new()

                a.stop = function()

                end
        "#
        ));
    }
}
