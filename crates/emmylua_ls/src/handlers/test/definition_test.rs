#[cfg(test)]
mod tests {
    use crate::handlers::test_lib::ProviderVirtualWorkspace;
    use emmylua_code_analysis::{DocSyntax, Emmyrc};
    use lsp_types::GotoDefinitionResponse;

    #[derive(Debug, Copy, Clone)]
    struct Expected {
        file: &'static str,
        line: u32,
    }

    fn assert_def(result: Option<GotoDefinitionResponse>, expected_items: &[Expected]) {
        let mut expected_items = Vec::from(expected_items);

        let Some(result) = result else {
            panic!("expect result, got None");
        };

        let mut items = match result {
            GotoDefinitionResponse::Scalar(item) => vec![item],
            GotoDefinitionResponse::Array(array) => array,
            GotoDefinitionResponse::Link(_) => {
                panic!("expect scalar, got Link");
            }
        };

        items.sort_by_key(|item| item.range.start.line);
        expected_items.sort_by_key(|item| item.line);

        assert_eq!(items.len(), expected_items.len());
        for (item, expected_item) in items.iter().zip(expected_items) {
            if !expected_item.file.is_empty() {
                assert!(
                    item.uri.path().as_str().ends_with(expected_item.file),
                    "expected uri {:?}, got {:?}",
                    expected_item.file,
                    item.uri
                );
            }
            if expected_item.line > 0 {
                assert_eq!(item.range.start.line, expected_item.line);
            }
        }
    }

    #[test]
    fn test_basic_definition() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws.check_definition(
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
        assert_def(result, &[Expected { file: "", line: 8 }])
    }

    #[test]
    fn test_table_field_definition_1() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws.check_definition(
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
        assert_def(
            result,
            &[
                Expected { file: "", line: 2 },
                Expected { file: "", line: 6 },
            ],
        )
    }

    #[test]
    fn test_table_field_definition_2() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws.check_definition(
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
        // XXX: only one result?
        assert_def(result, &[Expected { file: "", line: 2 }])
    }

    #[test]
    fn test_goto_field() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws.check_definition(
            r#"
                local t = {}
                function t:test(a)
                    self.abc = a
                end

                print(t.abc<??>)
            "#,
        );
        assert_def(result, &[Expected { file: "", line: 3 }])
    }

    #[test]
    fn test_goto_overload() {
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

                -- impl
                function T:func(a)
                end
            "#,
        );

        {
            let result = ws.check_definition(
                r#"
                ---@type Goto2
                local Goto2

                ---@type T
                local t
                t.fu<??>nc(Goto2)
                 "#,
            );
            // XXX: why reference to 1, and no references to impl?
            assert_def(
                result,
                &[
                    Expected {
                        file: "test.lua",
                        line: 6,
                    },
                    Expected {
                        file: "test.lua",
                        line: 7,
                    },
                ],
            )
        }

        {
            let result = ws.check_definition(
                r#"
                ---@type T
                local t
                t.fu<??>nc()
                 "#,
            );
            assert_def(
                result,
                &[
                    Expected {
                        file: "test.lua",
                        line: 6,
                    },
                    Expected {
                        file: "test.lua",
                        line: 7,
                    },
                    Expected {
                        file: "test.lua",
                        line: 8,
                    },
                    Expected {
                        file: "test.lua",
                        line: 12,
                    },
                ],
            )
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
        let result = ws.check_definition(
            r#"
            local t = require("test")
            local test = t.test
            te<??>st()
            "#,
        );
        assert_def(
            result,
            &[Expected {
                file: "test.lua",
                line: 1,
            }],
        )
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
        let result = ws.check_definition(
            r#"
            local new = require("test").new
            new<??>("A")
            "#,
        );
        assert_def(
            result,
            &[Expected {
                file: "test.lua",
                line: 8,
            }],
        )
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
        let result = ws.check_definition(
            r#"
                new("AAA.BBB<??>")
            "#,
        );
        assert_def(
            result,
            &[Expected {
                file: "2.lua",
                line: 2,
            }],
        )
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
        let result = ws.check_definition(
            r#"
                local create = require('a')
                create<??>()
            "#,
        );
        assert_def(
            result,
            &[Expected {
                file: "a.lua",
                line: 1,
            }],
        )
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
        let result = ws.check_definition(
            r#"
                local create = require('b').create
                create<??>()
            "#,
        );
        assert_def(
            result,
            &[Expected {
                file: "a.lua",
                line: 4,
            }],
        )
    }

    #[test]
    fn test_doc_resolve() {
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

        let result = ws.check_definition(
            r#"
                --- {lua:obj}`X<??>`
            "#,
        );
        assert_def(
            result,
            &[Expected {
                file: "a.lua",
                line: 1,
            }],
        );

        let result = ws.check_definition(
            r#"
                --- {lua:obj}`X<??>.a`
            "#,
        );
        assert_def(
            result,
            &[Expected {
                file: "a.lua",
                line: 1,
            }],
        );

        let result = ws.check_definition(
            r#"
                --- {lua:obj}`X.a<??>`
            "#,
        );
        assert_def(
            result,
            &[Expected {
                file: "a.lua",
                line: 2,
            }],
        );

        let result = ws.check_definition(
            r#"
                --- @using ns

                --- {lua:obj}`X<??>`
            "#,
        );
        assert_def(
            result,
            &[Expected {
                file: "a.lua",
                line: 1,
            }],
        );

        let result = ws.check_definition(
            r#"
                --- @using ns

                --- {lua:obj}`Y<??>`
            "#,
        );
        assert_def(
            result,
            &[Expected {
                file: "a.lua",
                line: 4,
            }],
        );

        let result = ws.check_definition(
            r#"
                --- @using ns

                --- {lua:obj}`ns.Y<??>`
            "#,
        );
        assert_def(
            result,
            &[Expected {
                file: "a.lua",
                line: 4,
            }],
        );

        let result = ws.check_definition(
            r#"
                --- {lua:obj}`c<??>`
                --- @class Z
                --- @field c string
            "#,
        );
        assert_def(result, &[Expected { file: "", line: 3 }]);
    }
}
