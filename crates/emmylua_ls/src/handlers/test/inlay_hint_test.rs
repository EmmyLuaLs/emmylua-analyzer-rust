#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use std::{ops::Deref, sync::Arc};

    use crate::handlers::test_lib::{ProviderVirtualWorkspace, VirtualInlayHint, check};

    #[gtest]
    fn test_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_inlay_hint(
            r#"
                ---@class Hint1

                ---@param a Hint1
                local function test(a)
                    local b = a
                end
            "#,
            vec![
                VirtualInlayHint {
                    label: ": Hint1".to_string(),
                    line: 4,
                    pos: 37,
                    ref_file: Some("".to_string()),
                },
                VirtualInlayHint {
                    label: ": Hint1".to_string(),
                    line: 5,
                    pos: 27,
                    ref_file: Some("".to_string()),
                },
            ]
        ));
        Ok(())
    }

    #[gtest]
    fn test_2() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        check!(ws.check_inlay_hint(
            r#"
                ---@param a number
                local function test(a)
                end
            "#,
            vec![VirtualInlayHint {
                label: ": number".to_string(),
                line: 2,
                pos: 37,
                ref_file: Some("builtin.lua".to_string()),
            }]
        ));
        Ok(())
    }

    #[gtest]
    fn test_local_hint_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_inlay_hint(
            r#"
                local a = 1
            "#,
            vec![]
        ));
        Ok(())
    }

    #[gtest]
    fn test_local_hint_2() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_inlay_hint(
            r#"
                local function test()
                end
            "#,
            vec![]
        ));
        check!(ws.check_inlay_hint(
            r#"
                local test = function()
                end
            "#,
            vec![]
        ));
        Ok(())
    }

    #[gtest]
    fn test_meta_call_hint() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_inlay_hint(
            r#"
                ---@class Hint1
                ---@overload fun(a: string): Hint1
                local Hint1

                local a = Hint1("a")
            "#,
            vec![
                VirtualInlayHint {
                    label: ": Hint1".to_string(),
                    line: 5,
                    pos: 23,
                    ref_file: Some("".to_string()),
                },
                VirtualInlayHint {
                    label: "a:".to_string(),
                    line: 5,
                    pos: 32,
                    ref_file: None,
                },
                VirtualInlayHint {
                    label: "new".to_string(),
                    line: 5,
                    pos: 26,
                    ref_file: Some("".to_string()),
                },
            ]
        ));
        Ok(())
    }

    #[gtest]
    fn test_class_def_var_hint() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_inlay_hint(
            r#"
                ---@class Hint.1
                ---@overload fun(a: integer): Hint.1
                local Hint1
            "#,
            vec![]
        ));
        Ok(())
    }

    #[gtest]
    fn test_class_call_hint() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        let mut emmyrc = ws.analysis.get_emmyrc().deref().clone();
        emmyrc.runtime.class_default_call.function_name = "__init".to_string();
        emmyrc.runtime.class_default_call.force_non_colon = true;
        emmyrc.runtime.class_default_call.force_return_self = true;
        ws.analysis.update_config(Arc::new(emmyrc));

        check!(ws.check_inlay_hint(
            r#"
                ---@class MyClass
                local A

                function A:__init(a)
                end

                A()
            "#,
            vec![VirtualInlayHint {
                label: "new".to_string(),
                line: 7,
                pos: 16,
                ref_file: Some("".to_string()),
            }]
        ));
        Ok(())
    }

    #[gtest]
    fn test_index_key_alias_hint() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(" ---@attribute index_alias(name: string)");
        check!(ws.check_inlay_hint(
            r#"
                local export = {
                    ---@[index_alias("nameX")]
                    [1] = 1,
                }
                print(export[1])
            "#,
            vec![VirtualInlayHint {
                label: ": nameX".to_string(),
                line: 5,
                pos: 30,
                ref_file: Some("".to_string()),
            }]
        ));
        Ok(())
    }

    #[gtest]
    fn test_enum_param_hint() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        let mut emmyrc = ws.get_emmyrc();
        emmyrc.hint.enum_param_hint = true;
        ws.update_emmyrc(emmyrc);
        ws.def(
            r#"
                ---@enum Status
                Status = {
                    Done = 1,
                    NotDone = 2,
                }

                ---@param a Status
                function test(a)
                end
            "#,
        );
        check!(ws.check_inlay_hint(
            r#"
                test(1)
            "#,
            vec![
                VirtualInlayHint {
                    label: "a:".to_string(),
                    line: 1,
                    pos: 21,
                    ref_file: Some("".to_string()),
                },
                VirtualInlayHint {
                    label: "Status.Done".to_string(),
                    line: 1,
                    pos: 22,
                    ref_file: None,
                },
            ]
        ));
        Ok(())
    }

    #[gtest]
    fn test_enum_param_hint_suppressed() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        let mut emmyrc = ws.get_emmyrc();
        emmyrc.hint.enum_param_hint = true;
        ws.update_emmyrc(emmyrc);
        ws.def(
            r#"
                ---@enum Status
                Status = {
                    Done = 1,
                    NotDone = 2,
                }

                ---@param a Status
                function test(a)
                end
            "#,
        );
        check!(ws.check_inlay_hint(
            r#"
                local Done = 1
                test(Done)
            "#,
            vec![VirtualInlayHint {
                label: "a:".to_string(),
                line: 2,
                pos: 21,
                ref_file: Some("".to_string()),
            }]
        ));
        Ok(())
    }

    #[gtest]
    fn test_enum_param_hint_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        let mut emmyrc = ws.get_emmyrc();
        emmyrc.hint.enum_param_hint = true;
        ws.update_emmyrc(emmyrc);
        ws.def(
            r#"
                ---@enum Status
                Status = {
                    Done = 1,
                    NotDone = 2,
                }

                ---@param a Status
                function test(a)
                end
            "#,
        );
        check!(ws.check_inlay_hint(
            r#"
                test(Status.Done)
            "#,
            vec![VirtualInlayHint {
                label: "a:".to_string(),
                line: 1,
                pos: 21,
                ref_file: Some("".to_string()),
            }]
        ));
        check!(ws.check_inlay_hint(
            r#"
                test(1)
            "#,
            vec![
                VirtualInlayHint {
                    label: "a:".to_string(),
                    line: 1,
                    pos: 21,
                    ref_file: Some("".to_string()),
                },
                VirtualInlayHint {
                    label: "Status.Done".to_string(),
                    line: 1,
                    pos: 22,
                    ref_file: None,
                },
            ]
        ));
        Ok(())
    }

    #[gtest]
    fn test_enum_param_hint_key() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        let mut emmyrc = ws.get_emmyrc();
        emmyrc.hint.enum_param_hint = true;
        ws.update_emmyrc(emmyrc);
        ws.def(
            r#"
                ---@enum (key) Status
                Status = {
                    Done = 1,
                    NotDone = 2,
                }

                ---@param a Status
                function test(a)
                end
            "#,
        );
        check!(ws.check_inlay_hint(
            r#"
                test("Done")
            "#,
            vec![VirtualInlayHint {
                label: "a:".to_string(),
                line: 1,
                pos: 21,
                ref_file: Some("".to_string()),
            }]
        ));
        Ok(())
    }

    #[gtest]
    fn test_generic_type_override() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
                ---@class A<T>
                ---@field aaa fun(a: integer): integer
            "#,
        );
        check!(ws.check_inlay_hint(
            r#"
                ---@class B<T>: A<T>
                local B

                function B:aaa(a)
                    return a
                end
            "#,
            vec![VirtualInlayHint {
                label: "override".to_string(),
                line: 4,
                pos: 33,
                ref_file: Some("".to_string()),
            }]
        ));
        Ok(())
    }
}
