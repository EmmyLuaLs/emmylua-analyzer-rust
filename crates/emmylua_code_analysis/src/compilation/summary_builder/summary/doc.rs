use rowan::TextSize;
use smol_str::SmolStr;

use super::SalsaDocTypeNodeKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub enum SalsaDocOwnerKindSummary {
    None,
    AssignStat,
    LocalStat,
    FuncStat,
    LocalFuncStat,
    TableField,
    Closure,
    CallExprStat,
    ReturnStat,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub struct SalsaDocOwnerSummary {
    pub kind: SalsaDocOwnerKindSummary,
    pub syntax_offset: Option<TextSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocGenericParamSummary {
    pub name: SmolStr,
    pub type_offset: Option<SalsaDocTypeNodeKey>,
    pub default_type_offset: Option<SalsaDocTypeNodeKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaDocTypeDefKindSummary {
    Class,
    Enum,
    Alias,
    Attribute,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTypeDefSummary {
    pub owner: SalsaDocOwnerSummary,
    pub name: SmolStr,
    pub kind: SalsaDocTypeDefKindSummary,
    pub visibility: SalsaDocVisibilityKindSummary,
    pub syntax_offset: TextSize,
    pub generic_params: Vec<SalsaDocGenericParamSummary>,
    pub super_type_offsets: Vec<SalsaDocTypeNodeKey>,
    pub value_type_offset: Option<SalsaDocTypeNodeKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocGenericSummary {
    pub owner: SalsaDocOwnerSummary,
    pub syntax_offset: TextSize,
    pub params: Vec<SalsaDocGenericParamSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocParamSummary {
    pub owner: SalsaDocOwnerSummary,
    pub name: SmolStr,
    pub syntax_offset: TextSize,
    pub type_offset: Option<SalsaDocTypeNodeKey>,
    pub is_nullable: bool,
    pub is_vararg: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocReturnItemSummary {
    pub name: Option<SmolStr>,
    pub type_offset: SalsaDocTypeNodeKey,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocReturnSummary {
    pub owner: SalsaDocOwnerSummary,
    pub syntax_offset: TextSize,
    pub items: Vec<SalsaDocReturnItemSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocOperatorSummary {
    pub owner: SalsaDocOwnerSummary,
    pub name: SmolStr,
    pub syntax_offset: TextSize,
    pub param_type_offsets: Vec<SalsaDocTypeNodeKey>,
    pub return_type_offset: Option<SalsaDocTypeNodeKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTypeTagSummary {
    pub owner: SalsaDocOwnerSummary,
    pub syntax_offset: TextSize,
    pub type_offsets: Vec<SalsaDocTypeNodeKey>,
    pub description: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocFieldSummary {
    pub owner: SalsaDocOwnerSummary,
    pub syntax_offset: TextSize,
    pub key: Option<SalsaDocTagFieldKeySummary>,
    pub type_offset: Option<SalsaDocTypeNodeKey>,
    pub visibility: Option<SalsaDocVisibilityKindSummary>,
    pub description: Option<SmolStr>,
    pub is_nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub enum SalsaDocTagKindSummary {
    AttributeUse,
    ReturnOverload,
    Overload,
    Module,
    See,
    Diagnostic,
    Deprecated,
    Version,
    Cast,
    Source,
    Schema,
    Other,
    Namespace,
    Using,
    Meta,
    Nodiscard,
    Readonly,
    Operator,
    Generic,
    Async,
    As,
    Visibility,
    ReturnCast,
    Language,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaDocVisibilityKindSummary {
    Public,
    Protected,
    Private,
    Internal,
    Package,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocVersionConditionSummary {
    pub text: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaDocTagFieldKeySummary {
    Name(SmolStr),
    String(SmolStr),
    Integer(i64),
    Type(SalsaDocTypeNodeKey),
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaDocTagDataSummary {
    None,
    Name(Option<SmolStr>),
    Content(Option<SmolStr>),
    Items(Vec<SmolStr>),
    NameItems {
        name: Option<SmolStr>,
        items: Vec<SmolStr>,
    },
    TypeOffsets(Vec<SalsaDocTypeNodeKey>),
    NameTypeOffsets {
        name: Option<SmolStr>,
        type_offsets: Vec<SalsaDocTypeNodeKey>,
    },
    NameContent {
        name: Option<SmolStr>,
        content: Option<SmolStr>,
    },
    VersionConds(Vec<SalsaDocVersionConditionSummary>),
    Visibility(Option<SalsaDocVisibilityKindSummary>),
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTagSummary {
    pub owner: SalsaDocOwnerSummary,
    pub kind: SalsaDocTagKindSummary,
    pub syntax_offset: TextSize,
    pub data: SalsaDocTagDataSummary,
}

impl SalsaDocTagSummary {
    pub fn name(&self) -> Option<&SmolStr> {
        match &self.data {
            SalsaDocTagDataSummary::Name(name) => name.as_ref(),
            SalsaDocTagDataSummary::NameItems { name, .. } => name.as_ref(),
            SalsaDocTagDataSummary::NameTypeOffsets { name, .. } => name.as_ref(),
            SalsaDocTagDataSummary::NameContent { name, .. } => name.as_ref(),
            _ => None,
        }
    }

    pub fn content(&self) -> Option<&SmolStr> {
        match &self.data {
            SalsaDocTagDataSummary::Content(content) => content.as_ref(),
            SalsaDocTagDataSummary::NameContent { content, .. } => content.as_ref(),
            _ => None,
        }
    }

    pub fn items(&self) -> &[SmolStr] {
        match &self.data {
            SalsaDocTagDataSummary::Items(items) => items,
            SalsaDocTagDataSummary::NameItems { items, .. } => items,
            _ => &[],
        }
    }

    pub fn type_offsets(&self) -> &[SalsaDocTypeNodeKey] {
        match &self.data {
            SalsaDocTagDataSummary::TypeOffsets(type_offsets) => type_offsets,
            SalsaDocTagDataSummary::NameTypeOffsets { type_offsets, .. } => type_offsets,
            _ => &[],
        }
    }

    pub fn visibility(&self) -> Option<&SalsaDocVisibilityKindSummary> {
        match &self.data {
            SalsaDocTagDataSummary::Visibility(visibility) => visibility.as_ref(),
            _ => None,
        }
    }

    pub fn version_conds(&self) -> &[SalsaDocVersionConditionSummary] {
        match &self.data {
            SalsaDocTagDataSummary::VersionConds(version_conds) => version_conds,
            _ => &[],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocSummary {
    pub type_defs: Vec<SalsaDocTypeDefSummary>,
    pub type_tags: Vec<SalsaDocTypeTagSummary>,
    pub fields: Vec<SalsaDocFieldSummary>,
    pub generics: Vec<SalsaDocGenericSummary>,
    pub params: Vec<SalsaDocParamSummary>,
    pub returns: Vec<SalsaDocReturnSummary>,
    pub operators: Vec<SalsaDocOperatorSummary>,
    pub tags: Vec<SalsaDocTagSummary>,
}
