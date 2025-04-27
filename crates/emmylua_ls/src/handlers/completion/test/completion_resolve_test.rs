#[cfg(test)]
mod tests {

    use crate::handlers::test_lib::{ProviderVirtualWorkspace, VirtualCompletionResolveItem};

    #[test]
    fn test_1() {
        let mut ws = ProviderVirtualWorkspace::new();

        assert!(ws.check_completion_resolve(
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
            },
        ));
    }
    #[test]
    fn test_2() {
        let mut ws = ProviderVirtualWorkspace::new();

        assert!(ws.check_completion_resolve(
            r#"
            ---@class Hover.Test2
            ---@field event fun(event: "游戏-初始化")
            ---@field event fun(event: "游戏-恢复", key: string)
            local Test2 = {}

            Test2.<??>
        "#,
            VirtualCompletionResolveItem {
                detail: "(field) Test2.event(event: \"游戏-初始化\") (+1 overloads)".to_string(),
            },
        ));
    }
}
