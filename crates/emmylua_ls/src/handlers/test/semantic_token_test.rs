#[cfg(test)]
mod tests {
    use crate::handlers::semantic_token::{SEMANTIC_TOKEN_MODIFIERS, SEMANTIC_TOKEN_TYPES};
    use crate::handlers::test_lib::ProviderVirtualWorkspace;
    use googletest::prelude::*;
    use lsp_types::{SemanticTokenModifier, SemanticTokenType};

    fn token_type_index(token_type: SemanticTokenType) -> u32 {
        SEMANTIC_TOKEN_TYPES
            .iter()
            .position(|t| t == &token_type)
            .unwrap() as u32
    }

    fn modifier_bitset(modifiers: &[SemanticTokenModifier]) -> u32 {
        modifiers.iter().fold(0, |acc, m| {
            let index = SEMANTIC_TOKEN_MODIFIERS
                .iter()
                .position(|x| x == m)
                .unwrap() as u32;
            acc | (1 << index)
        })
    }

    fn decode(data: &[u32]) -> Vec<(u32, u32, u32, u32, u32)> {
        let mut result = Vec::new();
        let mut line = 0;
        let mut col = 0;
        for chunk in data.chunks_exact(5) {
            let delta_line = chunk[0];
            let delta_start = chunk[1];
            let length = chunk[2];
            let token_type = chunk[3];
            let token_modifiers = chunk[4];

            if delta_line > 0 {
                line += delta_line;
                col = 0;
            }
            col += delta_start;

            result.push((line, col, length, token_type, token_modifiers));
        }
        result
    }

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

    #[gtest]
    fn test_require_alias_prefix_is_namespace_in_index_expr() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file("mod.lua", "return {}");
        let main = ws.def_file(
            "main.lua",
            r#"local m = require("mod")
m.foo()
"#,
        );

        let data = ws.get_semantic_token_data_for_file(main)?;
        let tokens = decode(&data);

        let class_idx = token_type_index(SemanticTokenType::CLASS);
        let namespace_idx = token_type_index(SemanticTokenType::NAMESPACE);
        let method_idx = token_type_index(SemanticTokenType::METHOD);
        let readonly = modifier_bitset(&[SemanticTokenModifier::READONLY]);

        // `local m = require("mod")`
        verify_that!(&tokens, contains(eq(&(0, 6, 1, class_idx, readonly))))?;

        // `m.foo()`
        verify_that!(
            &tokens,
            all![
                contains(eq(&(1, 0, 1, namespace_idx, 0))),
                contains(eq(&(1, 2, 3, method_idx, 0))),
            ]
        )?;

        Ok(())
    }
}
