use crate::handlers::test_lib::{ProviderVirtualWorkspace, VirtualLocation, check};
use googletest::prelude::*;

#[gtest]
fn test_function_references() -> Result<()> {
    let mut ws = ProviderVirtualWorkspace::new();
    ws.def_file(
        "1.lua",
        r#"
                local flush = require("virtual_0").flush
                flush()
            "#,
    );
    check!(ws.check_references(
        r#"
                local export = {}
                local function fl<??>ush()
                end
                export.flush = flush
                return export
            "#,
        vec![
            VirtualLocation {
                file: "".to_string(),
                line: 2,
            },
            VirtualLocation {
                file: "".to_string(),
                line: 4,
            },
            VirtualLocation {
                file: "1.lua".to_string(),
                line: 1,
            },
            VirtualLocation {
                file: "1.lua".to_string(),
                line: 1,
            },
            VirtualLocation {
                file: "1.lua".to_string(),
                line: 2,
            },
            VirtualLocation {
                file: "1.lua".to_string(),
                line: 1,
            },
            VirtualLocation {
                file: "".to_string(),
                line: 4,
            },
        ]
    ));
    Ok(())
}

#[gtest]
fn test_function_references_2() -> Result<()> {
    let mut ws = ProviderVirtualWorkspace::new();
    ws.def_file(
        "1.lua",
        r#"
                local flush = require("virtual_0").flush
                flush()
            "#,
    );
    check!(ws.check_references(
        r#"
                local function fl<??>ush()
                end
                return {
                    flush = flush,
                }
            "#,
        vec![
            VirtualLocation {
                file: "".to_string(),
                line: 1,
            },
            VirtualLocation {
                file: "".to_string(),
                line: 4,
            },
            VirtualLocation {
                file: "1.lua".to_string(),
                line: 1,
            },
            VirtualLocation {
                file: "1.lua".to_string(),
                line: 1,
            },
            VirtualLocation {
                file: "1.lua".to_string(),
                line: 2,
            },
            VirtualLocation {
                file: "1.lua".to_string(),
                line: 1,
            },
            VirtualLocation {
                file: "".to_string(),
                line: 4,
            },
        ]
    ));
    Ok(())
}
