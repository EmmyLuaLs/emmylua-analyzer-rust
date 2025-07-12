#[cfg(test)]
mod tests {
    use crate::handlers::test_lib::ProviderVirtualWorkspace;
    use lsp_types::CodeActionOrCommand;

    #[test]
    fn test_1() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Cast1
            ---@field get fun(self: self, a: number): Cast1?
        "#,
        );

        let actions = ws
            .check_code_action(
                r#"
                ---@type Cast1
                local A

                local _a = A:get(1):get(2):get(3)
            "#,
            )
            .unwrap();
        // 6 个禁用 + 2 个修复
        assert_eq!(actions.len(), 8);
    }

    #[test]
    fn test_add_doc_tag() {
        let mut ws = ProviderVirtualWorkspace::new();
        let actions = ws
            .check_code_action(
                r#"
                ---@class Cast1
                ---@foo bar
            "#,
            )
            .unwrap();
        // 3 disable + 1 fix
        assert_eq!(actions.len(), 4);

        let CodeActionOrCommand::CodeAction(action) = &actions[0] else {
            panic!()
        };
        assert_eq!(action.title, "Add @foo to the list of known tags")
    }
}
