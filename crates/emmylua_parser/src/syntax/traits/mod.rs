mod comment_trait;
mod description_trait;

use std::marker::PhantomData;

use rowan::{TextRange, TextSize, WalkEvent};

use crate::{
    kind::{LuaSyntaxKind, LuaTokenKind},
    LuaAstPtr,
};

use super::LuaSyntaxId;
pub use super::{
    node::*, LuaSyntaxElementChildren, LuaSyntaxNode, LuaSyntaxNodeChildren, LuaSyntaxToken,
};
pub use comment_trait::*;
pub use description_trait::*;

pub trait LuaAstNode {
    fn syntax(&self) -> &LuaSyntaxNode;

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized;

    fn cast(syntax: LuaSyntaxNode) -> Option<Self>
    where
        Self: Sized;

    fn child<N: LuaAstNode>(&self) -> Option<N> {
        self.syntax().children().find_map(N::cast)
    }

    fn token<N: LuaAstToken>(&self) -> Option<N> {
        self.syntax()
            .children_with_tokens()
            .find_map(|it| it.into_token().and_then(N::cast))
    }

    fn token_by_kind(&self, kind: LuaTokenKind) -> Option<LuaGeneralToken> {
        let token = self
            .syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == kind.into())?;

        LuaGeneralToken::cast(token)
    }

    fn tokens<N: LuaAstToken>(&self) -> LuaAstTokenChildren<N> {
        LuaAstTokenChildren::new(self.syntax())
    }

    fn children<N: LuaAstNode>(&self) -> LuaAstChildren<N> {
        LuaAstChildren::new(self.syntax())
    }

    fn descendants<N: LuaAstNode>(&self) -> impl Iterator<Item = N> {
        self.syntax().descendants().filter_map(N::cast)
    }

    fn walk_descendants<N: LuaAstNode>(&self) -> impl Iterator<Item = WalkEvent<N>> {
        self.syntax().preorder().filter_map(|event| match event {
            WalkEvent::Enter(node) => N::cast(node).map(WalkEvent::Enter),
            WalkEvent::Leave(node) => N::cast(node).map(WalkEvent::Leave),
        })
    }

    fn ancestors<N: LuaAstNode>(&self) -> impl Iterator<Item = N> {
        self.syntax().ancestors().filter_map(N::cast)
    }

    fn get_root(&self) -> LuaSyntaxNode {
        let syntax = self.syntax();
        if syntax.kind() == LuaSyntaxKind::Chunk.into() {
            syntax.clone()
        } else {
            syntax.ancestors().last().unwrap()
        }
    }

    fn get_parent<N: LuaAstNode>(&self) -> Option<N> {
        self.syntax().parent().and_then(N::cast)
    }

    fn get_position(&self) -> TextSize {
        let range = self.syntax().text_range();
        range.start()
    }

    fn get_range(&self) -> TextRange {
        self.syntax().text_range()
    }

    fn get_syntax_id(&self) -> LuaSyntaxId {
        LuaSyntaxId::from_node(self.syntax())
    }

    fn get_text(&self) -> String {
        format!("{}", self.syntax().text())
    }

    fn dump(&self) -> String {
        format!("{:#?}", self.syntax())
    }

    fn to_ptr(&self) -> LuaAstPtr<Self>
    where
        Self: Sized,
    {
        LuaAstPtr::new(&self)
    }
}

/// An iterator over `SyntaxNode` children of a particular AST type.
#[derive(Debug, Clone)]
pub struct LuaAstChildren<N> {
    inner: LuaSyntaxNodeChildren,
    ph: PhantomData<N>,
}

impl<N> LuaAstChildren<N> {
    pub fn new(parent: &LuaSyntaxNode) -> LuaAstChildren<N> {
        LuaAstChildren {
            inner: parent.children(),
            ph: PhantomData,
        }
    }
}

impl<N: LuaAstNode> Iterator for LuaAstChildren<N> {
    type Item = N;

    fn next(&mut self) -> Option<N> {
        self.inner.find_map(N::cast)
    }
}

pub trait LuaAstToken {
    fn syntax(&self) -> &LuaSyntaxToken;

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized;

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized;

    fn get_token_kind(&self) -> LuaTokenKind {
        self.syntax().kind().into()
    }

    fn get_position(&self) -> TextSize {
        let range = self.syntax().text_range();
        range.start()
    }

    fn get_range(&self) -> TextRange {
        self.syntax().text_range()
    }

    fn get_syntax_id(&self) -> LuaSyntaxId {
        LuaSyntaxId::from_token(self.syntax())
    }

    fn get_text(&self) -> &str {
        self.syntax().text()
    }

    fn get_parent<N: LuaAstNode>(&self) -> Option<N> {
        self.syntax().parent().and_then(N::cast)
    }

    fn ancestors<N: LuaAstNode>(&self) -> impl Iterator<Item = N> {
        self.syntax().parent_ancestors().filter_map(N::cast)
    }

    fn dump(&self) -> String {
        format!("{:#?}", self.syntax())
    }
}

#[derive(Debug, Clone)]
pub struct LuaAstTokenChildren<N> {
    inner: LuaSyntaxElementChildren,
    ph: PhantomData<N>,
}

impl<N> LuaAstTokenChildren<N> {
    pub fn new(parent: &LuaSyntaxNode) -> LuaAstTokenChildren<N> {
        LuaAstTokenChildren {
            inner: parent.children_with_tokens(),
            ph: PhantomData,
        }
    }
}

impl<N: LuaAstToken> Iterator for LuaAstTokenChildren<N> {
    type Item = N;

    fn next(&mut self) -> Option<N> {
        self.inner.find_map(|it| it.into_token().and_then(N::cast))
    }
}
