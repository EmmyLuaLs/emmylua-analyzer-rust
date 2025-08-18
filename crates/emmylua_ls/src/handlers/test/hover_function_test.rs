#[cfg(test)]
mod tests {
    use crate::handlers::test_lib::{ProviderVirtualWorkspace, VirtualHoverResult, check};
    use googletest::prelude::*;

    #[gtest]
    fn test_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---@param a number 参数a
                ---@return number a 返回值a
                local function delete(a)
                end

                local delete2 = delete
                local delete3 = delete2
                local <??>delete4 = delete3
            "#,
            VirtualHoverResult {
                value: "```lua\nlocal function delete(a: number)\n  -> a: number\n\n```\n\n---\n\n@*param* `a` — 参数a\n\n\n\n@*return* `a`  — 返回值a".to_string(),
            },
        ));

        check!(ws.check_hover(
            r#"
                -- 删除
                ---@param a number 参数a
                ---@return number a 返回值a
                local function delete(a)
                end

                local delete2 = delete
                local delete3 = delete2
                local delete4 = delete3
                local deleteObj = {
                    <??>aaa = delete4
                }
            "#,
            VirtualHoverResult {
                value: "```lua\nlocal function delete(a: number)\n  -> a: number\n\n```\n\n---\n\n删除\n\n@*param* `a` — 参数a\n\n\n\n@*return* `a`  — 返回值a".to_string(),
            },
        ));

        check!(ws.check_hover(
            r#"
                ---@param a number 参数a
                ---@return number a 返回值a
                local function delete(a)
                end

                local delete2 = delete
                local delete3 = delete2
                local delete4 = delete3
                local deleteObj = {
                    aa = delete4
                }

                local deleteObj2 = {
                    <??>aa = deleteObj.aa
                }
            "#,
            VirtualHoverResult {
                value: "```lua\nlocal function delete(a: number)\n  -> a: number\n\n```\n\n---\n\n@*param* `a` — 参数a\n\n\n\n@*return* `a`  — 返回值a".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_2() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@class Game
                ---@field event fun(self: self, owner: "abc"): any 测试1
                ---@field event fun(self: self, owner: "def"): any 测试2
                local Game = {}

                ---说明
                ---@param key string 参数key
                ---@param value string 参数value
                ---@return number ret @返回值
                function Game:add(key, value)
                    self.aaa = 1
                end
            "#,
        );

        check!(ws.check_hover(
            r#"
                ---@type Game
                local game

                local local_a = game.add
                local <??>local_b = local_a
            "#,
            VirtualHoverResult {
                value: "```lua\n(method) Game:add(key: string, value: string)\n  -> ret: number\n\n```\n\n---\n\n说明\n\n@*param* `key` — 参数key\n\n@*param* `value` — 参数value\n\n\n\n@*return* `ret`  — 返回值".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_3() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@class Hover.Test3<T>
                ---@field event fun(self: self, event: "A", key: T)
                ---@field event fun(self: self, event: "B", key: T)
                local Test3 = {}
            "#,
        );

        check!(ws.check_hover(
            r#"
                ---@type Hover.Test3<string>
                local test3

                local <??>event = test3.event
            "#,
            VirtualHoverResult {
                value: "```lua\n(method) Test3:event(event: \"B\", key: string)\n```\n\n&nbsp;&nbsp;in class `Hover.Test3`\n\n---\n\n---\n\n```lua\n(method) Test3:event(event: \"A\", key: string)\n```".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_union_function() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---@diagnostic disable: missing-return
                ---@class Trigger
                ---@class EventTypeA

                ---@class (partial) GameA
                local M

                -- 注册引擎事件
                ---@param event_type EventTypeA
                ---@param ... any
                ---@return Trigger
                function M:<??>event(event_type, ...)
                end

                ---@class (partial) GameA
                ---@field event fun(self: self, event: "游戏-初始化"): Trigger
                ---@field event fun(self: self, event: "游戏-追帧完成"): Trigger
                ---@field event fun(self: self, event: "游戏-逻辑不同步"): Trigger
                ---@field event fun(self: self, event: "游戏-地形预设加载完成"): Trigger
                ---@field event fun(self: self, event: "游戏-结束"): Trigger
                ---@field event fun(self: self, event: "游戏-暂停"): Trigger
                ---@field event fun(self: self, event: "游戏-恢复"): Trigger
                ---@field event fun(self: self, event: "游戏-昼夜变化"): Trigger
                ---@field event fun(self: self, event: "区域-进入"): Trigger
                ---@field event fun(self: self, event: "区域-离开"): Trigger
                ---@field event fun(self: self, event: "游戏-http返回"): Trigger
            "#,
            VirtualHoverResult {
                value: "```lua\n(method) GameA:event(event_type: EventTypeA, ...: any)\n  -> Trigger\n\n```\n\n---\n\n注册引擎事件\n\n---\n\n```lua\n(method) GameA:event(event: \"游戏-初始化\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"游戏-追帧完成\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"游戏-逻辑不同步\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"游戏-地形预设加载完成\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"游戏-结束\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"游戏-暂停\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"游戏-恢复\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"游戏-昼夜变化\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"区域-进入\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"区域-离开\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"游戏-http返回\") -> Trigger\n```".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_4() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---@class ClosureTest
                ---@field e fun(a: string, b: number)
                local Test

                function Test.<??>e(a, b)
                    A = a
                end
            "#,
            VirtualHoverResult {
                value: "```lua\nfunction ClosureTest.e(a: string, b: number)\n```\n\n---\n\n---\n\n```lua\n(field) ClosureTest.e(a: string, b: number)\n```".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_table_field_function_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---@class T
                ---@field func fun(self:T) 注释注释

                ---@type T
                local t = {
                    func<??> = function(self)

                    end
                }
            "#,
            VirtualHoverResult {
                value: "```lua\n(method) T:func()\n```\n\n---\n\n注释注释".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_issue_499() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---@class T
                ---@field a string 注释注释a

                ---@type T
                local t = {
                    a<??> = "a"
                }
            "#,
            VirtualHoverResult {
                value: "```lua\n(field) a: string = \"a\"\n```\n\n---\n\n注释注释a".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_issue_499_2() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---@class T
                ---@field func fun(self:string) 注释注释

                ---@type T
                local t = {
                    fu<??>nc = function(self)
                    end,
                }
            "#,
            VirtualHoverResult {
                value: "```lua\n(field) T.func(self: string)\n```\n\n---\n\n注释注释".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_issue_499_3() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---@class T
                ---@field func fun(a:string) 注释1
                ---@field func fun(a:number) 注释2

                ---@type T
                local t = {
                    fu<??>nc = function(a)
                    end,
                }
            "#,
            VirtualHoverResult {
                value: "```lua\n(field) T.func(a: (string | number))\n```\n\n---\n\n注释1\n\n注释2\n\n---\n\n```lua\n(field) T.func(a: string)\n```\n\n```lua\n(field) T.func(a: number)\n```"
                    .to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_issue_499_4() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---@class T
                ---@field func fun(a:string) 注释1
                ---@field func fun(a:number) 注释2

                ---@type T
                local t = {
                    func = function(a)
                    end
                }

                t.fu<??>nc(1)
            "#,
            VirtualHoverResult {
                value: "```lua\n(field) T.func(a: number)\n```\n\n---\n\n注释2".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_origin_decl_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---@class T
                ---@field func fun(a:string) 注释1
                ---@field func fun(a:number) 注释2

                ---@type T
                local t = {
                    func = function(a)
                    end
                }
                local ab<??>c = t.func
            "#,
            VirtualHoverResult {
                value: "```lua\n(field) T.func(a: number)\n```\n\n---\n\n注释2\n\n注释1\n\n---\n\n```lua\n(field) T.func(a: string)\n```".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_first_generic() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---@class Reactive
                local M

                ---@generic T: table
                ---@param target T
                ---@return T
                function M.reac<??>tive(target)
                end
            "#,
            VirtualHoverResult {
                value: "```lua\nfunction Reactive.reactive(target: T)\n  -> T\n\n```".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_table_field_function() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                local export = {}
                ---@type fun()
                export.NO<??>OP = function() end
            "#,
            VirtualHoverResult {
                value: "```lua\nfunction export.NOOP()\n```".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_return_union_function() -> Result<()> {
        // temp remove the test
        // let mut ws = ProviderVirtualWorkspace::new();
        // check!(ws.check_hover(
        //     r#"
        //         ---@generic T
        //         ---@param initialValue? T
        //         ---@return (fun(): T) | (fun(value: T))
        //         local function signal(initialValue)
        //         end

        //         ---测试
        //         local cou<??>nt = signal(1)
        //     "#,
        //     VirtualHoverResult {
        //         value: "```lua\nfunction count(value: 1)\n```\n\n---\n\n测试\n\n---\n\n```lua\nfunction count() -> 1\n```".to_string(),
        //     },
        // ));
        Ok(())
    }

    #[gtest]
    fn test_require_function() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "test.lua",
            r#"
                ---测试
                local function signal()
                end

                return {
                    signal = signal
                }
            "#,
        );
        check!(ws.check_hover(
            r#"
                local test = require("test")
                local si<??>gnal = test.signal
            "#,
            VirtualHoverResult {
                value: "```lua\nlocal function signal()\n```\n\n---\n\n测试".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_generic_function() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "test.lua",
            r#"
                ---@class Observable<T>
                local Observable

                ---@generic R
                ---@param selector fun(value: T, index?: integer): R
                function Observable:select(selector)
                end

                ---@type Observable<integer>
                source = {}
            "#,
        );
        check!(ws.check_hover(
            r#"
                source:<??>select(function(value)
                    return value
                end)
            "#,
            VirtualHoverResult {
                value: "```lua\n(method) Observable:select(selector: fun(value: integer, index: integer?) -> R)\n```".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_other_file_function() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "a.lua",
            r#"
                ---测试
                local function zipLatest(...)
                end
                return zipLatest
            "#,
        );
        check!(ws.check_hover(
            r#"
                local zipLatest = require("a")
                <??>zipLatest()
            "#,
            VirtualHoverResult {
                value: "```lua\nlocal function zipLatest(...)\n```\n\n---\n\n测试".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_hover_generic_function_params_description() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "a.lua",
            r#"
                ---@class RingBuffer<T>
                local RingBuffer = {}

                ---@param index integer 索引
                ---@return T? item
                function RingBuffer:get(index)
                end
            "#,
        );
        check!(ws.check_hover(
            r#"
                ---@type RingBuffer<string>
                local RingBuffer
                RingBuffer:<??>get(1)
            "#,
            VirtualHoverResult {
                value: "```lua\n(method) RingBuffer:get(index: integer) -> string?\n```\n\n---\n\n@*param* `index` — 索引".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_annotation_search() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "a.lua",
            r#"
                ---@version 5.4
                ---测试
                function test()
                end
            "#,
        );
        check!(ws.check_hover(
            r#"
                <??>test()
            "#,
            VirtualHoverResult {
                value: "```lua\nfunction test()\n```\n\n---\n\n测试".to_string(),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_field_remove_first() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_hover(
            r#"
                ---@class A<T>
                ---@field next fun(value: T) # 测试
                local A = {}

                A.<??>next()
            "#,
            VirtualHoverResult {
                value: "```lua\n(field) A.next(value: T)\n```\n\n---\n\n测试".to_string(),
            },
        ));
        Ok(())
    }
}
