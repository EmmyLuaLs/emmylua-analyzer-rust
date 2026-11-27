#[cfg(test)]
mod test {
    use crate::DiagnosticCode;
    use crate::VirtualWorkspace;

    // 函数字段及其别名都必须拒绝把异步函数传给同步回调参数.
    #[test]
    fn test_doc_projection_sync_callback_field() {
        for field_type in ["fun(callback: sync fun())", "Run"] {
            for (annotation, accepted) in [("---@async", false), ("", true)] {
                let mut ws = VirtualWorkspace::new();
                assert_eq!(
                    ws.has_no_diagnostic(
                        DiagnosticCode::AwaitInSync,
                        &format!(
                            r#"
                            ---@alias Run fun(callback: sync fun())
                            ---@class Runner
                            ---@field run {field_type}
                            ---@type Runner
                            local runner
                            {annotation}
                            local function callback() end
                            runner.run(callback)
                            "#,
                        ),
                    ),
                    accepted,
                    "{field_type}, {annotation}",
                );
            }
        }
    }

    #[test]
    fn test_await_in_sync() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

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
    fn test_async_callback_member_alias_from_required_module() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "async.lua",
            r#"
            local M = {}
            ---@async
            function M.yield() end
            return M
            "#,
        );
        ws.def_file(
            "luastd_test.lua",
            r#"
            ---@class luastd_test.OrderedExecution
            local OrderedExecution = {}
            local M = {}
            ---@overload fun(body: async fun(t: luastd_test.OrderedExecution, scope: any)): fun()
            ---@param options table
            ---@param body async fun(t: luastd_test.OrderedExecution, scope: any)
            ---@return fun()
            function M.runtest(options, body)
                return function() end
            end
            return M
            "#,
        );

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AwaitInSync,
            r#"
            local async = require("async")
            local luastd_test = require("luastd_test")
            local wrapped = luastd_test.runtest(function ()
                async.yield()
            end)
            "#,
        ));
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AwaitInSync,
            r#"
            local async = require("async")
            local luastd_test = require("luastd_test")
            local runtest = luastd_test.runtest
            local wrapped = runtest(function ()
                async.yield()
            end)
            "#,
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AwaitInSync,
            r#"
            local async = require("async")
            local luastd_test = require("luastd_test")
            local runtest = luastd_test.runtest
            ---@param body fun()
            runtest = function(body) return function() end end
            runtest(function ()
                async.yield()
            end)
            "#,
        ));
    }

    #[test]
    fn test_issue_99() {
        let mut ws = VirtualWorkspace::new();

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
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

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
        let mut ws = VirtualWorkspace::new();
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
        let mut ws = VirtualWorkspace::new();
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
