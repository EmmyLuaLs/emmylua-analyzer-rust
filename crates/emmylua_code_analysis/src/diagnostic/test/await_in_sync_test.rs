#[cfg(test)]
mod test {
    use crate::DiagnosticCode;

    #[test]
    fn test_await_in_sync() {
        let mut ws = crate::VirtualWorkspace::new_with_init_std_lib();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AwaitInSync,
            r#"
        local function name(callback)

        end

        name(function()
            local a = coroutine.yield(1)
        end)
        "#
        ));

        assert!(ws.has_no_diagnostic(
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

        assert!(ws.has_no_diagnostic(
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

        assert!(ws.has_no_diagnostic(
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

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AwaitInSync,
            r#"
        ---@param callback async fun()
        local function name(callback)
            callback()
        end
        "#
        ));
    }

    #[test]
    fn test_issue_99() {
        let mut ws = crate::VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
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
        let mut ws = crate::VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        --- @async
        function bar() end
        "#,
        );

        assert!(ws.has_no_diagnostic(
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

        assert!(!ws.has_no_diagnostic(
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
        assert!(ws.has_no_diagnostic(
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

    #[test]
    fn test_issue_721() {
        let mut ws = crate::VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AwaitInSync,
            r#"
            --- @param f sync fun()
            local function run(f) end

            --- @async
            local function async_fn() end

            run(async_fn) -- error required
        "#
        ));
    }
}
