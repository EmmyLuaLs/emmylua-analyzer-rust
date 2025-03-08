mod description;
mod tag;
mod test;
mod types;

pub use description::*;
pub use tag::*;
pub use types::*;

use super::{LuaAst, LuaBinaryOpToken, LuaNameToken, LuaNumberToken, LuaStringToken};
use crate::{
    kind::{LuaSyntaxKind, LuaTokenKind},
    syntax::traits::LuaAstNode,
    LuaAstChildren, LuaAstToken, LuaAstTokenChildren, LuaKind, LuaSyntaxNode,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaComment {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaComment {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::Comment
    }

    fn cast(syntax: LuaSyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(Self { syntax })
        } else {
            None
        }
    }
}

impl LuaDocDescriptionOwner for LuaComment {}

impl LuaComment {
    pub fn get_owner(&self) -> Option<LuaAst> {
        if let Some(inline_node) = find_inline_node(&self.syntax) {
            LuaAst::cast(inline_node)
        } else if let Some(attached_node) = find_attached_node(&self.syntax) {
            LuaAst::cast(attached_node)
        } else {
            None
        }
    }

    pub fn get_doc_tags(&self) -> LuaAstChildren<LuaDocTag> {
        self.children()
    }
}

fn find_inline_node(comment: &LuaSyntaxNode) -> Option<LuaSyntaxNode> {
    let mut prev_sibling = comment.prev_sibling_or_token();
    loop {
        if prev_sibling.is_none() {
            return None;
        }

        if let Some(sibling) = prev_sibling {
            match sibling.kind() {
                LuaKind::Token(
                    LuaTokenKind::TkWhitespace | LuaTokenKind::TkComma | LuaTokenKind::TkSemicolon,
                ) => {}
                LuaKind::Token(LuaTokenKind::TkEndOfLine)
                | LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                    return None;
                }
                LuaKind::Token(k) if k != LuaTokenKind::TkName => {
                    return Some(comment.parent()?);
                }
                _ => match sibling {
                    rowan::NodeOrToken::Node(node) => {
                        return Some(node);
                    }
                    rowan::NodeOrToken::Token(token) => {
                        return Some(token.parent()?);
                    }
                },
            }
            prev_sibling = sibling.prev_sibling_or_token();
        } else {
            return None;
        }
    }
}

fn find_attached_node(comment: &LuaSyntaxNode) -> Option<LuaSyntaxNode> {
    let mut meet_end_of_line = false;

    let mut next_sibling = comment.next_sibling_or_token();
    loop {
        if next_sibling.is_none() {
            return None;
        }

        if let Some(sibling) = next_sibling {
            match sibling.kind() {
                LuaKind::Token(LuaTokenKind::TkEndOfLine) => {
                    if meet_end_of_line {
                        return None;
                    }

                    meet_end_of_line = true;
                }
                LuaKind::Token(LuaTokenKind::TkWhitespace) => {}
                LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                    return None;
                }
                LuaKind::Syntax(LuaSyntaxKind::Block) => {
                    let first_child = comment.first_child()?;
                    if first_child.kind() == LuaKind::Syntax(LuaSyntaxKind::Comment) {
                        return None;
                    }
                    return Some(first_child);
                }
                _ => match sibling {
                    rowan::NodeOrToken::Node(node) => {
                        return Some(node);
                    }
                    rowan::NodeOrToken::Token(token) => {
                        return Some(token.parent()?);
                    }
                },
            }
            next_sibling = sibling.next_sibling_or_token();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocGenericDeclList {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocGenericDeclList {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::DocGenericDeclareList
    }

    fn cast(syntax: LuaSyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(Self { syntax })
        } else {
            None
        }
    }
}

impl LuaDocGenericDeclList {
    pub fn get_generic_decl(&self) -> LuaAstChildren<LuaDocGenericDecl> {
        self.children()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocGenericDecl {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocGenericDecl {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::DocGenericParameter
    }

    fn cast(syntax: LuaSyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(Self { syntax })
        } else {
            None
        }
    }
}

impl LuaDocGenericDecl {
    pub fn get_name_token(&self) -> Option<LuaNameToken> {
        self.token()
    }

    pub fn get_type(&self) -> Option<LuaDocType> {
        self.child()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocTypeList {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocTypeList {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::DocTypeList
    }

    fn cast(syntax: LuaSyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(Self { syntax })
        } else {
            None
        }
    }
}

impl LuaDocTypeList {
    pub fn get_types(&self) -> LuaAstChildren<LuaDocType> {
        self.children()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocOpType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocOpType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::DocOpType
    }

    fn cast(syntax: LuaSyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(Self { syntax })
        } else {
            None
        }
    }
}

impl LuaDocOpType {
    pub fn get_op(&self) -> Option<LuaBinaryOpToken> {
        self.token()
    }

    pub fn get_type(&self) -> Option<LuaDocType> {
        self.child()
    }

    pub fn is_nullable(&self) -> bool {
        self.token_by_kind(LuaTokenKind::TkDocQuestion).is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocObjectField {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocObjectField {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::DocObjectField
    }

    fn cast(syntax: LuaSyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(Self { syntax })
        } else {
            None
        }
    }
}

impl LuaDocObjectField {
    pub fn get_field_key(&self) -> Option<LuaDocObjectFieldKey> {
        for child in self.syntax.children_with_tokens() {
            match child.kind() {
                LuaKind::Token(LuaTokenKind::TkName) => {
                    return LuaNameToken::cast(child.into_token().unwrap())
                        .map(LuaDocObjectFieldKey::Name);
                }
                LuaKind::Token(LuaTokenKind::TkString) => {
                    return LuaStringToken::cast(child.into_token().unwrap())
                        .map(LuaDocObjectFieldKey::String);
                }
                LuaKind::Token(LuaTokenKind::TkInt) => {
                    return LuaNumberToken::cast(child.into_token().unwrap())
                        .map(LuaDocObjectFieldKey::Integer);
                }
                kind if LuaDocType::can_cast(kind.into()) => {
                    return LuaDocType::cast(child.into_node().unwrap())
                        .map(LuaDocObjectFieldKey::Type);
                }
                LuaKind::Token(LuaTokenKind::TkColon) => {
                    return None;
                }
                _ => {}
            }
        }

        None
    }

    pub fn get_type(&self) -> Option<LuaDocType> {
        self.children().last()
    }

    pub fn is_nullable(&self) -> bool {
        self.token_by_kind(LuaTokenKind::TkDocQuestion).is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LuaDocObjectFieldKey {
    Name(LuaNameToken),
    String(LuaStringToken),
    Integer(LuaNumberToken),
    Type(LuaDocType),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocAttribute {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocAttribute {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::DocAttribute
    }

    fn cast(syntax: LuaSyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(Self { syntax })
        } else {
            None
        }
    }
}

impl LuaDocAttribute {
    pub fn get_attrib_tokens(&self) -> LuaAstTokenChildren<LuaNameToken> {
        self.tokens()
    }
}
