mod lua_features;
mod lua_language_level;
mod lua_operator_kind;
mod lua_syntax_kind;
mod lua_token_kind;
mod lua_type_operator_kind;
mod lua_version;
mod lua_visibility_kind;

pub use lua_features::{LuaFeatures, LuaFeaturesSet};
pub use lua_language_level::LuaLanguageLevel;
pub use lua_operator_kind::{BinaryOperator, UNARY_PRIORITY, UnaryOperator};
pub use lua_syntax_kind::LuaSyntaxKind;
pub use lua_token_kind::LuaTokenKind;
pub use lua_type_operator_kind::{
    LuaTypeBinaryOperator, LuaTypeTernaryOperator, LuaTypeUnaryOperator, UNARY_TYPE_PRIORITY,
};
pub use lua_version::{LuaVersionCondition, LuaVersionNumber};
pub use lua_visibility_kind::VisibilityKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum LuaKind {
    Syntax(LuaSyntaxKind),
    Token(LuaTokenKind),
}

impl From<LuaSyntaxKind> for LuaKind {
    fn from(kind: LuaSyntaxKind) -> Self {
        LuaKind::Syntax(kind)
    }
}

impl From<LuaTokenKind> for LuaKind {
    fn from(kind: LuaTokenKind) -> Self {
        LuaKind::Token(kind)
    }
}

impl From<LuaKind> for LuaSyntaxKind {
    fn from(val: LuaKind) -> Self {
        match val {
            LuaKind::Syntax(kind) => kind,
            _ => LuaSyntaxKind::None,
        }
    }
}

impl From<LuaKind> for LuaTokenKind {
    fn from(val: LuaKind) -> Self {
        match val {
            LuaKind::Token(kind) => kind,
            _ => LuaTokenKind::None,
        }
    }
}

impl LuaKind {
    pub fn is_syntax(self) -> bool {
        matches!(self, LuaKind::Syntax(_))
    }

    pub fn is_token(self) -> bool {
        matches!(self, LuaKind::Token(_))
    }

    pub fn to_syntax(self) -> LuaSyntaxKind {
        match self {
            LuaKind::Syntax(kind) => kind,
            LuaKind::Token(_) => LuaSyntaxKind::None,
        }
    }

    pub fn to_token(self) -> LuaTokenKind {
        match self {
            LuaKind::Token(kind) => kind,
            LuaKind::Syntax(_) => LuaTokenKind::None,
        }
    }

    pub fn get_raw(self) -> u16 {
        match self {
            LuaKind::Syntax(kind) => kind as u16 | 0x8000,
            LuaKind::Token(kind) => kind as u16,
        }
    }

    pub fn from_raw(raw: u16) -> LuaKind {
        if raw & 0x8000 != 0 {
            LuaKind::Syntax(unsafe { std::mem::transmute::<u16, LuaSyntaxKind>(raw & 0x7FFF) })
        } else {
            LuaKind::Token(unsafe { std::mem::transmute::<u16, LuaTokenKind>(raw) })
        }
    }
}

#[derive(Debug)]
pub struct PriorityTable {
    pub left: i32,
    pub right: i32,
}

#[derive(Debug, PartialEq)]
pub enum LuaOpKind {
    None,
    Unary(UnaryOperator),
    Binary(BinaryOperator),
    TypeUnary(LuaTypeUnaryOperator),
    TypeBinary(LuaTypeBinaryOperator),
    TypeTernary(LuaTypeTernaryOperator),
}

impl From<UnaryOperator> for LuaOpKind {
    fn from(op: UnaryOperator) -> Self {
        LuaOpKind::Unary(op)
    }
}

impl From<BinaryOperator> for LuaOpKind {
    fn from(op: BinaryOperator) -> Self {
        LuaOpKind::Binary(op)
    }
}

impl From<LuaTypeUnaryOperator> for LuaOpKind {
    fn from(op: LuaTypeUnaryOperator) -> Self {
        LuaOpKind::TypeUnary(op)
    }
}

impl From<LuaTypeBinaryOperator> for LuaOpKind {
    fn from(op: LuaTypeBinaryOperator) -> Self {
        LuaOpKind::TypeBinary(op)
    }
}

impl From<LuaTypeTernaryOperator> for LuaOpKind {
    fn from(op: LuaTypeTernaryOperator) -> Self {
        LuaOpKind::TypeTernary(op)
    }
}

impl LuaOpKind {
    pub fn to_unary_operator(kind: LuaTokenKind) -> UnaryOperator {
        match kind {
            LuaTokenKind::TkNot => UnaryOperator::OpNot,
            LuaTokenKind::TkLen => UnaryOperator::OpLen,
            LuaTokenKind::TkMinus => UnaryOperator::OpUnm,
            LuaTokenKind::TkBitXor => UnaryOperator::OpBNot,
            _ => UnaryOperator::OpNop,
        }
    }

    pub fn to_binary_operator(kind: LuaTokenKind) -> BinaryOperator {
        match kind {
            LuaTokenKind::TkPlus => BinaryOperator::OpAdd,
            LuaTokenKind::TkMinus => BinaryOperator::OpSub,
            LuaTokenKind::TkMul => BinaryOperator::OpMul,
            LuaTokenKind::TkMod => BinaryOperator::OpMod,
            LuaTokenKind::TkPow => BinaryOperator::OpPow,
            LuaTokenKind::TkDiv => BinaryOperator::OpDiv,
            LuaTokenKind::TkIDiv => BinaryOperator::OpIDiv,
            LuaTokenKind::TkBitAnd => BinaryOperator::OpBAnd,
            LuaTokenKind::TkBitOr => BinaryOperator::OpBOr,
            LuaTokenKind::TkBitXor => BinaryOperator::OpBXor,
            LuaTokenKind::TkShl => BinaryOperator::OpShl,
            LuaTokenKind::TkShr => BinaryOperator::OpShr,
            LuaTokenKind::TkConcat => BinaryOperator::OpConcat,
            LuaTokenKind::TkLt => BinaryOperator::OpLt,
            LuaTokenKind::TkLe => BinaryOperator::OpLe,
            LuaTokenKind::TkGt => BinaryOperator::OpGt,
            LuaTokenKind::TkGe => BinaryOperator::OpGe,
            LuaTokenKind::TkEq => BinaryOperator::OpEq,
            LuaTokenKind::TkNe => BinaryOperator::OpNe,
            LuaTokenKind::TkAnd => BinaryOperator::OpAnd,
            LuaTokenKind::TkOr => BinaryOperator::OpOr,
            LuaTokenKind::TkShrArithmetic => BinaryOperator::OpShrAthrimetic,
            LuaTokenKind::TkNilCoalescing => BinaryOperator::OpNilCoalescing,
            _ => BinaryOperator::OpNop,
        }
    }

    pub fn to_type_unary_operator(kind: LuaTokenKind) -> LuaTypeUnaryOperator {
        match kind {
            LuaTokenKind::TkDocKeyOf => LuaTypeUnaryOperator::Keyof,
            LuaTokenKind::TkMinus => LuaTypeUnaryOperator::Neg,
            _ => LuaTypeUnaryOperator::None,
        }
    }

    pub fn to_parse_binary_operator(kind: LuaTokenKind) -> LuaTypeBinaryOperator {
        match kind {
            LuaTokenKind::TkDocOr => LuaTypeBinaryOperator::Union,
            LuaTokenKind::TkDocAnd => LuaTypeBinaryOperator::Intersection,
            LuaTokenKind::TkIn => LuaTypeBinaryOperator::In,
            LuaTokenKind::TkDocExtends => LuaTypeBinaryOperator::Extends,
            LuaTokenKind::TkPlus => LuaTypeBinaryOperator::Add,
            LuaTokenKind::TkMinus => LuaTypeBinaryOperator::Sub,
            _ => LuaTypeBinaryOperator::None,
        }
    }

    pub fn to_type_binary_operator(kind: LuaTokenKind) -> LuaTypeBinaryOperator {
        match kind {
            LuaTokenKind::TkDocOr | LuaTokenKind::TkDocContinueOr => LuaTypeBinaryOperator::Union,
            LuaTokenKind::TkDocAnd => LuaTypeBinaryOperator::Intersection,
            LuaTokenKind::TkIn => LuaTypeBinaryOperator::In,
            LuaTokenKind::TkDocExtends => LuaTypeBinaryOperator::Extends,
            LuaTokenKind::TkPlus => LuaTypeBinaryOperator::Add,
            LuaTokenKind::TkMinus => LuaTypeBinaryOperator::Sub,
            _ => LuaTypeBinaryOperator::None,
        }
    }
}
