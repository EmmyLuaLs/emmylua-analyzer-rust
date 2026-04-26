#[cfg(test)]
mod tests {
    use crate::handlers::test_lib::{
        ProviderVirtualWorkspace, VirtualCompletionResolveItem, check,
    };
    use googletest::prelude::*;

    #[gtest]
    fn test_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();

        check!(ws.check_completion_resolve(
            r#"
                ---@overload fun(event: "AAA", callback: fun(trg: string, data: number)): number
                ---@overload fun(event: "BBB", callback: fun(trg: string, data: string)): string
                ---@param event string
                ---@param callback fun(trg: string, data: number)
                ---@return number
                local function test(event, callback)
                    if event == "" then
                    end
                end

                test<??>
            "#,
            VirtualCompletionResolveItem {
                detail:
                    "local function test(event: string, callback: fun(trg: string, data: number)) -> number (+2 overloads)"
                        .to_string(),
                documentation: None,
            },
        ));
        Ok(())
    }
    #[gtest]
    fn test_2() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();

        check!(ws.check_completion_resolve(
            r#"
                ---@class Hover.Test2
                ---@field event fun(event: "游戏-初始化")
                ---@field event fun(event: "游戏-恢复", key: string)
                local Test2 = {}

                Test2.<??>
            "#,
            VirtualCompletionResolveItem {
                detail: "(field) Test2.event(event: \"游戏-初始化\") (+1 overloads)".to_string(),
                documentation: None,
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_table_field_function_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_completion_resolve(
            r#"
                ---@class T
                ---@field func fun(self:string) 注释注释

                ---@type T
                local t = {
                    <??>
                }
            "#,
            VirtualCompletionResolveItem {
                detail: "(field) T.func(self: string)".to_string(),
                documentation: Some("\n注释注释".to_string()),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_table_field_function_2() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_completion_resolve(
            r#"
                ---@class T
                ---@field func fun(self: T) 注释注释

                ---@type T
                local t = {
                    <??>
                }
            "#,
            VirtualCompletionResolveItem {
                detail: "(method) T:func()".to_string(),
                documentation: Some("\n注释注释".to_string()),
            },
        ));
        Ok(())
    }

    #[gtest]
    fn test_intersection() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Matchers<T>
            ---@field toBe fun(self: Assertion<T>, expected: any) -- 测试

            ---@class Inverse<T>
            ---@field not_ T

            ---@class Assertion<T>: Matchers<T>
        "#,
        );
        check!(ws.check_completion_resolve(
            r#"

            ---@type Assertion<any>
            local expect
            expect:<??>
            "#,
            VirtualCompletionResolveItem {
                detail: "(method) Matchers:toBe(expected: any)".to_string(),
                documentation: Some("\n测试".to_string()),
            },
        ));
        Ok(())
    }
}
