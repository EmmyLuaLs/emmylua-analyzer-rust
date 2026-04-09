#[cfg(test)]
mod tests {
    use crate::handlers::{
        semantic_token::{SemanticTokenModifierKind, SemanticTokenTypeKind},
        test_lib::ProviderVirtualWorkspace,
    };
    use googletest::prelude::*;

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

    fn make_issue_1028_repeated_prefix_guard_chain_content() -> String {
        let mut content = String::from("V_cfad19afc42b = V_cfad19afc42b or {}\n");
        for i in 0..600 {
            let table_key = 3_121_212;
            let field_key = 1_111_112 + i;
            content.push_str(&format!(
                "if V_cfad19afc42b[{table_key}] and V_cfad19afc42b[{table_key}][{field_key}] then\n    V_cfad19afc42b[{table_key}][{field_key}][\"__STR_{i}__\"] = \"__STR_{}__\"\nend\n\n",
                i + 1,
            ));
        }
        content
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

        let class_idx = SemanticTokenTypeKind::Class.to_u32();
        let namespace_idx = SemanticTokenTypeKind::Namespace.to_u32();
        let method_idx = SemanticTokenTypeKind::Method.to_u32();
        let readonly = SemanticTokenModifierKind::READONLY.to_u32();

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

    #[gtest]
    fn test_return_overload_tag_is_documentation_keyword() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        let data = ws.get_semantic_token_data(
            r#"---@return_overload true, integer
"#,
        )?;
        let tokens = decode(&data);
        let keyword = SemanticTokenTypeKind::Keyword.to_u32();
        let doc = SemanticTokenModifierKind::DOCUMENTATION.to_u32();

        verify_that!(&tokens, contains(eq(&(0, 4, 15, keyword, doc))))?;
        Ok(())
    }

    #[gtest]
    fn test_return_overload_rows_highlight_types() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        let data = ws.get_semantic_token_data(concat!(
            "--- @return_overload false, [string,string]\n",
            "--- @return_overload true, string\n",
        ))?;
        let tokens = decode(&data);
        let typ = SemanticTokenTypeKind::Type.to_u32();
        let variable = SemanticTokenTypeKind::Variable.to_u32();
        let default_library = SemanticTokenModifierKind::DEFAULT_LIBRARY.to_u32();

        verify_that!(
            &tokens,
            all![
                contains(eq(&(0, 21, 5, typ, 0))),
                contains(eq(&(0, 29, 6, typ, default_library))),
                contains(eq(&(0, 36, 6, typ, default_library))),
                contains(eq(&(1, 21, 4, typ, 0))),
                contains(eq(&(1, 27, 6, typ, default_library))),
                not(contains(eq(&(0, 29, 6, variable, 0)))),
                not(contains(eq(&(0, 36, 6, variable, 0)))),
                not(contains(eq(&(1, 27, 6, variable, 0)))),
            ]
        )?;
        Ok(())
    }

    #[gtest]
    fn test_issue_1028_i18n_semantic_tokens_repeated_prefix_guard_chain() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        let content = make_issue_1028_repeated_prefix_guard_chain_content();
        let file_id = ws.def_file("i18n.lua", &content);
        let data = ws.get_semantic_token_data_for_file(file_id)?;
        assert!(!data.is_empty());
        Ok(())
    }
}
