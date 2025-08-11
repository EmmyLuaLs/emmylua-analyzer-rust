use crate::handlers::test_lib::{ProviderVirtualWorkspace, VirtualSignatureHelp, check};
use googletest::prelude::*;

#[gtest]
fn test_1() -> Result<()> {
    let mut ws = ProviderVirtualWorkspace::new();
    check!(ws.check_signature_helper(
        r#"
                ---@class Action
                ---@field id fun(self:Action, itemId:integer, ...:integer?):boolean
                ---@overload fun():Action
                Action = {}

                Action:id(1, <??>)
            "#,
        VirtualSignatureHelp {
            target_label: "Action:id(itemId: integer, ...: integer?): boolean".to_string(),
            active_signature: 0,
            active_parameter: 1,
        },
    ));
    Ok(())
}

#[gtest]
fn test_2() -> Result<()> {
    let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
    check!(ws.check_signature_helper(
        r#"
                ---@param path string
                local function readFile(path)
                end

                pcall(readFile, <??>)
            "#,
        VirtualSignatureHelp {
            target_label: "pcall(f: fun(path: string), path: string): boolean".to_string(),
            active_signature: 0,
            active_parameter: 1,
        },
    ));
    Ok(())
}
