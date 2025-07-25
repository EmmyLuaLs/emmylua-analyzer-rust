use crate::{LuaAstNode, LuaDocDetailToken, LuaSyntaxKind, LuaSyntaxNode, LuaTokenKind};

#[allow(unused)]
pub trait LuaDocDetailOwner: LuaAstNode {
    fn get_detail(&self) -> Option<LuaDocDetailToken> {
        self.token()
    }

    fn get_detail_text(&self) -> Option<String> {
        self.get_detail().map(|it| it.get_detail().to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocDescription {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocDescription {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::DocDescription
    }

    fn cast(syntax: LuaSyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        if syntax.kind() == LuaSyntaxKind::DocDescription.into() {
            Some(Self { syntax })
        } else {
            None
        }
    }
}

impl LuaDocDetailOwner for LuaDocDescription {}

impl LuaDocDescription {
    pub fn get_description_text(&self) -> String {
        let mut text = String::new();
        for token in self
            .syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
        {
            match token.kind().into() {
                LuaTokenKind::TkDocDetail => {
                    text.push_str(token.text());
                }
                LuaTokenKind::TkEndOfLine => {
                    text.push('\n');
                }
                _ => {}
            }
        }

        text
    }
}
