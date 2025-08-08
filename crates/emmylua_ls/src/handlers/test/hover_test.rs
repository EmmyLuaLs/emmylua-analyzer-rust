#[cfg(test)]
mod tests {
    use crate::handlers::test_lib::{ProviderVirtualWorkspace, VirtualHoverResult, check};
    use googletest::prelude::*;
    #[gtest]
    fn test_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---@class <??>A
                ---@field a number
                ---@field b string
                ---@field c boolean
            "#,
            VirtualHoverResult {
                value:
                    "```lua\n(class) A {\n    a: number,\n    b: string,\n    c: boolean,\n}\n```"
                        .to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_right_to_left() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        // check!(ws.check_hover(
        //     r#"
        //         ---@class H4
        //         local m = {
        //             x = 1
        //         }

        //         ---@type H4
        //         local m1

        //         m1.x = {}
        //         m1.<??>x = {}
        //     "#,
        //     VirtualHoverResult {
        //         value: "```lua\n(field) x: integer = 1\n```".to_string(),
        //     },
        // ));

        check!(ws.check_hover(
            r#"
                ---@class Node
                ---@field x number
                ---@field right Node?

                ---@return Node
                local function createRBNode()
                end

                ---@type Node
                local node

                if node.right then
                else
                    node.<??>right = createRBNode()
                end
            "#,
            VirtualHoverResult {
                value: "```lua\n(field) right: Node\n```".to_string(),
            },
        ));

        check!(ws.check_hover(
            r#"
                 ---@class Node1
                ---@field x number

                ---@return Node1
                local function createRBNode()
                end

                ---@type Node1?
                local node

                if node then
                else
                    <??>node = createRBNode()
                end
            "#,
            VirtualHoverResult {
                value: "```lua\nlocal node: Node1 {\n    x: number,\n}\n```".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_hover_nil() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---@class A
                ---@field a? number

                ---@type A
                local a

                local d = a.<??>a
            "#,
            VirtualHoverResult {
                value: "```lua\n(field) a: number?\n```".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_function_infer_return_val() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                local function <??>f(a, b)
                    a = 1
                end
            "#,
            VirtualHoverResult {
                value: "```lua\nlocal function f(a, b)\n```".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_decl_desc() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---@class Buff.AddData
                ---@field pulse? number 心跳周期

                ---@type Buff.AddData
                local data

                data.pu<??>lse
            "#,
            VirtualHoverResult {
                value: "```lua\n(field) pulse: number?\n```\n\n&nbsp;&nbsp;in class `Buff.AddData`\n\n---\n\n心跳周期".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_issue_535() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---@type table<string, number>
                local t

                ---@class T1
                local a

                function a:init(p)
                    self._c<??>fg = t[p]
                end
            "#,
            VirtualHoverResult {
                value: "```lua\n(field) _cfg: number\n```".to_string(),
            },
        ));

        check!(ws.check_hover(
            r#"
                ---@type table<string, number>
                local t = {
                }
                ---@class T2
                local a = {}

                function a:init(p)
                    self._cfg = t[p]
                end

                ---@param p T2
                function fun(p)
                    local x = p._c<??>fg
                end
            "#,
            VirtualHoverResult {
                value: "```lua\n(field) _cfg: number\n```".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_signature_desc() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                -- # A
                local function a<??>bc()
                end
            "#,
            VirtualHoverResult {
                value: "```lua\nlocal function abc()\n```\n\n---\n\n# A".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_class_desc() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---A1
                ---@class AB<??>C
                ---A2
            "#,
            VirtualHoverResult {
                value: "```lua\n(class) ABC\n```\n\n---\n\nA1".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_alias_desc() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---@alias Tes<??>Alias
                ---| 'A' # A1
                ---| 'B' # A2
            "#,
            VirtualHoverResult {
                value: "```lua\n(alias) TesAlias = (\"A\"|\"B\")\n    | \"A\" -- A1\n    | \"B\" -- A2\n\n```".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_type_desc() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                local export = {
                    ---@type number? activeSub
                    vvv = nil
                }

                export.v<??>vv
            "#,
            VirtualHoverResult {
                value: "```lua\n(field) vvv: number?\n```\n\n---\n\nactiveSub".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_field_key() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@class ObserverParams
                ---@field next fun() # 测试

                ---@param params fun() | ObserverParams
                function test(params)
                end
            "#,
        );
        check!(ws.check_hover(
            r#"
                test({
                    <??>next = function()
                    end
                })
            "#,
            VirtualHoverResult {
                value: "```lua\n(field) ObserverParams.next()\n```\n\n---\n\n测试".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_field_key_for_generic() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@class ObserverParams<T>
                ---@field next fun() # 测试

                ---@generic T
                ---@param params fun() | ObserverParams<T>
                function test(params)
                end
            "#,
        );
        check!(ws.check_hover(
            r#"
                test({
                    <??>next = function()
                    end
                })
            "#,
            VirtualHoverResult {
                value: "```lua\n(field) ObserverParams.next()\n```\n\n---\n\n测试".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_before_dot_returns_object_info() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@class Node
                ---@field field number?
                ---@field method fun(self: Node)

                ---@type Node
                node = {}

                function node.method() end
            "#,
        );

        check!(ws.check_hover(
            r#"
                node<??>.field = nil
            "#,
            VirtualHoverResult {
                value: "```lua\n(global) node: Node {\n    field: number?,\n    method: function,\n}\n```".to_string(),
            },
        ));

        check!(ws.check_hover(
            r#"
                node<??>:method()
            "#,
            VirtualHoverResult {
                value: "```lua\n(global) node: Node {\n    field: number?,\n    method: function,\n}\n```".to_string(),
            },
        ));

        check!(ws.check_hover(
            r#"
                node<??>["key"] = "value"
            "#,
            VirtualHoverResult {
                value: "```lua\n(global) node: Node {\n    field: number?,\n    method: function,\n}\n```".to_string(),
            },
        ));

        check!(ws.check_hover(
            r#"
                node["key"<??>] = "value"
            "#,
            VirtualHoverResult {
                value: "\"key\"".to_string(),
            },
        ));

        Ok(())
    }

    #[gtest]
    fn test_see_tag() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                --- Description
                ---
                --- @see a.b.c
                local function te<??>st() end
            "#,
            VirtualHoverResult {
                value: "```lua\nlocal function test()\n```\n\n---\n\nDescription\n\n---\n\n@*see* a.b.c".to_string(),
            },
        ));

        check!(ws.check_hover(
            r#"
                --- Description
                ---
                --- @see a.b.c see description
                local function te<??>st() end
            "#,
            VirtualHoverResult {
                value: "```lua\nlocal function test()\n```\n\n---\n\nDescription\n\n---\n\n@*see* a.b.c see description".to_string(),
            },
        ));

        Ok(())
    }

    #[gtest]
    fn test_other_tag() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                --- Description
                ---
                --- @xyz content
                local function te<??>st() end
            "#,
            VirtualHoverResult {
                value: "```lua\nlocal function test()\n```\n\n---\n\nDescription\n\n---\n\n@*xyz* content".to_string(),
            },
        ));

        Ok(())
    }
}
