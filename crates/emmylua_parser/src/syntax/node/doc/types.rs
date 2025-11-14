use crate::{
    LuaAstChildren, LuaAstNode, LuaAstToken, LuaDocDescriptionOwner, LuaDocTypeBinaryToken,
    LuaDocTypeUnaryToken, LuaLiteralToken, LuaNameToken, LuaSyntaxKind, LuaSyntaxNode,
    LuaTokenKind,
};

use rowan::SyntaxElement;

use super::{LuaDocGenericDecl, LuaDocGenericDeclList, LuaDocObjectField, LuaDocTypeList};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LuaDocType {
    Name(LuaDocNameType),
    Infer(LuaDocInferType),
    Array(LuaDocArrayType),
    Func(LuaDocFuncType),
    Object(LuaDocObjectType),
    Binary(LuaDocBinaryType),
    Unary(LuaDocUnaryType),
    Conditional(LuaDocConditionalType),
    Tuple(LuaDocTupleType),
    Literal(LuaDocLiteralType),
    Variadic(LuaDocVariadicType),
    Nullable(LuaDocNullableType),
    Generic(LuaDocGenericType),
    StrTpl(LuaDocStrTplType),
    MultiLineUnion(LuaDocMultiLineUnionType),
    Attribute(LuaDocAttributeType),
    Mapped(LuaDocMappedType),
    IndexAccess(LuaDocIndexAccessType),
}

impl LuaAstNode for LuaDocType {
    fn syntax(&self) -> &LuaSyntaxNode {
        match self {
            LuaDocType::Name(it) => it.syntax(),
            LuaDocType::Infer(it) => it.syntax(),
            LuaDocType::Array(it) => it.syntax(),
            LuaDocType::Func(it) => it.syntax(),
            LuaDocType::Object(it) => it.syntax(),
            LuaDocType::Binary(it) => it.syntax(),
            LuaDocType::Unary(it) => it.syntax(),
            LuaDocType::Conditional(it) => it.syntax(),
            LuaDocType::Tuple(it) => it.syntax(),
            LuaDocType::Literal(it) => it.syntax(),
            LuaDocType::Variadic(it) => it.syntax(),
            LuaDocType::Nullable(it) => it.syntax(),
            LuaDocType::Generic(it) => it.syntax(),
            LuaDocType::StrTpl(it) => it.syntax(),
            LuaDocType::MultiLineUnion(it) => it.syntax(),
            LuaDocType::Attribute(it) => it.syntax(),
            LuaDocType::Mapped(it) => it.syntax(),
            LuaDocType::IndexAccess(it) => it.syntax(),
        }
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(
            kind,
            LuaSyntaxKind::TypeName
                | LuaSyntaxKind::TypeInfer
                | LuaSyntaxKind::TypeArray
                | LuaSyntaxKind::TypeFun
                | LuaSyntaxKind::TypeObject
                | LuaSyntaxKind::TypeBinary
                | LuaSyntaxKind::TypeUnary
                | LuaSyntaxKind::TypeConditional
                | LuaSyntaxKind::TypeTuple
                | LuaSyntaxKind::TypeLiteral
                | LuaSyntaxKind::TypeVariadic
                | LuaSyntaxKind::TypeNullable
                | LuaSyntaxKind::TypeGeneric
                | LuaSyntaxKind::TypeStringTemplate
                | LuaSyntaxKind::TypeMultiLineUnion
                | LuaSyntaxKind::TypeAttribute
                | LuaSyntaxKind::TypeMapped
                | LuaSyntaxKind::TypeIndexAccess
        )
    }

    fn cast(syntax: LuaSyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind().into() {
            LuaSyntaxKind::TypeName => Some(LuaDocType::Name(LuaDocNameType::cast(syntax)?)),
            LuaSyntaxKind::TypeInfer => Some(LuaDocType::Infer(LuaDocInferType::cast(syntax)?)),
            LuaSyntaxKind::TypeArray => Some(LuaDocType::Array(LuaDocArrayType::cast(syntax)?)),
            LuaSyntaxKind::TypeFun => Some(LuaDocType::Func(LuaDocFuncType::cast(syntax)?)),
            LuaSyntaxKind::TypeObject => Some(LuaDocType::Object(LuaDocObjectType::cast(syntax)?)),
            LuaSyntaxKind::TypeMapped => Some(LuaDocType::Mapped(LuaDocMappedType::cast(syntax)?)),
            LuaSyntaxKind::TypeIndexAccess => Some(LuaDocType::IndexAccess(
                LuaDocIndexAccessType::cast(syntax)?,
            )),
            LuaSyntaxKind::TypeBinary => Some(LuaDocType::Binary(LuaDocBinaryType::cast(syntax)?)),
            LuaSyntaxKind::TypeUnary => Some(LuaDocType::Unary(LuaDocUnaryType::cast(syntax)?)),
            LuaSyntaxKind::TypeConditional => Some(LuaDocType::Conditional(
                LuaDocConditionalType::cast(syntax)?,
            )),
            LuaSyntaxKind::TypeTuple => Some(LuaDocType::Tuple(LuaDocTupleType::cast(syntax)?)),
            LuaSyntaxKind::TypeLiteral => {
                Some(LuaDocType::Literal(LuaDocLiteralType::cast(syntax)?))
            }
            LuaSyntaxKind::TypeVariadic => {
                Some(LuaDocType::Variadic(LuaDocVariadicType::cast(syntax)?))
            }
            LuaSyntaxKind::TypeNullable => {
                Some(LuaDocType::Nullable(LuaDocNullableType::cast(syntax)?))
            }
            LuaSyntaxKind::TypeGeneric => {
                Some(LuaDocType::Generic(LuaDocGenericType::cast(syntax)?))
            }
            LuaSyntaxKind::TypeStringTemplate => {
                Some(LuaDocType::StrTpl(LuaDocStrTplType::cast(syntax)?))
            }
            LuaSyntaxKind::TypeMultiLineUnion => Some(LuaDocType::MultiLineUnion(
                LuaDocMultiLineUnionType::cast(syntax)?,
            )),
            LuaSyntaxKind::TypeAttribute => {
                Some(LuaDocType::Attribute(LuaDocAttributeType::cast(syntax)?))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocNameType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocNameType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeName
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

impl LuaDocNameType {
    pub fn get_name_token(&self) -> Option<LuaNameToken> {
        self.token()
    }

    pub fn get_name_text(&self) -> Option<String> {
        self.get_name_token()
            .map(|it| it.get_name_text().to_string())
    }

    pub fn get_generic_param(&self) -> Option<LuaDocGenericDecl> {
        self.child()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocInferType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocInferType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeInfer
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

impl LuaDocInferType {
    pub fn get_generic_decl(&self) -> Option<LuaDocGenericDecl> {
        self.child()
    }

    pub fn get_generic_decl_name_text(&self) -> Option<String> {
        self.get_generic_decl()?
            .get_name_token()
            .map(|it| it.get_name_text().to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocArrayType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocArrayType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeArray
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

impl LuaDocArrayType {
    pub fn get_type(&self) -> Option<LuaDocType> {
        self.child()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocFuncType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocFuncType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeFun
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

impl LuaDocFuncType {
    pub fn is_async(&self) -> bool {
        match self.token::<LuaNameToken>() {
            Some(it) => it.get_name_text() == "async",
            None => false,
        }
    }

    pub fn is_sync(&self) -> bool {
        match self.token::<LuaNameToken>() {
            Some(it) => it.get_name_text() == "sync",
            None => false,
        }
    }

    pub fn get_params(&self) -> LuaAstChildren<LuaDocTypeParam> {
        self.children()
    }

    pub fn get_generic_decl_list(&self) -> Option<LuaDocGenericDeclList> {
        self.child()
    }

    pub fn get_return_type_list(&self) -> Option<LuaDocTypeList> {
        self.child()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocTypeParam {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocTypeParam {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::DocTypedParameter
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

impl LuaDocTypeParam {
    pub fn is_dots(&self) -> bool {
        self.token_by_kind(LuaTokenKind::TkDots).is_some()
    }

    pub fn get_name_token(&self) -> Option<LuaNameToken> {
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
pub struct LuaDocObjectType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocObjectType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeObject
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

impl LuaDocObjectType {
    pub fn get_fields(&self) -> LuaAstChildren<LuaDocObjectField> {
        self.children()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocBinaryType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocBinaryType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeBinary
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

impl LuaDocBinaryType {
    pub fn get_op_token(&self) -> Option<LuaDocTypeBinaryToken> {
        self.token()
    }

    pub fn get_types(&self) -> Option<(LuaDocType, LuaDocType)> {
        let mut children = self.children();
        let left = children.next()?;
        let right = children.next()?;
        Some((left, right))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocUnaryType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocUnaryType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeUnary
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

impl LuaDocUnaryType {
    pub fn get_op_token(&self) -> Option<LuaDocTypeUnaryToken> {
        self.token()
    }

    pub fn get_type(&self) -> Option<LuaDocType> {
        self.child()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocConditionalType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocConditionalType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeConditional
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

impl LuaDocConditionalType {
    pub fn get_types(&self) -> Option<(LuaDocType, LuaDocType, LuaDocType)> {
        let mut children = self.children();
        let condition = children.next()?;
        let true_type = children.next()?;
        let false_type = children.next()?;
        Some((condition, true_type, false_type))
    }

    pub fn get_true_type(&self) -> Option<LuaDocType> {
        let mut children = self.children();
        children.next()?;
        children.next()
    }

    pub fn has_new(&self) -> Option<bool> {
        let condition = self.children().next()?;
        let binary = match condition {
            LuaDocType::Binary(binary) => binary,
            _ => return None,
        };

        let mut seen_extends = false;

        for element in binary.syntax().children_with_tokens() {
            match element {
                SyntaxElement::Token(token) => {
                    let kind: LuaTokenKind = token.kind().into();
                    if !seen_extends {
                        if kind == LuaTokenKind::TkDocExtends {
                            seen_extends = true;
                        }
                    } else if kind == LuaTokenKind::TkDocNew {
                        return Some(true);
                    }
                }
                SyntaxElement::Node(node) => {
                    if !seen_extends {
                        continue;
                    }

                    if node.kind() == LuaSyntaxKind::TypeFun.into() {
                        return Some(false);
                    }
                }
            }
        }

        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocTupleType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocTupleType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeTuple
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

impl LuaDocTupleType {
    pub fn get_types(&self) -> LuaAstChildren<LuaDocType> {
        self.children()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocLiteralType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocLiteralType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeLiteral
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

impl LuaDocLiteralType {
    pub fn get_literal(&self) -> Option<LuaLiteralToken> {
        self.token()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocVariadicType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocVariadicType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeVariadic
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

impl LuaDocVariadicType {
    pub fn get_type(&self) -> Option<LuaDocType> {
        self.child()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocNullableType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocNullableType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeNullable
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

impl LuaDocNullableType {
    pub fn get_type(&self) -> Option<LuaDocType> {
        self.child()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocGenericType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocGenericType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeGeneric
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

impl LuaDocGenericType {
    pub fn get_name_type(&self) -> Option<LuaDocNameType> {
        self.child()
    }

    pub fn get_generic_types(&self) -> Option<LuaDocTypeList> {
        self.child()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocStrTplType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocStrTplType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeStringTemplate
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

impl LuaDocStrTplType {
    /// `T` or  xxx.`T` or xxx.`T`.xxxx
    pub fn get_name(&self) -> (Option<String>, Option<String>, Option<String>) {
        let str_tpl = self.token_by_kind(LuaTokenKind::TkStringTemplateType);
        if str_tpl.is_none() {
            return (None, None, None);
        }
        let str_tpl = str_tpl.unwrap();
        let text = str_tpl.get_text();
        let mut iter = text.split('`');
        let first = iter.next().map(|it| it.to_string());
        let second = iter.next().map(|it| it.to_string());
        let third = iter.next().map(|it| it.to_string());

        (first, second, third)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocMultiLineUnionType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocMultiLineUnionType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeMultiLineUnion
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

impl LuaDocMultiLineUnionType {
    pub fn get_fields(&self) -> LuaAstChildren<LuaDocOneLineField> {
        self.children()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocOneLineField {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocOneLineField {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::DocOneLineField
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

impl LuaDocDescriptionOwner for LuaDocOneLineField {}

impl LuaDocOneLineField {
    pub fn get_type(&self) -> Option<LuaDocType> {
        self.child()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocAttributeType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocAttributeType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeAttribute
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

impl LuaDocAttributeType {
    pub fn get_params(&self) -> LuaAstChildren<LuaDocTypeParam> {
        self.children()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocMappedType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocMappedType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeMapped
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

impl LuaDocMappedType {
    pub fn get_key(&self) -> Option<LuaDocMappedKey> {
        self.child()
    }

    pub fn get_value_type(&self) -> Option<LuaDocType> {
        self.child()
    }

    pub fn is_readonly(&self) -> bool {
        let mut modifier: Option<bool> = None;

        for element in self.syntax().children_with_tokens() {
            match element {
                SyntaxElement::Node(node) => {
                    if node.kind() == LuaSyntaxKind::DocMappedKey.into() {
                        break;
                    }
                }
                SyntaxElement::Token(token) => {
                    let kind: LuaTokenKind = token.kind().into();
                    match kind {
                        LuaTokenKind::TkPlus => modifier = Some(true),
                        LuaTokenKind::TkMinus => modifier = Some(false),
                        LuaTokenKind::TkDocReadonly => return modifier.unwrap_or(true),
                        _ => {}
                    }
                }
            }
        }

        false
    }

    pub fn is_optional(&self) -> bool {
        let mut seen_key = false;
        let mut modifier: Option<bool> = None;

        for element in self.syntax().children_with_tokens() {
            match element {
                SyntaxElement::Node(node) => {
                    if node.kind() == LuaSyntaxKind::DocMappedKey.into() {
                        seen_key = true;
                    }
                }
                SyntaxElement::Token(token) => {
                    if !seen_key {
                        continue;
                    }

                    let kind: LuaTokenKind = token.kind().into();
                    match kind {
                        LuaTokenKind::TkPlus => modifier = Some(true),
                        LuaTokenKind::TkMinus => modifier = Some(false),
                        LuaTokenKind::TkDocQuestion => return modifier.unwrap_or(true),
                        LuaTokenKind::TkColon => break,
                        _ => {}
                    }
                }
            }
        }

        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocIndexAccessType {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocIndexAccessType {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::TypeIndexAccess
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

impl LuaDocIndexAccessType {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocMappedKey {
    syntax: LuaSyntaxNode,
}

impl LuaAstNode for LuaDocMappedKey {
    fn syntax(&self) -> &LuaSyntaxNode {
        &self.syntax
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaSyntaxKind::DocMappedKey
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

impl LuaDocMappedKey {}
