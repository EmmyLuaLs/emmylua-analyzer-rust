#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_issue_276() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::MissingParameter,
            r#"
                --- @param a string
                --- @param b? string
                --- @param c? string
                --- @return string
                --- @overload fun(a: string, b: string): number
                local function myfun2(a, b, c) end

                local a = myfun2('string')
        "#
        ));
    }

    #[test]
    fn test_issue_249() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::MissingParameter,
            r#"
            ---@param path string
            ---@return string? realpath
            ---@overload fun(path:string, callback:function):userdata
            function realpath(path)
            end

            local path = realpath('/', function(err, path)
            end)

        "#
        ));
    }

    #[test]
    fn test_1() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::MissingParameter,
            r#"
            ---@class A
            ---@field event fun(aaa: integer)

            ---@type A
            local a = {
                event = function()
                end,
            }
        "#
        ));
    }

    #[test]
    fn test_issue_98() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::MissingParameter,
            r#"
        ---@param callback fun(i?: integer)
        function foo(callback)
            callback()
            callback(1123)
        end
        "#
        ));
    }

    #[test]
    fn test_multi_return() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::MissingParameter,
            r#" 
            ---@param a number
            ---@param b number
            ---@param c number
            local function testA(a, b, c)
            end

            ---@return number
            ---@return number
            ---@return string
            local function testB()
                return 1, 2, 3, 4, 5
            end

            testA(1, testB())
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::MissingParameter,
            r#" 
            ---@param a number
            ---@param b number
            ---@param c number
            local function testA(a, b, c)
            end

            ---@return number
            ---@return number
            local function testB()
                return 1, 2, 3
            end

            testA(testB())
            "#
        ));
    }

    #[test]
    fn test_table_unpack() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::MissingParameter,
            r#"
            local table = {}
            ---@generic T
            ---@param list [T...] | T[] | table<any, T>
            ---@param i? integer
            ---@param j? integer
            ---@return T...
            function table.unpack(list, i, j) end

            ---@param a number
            ---@param b number
            local function test(a,b)
            end
            
            ---@type number[]
            local a = {1,2,3}
            
            test(table.unpack(a))
        "#
        ));
    }

    #[test]
    fn test_alias() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::MissingParameter,
            r#"
            ---@alias Serialization.SupportTypes
            ---| number
            ---| nil

            ---@param data Serialization.SupportTypes
            local function send(data)
            end
            send()
        "#
        ));
    }

    #[test]
    fn test_issue_450() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.check_code_for_namespace(
            DiagnosticCode::MissingParameter,
            r#"
                ---@class D31.A
                local A = {}

                function A:foo()
                end

                local a = A.foo()
        "#
        ));

        assert!(ws.check_code_for_namespace(
            DiagnosticCode::MissingParameter,
            r#"
                ---@class D31.A
                local A = {}

                function A:foo()
                end

                local a = A.foo(A)
        "#
        ));
    }

    #[test]
    fn test_issue_633() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "test.lua",
            r#"
            ---@param mode number
            ---@param a number
            ---@param b number
            ---@param c number
            ---@param d number?
            ---@return string
            ---@overload fun(mode:number, a:number, b:number):number
            function test(mode, a, b, c, d)
            end

            ---@return number, number
            function getNumbers()
                return 1, 2
            end
        "#,
        );

        assert!(ws.check_code_for_namespace(
            DiagnosticCode::MissingParameter,
            r#"
            test(1, getNumbers())
        "#
        ));
    }
}
