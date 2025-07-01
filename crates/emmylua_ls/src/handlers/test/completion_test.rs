#[cfg(test)]
mod tests {

    use emmylua_code_analysis::EmmyrcFilenameConvention;
    use lsp_types::{CompletionItemKind, CompletionTriggerKind};

    use crate::handlers::test_lib::{ProviderVirtualWorkspace, VirtualCompletionItem};

    #[test]
    fn test_1() {
        let mut ws = ProviderVirtualWorkspace::new();

        assert!(ws.check_completion(
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
    }

    #[test]
    fn test_2() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(ws.check_completion(
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
        assert!(ws.check_completion(
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
        assert!(ws.check_completion_with_kind(
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
    }

    #[test]
    fn test_3() {
        let mut ws = ProviderVirtualWorkspace::new();
        // 被动触发补全
        assert!(ws.check_completion_with_kind(
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
        assert!(ws.check_completion(
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

        assert!(ws.check_completion(
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
    }

    #[test]
    fn test_4() {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_completion(
            r#"
                local isIn = setmetatable({}, {
                    ---@return string <??>
                    __index = function(t, k) return k end,
                })
        "#,
            vec![]
        ));
    }

    #[test]
    fn test_5() {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_completion(
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

        assert!(ws.check_completion(
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
    }

    #[test]
    fn test_enum() {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_completion(
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
    }

    #[test]
    fn test_enum_string() {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_completion(
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
    }

    #[test]
    fn test_type_comparison() {
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
        assert!(ws.check_completion(
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

        assert!(ws.check_completion_with_kind(
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

        assert!(ws.check_completion_with_kind(
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
    }

    #[test]
    fn test_issue_272() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(ws.check_completion_with_kind(
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
            },],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
    }

    #[test]
    fn test_function_self() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(ws.check_completion_with_kind(
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
            },],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
    }

    #[test]
    fn test_class_attr() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(ws.check_completion_with_kind(
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
                    label: "key".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "constructor".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "exact".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "meta".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));

        assert!(ws.check_completion_with_kind(
            r#"
            ---@class (partial,<??>) B
            ---@field a string
            "#,
            vec![
                VirtualCompletionItem {
                    label: "key".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "constructor".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "exact".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "meta".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));

        assert!(ws.check_completion_with_kind(
            r#"
            ---@class (partial, <??>) C
            ---@field a string
            "#,
            vec![
                VirtualCompletionItem {
                    label: "key".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "constructor".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "exact".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
                VirtualCompletionItem {
                    label: "meta".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
    }

    #[test]
    fn test_str_tpl_ref_1() {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_completion_with_kind(
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
    }

    #[test]
    fn test_str_tpl_ref_2() {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@namespace N
            ---@class C
            "#,
        );
        assert!(ws.check_completion_with_kind(
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
            },],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
    }

    #[test]
    fn test_str_tpl_ref_3() {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@class Component
            ---@class C: Component

            ---@class D: C
            "#,
        );
        assert!(ws.check_completion_with_kind(
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
                    label: "\"D\"".to_string(),
                    kind: CompletionItemKind::ENUM_MEMBER,
                    ..Default::default()
                },
            ],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
    }

    #[test]
    fn test_table_field_function_1() {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_completion_with_kind(
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
            },],
            CompletionTriggerKind::INVOKED,
        ));
    }
    #[test]
    fn test_table_field_function_2() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(ws.check_completion_with_kind(
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
            },],
            CompletionTriggerKind::INVOKED,
        ));
    }

    #[test]
    fn test_issue_499() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(ws.check_completion_with_kind(
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
            },],
            CompletionTriggerKind::INVOKED,
        ));
    }

    #[test]
    fn test_enum_field_1() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@enum Enum
                local Enum = {
                    a = 1,
                }
        "#,
        );
        assert!(ws.check_completion_with_kind(
            r#"
                ---@param p Enum
                function func(p)
                    local x1 = p.<??>
                end
            "#,
            vec![],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
    }

    #[test]
    fn test_issue_502() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@param a { foo: { bar: number } }
                function buz(a) end
        "#,
        );
        assert!(ws.check_completion_with_kind(
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
            },],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
    }

    #[test]
    fn test_class_function_1() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@class C1
                ---@field on_add fun(a: string, b: string)
        "#,
        );
        assert!(ws.check_completion_with_kind(
            r#"
                ---@type C1
                local c1

                c1.on_add = <??>
            "#,
            vec![VirtualCompletionItem {
                label: "function(a, b) end".to_string(),
                kind: CompletionItemKind::FUNCTION,
                ..Default::default()
            },],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
    }

    #[test]
    fn test_class_function_2() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@class C1
                ---@field on_add fun(self: C1, a: string, b: string)
        "#,
        );
        assert!(ws.check_completion_with_kind(
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
            },],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
    }

    #[test]
    fn test_class_function_3() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@class (partial) SkillMutator
                ---@field on_add? fun(self: self, owner: string)

                ---@class (partial) SkillMutator.A
                ---@field on_add? fun(self: self, owner: string)
        "#,
        );
        assert!(ws.check_completion_with_kind(
            r#"
                ---@class (partial) SkillMutator.A
                local a
                a.on_add = <??>
            "#,
            vec![VirtualCompletionItem {
                label: "function(self, owner) end".to_string(),
                kind: CompletionItemKind::FUNCTION,
                ..Default::default()
            },],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
    }

    #[test]
    fn test_class_function_4() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@class (partial) SkillMutator
                ---@field on_add? fun(self: self, owner: string)

                ---@class (partial) SkillMutator.A
                ---@field on_add? fun(self: self, owner: string)
        "#,
        );
        assert!(ws.check_completion_with_kind(
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
            },],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
    }

    #[test]
    fn test_auto_require() {
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
        assert!(ws.check_completion(
            r#"
                ma<??>
            "#,
            vec![VirtualCompletionItem {
                label: "Map".to_string(),
                kind: CompletionItemKind::MODULE,
                label_detail: Some("    (in map)".to_string()),
            },],
        ));
    }

    #[test]
    fn test_auto_require_table_field() {
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
        assert!(ws.check_completion(
            r#"
                mapn<??>
            "#,
            vec![VirtualCompletionItem {
                label: "MapName".to_string(),
                kind: CompletionItemKind::MODULE,
                label_detail: Some("    (in aaaa)".to_string()),
            },],
        ));
    }

    #[test]
    fn test_field_is_alias_function() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@alias ProxyHandler.Setter fun(raw: any)

                ---@class ProxyHandler
                ---@field set? ProxyHandler.Setter
            "#,
        );
        assert!(ws.check_completion_with_kind(
            r#"
            ---@class MHandler: ProxyHandler
            local MHandler

            MHandler.set = <??>

            "#,
            vec![VirtualCompletionItem {
                label: "function(raw) end".to_string(),
                kind: CompletionItemKind::FUNCTION,
                ..Default::default()
            },],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));
    }

    #[test]
    fn test_namespace_base() {
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
        assert!(ws.check_completion_with_kind(
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

        assert!(ws.check_completion_with_kind(
            r#"
            ---@namespace Reactive
            ---@namespace <??>

            "#,
            vec![],
            CompletionTriggerKind::TRIGGER_CHARACTER,
        ));

        assert!(ws.check_completion_with_kind(
            r#"
            ---@namespace Reactive
            ---@using <??>

            "#,
            vec![VirtualCompletionItem {
                label: "using AlienSignals".to_string(),
                kind: CompletionItemKind::MODULE,
                ..Default::default()
            },],
            CompletionTriggerKind::INVOKED,
        ));
    }

    #[test]
    fn test_auto_require_field_1() {
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
        assert!(ws.check_completion(
            r#"
                map<??>
            "#,
            vec![],
        ));
    }

    #[test]
    fn test_issue_558() {
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
        assert!(ws.check_completion(
            r#"
            test({
                t = <??>
            })
            "#,
            vec![VirtualCompletionItem {
                label: "\"passive\"".to_string(),
                kind: CompletionItemKind::ENUM_MEMBER,
                ..Default::default()
            },],
        ));
    }
}
