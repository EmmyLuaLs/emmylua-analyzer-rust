use rowan::TextSize;

use super::{SalsaDeclId, SalsaMemberTargetId};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub enum SalsaSemanticGraphNodeSummary {
    DeclValue(SalsaDeclId),
    MemberValue(SalsaMemberTargetId),
    SignatureReturn(TextSize),
    ForRangeIter(TextSize),
    ModuleExport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub enum SalsaSemanticGraphEdgeKindSummary {
    ExportTarget,
    InitializerDecl,
    InitializerMember,
    SignatureReturnDecl,
    SignatureReturnMember,
    InitializerResolvedCallReturn,
    InitializerCandidateCallReturn,
    ResolvedCallReturn,
    CandidateCallReturn,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub struct SalsaSemanticGraphEdgeSummary {
    pub from: SalsaSemanticGraphNodeSummary,
    pub to: SalsaSemanticGraphNodeSummary,
    pub kind: SalsaSemanticGraphEdgeKindSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticGraphSummary {
    pub nodes: Vec<SalsaSemanticGraphNodeSummary>,
    pub edges: Vec<SalsaSemanticGraphEdgeSummary>,
}
