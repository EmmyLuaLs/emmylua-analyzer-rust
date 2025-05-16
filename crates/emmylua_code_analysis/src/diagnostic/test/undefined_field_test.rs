#[cfg(test)]
mod test {
    use std::{ops::Deref, sync::Arc};

    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_1() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                ---@alias std.NotNull<T> T - ?

                ---@generic V
                ---@param t {[any]: V}
                ---@return fun(tbl: any):int, std.NotNull<V>
                function ipairs(t) end

                ---@type {[integer]: string|table}
                local a = {}

                for i, extendsName in ipairs(a) do
                    print(extendsName.a)
                end 
            "#
        ));
    }

    #[test]
    fn test() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                ---@class diagnostic.test3
                ---@field private a number

                ---@type diagnostic.test3
                local test = {}

                local b = test.b
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                ---@class diagnostic.test3
                ---@field private a number
                local Test3 = {}

                local b = Test3.b
            "#
        ));
    }

    #[test]
    fn test_enum() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                ---@enum diagnostic.enum
                local Enum = {
                    A = 1,
                }

                local enum_b = Enum["B"]
            "#
        ));
    }
    #[test]
    fn test_issue_194() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
            local a ---@type 'A'
            local _ = a:lower()
            "#
        ));
    }

    #[test]
    fn test_any_key() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                ---@class LogicalOperators
                local logicalOperators <const> = {}

                ---@param key any
                local function test(key)
                    print(logicalOperators[key])
                end
            "#
        ));
    }

    #[test]
    fn test_class_key_to_class_key() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                --- @type table<string, integer>
                local FUNS = {}

                ---@class D10.AAA

                ---@type D10.AAA
                local Test1

                local a = FUNS[Test1]
            "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                ---@generic K, V
                ---@param t table<K, V> | V[] | {[K]: V}
                ---@return fun(tbl: any):K, std.NotNull<V>
                local function pairs(t) end

                ---@class D11.AAA
                ---@field name string
                ---@field key string
                local AAA = {}

                ---@type D11.AAA
                local a

                for k, v in pairs(AAA) do
                    if not a[k] then
                        -- a[k] = v
                    end
                end
            "#
        ));
    }

    #[test]
    fn test_2() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                local function sortCallbackOfIndex()
                    ---@type table<string, integer>
                    local indexMap = {}
                    return function(v)
                        return -indexMap[v]
                    end
                end
            "#
        ));
    }

    #[test]
    fn test_index_key_define() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                local Flags = {
                    A = {},
                }

                ---@class (constructor) RefImpl
                local a = {
                    [Flags.A] = true,
                }

                print(a[Flags.A])
            "#
        ));
    }

    #[test]
    fn test_issue_292() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
            --- @type {head:string}[]?
            local b
            ---@diagnostic disable-next-line: need-check-nil
            _ = b[1].head == 'b'
            "#
        ));
    }

    #[test]
    fn test_issue_317() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                --- @class A
                --- @field [string] string
                --- @field [integer] integer
                local foo = {}

                local bar = foo[1]
            "#
        ));
    }

    #[test]
    fn test_issue_345() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                --- @class C
                --- @field a string
                --- @field b string

                local scope --- @type 'a'|'b'

                local m --- @type C

                a = m[scope]
        "#
        ));
        let ty = ws.expr_ty("a");
        let expected = ws.ty("string");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));
    }

    #[test]
    fn test_index_key_by_string() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
            ---@enum (key) K1
            local apiAlias = {
                Unit         = 'unit_entity',
            }

            ---@type string?
            local cls
            local a = apiAlias[cls]
        "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
            ---@enum (key) K2
            local apiAlias = {
                Unit         = 'unit_entity',
            }

            ---@type string?
            local cls
            local a = apiAlias["1" .. cls]
        "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
            ---@enum K3
            local apiAlias = {
                Unit         = 'unit_entity',
            }

            ---@type string?
            local cls
            local a = apiAlias["Unit1"]
        "#
        ));
    }

    #[test]
    fn test_unknown_type() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                local function test(...)
                    local args = { ... }
                    local a = args[1]
                end
        "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::InjectField,
            r#"
                local function test(...)
                    local args = { ... }
                    args[1] = 1
                end
        "#
        ));
    }

    #[test]
    fn test_g() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                print(_G['game_lua_files'])
        "#
        ));
    }

    #[test]
    fn test_def() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::InjectField,
            r#"
                ---@class ECABind
                Bind = {}

                ---@class ECAFunction
                ---@field call_name string
                local M = {}

                ---@param func function
                function M:call(func)
                    Bind[self.call_name] = function(...)
                        return
                    end
                end
        "#
        ));
    }

    #[test]
    fn test_enum_1() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                ---@enum (key) UnitAttr
                local UnitAttr = {
                    ['hp_cur'] = 'hp_cur',
                    ['mp_cur'] = 1,
                }

                ---@param name UnitAttr
                local function get(name)
                    local a = UnitAttr[name]
                end
        "#
        ));
    }

    #[test]
    fn test_enum_2() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
            ---@enum AbilityType
            local AbilityType = {
                HIDE    = 0,
                NORMAL  = 1,
                ['隐藏'] = 0,
                ['普通'] = 1,
            }

            ---@alias AbilityTypeAlias
            ---| '隐藏'
            ---| '普通'

            
            ---@param name AbilityType | AbilityTypeAlias
            local function get(name)
                local a = AbilityType[name]
            end
        "#
        ));
    }

    #[test]
    fn test_enum_3() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
            ---@enum (key) PlayerAttr
            local PlayerAttr = {}

            ---@param key PlayerAttr
            local function add(key)
                local a = PlayerAttr[key]
            end
        "#
        ));
    }

    #[test]
    fn test_enum_alias() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                ---@enum EA
                A = {
                    ['GAME_INIT'] = "ET_GAME_INIT",
                }

                ---@enum EB
                B = {
                    ['GAME_PAUSE'] = "ET_GAME_PAUSE",
                }

                ---@alias EventName EA | EB

                ---@class Event
                local event = {}
                event.ET_GAME_INIT = {}
                event.ET_GAME_PAUSE = {}


                ---@param name EventName
                local function test(name)
                    local a = event[name]
                end
        "#
        ));
    }

    #[test]
    fn test_userdata() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
            ---@type any
            local value
            local tp = type(value)

            if tp == 'userdata' then
                ---@cast value userdata
                if value['type'] then
                end
            end
        "#
        ));
    }

    #[test]
    fn test_has_nil() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"

                ---@type table<string, boolean>
                local includedNameMap = {}

                ---@param name? string
                local function a(name)
                    if not includedNameMap[name] then
                    end
                end
        "#
        ));
    }

    #[test]
    fn test_super_integer() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
            ---@type table<integer, string>
            local t = {}

            ---@class NewKey: integer

            ---@type NewKey
            local key = 1

            local a = t[key]

        "#
        ));
    }

    #[test]
    fn test_generic_super() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
            ---@generic Super: string
            ---@param super? `Super`
            local function declare(super)
                ---@type table<string, string>
                local config

                local superClass = config[super]
            end
        "#
        ));
    }

    #[test]
    fn test_ref_field() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                ---@enum ReactiveFlags
                local ReactiveFlags = {
                    IS_REF = { '<IS_REF>' },
                }
                local IS_REF = ReactiveFlags.IS_REF

                ---@class ObjectRefImpl
                local ObjectRefImpl = {}

                function ObjectRefImpl.new()
                    ---@class (constructor) ObjectRefImpl
                    local self = {
                        [IS_REF] = true, -- 标记为ref
                    }
                end

                ---@param a ObjectRefImpl
                local function name(a)
                    local c = a[IS_REF]
                end
        "#
        ));
    }

    #[test]
    fn test_string_add_enum_key() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
                ---@class py.GameAPI
                GameAPI = {}

                function GameAPI.get_kv_pair_value_unit_entity(handle, key) end

                function GameAPI.get_kv_pair_value_unit_name() end

                ---@enum(key) KV.SupportTypeEnum
                local apiAlias = {
                    Unit         = 'unit_entity',
                    UnitKey      = 'unit_name',
                }

                ---@param lua_type 'boolean' | 'number' | 'integer' | 'string' | 'table' | KV.SupportTypeEnum
                ---@return any
                local function kv_load_from_handle(lua_type)
                    local alias = apiAlias[lua_type]
                    local api = GameAPI['get_kv_pair_value_' .. alias]
                end
        "#
        ));
    }

    #[test]
    fn test_global_arg_override() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let mut emmyrc = ws.analysis.emmyrc.deref().clone();
        emmyrc.strict.meta_override_file_define = false;
        ws.analysis.update_config(Arc::new(emmyrc));

        ws.def(
            r#"
        ---@class py.Dict
        
        ---@return py.Dict
        local function lua_get_start_args() end
        
        ---@type table<string, string>
        arg = lua_get_start_args()
        "#,
        );
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
            local function isDebuggerValid()
                if arg['lua_multi_mode'] == 'true' then
                end
            end
        "#
        ));
    }
}
