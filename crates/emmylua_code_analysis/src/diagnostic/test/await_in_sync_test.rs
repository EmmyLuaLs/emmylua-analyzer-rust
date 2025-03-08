#[cfg(test)]
mod test {
    use crate::DiagnosticCode;

    #[test]
    fn test_await_in_sync() {
        let mut ws = crate::VirtualWorkspace::new_with_init_std_lib();

        assert!(!ws.check_code_for(
            DiagnosticCode::AwaitInSync,
            r#"
        local function name(callback)

        end

        name(function()
            local a = coroutine.yield(1)
        end)
        "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::AwaitInSync,
            r#"
        local function name(callback)

        end

        ---@async
        name(function()
            local a = coroutine.yield(1)
        end)
        "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::AwaitInSync,
            r#"
        ---@param callback async fun()
        local function name(callback)

        end

        name(function()
            local a = coroutine.yield(1)
        end)
        "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::AwaitInSync,
            r#"
            ---@generic T, R
            ---@param call async fun(...: T...): R...
            ---@return async fun(...: T...): R...
            local function name(call)

            end

            local d = name(function()
                local a = coroutine.yield(1)
            end)
            "#
        ));
    }

    #[test]
    fn test_issue_99() {
        let mut ws = crate::VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::AwaitInSync,
            r#"
        ---@async
        local function foo()

        end

        ---@async
        return function()
            foo()
        end
        "#
        ));
    }

    #[test]
    fn test_issue_101() {
        let mut ws = crate::VirtualWorkspace::new();

        ws.def(
            r#"
        --- @async
        function bar() end
        "#,
        );

        assert!(ws.check_code_for(
            DiagnosticCode::AwaitInSync,
            r#"
        --- @async
        function foo()
            pcall(function()
                bar() -- EXPECTED OK
            end)
        end
        "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::AwaitInSync,
            r#"
        function baz()
            pcall(bar)
        end
        "#
        ));
    }

    #[test]
    fn test_issue_161() {
        let mut ws = crate::VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::AwaitInSync,
            r#"
        local function create(_f) end

        local _a = create(
        --- @async
        function()
            coroutine.yield()
        end
        )

        --- @async
        local _b = create(function()
        coroutine.yield()
        end)
        "#
        ));
    }
}
