use crate::handlers::test_lib::{ProviderVirtualWorkspace, VirtualSemanticToken, check};
use googletest::prelude::*;
use lsp_types::{SemanticTokenModifier, SemanticTokenType};
use std::collections::HashSet;

#[gtest]
fn test_1() -> Result<()> {
    let mut ws = ProviderVirtualWorkspace::new();
    ws.def(
        r#"
            ---@class Cast1
            ---@field get fun(self: self, a: number): Cast1?
        "#,
    );

    check!(ws.check_semantic_token(
        r#"
                ---@type Cast1
                local A

                local _a = A:get(1) --[[@cast -?]]:get(2)
            "#,
        vec![
            VirtualSemanticToken {
                line: 1,
                start: 16,
                length: 3,
                token_type: SemanticTokenType::COMMENT,
                token_modifier: HashSet::new(),
            },
            VirtualSemanticToken {
                line: 1,
                start: 19,
                length: 1,
                token_type: SemanticTokenType::KEYWORD,
                token_modifier: HashSet::from([SemanticTokenModifier::DOCUMENTATION]),
            },
            VirtualSemanticToken {
                line: 1,
                start: 20,
                length: 4,
                token_type: SemanticTokenType::KEYWORD,
                token_modifier: HashSet::from([SemanticTokenModifier::DOCUMENTATION]),
            },
            VirtualSemanticToken {
                line: 1,
                start: 25,
                length: 5,
                token_type: SemanticTokenType::TYPE,
                token_modifier: HashSet::new(),
            },
            VirtualSemanticToken {
                line: 2,
                start: 22,
                length: 1,
                token_type: SemanticTokenType::VARIABLE,
                token_modifier: HashSet::new(),
            },
            VirtualSemanticToken {
                line: 4,
                start: 22,
                length: 2,
                token_type: SemanticTokenType::VARIABLE,
                token_modifier: HashSet::new(),
            },
            VirtualSemanticToken {
                line: 4,
                start: 27,
                length: 1,
                token_type: SemanticTokenType::VARIABLE,
                token_modifier: HashSet::new(),
            },
            VirtualSemanticToken {
                line: 4,
                start: 29,
                length: 3,
                token_type: SemanticTokenType::FUNCTION,
                token_modifier: HashSet::new(),
            },
            VirtualSemanticToken {
                line: 4,
                start: 33,
                length: 1,
                token_type: SemanticTokenType::NUMBER,
                token_modifier: HashSet::new(),
            },
            VirtualSemanticToken {
                line: 4,
                start: 36,
                length: 4,
                token_type: SemanticTokenType::COMMENT,
                token_modifier: HashSet::new(),
            },
            VirtualSemanticToken {
                line: 4,
                start: 40,
                length: 1,
                token_type: SemanticTokenType::KEYWORD,
                token_modifier: HashSet::from([SemanticTokenModifier::DOCUMENTATION]),
            },
            VirtualSemanticToken {
                line: 4,
                start: 41,
                length: 4,
                token_type: SemanticTokenType::KEYWORD,
                token_modifier: HashSet::from([SemanticTokenModifier::DOCUMENTATION]),
            },
            VirtualSemanticToken {
                line: 4,
                start: 46,
                length: 1,
                token_type: SemanticTokenType::OPERATOR,
                token_modifier: HashSet::new(),
            },
            VirtualSemanticToken {
                line: 4,
                start: 47,
                length: 1,
                token_type: SemanticTokenType::OPERATOR,
                token_modifier: HashSet::from([SemanticTokenModifier::DOCUMENTATION]),
            },
            VirtualSemanticToken {
                line: 4,
                start: 51,
                length: 3,
                token_type: SemanticTokenType::FUNCTION,
                token_modifier: HashSet::new(),
            },
            VirtualSemanticToken {
                line: 4,
                start: 55,
                length: 1,
                token_type: SemanticTokenType::NUMBER,
                token_modifier: HashSet::new(),
            },
        ],
    ));
    Ok(())
}
