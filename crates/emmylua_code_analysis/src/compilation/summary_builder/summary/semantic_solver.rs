use rowan::TextSize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, salsa::Update)]
pub enum SalsaSemanticResolveStateSummary {
    Unknown,
    Partial,
    Resolved,
    RecursiveDependency,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticValueShellSummary {
    pub state: SalsaSemanticResolveStateSummary,
    pub candidate_type_offsets: Vec<crate::SalsaDocTypeNodeKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticSolverComponentTaskSummary {
    pub component_id: usize,
    pub predecessor_component_ids: Vec<usize>,
    pub successor_component_ids: Vec<usize>,
    pub is_cycle: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticSolverWorklistSummary {
    pub tasks: Vec<SalsaSemanticSolverComponentTaskSummary>,
    pub ready_component_ids: Vec<usize>,
    pub topo_order: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, salsa::Update)]
pub enum SalsaSemanticSolverTaskStateSummary {
    Blocked,
    Ready,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticSolverExecutionTaskSummary {
    pub component_id: usize,
    pub state: SalsaSemanticSolverTaskStateSummary,
    pub pending_predecessor_component_ids: Vec<usize>,
    pub successor_component_ids: Vec<usize>,
    pub is_cycle: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticSolverExecutionSummary {
    pub tasks: Vec<SalsaSemanticSolverExecutionTaskSummary>,
    pub ready_component_ids: Vec<usize>,
    pub completed_component_ids: Vec<usize>,
    pub component_results: Vec<SalsaSemanticSolverComponentResultSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticSolverStepSummary {
    pub completed_component_id: usize,
    pub component_result: Option<SalsaSemanticSolverComponentResultSummary>,
    pub next_execution: SalsaSemanticSolverExecutionSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticSignatureReturnComponentSummary {
    pub component_id: usize,
    pub signature_offsets: Vec<TextSize>,
    pub propagated_value_shell: SalsaSemanticValueShellSummary,
    pub local_value_shell: SalsaSemanticValueShellSummary,
    pub fixedpoint_value_shell: SalsaSemanticValueShellSummary,
    pub value_shell: SalsaSemanticValueShellSummary,
    pub is_cycle: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticSignatureReturnSummary {
    pub component_id: usize,
    pub signature_offset: TextSize,
    pub state: crate::SalsaSignatureReturnResolveStateSummary,
    pub doc_returns: Vec<crate::SalsaSignatureReturnExplainSummary>,
    pub values: Vec<crate::SalsaSignatureReturnValueSummary>,
    pub propagated_value_shell: SalsaSemanticValueShellSummary,
    pub local_value_shell: SalsaSemanticValueShellSummary,
    pub fixedpoint_value_shell: SalsaSemanticValueShellSummary,
    pub value_shell: SalsaSemanticValueShellSummary,
    pub is_cycle: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticSignatureSummary {
    pub signature: crate::SalsaSignatureSummary,
    pub doc_owners: Vec<crate::SalsaDocOwnerResolveSummary>,
    pub tag_properties: Vec<crate::SalsaDocTagPropertySummary>,
    pub properties: Vec<crate::SalsaPropertySummary>,
    pub explain: crate::SalsaSignatureExplainSummary,
    pub return_summary: Option<SalsaSemanticSignatureReturnSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticDeclSummary {
    pub component_id: usize,
    pub decl_type: crate::SalsaDeclTypeInfoSummary,
    pub propagated_value_shell: SalsaSemanticValueShellSummary,
    pub local_value_shell: SalsaSemanticValueShellSummary,
    pub fixedpoint_value_shell: SalsaSemanticValueShellSummary,
    pub value_shell: SalsaSemanticValueShellSummary,
    pub is_cycle: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticMemberSummary {
    pub component_id: usize,
    pub member_type: crate::SalsaMemberTypeInfoSummary,
    pub propagated_value_shell: SalsaSemanticValueShellSummary,
    pub local_value_shell: SalsaSemanticValueShellSummary,
    pub fixedpoint_value_shell: SalsaSemanticValueShellSummary,
    pub value_shell: SalsaSemanticValueShellSummary,
    pub is_cycle: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticForRangeIterComponentSummary {
    pub component_id: usize,
    pub loop_offset: TextSize,
    pub iter_expr_offsets: Vec<TextSize>,
    pub state: crate::SalsaForRangeIterResolveStateSummary,
    pub source: Option<crate::SalsaForRangeIterSourceSummary>,
    pub iter_vars: Vec<crate::SalsaForRangeIterVarSummary>,
    pub propagated_value_shell: SalsaSemanticValueShellSummary,
    pub local_value_shell: SalsaSemanticValueShellSummary,
    pub fixedpoint_value_shell: SalsaSemanticValueShellSummary,
    pub value_shell: SalsaSemanticValueShellSummary,
    pub is_cycle: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticModuleExportComponentSummary {
    pub component_id: usize,
    pub export_target: crate::SalsaExportTargetSummary,
    pub export: Option<crate::SalsaModuleExportSummary>,
    pub semantic_target: Option<crate::SalsaSemanticTargetSummary>,
    pub doc_owners: Vec<crate::SalsaDocOwnerResolveSummary>,
    pub tag_properties: Vec<crate::SalsaDocTagPropertySummary>,
    pub properties: Vec<crate::SalsaPropertySummary>,
    pub state: crate::SalsaModuleExportResolveStateSummary,
    pub propagated_value_shell: SalsaSemanticValueShellSummary,
    pub local_value_shell: SalsaSemanticValueShellSummary,
    pub fixedpoint_value_shell: SalsaSemanticValueShellSummary,
    pub value_shell: SalsaSemanticValueShellSummary,
    pub is_cycle: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticSolverComponentResultSummary {
    pub component_id: usize,
    pub decl_ids: Vec<crate::SalsaDeclId>,
    pub member_targets: Vec<crate::SalsaMemberTargetId>,
    pub signature_offsets: Vec<TextSize>,
    pub for_range_loop_offsets: Vec<TextSize>,
    pub includes_module_export: bool,
    pub consumed_predecessor_component_ids: Vec<usize>,
    pub input_value_shell: SalsaSemanticValueShellSummary,
    pub propagated_value_shell: SalsaSemanticValueShellSummary,
    pub local_value_shell: SalsaSemanticValueShellSummary,
    pub value_shell: SalsaSemanticValueShellSummary,
    pub fixedpoint_value_shell: SalsaSemanticValueShellSummary,
    pub fixedpoint_iterations: usize,
    pub is_cycle: bool,
}
