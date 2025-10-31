mod description;
mod tag;
mod test;
mod types;

pub use description::*;
pub use tag::*;
pub use types::*;

use super::{
    LuaAst, LuaBinaryOpToken, LuaLiteralToken, LuaNameToken, LuaNumberToken, LuaStringToken,
};
use crate::{
    LuaAstChildren, LuaAstToken, LuaAstTokenChildren, LuaKind, LuaLiteralExpr, LuaSyntaxNode,
    kind::{LuaSyntaxKind, LuaTokenKind},
    syntax::traits::LuaAstNode,
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

/// 检查语法节点是否为附加性质的文档标签
///
/// 附加性质的标签不会阻止查找 DocDescription
fn is_additive_doc_tag(kind: LuaSyntaxKind) -> bool {
    matches!(
        kind,
        LuaSyntaxKind::DocTagVisibility
            | LuaSyntaxKind::DocTagExport
            | LuaSyntaxKind::DocTagVersion
            | LuaSyntaxKind::DocTagNodiscard
    )
}

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

    pub fn get_description(&self) -> Option<LuaDocDescription> {
        for child in self.syntax.children_with_tokens() {
            match child.kind() {
                LuaKind::Syntax(LuaSyntaxKind::DocDescription) => {
                    return LuaDocDescription::cast(child.into_node().unwrap());
                }
                LuaKind::Token(LuaTokenKind::TkDocStart) => {}
                LuaKind::Syntax(syntax_kind) => {
                    if !is_additive_doc_tag(syntax_kind) {
                        return None;
                    }
                }
                _ => {}
            }
        }
        None
    }
}

fn find_inline_node(comment: &LuaSyntaxNode) -> Option<LuaSyntaxNode> {
    let mut prev_sibling = comment.prev_sibling_or_token();
    loop {
        prev_sibling.as_ref()?;

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
                    return comment.parent();
                }
                _ => match sibling {
                    rowan::NodeOrToken::Node(node) => {
                        return Some(node);
                    }
                    rowan::NodeOrToken::Token(token) => {
                        return token.parent();
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
        next_sibling.as_ref()?;

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
                        return token.parent();
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

    pub fn is_variadic(&self) -> bool {
        self.token_by_kind(LuaTokenKind::TkDots).is_some()
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

    pub fn get_return_type_list(&self) -> LuaAstChildren<LuaDocNamedReturnType> {
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
                kind if LuaDocType::can_cast(kind.into()) => {
                    let doc_type = LuaDocType::cast(child.into_node().unwrap())?;
                    if let LuaDocType::Literal(literal) = &doc_type {
                        let literal = literal.get_literal()?;
                        match literal {
                            LuaLiteralToken::Number(num) => {
                                return Some(LuaDocObjectFieldKey::Integer(num));
                            }
                            LuaLiteralToken::String(str) => {
                                return Some(LuaDocObjectFieldKey::String(str));
                            }
                            _ => {}
                        }
                    }

                    return LuaDocObjectFieldKey::Type(doc_type).into();
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
pub struct LuaDocTypeFlag {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocTypeFlag {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::DocTypeFlag
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

impl LuaDocTypeFlag {
    pub fn get_attrib_tokens(&self) -> LuaAstTokenChildren<LuaNameToken> {
        self.tokens()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocNamedReturnType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocNamedReturnType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::DocNamedReturnType
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

impl LuaDocNamedReturnType {
    pub fn get_name_and_type(&self) -> (Option<LuaNameToken>, Option<LuaDocType>) {
        let types = self.children().collect::<Vec<LuaDocType>>();
        if types.len() == 1 {
            (None, Some(types[0].clone()))
        } else if types.len() == 2 {
            if let LuaDocType::Name(name) = &types[0] {
                (name.get_name_token(), Some(types[1].clone()))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocAttributeUse {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocAttributeUse {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::DocAttributeUse
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

impl LuaDocAttributeUse {
    pub fn get_type(&self) -> Option<LuaDocNameType> {
        self.child()
    }

    pub fn get_arg_list(&self) -> Option<LuaDocAttributeCallArgList> {
        self.child()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocAttributeCallArgList {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocAttributeCallArgList {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::DocAttributeCallArgList
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

impl LuaDocAttributeCallArgList {
    pub fn get_args(&self) -> LuaAstChildren<LuaLiteralExpr> {
        self.children()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocTagCallGeneric {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocTagCallGeneric {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::DocTagCallGeneric
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

impl LuaDocTagCallGeneric {
    pub fn get_type_list(&self) -> Option<LuaDocTypeList> {
        self.child()
    }
}
