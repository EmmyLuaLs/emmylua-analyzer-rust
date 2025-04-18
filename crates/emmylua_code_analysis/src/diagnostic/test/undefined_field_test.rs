#[cfg(test)]
mod test {
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
}
