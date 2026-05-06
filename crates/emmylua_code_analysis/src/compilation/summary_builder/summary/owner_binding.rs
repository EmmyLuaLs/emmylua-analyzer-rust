use rowan::TextSize;

use super::{SalsaDeclId, SalsaDocOwnerKindSummary, SalsaMemberTargetId};

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaBindingTargetSummary {
    Decl(SalsaDeclId),
    Member(SalsaMemberTargetId),
    Signature(TextSize),
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocOwnerBindingSummary {
    pub owner_kind: SalsaDocOwnerKindSummary,
    pub owner_offset: TextSize,
    pub targets: Vec<SalsaBindingTargetSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocOwnerBindingIndexSummary {
    pub bindings: Vec<SalsaDocOwnerBindingSummary>,
}
