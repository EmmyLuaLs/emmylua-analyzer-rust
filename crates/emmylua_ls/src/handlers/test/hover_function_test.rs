#[cfg(test)]
mod tests {

    use crate::handlers::test_lib::{ProviderVirtualWorkspace, VirtualHoverResult};

    #[test]
    fn test_1() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(ws.check_hover(
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
                value: "\n```lua\nlocal function delete(a: number)\n  -> a: number\n\n```\n\n---\n\n@*param* `a` — 参数a\n\n\n\n@*return* `a`  — 返回值a\n\n\n".to_string(),
            },
        ));

        assert!(ws.check_hover(
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
                value: "\n```lua\nlocal function delete(a: number)\n  -> a: number\n\n```\n\n---\n\n删除\n\n@*param* `a` — 参数a\n\n\n\n@*return* `a`  — 返回值a\n\n\n".to_string(),
            },
        ));

        assert!(ws.check_hover(
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
                value: "\n```lua\nlocal function delete(a: number)\n  -> a: number\n\n```\n\n---\n\n@*param* `a` — 参数a\n\n\n\n@*return* `a`  — 返回值a\n\n\n".to_string(),
            },
        ));
    }

    #[test]
    fn test_2() {
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

        assert!(ws.check_hover(
            r#"


            ---@type Game
            local game

            local local_a = game.add
            local <??>local_b = local_a
            "#,
            VirtualHoverResult {
                value: "\n```lua\n(method) Game:add(key: string, value: string)\n  -> ret: number\n\n```\n\n---\n\n说明\n\n@*param* `key` — 参数key\n\n@*param* `value` — 参数value\n\n\n\n@*return* `ret`  — 返回值\n\n\n" .to_string(),
            },
        ));
    }

    #[test]
    fn test_3() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Hover.Test3<T>
            ---@field event fun(self: self, event: "A", key: T)
            ---@field event fun(self: self, event: "B", key: T)
            local Test3 = {}
            "#,
        );

        assert!(ws.check_hover(
            r#"
                ---@type Hover.Test3<string>
                local test3

                local <??>event = test3.event
            "#,
            VirtualHoverResult {
                value: "\n```lua\n(method) Test3:event(event: \"B\", key: string)\n```\n\n&nbsp;&nbsp;in class `Hover.Test3`\n\n---\n\n---\n\n```lua\n(method) Test3:event(event: \"A\", key: string)\n```\n".to_string(),
            },
        ));
    }

    #[test]
    fn test_union_function() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(ws.check_hover(
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
                value: "\n```lua\n(method) GameA:event(event_type: EventTypeA, ...: any)\n  -> Trigger\n\n```\n\n---\n\n注册引擎事件\n\n---\n\n```lua\n(method) GameA:event(event: \"游戏-初始化\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"游戏-追帧完成\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"游戏-逻辑不同步\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"游戏-地形预设加载完成\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"游戏-结束\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"游戏-暂停\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"游戏-恢复\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"游戏-昼夜变化\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"区域-进入\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"区域-离开\") -> Trigger\n```\n\n```lua\n(method) GameA:event(event: \"游戏-http返回\") -> Trigger\n```\n".to_string(),
            },
        ));
    }

    #[test]
    fn test_4() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(ws.check_hover(
            r#"
                ---@class ClosureTest
                ---@field e fun(a: string, b: number)
                local Test

                function Test.<??>e(a, b)
                    A = a
                end
            "#,
            VirtualHoverResult {
                value: "\n```lua\nfunction ClosureTest.e(a: string, b: number)\n```\n\n---\n\n---\n\n```lua\n(field) ClosureTest.e(a: string, b: number)\n```\n".to_string(),
            },
        ));
    }

    #[test]
    fn test_table_field_function_1() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(ws.check_hover(
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
                value: "\n```lua\n(method) T:func()\n```\n\n---\n\n注释注释\n".to_string(),
            },
        ));
    }

    #[test]
    fn test_issue_499() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(ws.check_hover(
            r#"
                ---@class T
                ---@field a string 注释注释a

                ---@type T
                local t = {
                    a<??> = "a"
                }
            "#,
            VirtualHoverResult {
                value: "\n```lua\n(field) a: string = \"a\"\n```\n\n---\n\n注释注释a\n".to_string(),
            },
        ));
    }

    #[test]
    fn test_issue_499_2() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(ws.check_hover(
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
                value: "\n```lua\n(field) T.func(self: string)\n```\n\n---\n\n注释注释\n"
                    .to_string(),
            },
        ));
    }

    #[test]
    fn test_issue_499_3() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(ws.check_hover(
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
                value: "\n```lua\n(field) T.func(a: (string|number))\n```\n\n---\n\n注释1\n\n注释2\n\n---\n\n```lua\n(field) T.func(a: string)\n```\n\n```lua\n(field) T.func(a: number)\n```\n"
                    .to_string(),
            },
        ));
    }

    #[test]
    fn test_issue_499_4() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(ws.check_hover(
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
                value: "\n```lua\n(field) T.func(a: number)\n```\n\n---\n\n注释2\n".to_string(),
            },
        ));
    }

    #[test]
    fn test_origin_decl_1() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(ws.check_hover(
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
                value: "\n```lua\n(field) T.func(a: number)\n```\n\n---\n\n注释2\n\n注释1\n\n---\n\n```lua\n(field) T.func(a: string)\n```\n".to_string(),
            },
        ));
    }

    // #[test]
    // fn test_origin_decl_1() {
    //     let mut ws = ProviderVirtualWorkspace::new();
    //     assert!(ws.check_hover(
    //         r#"
    //             ---@class T
    //             ---@field func fun(a:string) 注释1
    //             ---@field func fun(a:number) 注释2

    //             ---@type T
    //             local t = {
    //                 func = function(a)
    //                 end
    //             }
    //             local abc = t.func
    //             a<??>bc(1)
    //         "#,
    //         VirtualHoverResult {
    //             value: "\n```lua\n(field) T.func(a: number)\n```\n\n---\n\n注释2\n".to_string(),
    //         },
    //     ));
    // }
}
