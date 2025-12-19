#[cfg(test)]
mod tests {
    use crate::handlers::test_lib::ProviderVirtualWorkspace;
    use googletest::prelude::*;

    #[gtest]
    fn test_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        let _ = ws.check_semantic_token(
            r#"
            ---@class Cast1
            ---@field a string      # test
        "#,
            vec![],
        );
        Ok(())
    }
}
