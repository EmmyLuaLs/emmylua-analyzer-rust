#[cfg(test)]
mod tests {
    use crate::handlers::test_lib::{ProviderVirtualWorkspace, VirtualLocation, check};
    use emmylua_code_analysis::{DocSyntax, Emmyrc};
    use googletest::prelude::*;

    type Expected = VirtualLocation;

    #[gtest]
    fn test_basic_definition() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_definition(
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
            vec![Expected {
                file: "".to_string(),
                line: 8
            }]
        ));
        Ok(())
    }

    #[gtest]
    fn test_table_field_definition_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_definition(
            r#"
                ---@class T
                ---@field func fun(self:string)

                ---@type T
                local t = {
                    f<??>unc = function(self)
                    end
                }
            "#,
            vec![
                Expected {
                    file: "".to_string(),
                    line: 2
                },
                Expected {
                    file: "".to_string(),
                    line: 6
                },
            ]
        ));
        Ok(())
    }

    #[gtest]
    fn test_table_field_definition_2() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_definition(
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
            vec![Expected {
                file: "".to_string(),
                line: 2
            }]
        ));
        Ok(())
    }

    #[gtest]
    fn test_goto_field() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_definition(
            r#"
                local t = {}
                function t:test(a)
                    self.abc = a
                end

                print(t.abc<??>)
            "#,
            vec![Expected {
                file: "".to_string(),
                line: 3
            }]
        ));
        Ok(())
    }

    #[gtest]
    fn test_goto_overload() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "test.lua",
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

        check!(ws.check_definition(
            r#"
                ---@type Goto2
                local Goto2

                ---@type T
                local t
                t.fu<??>nc(Goto2)
             "#,
            vec![
                Expected {
                    file: "test.lua".to_string(),
                    line: 6,
                },
                Expected {
                    file: "test.lua".to_string(),
                    line: 7,
                },
            ]
        ));

        check!(ws.check_definition(
            r#"
                ---@type T
                local t
                t.fu<??>nc()
             "#,
            vec![
                Expected {
                    file: "test.lua".to_string(),
                    line: 6,
                },
                Expected {
                    file: "test.lua".to_string(),
                    line: 7,
                },
                Expected {
                    file: "test.lua".to_string(),
                    line: 8,
                },
                Expected {
                    file: "test.lua".to_string(),
                    line: 11,
                },
            ]
        ));
        Ok(())
    }

    #[gtest]
    fn test_goto_return_field() -> Result<()> {
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
        check!(ws.check_definition(
            r#"
                local t = require("test")
                local test = t.test
                te<??>st()
            "#,
            vec![VirtualLocation {
                file: "test.lua".to_string(),
                line: 1
            }]
        ));

        Ok(())
    }

    #[gtest]
    fn test_goto_return_field_2() -> Result<()> {
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
        check!(ws.check_definition(
            r#"
                local new = require("test").new
                new<??>("A")
            "#,
            vec![Expected {
                file: "test.lua".to_string(),
                line: 8
            }]
        ));
        Ok(())
    }

    #[gtest]
    fn test_goto_generic_type() -> Result<()> {
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
        check!(ws.check_definition(
            r#"
                new("AAA.BBB<??>")
            "#,
            vec![Expected {
                file: "2.lua".to_string(),
                line: 2
            }]
        ));
        Ok(())
    }

    #[gtest]
    fn test_goto_export_function() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "a.lua",
            r#"
                local function create()
                end

                return create
            "#,
        );
        check!(ws.check_definition(
            r#"
                local create = require('a')
                create<??>()
            "#,
            vec![Expected {
                file: "a.lua".to_string(),
                line: 1
            }]
        ));
        Ok(())
    }

    #[gtest]
    fn test_goto_export_function_2() -> Result<()> {
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
        check!(ws.check_definition(
            r#"
                local create = require('b').create
                create<??>()
            "#,
            vec![Expected {
                file: "a.lua".to_string(),
                line: 4
            }]
        ));
        Ok(())
    }

    #[gtest]
    fn test_doc_resolve() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();

        let mut emmyrc = Emmyrc::default();
        emmyrc.doc.syntax = DocSyntax::Myst;
        ws.analysis.update_config(emmyrc.into());

        ws.def_file(
            "a.lua",
            r#"
                --- @class X
                --- @field a string

                --- @class ns.Y
                --- @field b string
            "#,
        );

        check!(ws.check_definition(
            r#"
                --- {lua:obj}`X<??>`
            "#,
            vec![Expected {
                file: "a.lua".to_string(),
                line: 1
            }]
        ));

        check!(ws.check_definition(
            r#"
                --- {lua:obj}`X<??>.a`
            "#,
            vec![Expected {
                file: "a.lua".to_string(),
                line: 1
            }]
        ));

        check!(ws.check_definition(
            r#"
                --- {lua:obj}`X.a<??>`
            "#,
            vec![Expected {
                file: "a.lua".to_string(),
                line: 2
            }]
        ));

        check!(ws.check_definition(
            r#"
                --- @using ns

                --- {lua:obj}`X<??>`
            "#,
            vec![Expected {
                file: "a.lua".to_string(),
                line: 1
            }]
        ));

        check!(ws.check_definition(
            r#"
                --- @using ns

                --- {lua:obj}`Y<??>`
            "#,
            vec![Expected {
                file: "a.lua".to_string(),
                line: 4
            }]
        ));

        check!(ws.check_definition(
            r#"
                --- @using ns

                --- {lua:obj}`ns.Y<??>`
            "#,
            vec![Expected {
                file: "a.lua".to_string(),
                line: 4
            }]
        ));

        check!(ws.check_definition(
            r#"
                --- {lua:obj}`c<??>`
                --- @class Z
                --- @field c string
            "#,
            vec![Expected {
                file: "".to_string(),
                line: 3
            }]
        ));

        Ok(())
    }

    #[gtest]
    fn test_goto_variable_param() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "a.lua",
            r#"
                ---@class Observable<T>

                ---test
                local function zipLatest(...)
                end
                return zipLatest
            "#,
        );
        ws.def_file(
            "b.lua",
            r#"
            local export = {}
            local zipLatest = require('a')
            export.zipLatest = zipLatest
            return export
            "#,
        );
        check!(ws.check_definition(
            r#"
                local zipLatest = require('b').zipLatest
                zipLatest<??>()
            "#,
            vec![Expected {
                file: "a.lua".to_string(),
                line: 4,
            }],
        ));
        Ok(())
    }

    #[gtest]
    fn test_goto_see() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        let mut emmyrc = Emmyrc::default();
        emmyrc.doc.syntax = DocSyntax::Myst;
        ws.analysis.update_config(emmyrc.into());

        ws.def_file(
            "a.lua",
            r#"
                ---@class Meep
            "#,
        );

        check!(ws.check_definition(
            r#"
                --- @see Mee<??>p
            "#,
            vec![Expected {
                file: "a.lua".to_string(),
                line: 1,
            }],
        ));

        check!(ws.check_definition(
            r#"
                --- @class Foo
                --- @field bar int
                local Foo = {}

                --- @see b<??>ar
                Foo.xxx = 0
            "#,
            vec![Expected {
                file: "".to_string(),
                line: 2,
            }],
        ));

        Ok(())
    }

    #[gtest]
    fn test_accessors() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
            ---@class A
            ---@[field_accessor]
            ---@field age number
            local A

            ---@private
            function A:getAge()
            end

            ---@private
            function A:setAge(value)
            end
            "#,
        );

        check!(ws.check_definition(
            r#"
                ---@type A
                local obj
                obj.age<??> = 1
            "#,
            vec![
                Expected {
                    file: "".to_string(),
                    line: 3,
                },
                Expected {
                    file: "".to_string(),
                    line: 7,
                },
                Expected {
                    file: "".to_string(),
                    line: 11,
                }
            ],
        ));

        Ok(())
    }

    #[gtest]
    fn test_intersection() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
        ---@class Matchers
        ---@field toBe fun(expected: any)

        ---@class Inverse
        ---@field negate number
        "#,
        );
        check!(ws.check_definition(
            r#"
            ---@type Matchers & Inverse
            local a
            a.ne<??>gate = 0
            "#,
            vec![Expected {
                file: "".to_string(),
                line: 5,
            },],
        ));
        Ok(())
    }
}
