#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_issue_392() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(DiagnosticCode::UnnecessaryIf,
        r#"
        local a = false ---@type boolean|nil
        if a == nil or a then -- Unnecessary `if` statement: this condition is always truthy [unnecessary-if]
            print('a is not false')
        end
        "#
        ));
    }

    #[test]
    fn test_issue_396() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(DiagnosticCode::UnnecessaryIf,
        r#"
        local a = false ---@type 'a'|'b'
        if a ~= 'a' then -- Unnecessary `if` statement: this condition is always truthy [unnecessary-if]
        end
        "#
        ));
    }

    #[test]
    fn test_metatable_separate_class_no_false_positive() {
        // When @class is defined on M and methods are defined on a separate mt
        // with mt.__index = mt, optional @fields should NOT be reported as
        // always falsy in mt methods.
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_code_for(DiagnosticCode::UnnecessaryIf,
        r#"
        ---@class TestMt.Rpc
        ---@field name string?
        local M = {}

        local mt = {}
        mt.__index = mt

        function mt:test()
            local name = self.name
            if name then
                print(name)
            end
        end

        ---@return TestMt.Rpc
        function M.new()
            return setmetatable({}, mt)
        end
        "#
        ));
    }
}
