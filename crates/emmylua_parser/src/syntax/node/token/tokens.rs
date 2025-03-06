use crate::{
    kind::{BinaryOperator, LuaTokenKind, UnaryOperator},
    syntax::traits::LuaAstToken,
    LuaOpKind, LuaSyntaxToken, LuaTypeBinaryOperator, LuaTypeUnaryOperator, LuaVersionNumber,
    VisibilityKind,
};

use super::{float_token_value, int_token_value, string_token_value};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaGeneralToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaGeneralToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(_: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        true
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        Some(LuaGeneralToken { token: syntax })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaNameToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaNameToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaTokenKind::TkName.into()
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(LuaNameToken { token: syntax })
        } else {
            None
        }
    }
}

impl LuaNameToken {
    pub fn get_name_text(&self) -> &str {
        self.token.text()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaStringToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaStringToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaTokenKind::TkString.into() || kind == LuaTokenKind::TkLongString.into()
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(LuaStringToken { token: syntax })
        } else {
            None
        }
    }
}

impl LuaStringToken {
    pub fn get_value(&self) -> String {
        match string_token_value(&self.token) {
            Ok(str) => str,
            Err(_) => String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaNumberToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaNumberToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaTokenKind::TkFloat.into() || kind == LuaTokenKind::TkInt.into()
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(LuaNumberToken { token: syntax })
        } else {
            None
        }
    }
}

impl LuaNumberToken {
    pub fn is_float(&self) -> bool {
        self.token.kind() == LuaTokenKind::TkFloat.into()
    }

    pub fn is_int(&self) -> bool {
        self.token.kind() == LuaTokenKind::TkInt.into()
    }

    pub fn get_float_value(&self) -> f64 {
        if !self.is_float() {
            return 0.0;
        }
        match float_token_value(&self.token) {
            Ok(float) => float,
            Err(_) => 0.0,
        }
    }

    pub fn get_int_value(&self) -> i64 {
        if !self.is_int() {
            return 0;
        }
        match int_token_value(&self.token) {
            Ok(int) => int,
            Err(_) => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaBinaryOpToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaBinaryOpToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        LuaOpKind::to_binary_operator(kind) != BinaryOperator::OpNop
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(LuaBinaryOpToken { token: syntax })
        } else {
            None
        }
    }
}

impl LuaBinaryOpToken {
    pub fn get_op(&self) -> BinaryOperator {
        LuaOpKind::to_binary_operator(self.token.kind().into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaUnaryOpToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaUnaryOpToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        LuaOpKind::to_unary_operator(kind) != UnaryOperator::OpNop
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(LuaUnaryOpToken { token: syntax })
        } else {
            None
        }
    }
}

impl LuaUnaryOpToken {
    pub fn get_op(&self) -> UnaryOperator {
        LuaOpKind::to_unary_operator(self.token.kind().into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaKeywordToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaKeywordToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        match kind {
            LuaTokenKind::TkAnd
            | LuaTokenKind::TkBreak
            | LuaTokenKind::TkDo
            | LuaTokenKind::TkElse
            | LuaTokenKind::TkElseIf
            | LuaTokenKind::TkEnd
            | LuaTokenKind::TkFalse
            | LuaTokenKind::TkFor
            | LuaTokenKind::TkFunction
            | LuaTokenKind::TkGoto
            | LuaTokenKind::TkIf
            | LuaTokenKind::TkIn
            | LuaTokenKind::TkLocal
            | LuaTokenKind::TkNil
            | LuaTokenKind::TkNot
            | LuaTokenKind::TkOr
            | LuaTokenKind::TkRepeat
            | LuaTokenKind::TkReturn
            | LuaTokenKind::TkThen
            | LuaTokenKind::TkTrue
            | LuaTokenKind::TkUntil
            | LuaTokenKind::TkWhile => true,
            _ => false,
        }
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(LuaKeywordToken { token: syntax })
        } else {
            None
        }
    }
}

impl LuaKeywordToken {
    pub fn get_keyword(&self) -> LuaTokenKind {
        self.token.kind().into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaBoolToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaBoolToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaTokenKind::TkTrue.into() || kind == LuaTokenKind::TkFalse.into()
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(LuaBoolToken { token: syntax })
        } else {
            None
        }
    }
}

impl LuaBoolToken {
    pub fn is_true(&self) -> bool {
        self.token.kind() == LuaTokenKind::TkTrue.into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaNilToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaNilToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaTokenKind::TkNil.into()
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(LuaNilToken { token: syntax })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LuaLiteralToken {
    String(LuaStringToken),
    Number(LuaNumberToken),
    Bool(LuaBoolToken),
    Nil(LuaNilToken),
    Dots(LuaGeneralToken),
    Question(LuaGeneralToken),
}

impl LuaAstToken for LuaLiteralToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        match self {
            LuaLiteralToken::String(token) => token.syntax(),
            LuaLiteralToken::Number(token) => token.syntax(),
            LuaLiteralToken::Bool(token) => token.syntax(),
            LuaLiteralToken::Nil(token) => token.syntax(),
            LuaLiteralToken::Dots(token) => token.syntax(),
            LuaLiteralToken::Question(token) => token.syntax(),
        }
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        match kind {
            LuaTokenKind::TkInt
            | LuaTokenKind::TkFloat
            | LuaTokenKind::TkComplex
            | LuaTokenKind::TkNil
            | LuaTokenKind::TkTrue
            | LuaTokenKind::TkFalse
            | LuaTokenKind::TkDots
            | LuaTokenKind::TkString
            | LuaTokenKind::TkLongString
            | LuaTokenKind::TkDocQuestion => true,
            _ => false,
        }
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind().into() {
            LuaTokenKind::TkString | LuaTokenKind::TkLongString => {
                LuaStringToken::cast(syntax).map(LuaLiteralToken::String)
            }
            LuaTokenKind::TkFloat | LuaTokenKind::TkInt | LuaTokenKind::TkComplex => {
                LuaNumberToken::cast(syntax).map(LuaLiteralToken::Number)
            }
            LuaTokenKind::TkTrue | LuaTokenKind::TkFalse => {
                LuaBoolToken::cast(syntax).map(LuaLiteralToken::Bool)
            }
            LuaTokenKind::TkNil => LuaNilToken::cast(syntax).map(LuaLiteralToken::Nil),
            LuaTokenKind::TkDots => LuaGeneralToken::cast(syntax).map(LuaLiteralToken::Dots),
            LuaTokenKind::TkDocQuestion => {
                LuaGeneralToken::cast(syntax).map(LuaLiteralToken::Question)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaSpaceToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaSpaceToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        match kind {
            LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine => true,
            _ => false,
        }
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(LuaSpaceToken { token: syntax })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaIndexToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaIndexToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaTokenKind::TkDot.into()
            || kind == LuaTokenKind::TkColon.into()
            || kind == LuaTokenKind::TkLeftBracket.into()
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(LuaIndexToken { token: syntax })
        } else {
            None
        }
    }
}

impl LuaIndexToken {
    pub fn is_dot(&self) -> bool {
        self.token.kind() == LuaTokenKind::TkDot.into()
    }

    pub fn is_colon(&self) -> bool {
        self.token.kind() == LuaTokenKind::TkColon.into()
    }

    pub fn is_left_bracket(&self) -> bool {
        self.token.kind() == LuaTokenKind::TkLeftBracket.into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocDetailToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaDocDetailToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaTokenKind::TkDocDetail
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(LuaDocDetailToken { token: syntax })
        } else {
            None
        }
    }
}

impl LuaDocDetailToken {
    pub fn get_detail(&self) -> &str {
        self.token.text()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocVisibilityToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaDocVisibilityToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaTokenKind::TkDocVisibility || kind == LuaTokenKind::TkTagVisibility
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(LuaDocVisibilityToken { token: syntax })
        } else {
            None
        }
    }
}

impl LuaDocVisibilityToken {
    pub fn get_visibility(&self) -> VisibilityKind {
        VisibilityKind::to_visibility_kind(self.token.text())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocVersionNumberToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaDocVersionNumberToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaTokenKind::TkDocVersionNumber
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(LuaDocVersionNumberToken { token: syntax })
        } else {
            None
        }
    }
}

impl LuaDocVersionNumberToken {
    pub fn get_version_number(&self) -> Option<LuaVersionNumber> {
        let text = self.token.text();
        LuaVersionNumber::from_str(text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocTypeBinaryToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaDocTypeBinaryToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaTokenKind::TkDocOr
            || kind == LuaTokenKind::TkDocAnd
            || kind == LuaTokenKind::TkDocExtends
            || kind == LuaTokenKind::TkDocIn
            || kind == LuaTokenKind::TkDocContinueOr
            || kind == LuaTokenKind::TkPlus
            || kind == LuaTokenKind::TkMinus
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(LuaDocTypeBinaryToken { token: syntax })
        } else {
            None
        }
    }
}

impl LuaDocTypeBinaryToken {
    pub fn get_op(&self) -> LuaTypeBinaryOperator {
        LuaOpKind::to_type_binary_operator(self.token.kind().into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaDocTypeUnaryToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaDocTypeUnaryToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaTokenKind::TkDocKeyOf
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(LuaDocTypeUnaryToken { token: syntax })
        } else {
            None
        }
    }
}

impl LuaDocTypeUnaryToken {
    pub fn get_op(&self) -> LuaTypeUnaryOperator {
        LuaOpKind::to_type_unary_operator(self.token.kind().into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaPathToken {
    token: LuaSyntaxToken,
}

impl LuaAstToken for LuaPathToken {
    fn syntax(&self) -> &LuaSyntaxToken {
        &self.token
    }

    fn can_cast(kind: LuaTokenKind) -> bool
    where
        Self: Sized,
    {
        kind == LuaTokenKind::TKDocPath
    }

    fn cast(syntax: LuaSyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            Some(LuaPathToken { token: syntax })
        } else {
            None
        }
    }
}

impl LuaPathToken {
    pub fn get_path(&self) -> &str {
        let text = self.token.text();
        if text.starts_with('\"') || text.starts_with('\'') {
            &text[1..text.len() - 1]
        } else {
            text
        }
    }
}
