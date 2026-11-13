#[cfg(test)]
mod test {
    use crate::{
        DiagnosticCode, VirtualWorkspace,
        check::test::{check_source, count_by_code},
    };

    #[test]
    fn test_index_signature_duplicates_use_key_domains() {
        for (fields, count) in [
            (
                "---@field [integer] string\n---@field [string] boolean\n---@field integer boolean",
                0,
            ),
            ("---@field [integer] string\n---@field [integer] boolean", 2),
            (
                "---@field [integer | boolean] string\n---@field [string | boolean] number",
                0,
            ),
        ] {
            let diagnostics = check_source(&format!("---@class Record\n{fields}"));
            assert_eq!(
                count_by_code(&diagnostics, DiagnosticCode::DuplicateDocField),
                count,
                "{fields}: {diagnostics:?}"
            );
        }
    }

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

        for (first, second, overloads) in [
            (
                "fun(value: string): string",
                "fun(value: integer): integer",
                true,
            ),
            ("Callback<string>", "Callback<integer>", true),
            (
                "Identity<Callback<string>>",
                "Identity<Callback<integer>>",
                true,
            ),
            ("Choose<string>", "Choose<'fixed'>", true),
            ("Identity<string>", "Identity<integer>", false),
            ("Callback<string>", "Identity<integer>", false),
        ] {
            assert_eq!(
                ws.has_no_diagnostic_in_namespace(
                    DiagnosticCode::DuplicateDocField,
                    &format!(
                        r#"
                    ---@alias Callback<T> fun(value: T): T
                    ---@alias Identity<T> T
                    ---@alias Choose<T> T extends string and Callback<T> or integer
                    ---@class Test
                    ---@field a {first}
                    ---@field a {second}
                    local Test = {{}}

                    function Test.a()
                    end
                    "#
                    )
                ),
                overloads,
                "field types: {first}, {second}"
            );
        }

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
        // Assigning to `.member` from a parameter is not reported.
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

    #[test]
    fn test_self_field_write_does_not_mask_direct_duplicates() {
        let diagnostics = check_source(
            r#"
                local M = {}
                function M:init()
                    self.run = function() end
                end
                function M.run() end
                M.run = function() end
        "#,
        );
        assert_eq!(
            count_by_code(&diagnostics, DiagnosticCode::DuplicateSetField),
            2,
            "{diagnostics:?}"
        );
    }

    #[test]
    fn test_repeated_self_field_writes_are_reported() {
        let diagnostics = check_source(
            r#"
                local M = {}
                function M:init()
                    self.run = function() end
                    self.run = function() end
                end
        "#,
        );
        assert_eq!(
            count_by_code(&diagnostics, DiagnosticCode::DuplicateSetField),
            2,
            "{diagnostics:?}"
        );
    }

    #[test]
    fn test_cross_method_self_field_writes_are_not_duplicates() {
        let diagnostics = check_source(
            r#"
                local M = {}
                function M:init()
                    self.run = function() end
                end
                function M:reset()
                    self.run = function() end
                end
        "#,
        );
        assert_eq!(
            count_by_code(&diagnostics, DiagnosticCode::DuplicateSetField),
            0,
            "{diagnostics:?}"
        );
    }

    #[test]
    fn test_same_key_on_different_nested_tables_is_not_duplicate() {
        for (source, count) in [
            (
                "local M = { A = {}, B = {} }\nfunction M.A:init() end\nfunction M.B:init() end\n",
                0,
            ),
            (
                "local M = { a = {}, b = {} }\nM.a.to = function() end\nM.b.to = function() end\n",
                0,
            ),
            // The same nested table twice still is a duplicate.
            (
                "local M = { A = {} }\nfunction M.A:init() end\nfunction M.A:init() end\n",
                2,
            ),
        ] {
            let diagnostics = check_source(source);
            assert_eq!(
                count_by_code(&diagnostics, DiagnosticCode::DuplicateSetField),
                count,
                "{source}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_setting_a_nil_field_of_a_required_module_is_not_duplicate() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "reg.lua",
            "local M = { a = { to = nil }, b = { to = nil } }\nreturn M\n",
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::DuplicateSetField,
            r#"
            local reg = require("reg")
            reg.a.to = function() end
            reg.b.to = function() end
        "#
        ));
    }
}
