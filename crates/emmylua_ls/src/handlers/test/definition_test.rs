#[cfg(test)]
mod tests {
    use lsp_types::GotoDefinitionResponse;

    use crate::handlers::test_lib::ProviderVirtualWorkspace;

    #[test]
    fn test_basic_definition() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.check_definition(
            r#"
                ---@generic T
                ---@param name `T`
                ---@return T
                local function new(name)
                    return name
                end

                ---@class Ability

                local a = new("<??>Ability")
            "#,
        );
    }

    #[test]
    fn test_table_field_definition_1() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.check_definition(
            r#"
                ---@class T
                ---@field func fun(self:string)

                ---@type T
                local t = {
                    f<??>unc = function(self)
                    end
                }
            "#,
        );
    }

    #[test]
    fn test_table_field_definition_2() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.check_definition(
            r#"
                ---@class T
                ---@field func fun(self: T) 注释注释

                ---@type T
                local t = {
                    func = function(self)
                    end,
                    a = 1,
                }

                t:func<??>()
            "#,
        );
    }

    #[test]
    fn test_goto_field() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.check_definition(
            r#"
                local t = {}
                function t:test(a)
                    self.abc = a
                end

                print(t.abc<??>)
            "#,
        );
    }

    #[test]
    fn test_goto_overload() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@class Goto1
                ---@class Goto2
                ---@class Goto3

                ---@class T
                ---@field func fun(a:Goto1) # 1
                ---@field func fun(a:Goto2) # 2
                ---@field func fun(a:Goto3) # 3
                local T = {}

                function T:func(a)
                end
            "#,
        );

        {
            let result = ws
                .check_definition(
                    r#"
                ---@type Goto2
                local Goto2

                ---@type T
                local t
                t.fu<??>nc(Goto2)
                 "#,
                )
                .unwrap();
            match result {
                GotoDefinitionResponse::Array(array) => {
                    assert_eq!(array.len(), 2);
                }
                _ => {
                    panic!("expect array");
                }
            }
        }

        {
            let result = ws
                .check_definition(
                    r#"
                ---@type T
                local t
                t.fu<??>nc()
                 "#,
                )
                .unwrap();
            match result {
                GotoDefinitionResponse::Array(array) => {
                    assert_eq!(array.len(), 4);
                }
                _ => {
                    panic!("expect array");
                }
            }
        }
    }

    #[test]
    fn test_goto_return_field() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "test.lua",
            r#"
            local function test()

            end

            return {
                test = test,
            }
            "#,
        );
        let result = ws
            .check_definition(
                r#"
            local t = require("test")
            local test = t.test
            te<??>st()
            "#,
            )
            .unwrap();
        match result {
            GotoDefinitionResponse::Array(locations) => {
                assert_eq!(locations.len(), 1);
                assert_eq!(locations[0].range.start.line, 1);
            }
            _ => {
                panic!("expect scalar");
            }
        }
    }

    #[test]
    fn test_goto_return_field_2() {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        ws.def_file(
            "test.lua",
            r#"
            ---@export
            ---@class Export
            local export = {}
            ---@generic T
            ---@param name `T`|T
            ---@param tbl? table
            ---@return T
            local function new(name, tbl)
            end

            export.new = new
            return export
            "#,
        );
        let result = ws
            .check_definition(
                r#"
            local new = require("test").new
            new<??>("A")
            "#,
            )
            .unwrap();
        match result {
            GotoDefinitionResponse::Array(locations) => {
                assert_eq!(locations.len(), 1);
                assert_eq!(locations[0].range.start.line, 8);
            }
            _ => {
                panic!("expect scalar");
            }
        }
    }

    #[test]
    fn test_goto_generic_type() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "1.lua",
            r#"
            ---@generic T
            ---@param name `T`|T
            ---@return T
            function new(name)
            end
            "#,
        );
        ws.def_file(
            "2.lua",
            r#"
            ---@namespace AAA
            ---@class BBB<T>
            "#,
        );
        let result = ws
            .check_definition(
                r#"
                new("AAA.BBB<??>")
            "#,
            )
            .unwrap();
        match result {
            GotoDefinitionResponse::Array(array) => {
                assert_eq!(array.len(), 1);
                let location = &array[0];
                assert_eq!(location.uri.path().as_str().ends_with("2.lua"), true);
            }
            _ => {
                panic!("expect array");
            }
        }
    }

    #[test]
    fn test_goto_export_function() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "a.lua",
            r#"
            local function create()
            end

            return create
            "#,
        );
        let result = ws
            .check_definition(
                r#"
                local create = require('a')
                create<??>()
            "#,
            )
            .unwrap();
        match result {
            GotoDefinitionResponse::Scalar(location) => {
                assert_eq!(location.uri.path().as_str().ends_with("a.lua"), true);
            }
            _ => {
                panic!("expect array");
            }
        }
    }

    #[test]
    fn test_goto_export_function_2() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "a.lua",
            r#"
            local function testA()
            end

            local function create()
            end

            return create
            "#,
        );
        ws.def_file(
            "b.lua",
            r#"
            local Rxlua = {}
            local create = require('a')

            Rxlua.create = create
            return Rxlua
            "#,
        );
        let result = ws
            .check_definition(
                r#"
                local create = require('b').create
                create<??>()
            "#,
            )
            .unwrap();
        match result {
            GotoDefinitionResponse::Array(array) => {
                assert_eq!(array.len(), 1);
                let location = &array[0];
                assert_eq!(location.uri.path().as_str().ends_with("a.lua"), true);
            }
            _ => {
                panic!("expect array");
            }
        }
    }
}
