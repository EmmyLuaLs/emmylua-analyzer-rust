#[cfg(test)]
mod tests {
    use std::{ops::Deref, sync::Arc};

    use lsp_types::{InlayHint, InlayHintLabel, Location, Position, Range};

    use crate::handlers::test_lib::ProviderVirtualWorkspace;

    fn extract_first_label_part_location(inlay_hint: &InlayHint) -> Option<&Location> {
        match &inlay_hint.label {
            InlayHintLabel::LabelParts(parts) => parts.first()?.location.as_ref(),
            InlayHintLabel::String(_) => None,
        }
    }

    #[test]
    fn test_1() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws
            .check_inlay_hint(
                r#"
                ---@class Hint1
    
                ---@param a Hint1
                local function test(a)
                    local b = a
                end
            "#,
            )
            .unwrap();

        let first = result.first().unwrap();
        let location = extract_first_label_part_location(first).unwrap();

        assert_eq!(
            location.range,
            Range::new(Position::new(1, 26), Position::new(1, 31))
        );
    }

    #[test]
    fn test_2() {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        let result = ws
            .check_inlay_hint(
                r#"
    
                ---@param a number
                local function test(a)
                end
            "#,
            )
            .unwrap();

        let first = result.first().unwrap();
        let location = extract_first_label_part_location(first).unwrap();
        assert!(location.uri.path().as_str().ends_with("builtin.lua"));
    }

    #[test]
    fn test_local_hint_1() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws
            .check_inlay_hint(
                r#"
                local a = 1
            "#,
            )
            .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_local_hint_2() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws
            .check_inlay_hint(
                r#"
                local function test()
                end
            "#,
            )
            .unwrap();
        assert!(result.is_empty());

        let result = ws
            .check_inlay_hint(
                r#"
                local test = function()
                end
            "#,
            )
            .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_meta_call_hint() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws
            .check_inlay_hint(
                r#"
                ---@class Hint1
                ---@overload fun(a: string): Hint1
                local Hint1

                local a = Hint1("a")
            "#,
            )
            .unwrap();
        assert!(result.len() == 3);
    }

    #[test]
    fn test_class_def_var_hint() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws
            .check_inlay_hint(
                r#"
                ---@class Hint.1
                ---@overload fun(a: integer): Hint.1
                local Hint1
            "#,
            )
            .unwrap();
        assert!(result.len() == 0);
    }

    #[test]
    fn test_class_call_hint() {
        let mut ws = ProviderVirtualWorkspace::new();
        let mut emmyrc = ws.analysis.get_emmyrc().deref().clone();
        emmyrc.runtime.class_default_call.function_name = "__init".to_string();
        emmyrc.runtime.class_default_call.force_non_colon = true;
        emmyrc.runtime.class_default_call.force_return_self = true;
        ws.analysis.update_config(Arc::new(emmyrc));

        let result = ws
            .check_inlay_hint(
                r#"
                ---@class MyClass
                local A

                function A:__init(a)
                end

                A()
            "#,
            )
            .unwrap();
        assert!(result.len() == 1);

        let location = match &result.get(0).unwrap().label {
            InlayHintLabel::LabelParts(parts) => parts.first().unwrap().location.as_ref().unwrap(),
            InlayHintLabel::String(_) => panic!(),
        };
        assert_eq!(
            location.range,
            Range::new(Position::new(4, 27), Position::new(4, 33))
        );
    }

    #[test]
    fn test_index_key_alias_hint() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws
            .check_inlay_hint(
                r#"
                local export = {
                    [1] = 1, -- [nameX]
                }
                print(export[1])
            "#,
            )
            .unwrap();
        assert!(result.len() == 1);
    }

    #[test]
    fn test_enum_param_hint() {
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
        let result = ws
            .check_inlay_hint(
                r#"
                test(1)
            "#,
            )
            .unwrap();
        assert_eq!(result.len(), 2);
        let enum_hint = result
            .iter()
            .find(|h| match &h.label {
                InlayHintLabel::String(s) => s == "Status.Done",
                _ => false,
            })
            .expect("Enum hint not found");

        assert_eq!(
            enum_hint.position,
            lsp_types::Position {
                line: 1,
                character: 22
            }
        );
    }

    #[test]
    fn test_enum_param_hint_suppressed() {
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
        let result = ws
            .check_inlay_hint(
                r#"
                local Done = 1
                test(Done)
            "#,
            )
            .unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_enum_param_hint_1() {
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
        {
            let result = ws
                .check_inlay_hint(
                    r#"
                test(Status.Done)
            "#,
                )
                .unwrap();
            assert_eq!(result.len(), 1);
        }
        {
            let result = ws
                .check_inlay_hint(
                    r#"
                test(1)
            "#,
                )
                .unwrap();
            assert_eq!(result.len(), 2);
        }
    }

    #[test]
    fn test_enum_param_hint_key() {
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
        let result = ws
            .check_inlay_hint(
                r#"
                test("Done")
            "#,
            )
            .unwrap();
        assert_eq!(result.len(), 1);
    }
}
