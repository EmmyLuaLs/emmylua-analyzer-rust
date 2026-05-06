use rowan::TextSize;
use smol_str::SmolStr;

use super::{
    SalsaDeclId, SalsaDocTypeNodeKey, SalsaGlobalRootSummary, SalsaMemberRootSummary,
    SalsaMemberTargetId, SalsaMemberTargetSummary, SalsaSyntaxIdSummary,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub enum SalsaPropertyOwnerSummary {
    Type(SmolStr),
    Decl {
        name: SmolStr,
        decl_id: SalsaDeclId,
        is_global: bool,
    },
    Member(SalsaMemberTargetId),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub enum SalsaPropertyKeySummary {
    Name(SmolStr),
    Integer(i64),
    Sequence(usize),
    Expr(SalsaSyntaxIdSummary),
    Type(SalsaDocTypeNodeKey),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub enum SalsaPropertySourceSummary {
    TableField,
    DocField,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaPropertyKindSummary {
    Value,
    Function,
    Table,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaPropertySummary {
    pub owner: SalsaPropertyOwnerSummary,
    pub key: SalsaPropertyKeySummary,
    pub source: SalsaPropertySourceSummary,
    pub kind: SalsaPropertyKindSummary,
    pub syntax_offset: TextSize,
    pub value_expr_offset: Option<TextSize>,
    pub value_expr_syntax_id: Option<SalsaSyntaxIdSummary>,
    pub value_result_index: usize,
    pub source_call_syntax_id: Option<SalsaSyntaxIdSummary>,
    pub expands_multi_result_tail: bool,
    pub doc_type_offset: Option<SalsaDocTypeNodeKey>,
    pub is_nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaPropertyIndexSummary {
    pub properties: Vec<SalsaPropertySummary>,
}

pub fn extend_property_owner_with_key(
    owner: &SalsaPropertyOwnerSummary,
    key: &SalsaPropertyKeySummary,
) -> Option<SalsaMemberTargetId> {
    let SalsaPropertyKeySummary::Name(member_name) = key else {
        return None;
    };

    match owner {
        SalsaPropertyOwnerSummary::Type(_) => None,
        SalsaPropertyOwnerSummary::Decl {
            name,
            decl_id,
            is_global,
        } => Some(
            SalsaMemberTargetSummary {
                root: if *is_global {
                    SalsaMemberRootSummary::Global(SalsaGlobalRootSummary::Name(name.clone()))
                } else {
                    SalsaMemberRootSummary::LocalDecl {
                        name: name.clone(),
                        decl_id: *decl_id,
                    }
                },
                owner_segments: Vec::new().into(),
                member_name: member_name.clone(),
            }
            .into(),
        ),
        SalsaPropertyOwnerSummary::Member(target) => Some(
            SalsaMemberTargetSummary {
                root: target.root.clone(),
                owner_segments: target
                    .owner_segments
                    .with_pushed(target.member_name.clone()),
                member_name: member_name.clone(),
            }
            .into(),
        ),
    }
}
