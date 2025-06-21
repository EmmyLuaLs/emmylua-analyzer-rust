#[cfg(test)]
mod tests {

    use crate::handlers::test_lib::ProviderVirtualWorkspace;

    #[test]
    fn test_1() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Cast1
            ---@field get fun(self: self, a: number): Cast1?
        "#,
        );

        ws.check_semantic_token(
            r#"
                ---@type Cast1
                local A

                local _a = A:get(1) --[[@cast -?]]:get(2)
            "#,
        )
        .unwrap();
    }
}
