use rowan::TextSize;
use smol_str::SmolStr;

use super::{SalsaCallKindSummary, SalsaDeclId, SalsaMemberTargetId, SalsaSyntaxIdSummary};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub enum SalsaUseSiteRoleSummary {
    Read,
    Write,
    CallCallee,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub enum SalsaNameUseResolutionSummary {
    LocalDecl(SalsaDeclId),
    Global,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaNameUseSummary {
    pub syntax_offset: TextSize,
    pub syntax_id: SalsaSyntaxIdSummary,
    pub name: SmolStr,
    pub role: SalsaUseSiteRoleSummary,
    pub resolution: SalsaNameUseResolutionSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaMemberUseSummary {
    pub syntax_offset: TextSize,
    pub syntax_id: SalsaSyntaxIdSummary,
    pub role: SalsaUseSiteRoleSummary,
    pub target: SalsaMemberTargetId,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaCallUseSummary {
    pub syntax_offset: TextSize,
    pub syntax_id: SalsaSyntaxIdSummary,
    pub kind: SalsaCallKindSummary,
    pub is_colon_call: bool,
    pub arg_count: usize,
    pub require_path: Option<SmolStr>,
    pub callee_name: Option<SmolStr>,
    pub callee_member: Option<SalsaMemberTargetId>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaUseSiteIndexSummary {
    pub names: Vec<SalsaNameUseSummary>,
    pub members: Vec<SalsaMemberUseSummary>,
    pub calls: Vec<SalsaCallUseSummary>,
}
