#[cfg(test)]
mod test {
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
}
