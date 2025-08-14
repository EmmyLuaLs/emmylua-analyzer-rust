#[cfg(test)]
mod tests {
    use emmylua_code_analysis::{DocSyntax, Emmyrc, EmmyrcFilenameConvention};
    use googletest::prelude::*;
    use lsp_types::{CompletionItemKind, CompletionTriggerKind};

    use crate::handlers::test_lib::{ProviderVirtualWorkspace, VirtualCompletionItem, check};

    #[gtest]
    fn test_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();

        check!(ws.check_completion(
            r#"
                local zabcde
                za<??>
            "#,
            vec![VirtualCompletionItem {
                label: "zabcde".to_string(),
                kind: CompletionItemKind::VARIABLE,
                ..Default::default()
            }],
        ));
        Ok(())
    }

    #[gtest]
    fn test_2() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_completion(
            r#"
                ---@overload fun(event: "AAA", callback: fun(trg: string, data: number)): number
                ---@overload fun(event: "BBB", callback: fun(trg: string, data: string)): string
                local function test(event, callback)
                end

                test("AAA", function(trg, data)
                <??>
                end)
            "#,
            vec![
                VirtualCompletionItem {
                    label: "data".to_string(),
                    kind: CompletionItemKind::VARIABLE,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "trg".to_string(),
                    kind: CompletionItemKind::VARIABLE,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "test".to_string(),
                    kind: CompletionItemKind::FUNCTION,
                    label_detail: Some("(event, callback)".to_string()),
                },
            ],
        ));

        // 主动触发补全
        check!(ws.check_completion(
            r#"
                ---@overload fun(event: "AAA", callback: fun(trg: string, data: number)): number
                ---@overload fun(event: "BBB", callback: fun(trg: string, data: string)): string
                local function test(event, callback)
                end
                test(<??>)
            "#,
            vec![
                VirtualCompletionItem {
                    label: "\"AAA\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "\"BBB\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "test".to_string(),
                    kind: CompletionItemKind::FUNCTION,
                    label_detail: Some("(event, callback)".to_string()),
                },
            ],
        ));

        // 被动触发补全
        check!(ws.check_completion_with_kind(
            r#"
                ---@overload fun(event: "AAA", callback: fun(trg: string, data: number)): number
                ---@overload fun(event: "BBB", callback: fun(trg: string, data: string)): string
                local function test(event, callback)
                end
                test(<??>)
            "#,
            vec![
                VirtualCompletionItem {
                    label: "\"AAA\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "\"BBB\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
        Ok(())
    }

    #[gtest]
    fn test_3() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        // 被动触发补全
        check!(ws.check_completion_with_kind(
            r#"
                ---@class Test
                ---@field event fun(a: "A", b: number)
                ---@field event fun(a: "B", b: string)
                local Test = {}
                Test.event(<??>)
            "#,
            vec![
                VirtualCompletionItem {
                    label: "\"A\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "\"B\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));

        // 主动触发补全
        check!(ws.check_completion(
            r#"
                ---@class Test1
                ---@field event fun(a: "A", b: number)
                ---@field event fun(a: "B", b: string)
                local Test = {}
                Test.event(<??>)
            "#,
            vec![
                VirtualCompletionItem {
                    label: "\"A\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "\"B\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "Test".to_string(),
                    kind: CompletionItemKind::CLASS,
                    ..Default::default()
                },
            ],
        ));

        check!(ws.check_completion(
            r#"
                ---@class Test2
                ---@field event fun(a: "A", b: number)
                ---@field event fun(a: "B", b: string)
                local Test = {}
                Test.<??>
            "#,
            vec![VirtualCompletionItem {
                label: "event".to_string(),
                kind: CompletionItemKind::FUNCTION,
                label_detail: Some("(a, b)".to_string()),
            }],
        ));
        Ok(())
    }

    #[gtest]
    fn test_4() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        check!(ws.check_completion(
            r#"
                local isIn = setmetatable({}, {
                    ---@return string <??>
                    __index = function(t, k) return k end,
                })
            "#,
            vec![]
        ));
        Ok(())
    }

    #[gtest]
    fn test_5() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        check!(ws.check_completion(
            r#"
                ---@class Test
                ---@field event fun(a: "A", b: number)
                ---@field event fun(a: "B", b: string)
                local Test = {}
                Test.event("<??>")
            "#,
            vec![
                VirtualCompletionItem {
                    label: "A".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "B".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
        ));

        check!(ws.check_completion(
            r#"
                ---@overload fun(event: "AAA", callback: fun(trg: string, data: number)): number
                ---@overload fun(event: "BBB", callback: fun(trg: string, data: string)): string
                local function test(event, callback)
                end
                test("<??>")
            "#,
            vec![
                VirtualCompletionItem {
                    label: "AAA".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "BBB".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
        ));
        Ok(())
    }

    #[gtest]
    fn test_enum() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();

        check!(ws.check_completion(
            r#"
                ---@overload fun(event: C6.Param, callback: fun(trg: string, data: number)): number
                ---@overload fun(event: C6.Param, callback: fun(trg: string, data: string)): string
                local function test2(event, callback)
                end

                ---@enum C6.Param
                local EP = {
                    A = "A",
                    B = "B"
                }

                test2(<??>)
            "#,
            vec![
                VirtualCompletionItem {
                    label: "EP.A".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "EP.B".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
        ));
        Ok(())
    }

    #[gtest]
    fn test_enum_string() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();

        check!(ws.check_completion(
            r#"
                ---@overload fun(event: C6.Param, callback: fun(trg: string, data: number)): number
                ---@overload fun(event: C6.Param, callback: fun(trg: string, data: string)): string
                local function test2(event, callback)
                end

                ---@enum C6.Param
                local EP = {
                    A = "A",
                    B = "B"
                }

                test2("<??>")
            "#,
            vec![
                VirtualCompletionItem {
                    label: "A".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "B".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
        ));
        Ok(())
    }

    #[gtest]
    fn test_type_comparison() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@alias std.type
                ---| "nil"
                ---| "number"
                ---| "string"

                ---@param v any
                ---@return std.type type
                function type(v) end
            "#,
        );
        check!(ws.check_completion(
            r#"
                local a = 1

                if type(a) == "<??>" then
                elseif type(a) == "boolean" then
                end
            "#,
            vec![
                VirtualCompletionItem {
                    label: "nil".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "number".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "string".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
        ));

        check!(ws.check_completion_with_kind(
            r#"
                local a = 1

                if type(a) == <??> then
                end
            "#,
            vec![
                VirtualCompletionItem {
                    label: "\"nil\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "\"number\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "\"string\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));

        check!(ws.check_completion_with_kind(
            r#"
                local a = 1

                if type(a) ~= "nil" then
                elseif type(a) == <??> then
                end
            "#,
            vec![
                VirtualCompletionItem {
                    label: "\"nil\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "\"number\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "\"string\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
        Ok(())
    }

    #[gtest]
    fn test_issue_272() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_completion_with_kind(
            r#"
                ---@class Box

                ---@class BoxyBox : Box

                ---@class Truck
                ---@field box Box
                local Truck = {}

                ---@class TruckyTruck : Truck
                ---@field box BoxyBox
                local TruckyTruck = {}
                TruckyTruck.<??>
            "#,
            vec![VirtualCompletionItem {
                label: "box".to_string(),
                kind: CompletionItemKind::VARIABLE,
                ..Default::default()
            }],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
        Ok(())
    }

    #[gtest]
    fn test_function_self() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_completion_with_kind(
            r#"
                ---@class A
                local A
                function A:test()
                s<??>
                end
            "#,
            vec![VirtualCompletionItem {
                label: "self".to_string(),
                kind: CompletionItemKind::VARIABLE,
                ..Default::default()
            }],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
        Ok(())
    }

    #[gtest]
    fn test_class_attr() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_completion_with_kind(
            r#"
                ---@class (<??>) A
                ---@field a string
            "#,
            vec![
                VirtualCompletionItem {
                    label: "partial".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "exact".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "constructor".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));

        check!(ws.check_completion_with_kind(
            r#"
                ---@class (partial,<??>) B
                ---@field a string
            "#,
            vec![
                VirtualCompletionItem {
                    label: "exact".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "constructor".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));

        check!(ws.check_completion_with_kind(
            r#"
                ---@enum (<??>) C
            "#,
            vec![
                VirtualCompletionItem {
                    label: "key".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "partial".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "exact".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
        Ok(())
    }

    #[gtest]
    fn test_str_tpl_ref_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        check!(ws.check_completion_with_kind(
            r#"
                ---@class A
                ---@class B
                ---@class C

                ---@generic T
                ---@param name `T`
                ---@return T
                local function new(name)
                    return name
                end

                local a = new(<??>)
            "#,
            vec![
                VirtualCompletionItem {
                    label: "\"A\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "\"B\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "\"C\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
        Ok(())
    }

    #[gtest]
    fn test_str_tpl_ref_2() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
                ---@namespace N
                ---@class C
            "#,
        );
        check!(ws.check_completion_with_kind(
            r#"
                ---@class A
                ---@class B

                ---@generic T
                ---@param name N.`T`
                ---@return T
                local function new(name)
                    return name
                end

                local a = new(<??>)
            "#,
            vec![VirtualCompletionItem {
                label: "\"C\"".to_string(),
                kind: CompletionItemKind::ENUM_MEMBER,
                ..Default::default()
            }],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
        Ok(())
    }

    #[gtest]
    fn test_str_tpl_ref_3() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
                ---@class Component
                ---@class C: Component

                ---@class D: C
            "#,
        );
        check!(ws.check_completion_with_kind(
            r#"
                ---@class A
                ---@class B

                ---@generic T: Component
                ---@param name `T`
                ---@return T
                local function new(name)
                    return name
                end

                local a = new(<??>)
            "#,
            vec![
                VirtualCompletionItem {
                    label: "\"C\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "\"Component\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "\"D\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
        Ok(())
    }

    #[gtest]
    fn test_str_tpl_ref_4() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@class C: string

            ---@class D: C
            "#,
        );
        check!(ws.check_completion_with_kind(
            r#"
            ---@generic T: string
            ---@param name `T`
            ---@return T
            local function new(name)
                return name
            end

            local a = new(<??>)
            "#,
            vec![
                VirtualCompletionItem {
                    label: "\"C\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "\"D\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
        Ok(())
    }

    #[gtest]
    fn test_table_field_function_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        check!(ws.check_completion_with_kind(
            r#"
                ---@class T
                ---@field func fun(self:string) 注释注释

                ---@type T
                local t = {
                    <??>
                }
            "#,
            vec![VirtualCompletionItem {
                label: "func =".to_string(),
                kind: CompletionItemKind::PROPERTY,
                ..Default::default()
            }],
            CompletionTriggerKind::INVOKED,
        ));
        Ok(())
    }
    #[gtest]
    fn test_table_field_function_2() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_completion_with_kind(
            r#"
                ---@class T
                ---@field func fun(self:string) 注释注释

                ---@type T
                local t = {
                    func = <??>
                }
            "#,
            vec![VirtualCompletionItem {
                label: "fun".to_string(),
                kind: CompletionItemKind::SNIPPET,
                label_detail: Some("(self)".to_string()),
            }],
            CompletionTriggerKind::INVOKED,
        ));
        Ok(())
    }

    #[gtest]
    fn test_issue_499() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_completion_with_kind(
            r#"
                ---@class T
                ---@field func fun(a:string): string

                ---@type T
                local t = {
                    func = <??>
                }
            "#,
            vec![VirtualCompletionItem {
                label: "fun".to_string(),
                kind: CompletionItemKind::SNIPPET,
                label_detail: Some("(a)".to_string()),
            }],
            CompletionTriggerKind::INVOKED,
        ));
        Ok(())
    }

    #[gtest]
    fn test_enum_field_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@enum Enum
                local Enum = {
                    a = 1,
                }
            "#,
        );
        check!(ws.check_completion_with_kind(
            r#"
                ---@param p Enum
                function func(p)
                    local x1 = p.<??>
                end
            "#,
            vec![],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
        Ok(())
    }

    #[gtest]
    fn test_issue_502() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@param a { foo: { bar: number } }
                function buz(a) end
            "#,
        );
        check!(ws.check_completion_with_kind(
            r#"
                buz({
                    foo = {
                        b<??>
                    }
                })
            "#,
            vec![VirtualCompletionItem {
                label: "bar = ".to_string(),
                kind: CompletionItemKind::PROPERTY,
                ..Default::default()
            }],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
        Ok(())
    }

    #[gtest]
    fn test_class_function_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@class C1
                ---@field on_add fun(a: string, b: string)
            "#,
        );
        check!(ws.check_completion_with_kind(
            r#"
                ---@type C1
                local c1

                c1.on_add = <??>
            "#,
            vec![VirtualCompletionItem {
                label: "function(a, b) end".to_string(),
                kind: CompletionItemKind::FUNCTION,
                ..Default::default()
            }],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
        Ok(())
    }

    #[gtest]
    fn test_class_function_2() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@class C1
                ---@field on_add fun(self: C1, a: string, b: string)
            "#,
        );
        check!(ws.check_completion_with_kind(
            r#"
                ---@type C1
                local c1

                function c1:<??>()

                end
            "#,
            vec![VirtualCompletionItem {
                label: "on_add".to_string(),
                kind: CompletionItemKind::FUNCTION,
                label_detail: Some("(a, b)".to_string()),
            }],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
        Ok(())
    }

    #[gtest]
    fn test_class_function_3() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@class (partial) SkillMutator
                ---@field on_add? fun(self: self, owner: string)

                ---@class (partial) SkillMutator.A
                ---@field on_add? fun(self: self, owner: string)
            "#,
        );
        check!(ws.check_completion_with_kind(
            r#"
                ---@class (partial) SkillMutator.A
                local a
                a.on_add = <??>
            "#,
            vec![VirtualCompletionItem {
                label: "function(self, owner) end".to_string(),
                kind: CompletionItemKind::FUNCTION,
                ..Default::default()
            }],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
        Ok(())
    }

    #[gtest]
    fn test_class_function_4() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@class (partial) SkillMutator
                ---@field on_add? fun(self: self, owner: string)

                ---@class (partial) SkillMutator.A
                ---@field on_add? fun(self: self, owner: string)
            "#,
        );
        check!(ws.check_completion_with_kind(
            r#"
                ---@class (partial) SkillMutator.A
                local a
                function a:<??>()

                end

            "#,
            vec![VirtualCompletionItem {
                label: "on_add".to_string(),
                kind: CompletionItemKind::FUNCTION,
                label_detail: Some("(owner)".to_string()),
            }],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
        Ok(())
    }

    #[gtest]
    fn test_auto_require() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        let mut emmyrc = ws.get_emmyrc();
        emmyrc.completion.auto_require_naming_convention = EmmyrcFilenameConvention::KeepClass;
        ws.update_emmyrc(emmyrc);
        ws.def_file(
            "map.lua",
            r#"
                ---@class Map
                local Map = {}

                return Map
            "#,
        );
        check!(ws.check_completion(
            r#"
                ma<??>
            "#,
            vec![VirtualCompletionItem {
                label: "Map".to_string(),
                kind: CompletionItemKind::MODULE,
                label_detail: Some("    (in map)".to_string()),
            }],
        ));
        Ok(())
    }

    #[gtest]
    fn test_auto_require_table_field() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "aaaa.lua",
            r#"
                ---@export
                local export = {}

                ---@enum MapName
                export.MapName = {
                    A = 1,
                    B = 2,
                }

                return export
            "#,
        );
        ws.def_file(
            "bbbb.lua",
            r#"
                local export = {}

                ---@enum PA
                export.PA = {
                    A = 1,
                }

                return export
            "#,
        );
        check!(ws.check_completion(
            r#"
                mapn<??>
            "#,
            vec![VirtualCompletionItem {
                label: "MapName".to_string(),
                kind: CompletionItemKind::CLASS,
                label_detail: Some("    (in aaaa)".to_string()),
            }],
        ));
        Ok(())
    }

    #[gtest]
    fn test_field_is_alias_function() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@alias ProxyHandler.Setter fun(raw: any)

                ---@class ProxyHandler
                ---@field set? ProxyHandler.Setter
            "#,
        );
        check!(ws.check_completion_with_kind(
            r#"
                ---@class MHandler: ProxyHandler
                local MHandler

                MHandler.set = <??>
            "#,
            vec![VirtualCompletionItem {
                label: "function(raw) end".to_string(),
                kind: CompletionItemKind::FUNCTION,
                ..Default::default()
            }],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
        Ok(())
    }

    #[gtest]
    fn test_namespace_base() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@namespace Reactive
            "#,
        );
        ws.def(
            r#"
                ---@namespace AlienSignals
            "#,
        );
        check!(ws.check_completion_with_kind(
            r#"
                ---@namespace <??>
            "#,
            vec![
                VirtualCompletionItem {
                    label: "AlienSignals".to_string(),
                    kind: CompletionItemKind::MODULE,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "Reactive".to_string(),
                    kind: CompletionItemKind::MODULE,
                    ..Default::default()
                },
            ],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));

        check!(ws.check_completion_with_kind(
            r#"
                ---@namespace Reactive
                ---@namespace <??>
            "#,
            vec![],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));

        check!(ws.check_completion_with_kind(
            r#"
                ---@namespace Reactive
                ---@using <??>
            "#,
            vec![VirtualCompletionItem {
                label: "using AlienSignals".to_string(),
                kind: CompletionItemKind::MODULE,
                ..Default::default()
            }],
            CompletionTriggerKind::INVOKED,
        ));
        Ok(())
    }

    #[gtest]
    fn test_auto_require_field_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        // 没有 export 标记, 不允许子字段自动导入
        ws.def_file(
            "AAA.lua",
            r#"
                local function map()
                end
                return {
                    map = map,
                }
            "#,
        );
        check!(ws.check_completion(
            r#"
                map<??>
            "#,
            vec![],
        ));
        Ok(())
    }

    #[gtest]
    fn test_issue_558() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "AAA.lua",
            r#"
                    ---@class ability
                    ---@field t abilityType

                    ---@enum (key) abilityType
                    local abilityType = {
                        passive = 1,
                    }

                    ---@param a ability
                    function test(a)

                    end
            "#,
        );
        check!(ws.check_completion(
            r#"
                test({
                    t = <??>
                })
            "#,
            vec![VirtualCompletionItem {
                label: "\"passive\"".to_string(),
                kind: CompletionItemKind::ENUM_MEMBER,
                ..Default::default()
            }],
        ));
        Ok(())
    }

    #[gtest]
    fn test_index_key_alias() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_completion(
            r#"
                local export = {
                    [1] = 1, -- [nameX]
                }

                export.<??>
            "#,
            vec![VirtualCompletionItem {
                label: "nameX".to_string(),
                kind: CompletionItemKind::CONSTANT,
                ..Default::default()
            }],
        ));
        Ok(())
    }

    #[gtest]
    fn test_issue_572() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_completion(
            r#"
                ---@class A
                ---@field optional_num number?
                local a = {}

                function a:set()
                end

                --- @class B : A
                local b = {}

                function b:set()
                    self.optional_num = 2
                end
                b.<??>
            "#,
            vec![
                VirtualCompletionItem {
                    label: "optional_num".to_string(),
                    kind: CompletionItemKind::VARIABLE,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "set".to_string(),
                    kind: CompletionItemKind::FUNCTION,
                    label_detail: Some("(self) -> nil".to_string()),
                },
            ],
        ));
        Ok(())
    }

    #[gtest]
    fn test_file_start() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        check!(ws.check_completion(
            "table<??>",
            vec![VirtualCompletionItem {
                label: "table".to_string(),
                kind: CompletionItemKind::CLASS,
                ..Default::default()
            }],
        ));
        Ok(())
    }

    #[gtest]
    fn test_field_index_function() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@class A<T>
                ---@field [1] fun() # [next]
                A = {}
            "#,
        );
        // 测试索引成员别名语法
        check!(ws.check_completion(
            r#"
                A.<??>
            "#,
            vec![VirtualCompletionItem {
                label: "next".to_string(),
                kind: CompletionItemKind::FUNCTION,
                label_detail: Some("()".to_string()),
            }],
        ));
        Ok(())
    }

    #[gtest]
    fn test_private_config() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        let mut emmyrc = ws.get_emmyrc();
        emmyrc.doc.private_name = vec!["_*".to_string()];
        ws.update_emmyrc(emmyrc);
        ws.def(
            r#"
                ---@class A
                ---@field _abc number
                ---@field _next fun()
                A = {}
            "#,
        );
        check!(ws.check_completion(
            r#"
                ---@type A
                local a
                a.<??>
            "#,
            vec![],
        ));
        check!(ws.check_completion(
            r#"
                A.<??>
            "#,
            vec![
                VirtualCompletionItem {
                    label: "_abc".to_string(),
                    kind: CompletionItemKind::VARIABLE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "_next".to_string(),
                    kind: CompletionItemKind::FUNCTION,
                    label_detail: Some("()".to_string()),
                },
            ],
        ));
        Ok(())
    }

    #[gtest]
    fn test_require_private() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        let mut emmyrc = ws.get_emmyrc();
        emmyrc.doc.private_name = vec!["_*".to_string()];
        ws.update_emmyrc(emmyrc);
        ws.def_file(
            "a.lua",
            r#"
                ---@class A
                ---@field _next fun()
                local A = {}

                return {
                    A = A,
                }
            "#,
        );
        check!(ws.check_completion(
            r#"
                local A = require("a").A
                A.<??>
            "#,
            vec![],
        ));
        Ok(())
    }

    #[gtest]
    fn test_doc_completion() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();

        let mut emmyrc = Emmyrc::default();
        emmyrc.doc.syntax = DocSyntax::Rst;
        ws.analysis.update_config(emmyrc.into());

        ws.def_file(
            "mod_empty.lua",
            r#"
            "#,
        );

        ws.def_file(
            "mod_with_class.lua",
            r#"
                --- @class mod_with_class.Cls
                --- @class mod_with_class.ns1.ns2.Cls
            "#,
        );

        ws.def_file(
            "mod_with_class_and_def.lua",
            r#"
                local ns = {}

                --- @class mod_with_class_and_def.Cls
                ns.Cls = {}

                function ns.foo() end

                return ns
            "#,
        );

        ws.def_file(
            "mod_with_sub_mod.lua",
            r#"
                GLOBAL = 0
                return {
                    x = 1
                }
            "#,
        );

        ws.def_file(
            "mod_with_sub_mod/sub_mod.lua",
            r#"
                return {
                    foo = 1,
                    bar = function() end,
                }
            "#,
        );

        ws.def_file(
            "cls.lua",
            r#"
                --- @class Foo
                --- @field x integer
                --- @field [1] string
            "#,
        );

        check!(ws.check_completion(
            r#"
                --- :lua:obj:`<??>`

                return {
                    foo = 0
                }
            "#,
            vec![
                VirtualCompletionItem {
                    label: "mod_with_class_and_def".to_string(),
                    kind: CompletionItemKind::MODULE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "mod_with_class".to_string(),
                    kind: CompletionItemKind::MODULE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "Foo".to_string(),
                    kind: CompletionItemKind::CLASS,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "GLOBAL".to_string(),
                    kind: CompletionItemKind::CONSTANT,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "mod_with_class_and_def".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "virtual_0".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "foo".to_string(),
                    kind: CompletionItemKind::CONSTANT,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "mod_with_class".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "cls".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "mod_empty".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "mod_with_sub_mod".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
            ],
        ));

        check!(ws.check_completion(r"--- :lua:obj:`mod_empty.<??>`", vec![]));

        check!(ws.check_completion(
            r"--- :lua:obj:`mod_with_class.<??>`",
            vec![
                VirtualCompletionItem {
                    label: "Cls".to_string(),
                    kind: CompletionItemKind::CLASS,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "ns1".to_string(),
                    kind: CompletionItemKind::MODULE,
                    label_detail: None,
                },
            ],
        ));

        check!(ws.check_completion(
            r"--- :lua:obj:`mod_with_class.ns1.<??>`",
            vec![VirtualCompletionItem {
                label: "ns2".to_string(),
                kind: CompletionItemKind::MODULE,
                label_detail: None,
            }],
        ));

        check!(ws.check_completion(
            r"--- :lua:obj:`mod_with_class.ns1.ns2.<??>`",
            vec![VirtualCompletionItem {
                label: "Cls".to_string(),
                kind: CompletionItemKind::CLASS,
                label_detail: None,
            }],
        ));

        check!(ws.check_completion(
            r"--- :lua:obj:`mod_with_class_and_def.<??>`",
            vec![
                VirtualCompletionItem {
                    label: "Cls".to_string(),
                    kind: CompletionItemKind::CLASS,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "foo".to_string(),
                    kind: CompletionItemKind::FUNCTION,
                    label_detail: Some("()".to_string()),
                },
            ],
        ));

        check!(ws.check_completion(
            r"--- :lua:obj:`mod_with_sub_mod.<??>`",
            vec![
                VirtualCompletionItem {
                    label: "sub_mod".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "x".to_string(),
                    kind: CompletionItemKind::CONSTANT,
                    label_detail: None,
                },
            ],
        ));

        check!(ws.check_completion(
            r"--- :lua:obj:`mod_with_sub_mod.sub_mod.<??>`",
            vec![
                VirtualCompletionItem {
                    label: "bar".to_string(),
                    kind: CompletionItemKind::FUNCTION,
                    label_detail: Some("()".to_string()),
                },
                VirtualCompletionItem {
                    label: "foo".to_string(),
                    kind: CompletionItemKind::CONSTANT,
                    label_detail: None,
                },
            ],
        ));

        check!(ws.check_completion(
            r"--- :lua:obj:`Foo.<??>`",
            vec![
                VirtualCompletionItem {
                    label: "[1]".to_string(),
                    kind: CompletionItemKind::VARIABLE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "x".to_string(),
                    kind: CompletionItemKind::VARIABLE,
                    label_detail: None,
                },
            ],
        ));

        Ok(())
    }

    #[gtest]
    fn test_doc_completion_in_members() -> Result<()> {
        let make_ws = || {
            let mut ws = ProviderVirtualWorkspace::new();

            let mut emmyrc = Emmyrc::default();
            emmyrc.doc.syntax = DocSyntax::Rst;
            ws.analysis.update_config(emmyrc.into());
            ws
        };

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- @class Foo
                --- @field x integer
                local Foo = {}

                --- :lua:obj:`<??>`
                Foo.y = 0
            "#,
            vec![
                VirtualCompletionItem {
                    label: "Foo".to_string(),
                    kind: CompletionItemKind::CLASS,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "virtual_0".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "x".to_string(),
                    kind: CompletionItemKind::VARIABLE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "y".to_string(),
                    kind: CompletionItemKind::CONSTANT,
                    label_detail: None,
                },
            ],
        ));

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- @class Foo
                --- @field x integer
                local Foo = {}

                --- :lua:obj:`<??>`
                Foo.y = function() end
            "#,
            vec![
                VirtualCompletionItem {
                    label: "Foo".to_string(),
                    kind: CompletionItemKind::CLASS,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "virtual_0".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "x".to_string(),
                    kind: CompletionItemKind::VARIABLE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "y".to_string(),
                    kind: CompletionItemKind::FUNCTION,
                    label_detail: Some("()".to_string()),
                },
            ],
        ));

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- @class Foo
                --- @field x integer
                local Foo = {}

                --- :lua:obj:`<??>`
                function Foo.y() end
            "#,
            vec![
                VirtualCompletionItem {
                    label: "Foo".to_string(),
                    kind: CompletionItemKind::CLASS,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "virtual_0".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "x".to_string(),
                    kind: CompletionItemKind::VARIABLE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "y".to_string(),
                    kind: CompletionItemKind::FUNCTION,
                    label_detail: Some("()".to_string()),
                },
            ],
        ));

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- @class Foo
                --- @field x integer
                local Foo = {}

                --- :lua:obj:`<??>`
                function Foo:y() end
            "#,
            vec![
                VirtualCompletionItem {
                    label: "Foo".to_string(),
                    kind: CompletionItemKind::CLASS,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "virtual_0".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "x".to_string(),
                    kind: CompletionItemKind::VARIABLE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "y".to_string(),
                    kind: CompletionItemKind::FUNCTION,
                    label_detail: Some("(self)".to_string()),
                },
            ],
        ));

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- @class Foo
                --- @field x integer
                local Foo = {
                    --- :lua:obj:`<??>`
                    y = 0
                }
            "#,
            vec![
                VirtualCompletionItem {
                    label: "Foo".to_string(),
                    kind: CompletionItemKind::CLASS,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "virtual_0".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "x".to_string(),
                    kind: CompletionItemKind::VARIABLE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "y".to_string(),
                    kind: CompletionItemKind::CONSTANT,
                    label_detail: None,
                },
            ],
        ));

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- @class Foo
                --- @field x integer
                local Foo = {
                    --- :lua:obj:`<??>`
                    y = function() end
                }
            "#,
            vec![
                VirtualCompletionItem {
                    label: "Foo".to_string(),
                    kind: CompletionItemKind::CLASS,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "virtual_0".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "x".to_string(),
                    kind: CompletionItemKind::VARIABLE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "y".to_string(),
                    kind: CompletionItemKind::FUNCTION,
                    label_detail: Some("()".to_string()),
                },
            ],
        ));

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- @class Foo
                --- @field x integer
                local Foo = {}

                function Foo:init()
                    --- :lua:obj:`<??>`
                    self.y = 0
                end
            "#,
            vec![
                VirtualCompletionItem {
                    label: "Foo".to_string(),
                    kind: CompletionItemKind::CLASS,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "virtual_0".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "x".to_string(),
                    kind: CompletionItemKind::VARIABLE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "y".to_string(),
                    kind: CompletionItemKind::CONSTANT,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "init".to_string(),
                    kind: CompletionItemKind::FUNCTION,
                    label_detail: Some("(self) -> nil".to_string()),
                },
            ],
        ));

        Ok(())
    }

    #[gtest]
    fn test_doc_completion_myst_empty() -> Result<()> {
        let make_ws = || {
            let mut ws = ProviderVirtualWorkspace::new();
            let mut emmyrc = Emmyrc::default();
            emmyrc.doc.syntax = DocSyntax::Myst;
            ws.analysis.update_config(emmyrc.into());

            ws.def_file(
                "a.lua",
                r#"
                ---@class A
            "#,
            );

            ws
        };

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- {lua:obj}<??>``...
            "#,
            vec![],
        ));

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- {lua:obj}`<??>`...
            "#,
            vec![
                VirtualCompletionItem {
                    label: "A".to_string(),
                    kind: CompletionItemKind::CLASS,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "a".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "virtual_0".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
            ],
        ));

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- {lua:obj}``<??>...
            "#,
            vec![],
        ));

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- {lua:obj}`<??>...
            "#,
            vec![
                VirtualCompletionItem {
                    label: "A".to_string(),
                    kind: CompletionItemKind::CLASS,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "a".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "virtual_0".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
            ],
        ));

        Ok(())
    }

    #[gtest]
    fn test_doc_completion_rst_empty() -> Result<()> {
        let make_ws = || {
            let mut ws = ProviderVirtualWorkspace::new();
            let mut emmyrc = Emmyrc::default();
            emmyrc.doc.syntax = DocSyntax::Rst;
            ws.analysis.update_config(emmyrc.into());

            ws.def_file(
                "a.lua",
                r#"
                ---@class A
            "#,
            );

            ws
        };

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- :lua:obj:<??>``...
            "#,
            vec![],
        ));

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- :lua:obj:`<??>`...
            "#,
            vec![
                VirtualCompletionItem {
                    label: "A".to_string(),
                    kind: CompletionItemKind::CLASS,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "a".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "virtual_0".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
            ],
        ));

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- :lua:obj:``<??>...
            "#,
            vec![],
        ));

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- :lua:obj:`<??>...
            "#,
            vec![
                VirtualCompletionItem {
                    label: "A".to_string(),
                    kind: CompletionItemKind::CLASS,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "a".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "virtual_0".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
            ],
        ));

        Ok(())
    }

    #[gtest]
    fn test_doc_completion_rst_default_role_empty() -> Result<()> {
        let make_ws = || {
            let mut ws = ProviderVirtualWorkspace::new();
            let mut emmyrc = Emmyrc::default();
            emmyrc.doc.syntax = DocSyntax::Rst;
            emmyrc.doc.rst_default_role = Some("lua:obj".to_string());
            ws.analysis.update_config(emmyrc.into());

            ws.def_file(
                "a.lua",
                r#"
                ---@class A
            "#,
            );

            ws
        };

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- <??>``...
            "#,
            vec![],
        ));

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- `<??>`...
            "#,
            vec![
                VirtualCompletionItem {
                    label: "A".to_string(),
                    kind: CompletionItemKind::CLASS,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "a".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "virtual_0".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
            ],
        ));

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- ``<??>...
            "#,
            vec![],
        ));

        let mut ws = make_ws();
        check!(ws.check_completion(
            r#"
                --- `<??>...
            "#,
            vec![
                VirtualCompletionItem {
                    label: "A".to_string(),
                    kind: CompletionItemKind::CLASS,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "a".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
                VirtualCompletionItem {
                    label: "virtual_0".to_string(),
                    kind: CompletionItemKind::FILE,
                    label_detail: None,
                },
            ],
        ));

        Ok(())
    }

    #[gtest]
    fn test_issue_646() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Base
            ---@field a string
            "#,
        );
        check!(ws.check_completion(
            r#"
            ---@generic T: Base
            ---@param file T
            function dirname(file)
                file.<??>
            end
            "#,
            vec![VirtualCompletionItem {
                label: "a".to_string(),
                kind: CompletionItemKind::VARIABLE,
                ..Default::default()
            },],
        ));
        Ok(())
    }

    #[gtest]
    fn test_see_completion() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Meep
            "#,
        );
        check!(ws.check_completion(
            r#"
            --- @see M<??>
            "#,
            vec![
                VirtualCompletionItem {
                    label: "Meep".to_string(),
                    kind: CompletionItemKind::CLASS,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "virtual_0".to_string(),
                    kind: CompletionItemKind::FILE,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "virtual_1".to_string(),
                    kind: CompletionItemKind::FILE,
                    ..Default::default()
                },
            ],
        ));
        Ok(())
    }
}
