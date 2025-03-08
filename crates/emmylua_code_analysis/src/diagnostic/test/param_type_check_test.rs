#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

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
}
