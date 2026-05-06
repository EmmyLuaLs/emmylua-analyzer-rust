use emmylua_parser::{LuaSyntaxId, LuaSyntaxKind};
use rowan::{TextRange, TextSize};
use smol_str::SmolStr;

use crate::{
    SalsaCallExplainSummary, SalsaProgramPointMemberTypeInfoSummary,
    SalsaProgramPointTypeInfoSummary, SalsaSignatureReturnExplainSummary,
};

use super::{SalsaDocOwnerKindSummary, SalsaDocTypeNodeKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, salsa::Update)]
pub struct SalsaSyntaxIdSummary {
    pub kind: LuaSyntaxKind,
    pub start_offset: TextSize,
    pub end_offset: TextSize,
}

impl From<LuaSyntaxId> for SalsaSyntaxIdSummary {
    fn from(value: LuaSyntaxId) -> Self {
        let kind = value.get_kind();
        let range = value.get_range();
        Self {
            kind,
            start_offset: range.start(),
            end_offset: range.end(),
        }
    }
}

impl SalsaSyntaxIdSummary {
    pub fn contains_offset(&self, offset: TextSize) -> bool {
        self.start_offset <= offset && offset <= self.end_offset
    }

    pub fn span_len(&self) -> TextSize {
        self.end_offset - self.start_offset
    }

    pub fn to_lua_syntax_id(&self) -> LuaSyntaxId {
        LuaSyntaxId::new(
            self.kind.into(),
            TextRange::new(self.start_offset, self.end_offset),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaSignatureSourceSummary {
    FuncStat,
    LocalFuncStat,
    ClosureExpr,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSignatureParamSummary {
    pub name: SmolStr,
    pub syntax_offset: TextSize,
    pub is_vararg: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSignatureSummary {
    pub syntax_offset: TextSize,
    pub owner_offset: TextSize,
    pub owner_kind: SalsaDocOwnerKindSummary,
    pub source: SalsaSignatureSourceSummary,
    pub name: Option<SmolStr>,
    pub is_method: bool,
    pub params: Vec<SalsaSignatureParamSummary>,
    pub return_expr_offsets: Vec<TextSize>,
    pub doc_generic_offsets: Vec<TextSize>,
    pub doc_param_offsets: Vec<TextSize>,
    pub doc_return_offsets: Vec<TextSize>,
    pub doc_operator_offsets: Vec<TextSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaCallKindSummary {
    Normal,
    Require,
    Error,
    Assert,
    Type,
    SetMetatable,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaCallSummary {
    pub syntax_offset: TextSize,
    pub syntax_id: SalsaSyntaxIdSummary,
    pub callee_offset: TextSize,
    pub kind: SalsaCallKindSummary,
    pub is_colon_call: bool,
    pub is_single_arg_no_parens: bool,
    pub arg_expr_offsets: Vec<TextSize>,
    pub call_generic_type_offsets: Vec<SalsaDocTypeNodeKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSignatureIndexSummary {
    pub signatures: Vec<SalsaSignatureSummary>,
    pub calls: Vec<SalsaCallSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaSignatureReturnResolveStateSummary {
    Resolved,
    Partial,
    RecursiveDependency,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaSignatureReturnExprKindSummary {
    Name,
    Member,
    Call,
    Literal,
    Closure,
    Table,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSignatureReturnValueSummary {
    pub expr_offset: TextSize,
    pub kind: SalsaSignatureReturnExprKindSummary,
    pub doc_return_type_offsets: Vec<SalsaDocTypeNodeKey>,
    pub name_type: Option<SalsaProgramPointTypeInfoSummary>,
    pub member_type: Option<SalsaProgramPointMemberTypeInfoSummary>,
    pub call: Option<SalsaCallExplainSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSignatureReturnQuerySummary {
    pub signature_offset: TextSize,
    pub state: SalsaSignatureReturnResolveStateSummary,
    pub doc_returns: Vec<SalsaSignatureReturnExplainSummary>,
    pub values: Vec<SalsaSignatureReturnValueSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSignatureReturnQueryIndex {
    pub signatures: Vec<SalsaSignatureReturnQuerySummary>,
}
