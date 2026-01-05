#[cfg(test)]
mod tests {
    use crate::handlers::test_lib::{ProviderVirtualWorkspace, VirtualLocation, check};
    use googletest::prelude::*;

    #[gtest]
    fn test_function_references() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_references(
            r#"
                local export = {}
                local function fl<??>ush()
                end
                export.flush = flush
                return export
            "#,
            vec![(
                "1.lua",
                r#"
                    local flush = require("virtual_0").flush
                    flush()
                "#,
            )],
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
        check!(ws.check_references(
            r#"
                local function fl<??>ush()
                end
                return {
                    flush = flush,
                }
            "#,
            vec![(
                "1.lua",
                r#"
                    local flush = require("virtual_0").flush
                    flush()
                "#,
            )],
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

    #[gtest]
    fn test_module_return() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();

        check!(ws.check_references(
            r#"
                local function init()
                end
                return in<??>it
            "#,
            vec![(
                "a.lua",
                r#"
                local init = require("virtual_0")
                init()
            "#,
            )],
            vec![
                VirtualLocation {
                    file: "virtual_0.lua".to_string(),
                    line: 1,
                },
                VirtualLocation {
                    file: "a.lua".to_string(),
                    line: 1,
                },
                VirtualLocation {
                    file: "a.lua".to_string(),
                    line: 2,
                },
                VirtualLocation {
                    file: "virtual_0.lua".to_string(),
                    line: 3,
                },
            ],
        ));
        Ok(())
    }

    #[gtest]
    fn test_module_return_2() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "a.lua",
            r#"
            local function getA()
            end
            return {
                getA = getA
            }
        "#,
        );

        check!(ws.check_references(
            r#"
                local AModule = require("a")
                AMo<??>dule.getA()
            "#,
            vec![],
            vec![
                VirtualLocation {
                    file: "virtual_0.lua".to_string(),
                    line: 1,
                },
                VirtualLocation {
                    file: "virtual_0.lua".to_string(),
                    line: 2,
                },
            ],
        ));
        Ok(())
    }
}
