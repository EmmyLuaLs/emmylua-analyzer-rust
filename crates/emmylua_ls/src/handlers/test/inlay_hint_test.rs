#[cfg(test)]
mod tests {
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
        assert!(result.len() == 4);
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
        assert!(result.len() == 1);
    }
}
