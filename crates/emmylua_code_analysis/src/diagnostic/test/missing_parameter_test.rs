#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

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
}
