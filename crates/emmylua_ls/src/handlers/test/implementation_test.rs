use crate::handlers::test_lib::{ProviderVirtualWorkspace, VirtualLocation, check};
use googletest::prelude::*;

#[gtest]
fn test_1() -> Result<()> {
    let mut ws = ProviderVirtualWorkspace::new();
    ws.def_file(
        "2.lua",
        r#"
               delete = require("virtual_0").delete
               delete()
            "#,
    );
    ws.def_file(
        "3.lua",
        r#"
               delete = require("virtual_0").delete
               delete()
            "#,
    );
    check!(ws.check_implementation(
        r#"
                local M = {}
                function M.de<??>lete(a)
                end
                return M
            "#,
        vec![VirtualLocation {
            file: "".to_string(),
            line: 2,
        }],
    ));
    Ok(())
}

#[gtest]
fn test_2() -> Result<()> {
    let mut ws = ProviderVirtualWorkspace::new();
    ws.def_file(
        "1.lua",
        r#"
                ---@class (partial) Test
                test = {}

                test.a = 1
            "#,
    );
    ws.def_file(
        "2.lua",
        r#"
                ---@class (partial) Test
                test = {}
                test.a = 1
            "#,
    );
    ws.def_file(
        "3.lua",
        r#"
                local a = test.a
            "#,
    );
    check!(ws.check_implementation(
        r#"
                t<??>est
            "#,
        vec![
            VirtualLocation {
                file: "".to_string(),
                line: 1,
            },
            VirtualLocation {
                file: "1.lua".to_string(),
                line: 2,
            },
            VirtualLocation {
                file: "2.lua".to_string(),
                line: 2,
            }
        ],
    ));
    Ok(())
}

#[gtest]
fn test_3() -> Result<()> {
    let mut ws = ProviderVirtualWorkspace::new();
    ws.def_file(
        "1.lua",
        r#"
                ---@class YYY
                ---@field a number
                yyy = {}

                if false then
                    yyy.a = 1
                    if yyy.a then
                    end
                end
            "#,
    );
    check!(ws.check_implementation(
        r#"
                yyy.<??>a = 2
            "#,
        vec![
            VirtualLocation {
                file: "".to_string(),
                line: 1,
            },
            VirtualLocation {
                file: "1.lua".to_string(),
                line: 2,
            },
            VirtualLocation {
                file: "1.lua".to_string(),
                line: 6,
            },
        ],
    ));
    Ok(())
}

#[gtest]
fn test_table_field_definition_1() -> Result<()> {
    let mut ws = ProviderVirtualWorkspace::new();
    check!(ws.check_implementation(
        r#"
                ---@class T
                ---@field func fun(self: T) 注释注释

                ---@type T
                local t = {
                    func = function(self)
                    end,
                }

                t:fun<??>c()
            "#,
        vec![
            VirtualLocation {
                file: "".to_string(),
                line: 2,
            },
            VirtualLocation {
                file: "".to_string(),
                line: 6,
            },
        ],
    ));
    Ok(())
}

#[gtest]
fn test_table_field_definition_2() -> Result<()> {
    let mut ws = ProviderVirtualWorkspace::new();
    check!(ws.check_implementation(
        r#"
                ---@class T
                ---@field func fun(self: T) 注释注释

                ---@type T
                local t = {
                    f<??>unc = function(self)
                    end,
                }
            "#,
        vec![
            VirtualLocation {
                file: "".to_string(),
                line: 2,
            },
            VirtualLocation {
                file: "".to_string(),
                line: 6,
            },
        ],
    ));
    Ok(())
}

#[gtest]
fn test_separation_of_define_and_impl() -> Result<()> {
    let mut ws = ProviderVirtualWorkspace::new();
    check!(ws.check_implementation(
        r#"
                local a<??>bc

                abc = function()
                end

                local _a = abc
                local _b = abc()

                abc = function()
                end
            "#,
        vec![
            VirtualLocation {
                file: "".to_string(),
                line: 1,
            },
            VirtualLocation {
                file: "".to_string(),
                line: 3,
            },
            VirtualLocation {
                file: "".to_string(),
                line: 9,
            },
        ],
    ));
    Ok(())
}
