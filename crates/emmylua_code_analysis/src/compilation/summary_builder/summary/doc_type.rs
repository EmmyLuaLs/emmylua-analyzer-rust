use emmylua_parser::{LuaAstNode, LuaDocType, LuaSyntaxId};

use super::{SalsaDocGenericParamSummary, SalsaSyntaxIdSummary};
use rowan::TextSize;
use smol_str::SmolStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub struct SalsaDocTypeNodeKey(pub SalsaSyntaxIdSummary);

impl From<SalsaSyntaxIdSummary> for SalsaDocTypeNodeKey {
    fn from(value: SalsaSyntaxIdSummary) -> Self {
        Self(value)
    }
}

impl From<LuaSyntaxId> for SalsaDocTypeNodeKey {
    fn from(value: LuaSyntaxId) -> Self {
        Self(value.into())
    }
}

impl From<LuaDocType> for SalsaDocTypeNodeKey {
    fn from(value: LuaDocType) -> Self {
        Self(value.get_syntax_id().into())
    }
}

impl SalsaDocTypeNodeKey {
    pub fn syntax_offset(self) -> TextSize {
        self.0.start_offset
    }

    pub fn contains_offset(self, offset: TextSize) -> bool {
        self.0.contains_offset(offset)
    }
}

impl From<SalsaDocTypeNodeKey> for TextSize {
    fn from(value: SalsaDocTypeNodeKey) -> Self {
        value.syntax_offset()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaDocTypeUnaryOperatorSummary {
    None,
    Keyof,
    Neg,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaDocTypeBinaryOperatorSummary {
    None,
    Union,
    Intersection,
    In,
    Extends,
    Add,
    Sub,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTypedParamSummary {
    pub name: Option<SmolStr>,
    pub type_offset: Option<SalsaDocTypeNodeKey>,
    pub is_dots: bool,
    pub is_nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocReturnTypeSummary {
    pub name: Option<SmolStr>,
    pub type_offset: Option<SalsaDocTypeNodeKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaDocObjectFieldKeySummary {
    Name(SmolStr),
    String(SmolStr),
    Integer(i64),
    Type(SalsaDocTypeNodeKey),
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocObjectFieldSummary {
    pub syntax_offset: TextSize,
    pub key: SalsaDocObjectFieldKeySummary,
    pub value_type_offset: Option<SalsaDocTypeNodeKey>,
    pub is_nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaDocTypeKindSummary {
    Name {
        name: Option<SmolStr>,
    },
    Infer {
        generic_name: Option<SmolStr>,
    },
    Array {
        item_type_offset: Option<SalsaDocTypeNodeKey>,
    },
    Function {
        is_async: bool,
        is_sync: bool,
        generic_params: Vec<SalsaDocGenericParamSummary>,
        params: Vec<SalsaDocTypedParamSummary>,
        returns: Vec<SalsaDocReturnTypeSummary>,
    },
    Object {
        fields: Vec<SalsaDocObjectFieldSummary>,
    },
    Binary {
        op: SalsaDocTypeBinaryOperatorSummary,
        left_type_offset: Option<SalsaDocTypeNodeKey>,
        right_type_offset: Option<SalsaDocTypeNodeKey>,
    },
    Unary {
        op: SalsaDocTypeUnaryOperatorSummary,
        inner_type_offset: Option<SalsaDocTypeNodeKey>,
    },
    Conditional {
        condition_type_offset: Option<SalsaDocTypeNodeKey>,
        true_type_offset: Option<SalsaDocTypeNodeKey>,
        false_type_offset: Option<SalsaDocTypeNodeKey>,
        has_new: Option<bool>,
    },
    Tuple {
        item_type_offsets: Vec<SalsaDocTypeNodeKey>,
    },
    Literal {
        text: SmolStr,
    },
    Variadic {
        item_type_offset: Option<SalsaDocTypeNodeKey>,
    },
    Nullable {
        inner_type_offset: Option<SalsaDocTypeNodeKey>,
    },
    Generic {
        base_type_offset: Option<SalsaDocTypeNodeKey>,
        arg_type_offsets: Vec<SalsaDocTypeNodeKey>,
    },
    StringTemplate {
        prefix: Option<SmolStr>,
        interpolated: Option<SmolStr>,
        suffix: Option<SmolStr>,
    },
    MultiLineUnion {
        item_type_offsets: Vec<SalsaDocTypeNodeKey>,
    },
    Attribute {
        params: Vec<SalsaDocTypedParamSummary>,
    },
    Mapped {
        key_type_offsets: Vec<SalsaDocTypeNodeKey>,
        value_type_offset: Option<SalsaDocTypeNodeKey>,
        is_readonly: bool,
        is_optional: bool,
    },
    IndexAccess {
        base_type_offset: Option<SalsaDocTypeNodeKey>,
        index_type_offset: Option<SalsaDocTypeNodeKey>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTypeNodeSummary {
    pub syntax_offset: TextSize,
    pub syntax_id: SalsaSyntaxIdSummary,
    pub kind: SalsaDocTypeKindSummary,
}

impl SalsaDocTypeNodeSummary {
    pub fn node_key(&self) -> SalsaDocTypeNodeKey {
        self.syntax_id.into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTypeIndexSummary {
    pub types: Vec<SalsaDocTypeNodeSummary>,
}
