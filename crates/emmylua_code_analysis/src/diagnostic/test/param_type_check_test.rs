#[cfg(test)]
mod test {
    use std::sync::Arc;

    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_issue_216() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            ---@alias F1 fun(x: integer):integer
            do
                ---@type F1
                local test = function(x) return x + 1 end
                
                test("wrong type")
            end
        "#
        ));
    }

    #[test]
    fn test_issue_82() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            ---@generic F: function
            ---@param _a F|integer
            ---@param _b? F
            ---@return F
            function foo(_a, _b)
                return _a
            end
            foo(function() end)
        "#
        ));
    }

    #[test]
    fn test_issue_75() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            local a, b = pcall(string.rep, "a", "w")
        "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            local a, b = pcall(string.rep, "a", 10000)
        "#
        ));
    }

    #[test]
    fn test_issue_85() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
        ---@param a table | nil
        local function foo(a)
            a = a or {}
            _ = a.b
        end

        ---@param a table?
        local function _bar(a)
            a = a or {}
            _ = a.b
        end
        "#
        ));
    }

    #[test]
    fn test_issue_84() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
        ---@param _a string[]
        local function bar(_a) end

        ---@param a? string[]?
        local function _foo(a)
            if not a then
                a = {}
            end

            bar(a)

            if not a then
                a = {}
            end

            bar(a)
        end
        "#
        ));
    }

    #[test]
    fn test_issue_83() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
        ---@param _t table<any, string>
        local function foo(_t) end

        foo({})
        foo({'a'})
        foo({'a', 'b'})

        local a ---@type string[]
        foo(a)
        "#
        ));
    }

    #[test]
    fn test_issue_113() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            ---@enum Baz
            local asd = {
                Foo = 0,
                Bar = 1,
                Baz = 2,
            }

            ---@param bob {a: Baz}
            function Foo(bob)
                return Bar(bob)
            end

            ---@param bob {a: Baz}
            function Bar(bob)
            end
        "#
        ));
    }

    #[test]
    fn test_issue_111() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
        local Table = {}

        ---@param target table
        ---@param ... table
        ---@return table
        function Table.mergeInto(target, ...)
            -- Stuff
        end

        ---@param ... table
        ---@return table
        function Table.merge(...)
            return Table.mergeInto({}, ...)
        end
        "#
        ));
    }

    #[test]
    fn test_var_param_check() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@param target table
        ---@param ... table
        ---@return table
        function mergeInto(target, ...)
            -- Stuff
        end
        "#,
        );

        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
        mergeInto({}, 1)
        "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
        mergeInto({}, {}, {})
        "#
        ));
    }

    #[test]
    fn test_issue_102() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
        ---@param _kind '' | 'Nr' | 'Ln' | 'Cul'
        function foo(_kind) end

        for _, kind in ipairs({ '', 'Nr', 'Ln', 'Cul' }) do
            foo(kind)
        end
        "#
        ));
    }

    #[test]
    fn test_issue_95() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
        local range ---@type { [1]: integer, [2]: integer }

        table.sort(range)
        "#
        ));
    }

    #[test]
    fn test_issue_135() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
        ---@alias A
        ---| "number" # A number

        ---@param a A
        local function f(a)
        end

        f("number")
        "#
        ));
    }

    #[test]
    fn test_colon_call_and_not_colon_define() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            ---@class Test
            local Test = {}

            ---@param a string
            function Test.name(a)
            end

            Test:name()
        "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            ---@class Test
            local Test = {}

            ---@param ... any
            function Test.dots(...)
            end

            Test:dots("a", "b", "c") 
        "#
        ));
    }

    #[test]
    fn test_issue_148() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            local a = (''):format()
        "#
        ));
    }

    #[test]
    fn test_generic_dots_param() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            local d = select(1, 1, 2, 3)
        "#
        ));
    }

    #[test]
    fn test_bool_as_type() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
        --- @param _x string|true
        function foo(_x) end

        foo(true)
        "#
        ));
    }

    #[test]
    fn test_function() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            ---@param sorter function
            ---@return string[]
            local function getTableKeys(sorter)
                local keys = {}
                table.sort(keys, sorter)
                return keys
            end
        "#
        ));
    }

    #[test]
    fn test_table_array() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@generic K, V
                ---@param t table<K, V>
                ---@return table<V, K>
                local function revertMap(t)
                end

                ---@param arr any[]
                local function sortCallbackOfIndex(arr)
                    ---@type table<any, integer>
                    local indexMap = revertMap(arr)
                end
        "#
        ));
    }

    #[test]
    fn test_table_class() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@param t table
                local function bar(t)
                end

                ---@class D11.A

                ---@type D11.A|any
                local a

                bar(a)
        "#
        ));
    }

    #[test]
    fn test_table_1() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@param t table[]
                local function bar(t)
                end

                ---@type table|any
                local a

                bar(a)
        "#
        ));
    }

    #[test]
    fn test_pairs() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@diagnostic disable: missing-return
                ---@generic K, V
                ---@param t table<K, V> | V[] | {[K]: V}
                ---@return fun(tbl: any):K, std.NotNull<V>
                local function _pairs(t) end

                ---@class D10.A

                ---@type {[string]: D10.A, _id: D10.A}
                local a

                for k, v in _pairs(a) do
                end
        "#
        ));
    }

    #[test]
    fn test_issue_278() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
        local a --- @type type|'callable'
        error(a)  -- expected `string` but found `(type|"callable")`
        "#
        ))
    }

    #[test]
    fn test_4() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#" 
            ---@class D13.Meta
            ---@field __defineGet fun(self: self, key: string, f: fun(self: self): any)

            ---@class D13.Impl: D13.Meta
            local impl = {}

            impl:__defineGet("value", function(self)
                return 1
            end)

            "#
        ));
    }

    #[test]
    fn test_issue_286() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                local a --- @type boolean
                local b --- @type integer?
                local c = a and b or nil
                -- type of c is (nil|true), should be (integer|nil)
                local d = a and b
                -- type of d is (boolean|nil), should be (false|integer|nil) 

                ---@param p integer?
                local function f1(p)
                end
                f1(c)

                ---@param p false|integer|nil
                local function f2(p)
                end
                f2(d)
        "#
        ));
    }

    #[test]
    fn test_issue_287() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@param a table
                local function f(a)
                end

                ---@type table?
                local a 
                a = a or {}

                f(a)
        "#
        ));
    }

    #[test]
    fn test_issue_336() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            --- @param b string
            --- @param c? boolean
            --- @param d? string
            --- @overload fun(b: string, d: string)
            function foo(b, c, d) end

            foo('number', true)
        "#
        ));
    }

    #[test]
    fn test_issue_348() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                local a --- @type type|'a'
                string.len(a)
        "#
        ));
    }

    #[test]
    fn test_super() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@class py.ETypeMeta

                ---@class py.Vector3: py.ETypeMeta
                ---@class py.FVector3: py.ETypeMeta

                ---@alias Point.HandleType py.FVector3

                ---@class py.Point: py.Vector3

                ---@param point py.Point
                local function test(point)
                end

                ---@type Point.HandleType
                local handle

                test(handle)
        "#
        ));
    }

    #[test]
    fn test_union_type() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            ---@class py.Area
            ---@class py.RecArea: py.Area
            ---@class py.CirArea: py.Area

            ---@param a py.Area
            local function test(a)
            end

            ---@type py.RecArea | py.CirArea
            local a

            test(a)
        "#
        ));
    }

    #[test]
    fn test_super_1() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@class py.SlotType: integer

                ---@param a py.SlotType
                local function test(a)
                end

                ---@type 0|1
                local a

                test(a)
        "#
        ));
    }

    #[test]
    fn test_alias_union_enum() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@alias EventType
                ---| GlobalEventType
                ---| UIEventType

                ---@enum UIEventType
                local UIEventType = {
                    ['UI_CREATE'] = "ET_UI_PREFAB_CREATE_EVENT",
                    ['UI_DELETE'] = "ET_UI_PREFAB_DEL_EVENT",
                }

                ---@enum GlobalEventType
                local GlobalEventType = {
                    ['GAME_INIT'] = "ET_GAME_INIT",
                    ['GAME_PAUSE'] = "ET_GAME_PAUSE",
                }

                ---@param event_name string
                local function get_py_event_name(event_name)
                end

                ---@param a EventType
                local function test(a)
                    get_py_event_name(a)
                end

        "#
        ));
    }

    #[test]
    fn test_alias_union_enum_2() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@alias EventType
                ---| GlobalEventType
                ---| UIEventType

                ---@enum UIEventType
                local UIEventType = {
                    ['UI_CREATE'] = "ET_UI_PREFAB_CREATE_EVENT",
                    ['UI_DELETE'] = "ET_UI_PREFAB_DEL_EVENT",
                }

                ---@enum GlobalEventType
                local GlobalEventType = {
                    ['GAME_INIT'] = 1,
                    ['GAME_PAUSE'] = "ET_GAME_PAUSE",
                }

                ---@param event_name string
                local function get_py_event_name(event_name)
                end

                ---@param a EventType
                local function test(a)
                    get_py_event_name(a)
                end

        "#
        ));
    }

    #[test]
    fn test_empty_class() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@class D4.A: table<integer, string>

                ---@param lua_conf D4.A 
                local function enable_global_lua_trigger(lua_conf) end

                ---@return { on_event: fun(trigger: table,  actor, data), [integer]: string }
                function new_global_trigger() end

                local a = new_global_trigger()

                enable_global_lua_trigger(a)
        "#
        ));
    }

    #[test]
    fn test_super_and_enum_1() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@enum AbilityType
                local AbilityType = {
                    HIDE   = 0,
                    NORMAL = 1,
                    COMMON = 2,
                    HERO   = 3,
                }
                ---@class py.AbilityType: integer

                ---@param ability_type py.AbilityType
                local function a(ability_type) end

                ---@param type AbilityType
                local function get(type)
                    local py_list = a(type)
                end
        "#
        ));
    }

    #[test]
    fn test_super_and_enum_2() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@enum AbilityType
                local AbilityType = {
                    HIDE   = "a",
                    NORMAL = 1,
                    COMMON = 2,
                    HERO   = 3,
                }
                ---@class py.AbilityType: integer

                ---@param ability_type py.AbilityType
                local function a(ability_type) end

                ---@param type AbilityType
                local function get(type)
                    local py_list = a(type)
                end
        "#
        ));
    }

    #[test]
    fn test_generic_array() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            ---@class LocalTimer

            ---@generic V
            ---@param list V[]
            ---@return integer
            local function sort(list) end

            ---@type { need_sort: true?, [integer]: LocalTimer }
            local queue = {}
            sort(queue)
        "#
        ));
    }

    #[test]
    fn test_function_union() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@class (partial) D21.A
                local M

                ---@alias EventType
                ---| GlobalEventType
                ---| UIEventType

                ---@enum UIEventType
                local UIEventType = {
                    ['UI_CREATE'] = "ET_UI_PREFAB_CREATE_EVENT",
                }
                ---@enum GlobalEventType
                local GlobalEventType = {
                    ['GAME_INIT'] = "ET_GAME_INIT",
                }

                ---@param event_type EventType
                function M:event(event_type)
                end

                ---@class (partial) D21.A
                ---@field event fun(self: self, event: "游戏-初始化")

                ---@param p string
                local function test(p)
                    M:event(p)
                end
        "#
        ));
    }

    #[test]
    fn test_function_union_2() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@class (partial) D21.A
                local M

                ---@alias EventType
                ---| GlobalEventType
                ---| UIEventType

                ---@enum UIEventType
                local UIEventType = {
                    ['UI_CREATE'] = "ET_UI_PREFAB_CREATE_EVENT",
                }
                ---@enum GlobalEventType
                local GlobalEventType = {
                    ['GAME_INIT'] = "ET_GAME_INIT",
                }

                ---@param event_type EventType
                function M:event(event_type)
                end

                ---@class (partial) D21.A
                ---@field event fun(self: self, event: "游戏-初始化")

                ---@param p EventType
                local function test(p)
                    M:event(p)
                end
        "#
        ));
    }

    #[test]
    fn test_function_union_meta() {
        let mut ws = VirtualWorkspace::new();
        let mut emmyrc = ws.analysis.emmyrc.as_ref().clone();
        emmyrc.strict.meta_override_file_define = false;
        ws.analysis.update_config(Arc::new(emmyrc));

        ws.def(
            r#"
                ---@meta
                ---@class (partial) D21.A
                ---@field event fun(self: self, event: "游戏-初始化")
            "#,
        );
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@class (partial) D21.A
                local M

                ---@alias EventType
                ---| GlobalEventType
                ---| UIEventType

                ---@enum UIEventType
                local UIEventType = {
                    ['UI_CREATE'] = "ET_UI_PREFAB_CREATE_EVENT",
                }
                ---@enum GlobalEventType
                local GlobalEventType = {
                    ['GAME_INIT'] = "ET_GAME_INIT",
                }

                ---@param event_type EventType
                function M:event(event_type)
                end

                ---@param p EventType
                local function test(p)
                    M:event(p)
                end
        "#
        ));
    }

    #[test]
    fn test_function_self() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@class D23.A

                ---@generic Extends: string
                ---@param init? fun(self: self, super: Extends)
                local function extends(init)
                end

                ---@generic Super: string
                ---@param super? `Super`
                ---@param superInit? fun(self: D23.A, super: Super, ...)
                local function declare(super, superInit)
                    extends(superInit)
                end
        "#
        ));
    }

    #[test]
    fn test_self_1() {
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
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            local A = require("1")

            ---@class D31.B
            local B = {}

            function B:__init()
                self.originalExecute = A.execute
                A.execute = function(trg, ...)
                    self.originalExecute(trg, ...)
                end
            end
        "#
        ));
    }
}
