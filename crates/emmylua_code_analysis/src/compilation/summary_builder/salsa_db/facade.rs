use crate::{
    FileId, SalsaDeclSummary, SalsaDocTypeNodeKey, SalsaFlowBreakSummary, SalsaFlowNodeRefSummary,
    SalsaGlobalFunctionSummary, SalsaGlobalVariableSummary, SalsaModuleExportQuerySummary,
    SalsaModuleResolveIndex, SalsaSemanticDeclSummary, SalsaSemanticForRangeIterComponentSummary,
    SalsaSemanticGraphEdgeSummary, SalsaSemanticGraphNodeSummary, SalsaSemanticGraphQueryIndex,
    SalsaSemanticGraphSccComponentSummary, SalsaSemanticGraphSccIndex, SalsaSemanticGraphSummary,
    SalsaSemanticMemberSummary, SalsaSemanticModuleExportComponentSummary,
    SalsaSemanticSignatureReturnComponentSummary, SalsaSemanticSignatureReturnSummary,
    SalsaSemanticSolverComponentResultSummary, SalsaSemanticSolverComponentTaskSummary,
    SalsaSemanticSolverExecutionTaskSummary, SalsaSemanticSolverStepSummary,
    SalsaSemanticTargetQueryIndex, SalsaUseSiteRoleSummary,
};
use rowan::TextSize;
use std::sync::Arc;

use super::{SalsaSummaryDatabase, tracked};
use crate::{
    SalsaCallExplainSummary, SalsaCallUseSummary, SalsaDeclId, SalsaDeclTreeSummary,
    SalsaDeclTypeInfoSummary, SalsaDeclTypeQueryIndex, SalsaDocOwnerBindingIndexSummary,
    SalsaDocOwnerResolveIndex, SalsaDocOwnerResolveSummary, SalsaDocOwnerSummary, SalsaDocSummary,
    SalsaDocTagKindSummary, SalsaDocTagPropertySummary, SalsaDocTagSummary,
    SalsaDocTypeIndexSummary, SalsaDocTypeLoweredIndex, SalsaDocTypeLoweredNode,
    SalsaDocTypeResolvedIndex, SalsaDocTypeResolvedSummary, SalsaExportTargetSummary,
    SalsaFileSummary, SalsaFlowBlockSummary, SalsaFlowBranchGraphSummary, SalsaFlowBranchSummary,
    SalsaFlowConditionGraphSummary, SalsaFlowConditionSummary, SalsaFlowEdgeSummary,
    SalsaFlowGotoSummary, SalsaFlowLabelSummary, SalsaFlowLoopGraphSummary, SalsaFlowLoopSummary,
    SalsaFlowQuerySummary, SalsaFlowReturnSummary, SalsaFlowSummary, SalsaFlowTerminalGraphSummary,
    SalsaForRangeIterQueryIndex, SalsaForRangeIterQuerySummary, SalsaGlobalSummary,
    SalsaGlobalTypeInfoSummary, SalsaGlobalTypeQueryIndex, SalsaLexicalUseIndex,
    SalsaLexicalUseSummary, SalsaMemberIndexSummary, SalsaMemberSummary, SalsaMemberTargetId,
    SalsaMemberTypeInfoSummary, SalsaMemberTypeQueryIndex, SalsaMemberUseSummary,
    SalsaModuleExportSemanticSummary, SalsaModuleExportSummary, SalsaModuleSummary,
    SalsaNameTypeInfoSummary, SalsaNameUseSummary, SalsaProgramPointMemberTypeInfoSummary,
    SalsaProgramPointTypeInfoSummary, SalsaPropertyIndexSummary, SalsaPropertyKeySummary,
    SalsaPropertySourceSummary, SalsaPropertySummary, SalsaResolvedDocDiagnosticActionSummary,
    SalsaSemanticSignatureSummary, SalsaSemanticSolverExecutionSummary,
    SalsaSemanticSolverWorklistSummary, SalsaSemanticTargetInfoSummary,
    SalsaSemanticValueShellSummary, SalsaSignatureExplainIndex, SalsaSignatureExplainSummary,
    SalsaSignatureIndexSummary, SalsaSignatureReturnQueryIndex, SalsaSignatureReturnQuerySummary,
    SalsaSingleFileSemanticSummary, SalsaSyntaxIdSummary, SalsaTableShapeIndexSummary,
    SalsaUseSiteIndexSummary,
};
use smol_str::SmolStr;

#[derive(Clone, Copy)]
pub struct SalsaSummaryFileQueries<'db> {
    db: &'db SalsaSummaryDatabase,
}

impl<'db> SalsaSummaryFileQueries<'db> {
    pub(crate) fn new(db: &'db SalsaSummaryDatabase) -> Self {
        Self { db }
    }

    pub fn summary(&self, file_id: FileId) -> Option<Arc<SalsaFileSummary>> {
        tracked::file_summary(self.db, file_id)
    }

    pub fn decl_tree(&self, file_id: FileId) -> Option<Arc<SalsaDeclTreeSummary>> {
        tracked::file_decl_tree_summary(self.db, file_id)
    }

    pub fn decl_by_syntax_id(
        &self,
        file_id: FileId,
        syntax_id: SalsaSyntaxIdSummary,
    ) -> Option<SalsaDeclSummary> {
        tracked::file_decl_by_syntax_id(self.db, file_id, syntax_id)
    }

    pub fn globals(&self, file_id: FileId) -> Option<Arc<SalsaGlobalSummary>> {
        tracked::file_global_summary(self.db, file_id)
    }

    pub fn members(&self, file_id: FileId) -> Option<Arc<SalsaMemberIndexSummary>> {
        tracked::file_member_summary(self.db, file_id)
    }

    pub fn member_by_syntax_id(
        &self,
        file_id: FileId,
        syntax_id: SalsaSyntaxIdSummary,
    ) -> Option<SalsaMemberSummary> {
        tracked::file_member_by_syntax_id(self.db, file_id, syntax_id)
    }

    pub fn properties(&self, file_id: FileId) -> Option<Arc<SalsaPropertyIndexSummary>> {
        tracked::file_property_summary(self.db, file_id)
    }

    pub fn table_shapes(&self, file_id: FileId) -> Option<Arc<SalsaTableShapeIndexSummary>> {
        tracked::file_table_shape_summary(self.db, file_id)
    }

    pub fn property_at(
        &self,
        file_id: FileId,
        syntax_offset: TextSize,
    ) -> Option<SalsaPropertySummary> {
        tracked::file_property_at(self.db, file_id, syntax_offset)
    }

    pub fn property_by_value_expr_syntax_id(
        &self,
        file_id: FileId,
        syntax_id: SalsaSyntaxIdSummary,
    ) -> Option<SalsaPropertySummary> {
        tracked::file_property_by_value_expr_syntax_id(self.db, file_id, syntax_id)
    }

    pub fn properties_for_decl(
        &self,
        file_id: FileId,
        decl_id: SalsaDeclId,
    ) -> Option<Vec<SalsaPropertySummary>> {
        tracked::file_properties_for_decl(self.db, file_id, decl_id)
    }

    pub fn properties_for_member(
        &self,
        file_id: FileId,
        member_target: impl Into<SalsaMemberTargetId>,
    ) -> Option<Vec<SalsaPropertySummary>> {
        tracked::file_properties_for_member(self.db, file_id, member_target.into())
    }

    pub fn properties_for_decl_and_key(
        &self,
        file_id: FileId,
        decl_id: SalsaDeclId,
        key: SalsaPropertyKeySummary,
    ) -> Option<Vec<SalsaPropertySummary>> {
        tracked::file_properties_for_decl_and_key(self.db, file_id, decl_id, key)
    }

    pub fn properties_for_member_and_key(
        &self,
        file_id: FileId,
        member_target: impl Into<SalsaMemberTargetId>,
        key: SalsaPropertyKeySummary,
    ) -> Option<Vec<SalsaPropertySummary>> {
        tracked::file_properties_for_member_and_key(self.db, file_id, member_target.into(), key)
    }

    pub fn properties_for_key(
        &self,
        file_id: FileId,
        key: SalsaPropertyKeySummary,
    ) -> Option<Vec<SalsaPropertySummary>> {
        tracked::file_properties_for_key(self.db, file_id, key)
    }

    pub fn properties_for_type(
        &self,
        file_id: FileId,
        type_name: SmolStr,
    ) -> Option<Vec<SalsaPropertySummary>> {
        tracked::file_properties_for_type(self.db, file_id, type_name)
    }

    pub fn properties_for_type_and_key(
        &self,
        file_id: FileId,
        type_name: SmolStr,
        key: SalsaPropertyKeySummary,
    ) -> Option<Vec<SalsaPropertySummary>> {
        tracked::file_properties_for_type_and_key(self.db, file_id, type_name, key)
    }

    pub fn properties_for_source(
        &self,
        file_id: FileId,
        source: SalsaPropertySourceSummary,
    ) -> Option<Vec<SalsaPropertySummary>> {
        tracked::file_properties_for_source(self.db, file_id, source)
    }
}

#[derive(Clone, Copy)]
pub struct SalsaSummaryDocQueries<'db> {
    db: &'db SalsaSummaryDatabase,
}

impl<'db> SalsaSummaryDocQueries<'db> {
    pub(crate) fn new(db: &'db SalsaSummaryDatabase) -> Self {
        Self { db }
    }

    pub fn summary(&self, file_id: FileId) -> Option<Arc<SalsaDocSummary>> {
        tracked::file_doc_summary(self.db, file_id)
    }

    pub fn tags(&self, file_id: FileId) -> Option<Vec<SalsaDocTagSummary>> {
        tracked::file_doc_tags(self.db, file_id)
    }

    pub fn tag_at(&self, file_id: FileId, syntax_offset: TextSize) -> Option<SalsaDocTagSummary> {
        tracked::file_doc_tag_at(self.db, file_id, syntax_offset)
    }

    pub fn tags_for_kind(
        &self,
        file_id: FileId,
        kind: SalsaDocTagKindSummary,
    ) -> Option<Vec<SalsaDocTagSummary>> {
        tracked::file_doc_tags_for_kind(self.db, file_id, kind)
    }

    pub fn tags_for_owner(
        &self,
        file_id: FileId,
        owner: SalsaDocOwnerSummary,
    ) -> Option<Vec<SalsaDocTagSummary>> {
        tracked::file_doc_tags_for_owner(self.db, file_id, owner)
    }

    pub fn tag_properties(&self, file_id: FileId) -> Option<Vec<SalsaDocTagPropertySummary>> {
        tracked::file_doc_tag_properties(self.db, file_id)
    }

    pub fn tag_property(
        &self,
        file_id: FileId,
        owner: SalsaDocOwnerSummary,
    ) -> Option<SalsaDocTagPropertySummary> {
        tracked::file_doc_tag_property(self.db, file_id, owner)
    }

    pub fn resolved_tag_diagnostics(
        &self,
        file_id: FileId,
        owner: SalsaDocOwnerSummary,
    ) -> Option<Vec<SalsaResolvedDocDiagnosticActionSummary>> {
        tracked::file_resolved_doc_tag_diagnostics(self.db, file_id, owner)
    }

    pub fn types(&self, file_id: FileId) -> Option<Arc<SalsaDocTypeIndexSummary>> {
        tracked::file_doc_type_summary(self.db, file_id)
    }

    pub fn lowered_types(&self, file_id: FileId) -> Option<Arc<SalsaDocTypeLoweredIndex>> {
        tracked::file_doc_type_lowered_index(self.db, file_id)
    }

    pub fn lowered_type_at(
        &self,
        file_id: FileId,
        syntax_offset: TextSize,
    ) -> Option<SalsaDocTypeLoweredNode> {
        tracked::file_doc_type_lowered_at(self.db, file_id, syntax_offset)
    }

    pub fn lowered_type_by_key(
        &self,
        file_id: FileId,
        type_key: SalsaDocTypeNodeKey,
    ) -> Option<SalsaDocTypeLoweredNode> {
        tracked::file_doc_type_lowered_by_key(self.db, file_id, type_key)
    }

    pub fn resolved_types(&self, file_id: FileId) -> Option<Arc<SalsaDocTypeResolvedIndex>> {
        tracked::file_doc_type_resolved_index(self.db, file_id)
    }

    pub fn resolved_type_at(
        &self,
        file_id: FileId,
        syntax_offset: TextSize,
    ) -> Option<SalsaDocTypeResolvedSummary> {
        tracked::file_doc_type_resolved_at(self.db, file_id, syntax_offset)
    }

    pub fn resolved_type_by_key(
        &self,
        file_id: FileId,
        type_key: SalsaDocTypeNodeKey,
    ) -> Option<SalsaDocTypeResolvedSummary> {
        tracked::file_doc_type_resolved_by_key(self.db, file_id, type_key)
    }

    pub fn signatures(&self, file_id: FileId) -> Option<Arc<SalsaSignatureIndexSummary>> {
        tracked::file_signature_summary(self.db, file_id)
    }

    pub fn signature_explain_index(
        &self,
        file_id: FileId,
    ) -> Option<Arc<SalsaSignatureExplainIndex>> {
        tracked::file_signature_explain_index(self.db, file_id)
    }

    pub fn signature_explain(
        &self,
        file_id: FileId,
        signature_offset: TextSize,
    ) -> Option<SalsaSignatureExplainSummary> {
        tracked::file_signature_explain(self.db, file_id, signature_offset)
    }

    pub fn call_explain(
        &self,
        file_id: FileId,
        call_offset: TextSize,
    ) -> Option<SalsaCallExplainSummary> {
        tracked::file_call_explain(self.db, file_id, call_offset)
    }

    pub fn signature_return_index(
        &self,
        file_id: FileId,
    ) -> Option<Arc<SalsaSignatureReturnQueryIndex>> {
        tracked::file_signature_return_query_index(self.db, file_id)
    }

    pub fn signature_return(
        &self,
        file_id: FileId,
        signature_offset: TextSize,
    ) -> Option<SalsaSignatureReturnQuerySummary> {
        tracked::file_signature_return_query(self.db, file_id, signature_offset)
    }

    pub fn owner_bindings(&self, file_id: FileId) -> Option<Arc<SalsaDocOwnerBindingIndexSummary>> {
        tracked::file_doc_owner_binding_summary(self.db, file_id)
    }

    pub fn owner_resolve_index(&self, file_id: FileId) -> Option<Arc<SalsaDocOwnerResolveIndex>> {
        tracked::file_doc_owner_resolve_index(self.db, file_id)
    }

    pub fn owner_resolve(
        &self,
        file_id: FileId,
        owner_offset: TextSize,
    ) -> Option<SalsaDocOwnerResolveSummary> {
        tracked::file_doc_owner_resolve(self.db, file_id, owner_offset)
    }

    pub fn owner_resolves_for_decl(
        &self,
        file_id: FileId,
        decl_id: SalsaDeclId,
    ) -> Option<Vec<SalsaDocOwnerResolveSummary>> {
        tracked::file_doc_owner_resolves_for_decl(self.db, file_id, decl_id)
    }

    pub fn owner_resolves_for_member(
        &self,
        file_id: FileId,
        member_target: impl Into<SalsaMemberTargetId>,
    ) -> Option<Vec<SalsaDocOwnerResolveSummary>> {
        tracked::file_doc_owner_resolves_for_member(self.db, file_id, member_target.into())
    }

    pub fn owner_resolves_for_signature(
        &self,
        file_id: FileId,
        signature_offset: TextSize,
    ) -> Option<Vec<SalsaDocOwnerResolveSummary>> {
        tracked::file_doc_owner_resolves_for_signature(self.db, file_id, signature_offset)
    }
}

#[derive(Clone, Copy)]
pub struct SalsaSummaryLexicalQueries<'db> {
    db: &'db SalsaSummaryDatabase,
}

impl<'db> SalsaSummaryLexicalQueries<'db> {
    pub(crate) fn new(db: &'db SalsaSummaryDatabase) -> Self {
        Self { db }
    }

    pub fn use_sites(&self, file_id: FileId) -> Option<Arc<SalsaUseSiteIndexSummary>> {
        tracked::file_use_site_summary(self.db, file_id)
    }

    pub fn name_resolution(
        &self,
        file_id: FileId,
        syntax_offset: TextSize,
    ) -> Option<SalsaNameUseSummary> {
        tracked::file_lexical_name_resolution(self.db, file_id, syntax_offset)
    }

    pub fn name_resolution_by_syntax_id(
        &self,
        file_id: FileId,
        syntax_id: SalsaSyntaxIdSummary,
    ) -> Option<SalsaNameUseSummary> {
        tracked::file_lexical_name_resolution_by_syntax_id(self.db, file_id, syntax_id)
    }

    pub fn use_index(&self, file_id: FileId) -> Option<Arc<SalsaLexicalUseIndex>> {
        tracked::file_lexical_use_index(self.db, file_id)
    }

    pub fn use_at(
        &self,
        file_id: FileId,
        syntax_offset: TextSize,
    ) -> Option<SalsaLexicalUseSummary> {
        tracked::file_lexical_use(self.db, file_id, syntax_offset)
    }

    pub fn member_resolution(
        &self,
        file_id: FileId,
        syntax_offset: TextSize,
    ) -> Option<SalsaMemberUseSummary> {
        tracked::file_lexical_member_resolution(self.db, file_id, syntax_offset)
    }

    pub fn member_resolution_by_syntax_id(
        &self,
        file_id: FileId,
        syntax_id: SalsaSyntaxIdSummary,
    ) -> Option<SalsaMemberUseSummary> {
        tracked::file_lexical_member_resolution_by_syntax_id(self.db, file_id, syntax_id)
    }

    pub fn call_at(&self, file_id: FileId, syntax_offset: TextSize) -> Option<SalsaCallUseSummary> {
        tracked::file_lexical_call_use(self.db, file_id, syntax_offset)
    }

    pub fn call_at_by_syntax_id(
        &self,
        file_id: FileId,
        syntax_id: SalsaSyntaxIdSummary,
    ) -> Option<SalsaCallUseSummary> {
        tracked::file_lexical_call_use_by_syntax_id(self.db, file_id, syntax_id)
    }

    pub fn decl_references(
        &self,
        file_id: FileId,
        decl_id: SalsaDeclId,
    ) -> Option<Vec<SalsaNameUseSummary>> {
        tracked::file_lexical_decl_references(self.db, file_id, decl_id)
    }

    pub fn global_name_references(
        &self,
        file_id: FileId,
        name: SmolStr,
    ) -> Option<Vec<SalsaNameUseSummary>> {
        tracked::file_lexical_global_name_references(self.db, file_id, name)
    }

    pub fn name_references_by_role(
        &self,
        file_id: FileId,
        role: SalsaUseSiteRoleSummary,
    ) -> Option<Vec<SalsaNameUseSummary>> {
        tracked::file_lexical_name_references_by_role(self.db, file_id, role)
    }

    pub fn member_references(
        &self,
        file_id: FileId,
        member_target: impl Into<SalsaMemberTargetId>,
    ) -> Option<Vec<SalsaMemberUseSummary>> {
        tracked::file_lexical_member_references(self.db, file_id, member_target.into())
    }

    pub fn member_references_by_role(
        &self,
        file_id: FileId,
        role: SalsaUseSiteRoleSummary,
    ) -> Option<Vec<SalsaMemberUseSummary>> {
        tracked::file_lexical_member_references_by_role(self.db, file_id, role)
    }

    pub fn call_references_for_name(
        &self,
        file_id: FileId,
        callee_name: SmolStr,
    ) -> Option<Vec<SalsaCallUseSummary>> {
        tracked::file_lexical_call_references_for_name(self.db, file_id, callee_name)
    }

    pub fn call_references_for_member(
        &self,
        file_id: FileId,
        member_target: impl Into<SalsaMemberTargetId>,
    ) -> Option<Vec<SalsaCallUseSummary>> {
        tracked::file_lexical_call_references_for_member(self.db, file_id, member_target.into())
    }
}

#[derive(Clone, Copy)]
pub struct SalsaSummaryFlowQueries<'db> {
    db: &'db SalsaSummaryDatabase,
}

impl<'db> SalsaSummaryFlowQueries<'db> {
    pub(crate) fn new(db: &'db SalsaSummaryDatabase) -> Self {
        Self { db }
    }

    pub fn summary(&self, file_id: FileId) -> Option<Arc<SalsaFlowSummary>> {
        tracked::file_flow_summary(self.db, file_id)
    }

    pub fn block_at(
        &self,
        file_id: FileId,
        block_offset: TextSize,
    ) -> Option<SalsaFlowBlockSummary> {
        tracked::file_flow_block(self.db, file_id, block_offset)
    }

    pub fn branch_at(
        &self,
        file_id: FileId,
        branch_offset: TextSize,
    ) -> Option<SalsaFlowBranchSummary> {
        tracked::file_flow_branch(self.db, file_id, branch_offset)
    }

    pub fn loop_at(&self, file_id: FileId, loop_offset: TextSize) -> Option<SalsaFlowLoopSummary> {
        tracked::file_flow_loop(self.db, file_id, loop_offset)
    }

    pub fn return_at(
        &self,
        file_id: FileId,
        return_offset: TextSize,
    ) -> Option<SalsaFlowReturnSummary> {
        tracked::file_flow_return(self.db, file_id, return_offset)
    }

    pub fn break_at(
        &self,
        file_id: FileId,
        break_offset: TextSize,
    ) -> Option<SalsaFlowBreakSummary> {
        tracked::file_flow_break(self.db, file_id, break_offset)
    }

    pub fn goto_at(&self, file_id: FileId, goto_offset: TextSize) -> Option<SalsaFlowGotoSummary> {
        tracked::file_flow_goto(self.db, file_id, goto_offset)
    }

    pub fn label(&self, file_id: FileId, label_offset: TextSize) -> Option<SalsaFlowLabelSummary> {
        tracked::file_flow_label(self.db, file_id, label_offset)
    }

    pub fn condition(
        &self,
        file_id: FileId,
        condition_node_offset: u32,
    ) -> Option<SalsaFlowConditionSummary> {
        tracked::file_flow_condition(self.db, file_id, condition_node_offset)
    }

    pub fn query(&self, file_id: FileId) -> Option<Arc<SalsaFlowQuerySummary>> {
        tracked::file_flow_query_summary(self.db, file_id)
    }

    pub fn for_range_iter_index(
        &self,
        file_id: FileId,
    ) -> Option<Arc<SalsaForRangeIterQueryIndex>> {
        tracked::file_for_range_iter_query_index(self.db, file_id)
    }

    pub fn for_range_iter(
        &self,
        file_id: FileId,
        loop_offset: TextSize,
    ) -> Option<SalsaForRangeIterQuerySummary> {
        tracked::file_for_range_iter_query(self.db, file_id, loop_offset)
    }

    pub fn successors(
        &self,
        file_id: FileId,
        node: SalsaFlowNodeRefSummary,
    ) -> Option<Vec<SalsaFlowNodeRefSummary>> {
        tracked::file_flow_successors(self.db, file_id, node)
    }

    pub fn outgoing_edges(
        &self,
        file_id: FileId,
        node: SalsaFlowNodeRefSummary,
    ) -> Option<Vec<SalsaFlowEdgeSummary>> {
        tracked::file_flow_outgoing_edges(self.db, file_id, node)
    }

    pub fn predecessors(
        &self,
        file_id: FileId,
        node: SalsaFlowNodeRefSummary,
    ) -> Option<Vec<SalsaFlowNodeRefSummary>> {
        tracked::file_flow_predecessors(self.db, file_id, node)
    }

    pub fn incoming_edges(
        &self,
        file_id: FileId,
        node: SalsaFlowNodeRefSummary,
    ) -> Option<Vec<SalsaFlowEdgeSummary>> {
        tracked::file_flow_incoming_edges(self.db, file_id, node)
    }

    pub fn reachable_nodes(
        &self,
        file_id: FileId,
        start: SalsaFlowNodeRefSummary,
    ) -> Option<Vec<SalsaFlowNodeRefSummary>> {
        tracked::file_flow_reachable_nodes(self.db, file_id, start)
    }

    pub fn can_reach(
        &self,
        file_id: FileId,
        from: SalsaFlowNodeRefSummary,
        to: SalsaFlowNodeRefSummary,
    ) -> Option<bool> {
        tracked::file_flow_can_reach(self.db, file_id, from, to)
    }

    pub fn condition_graph(
        &self,
        file_id: FileId,
        condition_node_offset: u32,
    ) -> Option<SalsaFlowConditionGraphSummary> {
        tracked::file_flow_condition_graph(self.db, file_id, condition_node_offset)
    }

    pub fn branch_graph(
        &self,
        file_id: FileId,
        branch_offset: TextSize,
    ) -> Option<SalsaFlowBranchGraphSummary> {
        tracked::file_flow_branch_graph(self.db, file_id, branch_offset)
    }

    pub fn loop_graph(
        &self,
        file_id: FileId,
        loop_offset: TextSize,
    ) -> Option<SalsaFlowLoopGraphSummary> {
        tracked::file_flow_loop_graph(self.db, file_id, loop_offset)
    }

    pub fn return_graph(
        &self,
        file_id: FileId,
        return_offset: TextSize,
    ) -> Option<SalsaFlowTerminalGraphSummary> {
        tracked::file_flow_return_graph(self.db, file_id, return_offset)
    }

    pub fn break_graph(
        &self,
        file_id: FileId,
        break_offset: TextSize,
    ) -> Option<SalsaFlowTerminalGraphSummary> {
        tracked::file_flow_break_graph(self.db, file_id, break_offset)
    }

    pub fn goto_graph(
        &self,
        file_id: FileId,
        goto_offset: TextSize,
    ) -> Option<SalsaFlowTerminalGraphSummary> {
        tracked::file_flow_goto_graph(self.db, file_id, goto_offset)
    }
}

#[derive(Clone, Copy)]
pub struct SalsaSummaryModuleQueries<'db> {
    db: &'db SalsaSummaryDatabase,
}

impl<'db> SalsaSummaryModuleQueries<'db> {
    pub(crate) fn new(db: &'db SalsaSummaryDatabase) -> Self {
        Self { db }
    }

    pub fn resolve_index(&self, file_id: FileId) -> Option<Arc<SalsaModuleResolveIndex>> {
        tracked::file_module_resolve_index(self.db, file_id)
    }

    pub fn summary(&self, file_id: FileId) -> Option<Arc<SalsaModuleSummary>> {
        tracked::file_module_summary(self.db, file_id)
    }

    pub fn export_target(&self, file_id: FileId) -> Option<SalsaExportTargetSummary> {
        tracked::file_module_export_target(self.db, file_id)
    }

    pub fn export(&self, file_id: FileId) -> Option<SalsaModuleExportSummary> {
        tracked::file_module_export(self.db, file_id)
    }

    pub fn exported_global_function(&self, file_id: FileId) -> Option<SalsaGlobalFunctionSummary> {
        tracked::file_module_exported_global_function(self.db, file_id)
    }

    pub fn exported_global_variable(&self, file_id: FileId) -> Option<SalsaGlobalVariableSummary> {
        tracked::file_module_exported_global_variable(self.db, file_id)
    }

    pub fn exports_decl(&self, file_id: FileId, decl_id: SalsaDeclId) -> Option<bool> {
        tracked::file_module_exports_decl(self.db, file_id, decl_id)
    }

    pub fn exports_member(
        &self,
        file_id: FileId,
        member_target: impl Into<SalsaMemberTargetId>,
    ) -> Option<bool> {
        tracked::file_module_exports_member(self.db, file_id, member_target.into())
    }

    pub fn exports_closure(&self, file_id: FileId, signature_offset: TextSize) -> Option<bool> {
        tracked::file_module_exports_closure(self.db, file_id, signature_offset)
    }

    pub fn exports_table(&self, file_id: FileId, table_offset: TextSize) -> Option<bool> {
        tracked::file_module_exports_table(self.db, file_id, table_offset)
    }

    pub fn exports_global_function(&self, file_id: FileId, name: SmolStr) -> Option<bool> {
        tracked::file_module_exports_global_function(self.db, file_id, name)
    }

    pub fn exports_global_variable(&self, file_id: FileId, name: SmolStr) -> Option<bool> {
        tracked::file_module_exports_global_variable(self.db, file_id, name)
    }
}

#[derive(Clone, Copy)]
pub struct SalsaSummarySemanticQueries<'db> {
    db: &'db SalsaSummaryDatabase,
}

impl<'db> SalsaSummarySemanticQueries<'db> {
    pub(crate) fn new(db: &'db SalsaSummaryDatabase) -> Self {
        Self { db }
    }

    pub fn file(&self) -> SalsaSummarySemanticFileQueries<'db> {
        SalsaSummarySemanticFileQueries::new(self.db)
    }

    pub fn target(&self) -> SalsaSummarySemanticTargetQueries<'db> {
        SalsaSummarySemanticTargetQueries::new(self.db)
    }
}

#[derive(Clone, Copy)]
pub struct SalsaSummaryTypeQueries<'db> {
    db: &'db SalsaSummaryDatabase,
}

impl<'db> SalsaSummaryTypeQueries<'db> {
    pub(crate) fn new(db: &'db SalsaSummaryDatabase) -> Self {
        Self { db }
    }

    pub fn decl_index(&self, file_id: FileId) -> Option<Arc<SalsaDeclTypeQueryIndex>> {
        tracked::file_decl_type_query_index(self.db, file_id)
    }

    pub fn decl(&self, file_id: FileId, decl_id: SalsaDeclId) -> Option<SalsaDeclTypeInfoSummary> {
        tracked::file_decl_type_info(self.db, file_id, decl_id)
    }

    pub fn global_index(&self, file_id: FileId) -> Option<Arc<SalsaGlobalTypeQueryIndex>> {
        tracked::file_global_type_query_index(self.db, file_id)
    }

    pub fn global(&self, file_id: FileId, name: SmolStr) -> Option<SalsaGlobalTypeInfoSummary> {
        tracked::file_global_type_info(self.db, file_id, name)
    }

    pub fn global_name(
        &self,
        file_id: FileId,
        syntax_offset: TextSize,
    ) -> Option<SalsaGlobalTypeInfoSummary> {
        tracked::file_global_name_type_info(self.db, file_id, syntax_offset)
    }

    pub fn member_index(&self, file_id: FileId) -> Option<Arc<SalsaMemberTypeQueryIndex>> {
        tracked::file_member_type_query_index(self.db, file_id)
    }

    pub fn member(
        &self,
        file_id: FileId,
        member_target: impl Into<SalsaMemberTargetId>,
    ) -> Option<SalsaMemberTypeInfoSummary> {
        tracked::file_member_type_info(self.db, file_id, member_target.into())
    }

    pub fn member_use(
        &self,
        file_id: FileId,
        syntax_offset: TextSize,
    ) -> Option<SalsaMemberTypeInfoSummary> {
        tracked::file_member_use_type_info(self.db, file_id, syntax_offset)
    }

    pub fn member_at(
        &self,
        file_id: FileId,
        syntax_offset: TextSize,
        program_point_offset: TextSize,
    ) -> Option<SalsaProgramPointMemberTypeInfoSummary> {
        tracked::file_member_type_at_program_point(
            self.db,
            file_id,
            syntax_offset,
            program_point_offset,
        )
    }

    pub fn name(
        &self,
        file_id: FileId,
        syntax_offset: TextSize,
    ) -> Option<SalsaNameTypeInfoSummary> {
        tracked::file_name_type_info(self.db, file_id, syntax_offset)
    }

    pub fn name_at(
        &self,
        file_id: FileId,
        syntax_offset: TextSize,
        program_point_offset: TextSize,
    ) -> Option<SalsaProgramPointTypeInfoSummary> {
        tracked::file_name_type_at_program_point(
            self.db,
            file_id,
            syntax_offset,
            program_point_offset,
        )
    }
}

#[derive(Clone, Copy)]
pub struct SalsaSummarySemanticFileQueries<'db> {
    db: &'db SalsaSummaryDatabase,
}

impl<'db> SalsaSummarySemanticFileQueries<'db> {
    pub(crate) fn new(db: &'db SalsaSummaryDatabase) -> Self {
        Self { db }
    }

    pub fn summary(&self, file_id: FileId) -> Option<Arc<SalsaSingleFileSemanticSummary>> {
        tracked::file_semantic_summary(self.db, file_id)
    }

    pub fn tag_properties(&self, file_id: FileId) -> Option<Vec<SalsaDocTagPropertySummary>> {
        tracked::file_semantic_tag_properties(self.db, file_id)
    }

    pub fn required_modules(&self, file_id: FileId) -> Option<Vec<SmolStr>> {
        tracked::file_semantic_required_modules(self.db, file_id)
    }

    pub fn module_export(&self, file_id: FileId) -> Option<Arc<SalsaModuleExportSemanticSummary>> {
        tracked::file_semantic_module_export(self.db, file_id)
    }

    pub fn module_export_query(&self, file_id: FileId) -> Option<SalsaModuleExportQuerySummary> {
        tracked::file_semantic_module_export_query(self.db, file_id)
    }

    pub fn signature_explain_index(
        &self,
        file_id: FileId,
    ) -> Option<Arc<SalsaSignatureExplainIndex>> {
        tracked::file_signature_explain_index(self.db, file_id)
    }

    pub fn call_explain(
        &self,
        file_id: FileId,
        call_offset: TextSize,
    ) -> Option<SalsaCallExplainSummary> {
        tracked::file_call_explain(self.db, file_id, call_offset)
    }

    pub fn signature_summary(
        &self,
        file_id: FileId,
        signature_offset: TextSize,
    ) -> Option<SalsaSemanticSignatureSummary> {
        tracked::file_semantic_signature_summary(self.db, file_id, signature_offset)
    }

    pub fn graph(&self, file_id: FileId) -> Option<Arc<SalsaSemanticGraphSummary>> {
        tracked::file_semantic_graph(self.db, file_id)
    }

    pub fn graph_index(&self, file_id: FileId) -> Option<Arc<SalsaSemanticGraphQueryIndex>> {
        tracked::file_semantic_graph_query_index(self.db, file_id)
    }

    pub fn graph_scc_index(&self, file_id: FileId) -> Option<Arc<SalsaSemanticGraphSccIndex>> {
        tracked::file_semantic_graph_scc_index(self.db, file_id)
    }

    pub fn graph_scc_component(
        &self,
        file_id: FileId,
        node: SalsaSemanticGraphNodeSummary,
    ) -> Option<SalsaSemanticGraphSccComponentSummary> {
        tracked::file_semantic_graph_scc_component(self.db, file_id, node)
    }

    pub fn graph_scc_component_by_id(
        &self,
        file_id: FileId,
        component_id: usize,
    ) -> Option<SalsaSemanticGraphSccComponentSummary> {
        tracked::file_semantic_graph_scc_component_by_id(self.db, file_id, component_id)
    }

    pub fn graph_scc_successors(
        &self,
        file_id: FileId,
        component_id: usize,
    ) -> Option<Vec<SalsaSemanticGraphSccComponentSummary>> {
        tracked::file_semantic_graph_scc_successors(self.db, file_id, component_id)
    }

    pub fn graph_scc_predecessors(
        &self,
        file_id: FileId,
        component_id: usize,
    ) -> Option<Vec<SalsaSemanticGraphSccComponentSummary>> {
        tracked::file_semantic_graph_scc_predecessors(self.db, file_id, component_id)
    }

    pub fn solver_worklist(
        &self,
        file_id: FileId,
    ) -> Option<Arc<SalsaSemanticSolverWorklistSummary>> {
        tracked::file_semantic_solver_worklist(self.db, file_id)
    }

    pub fn solver_execution(
        &self,
        file_id: FileId,
    ) -> Option<Arc<SalsaSemanticSolverExecutionSummary>> {
        tracked::file_semantic_solver_execution(self.db, file_id)
    }

    pub fn solver_execution_task(
        &self,
        file_id: FileId,
        component_id: usize,
    ) -> Option<SalsaSemanticSolverExecutionTaskSummary> {
        tracked::file_semantic_solver_execution_task(self.db, file_id, component_id)
    }

    pub fn solver_next_ready_execution_task(
        &self,
        file_id: FileId,
    ) -> Option<SalsaSemanticSolverExecutionTaskSummary> {
        tracked::file_semantic_solver_next_ready_execution_task(self.db, file_id)
    }

    pub fn solver_execution_is_complete(&self, file_id: FileId) -> Option<bool> {
        tracked::file_semantic_solver_execution_is_complete(self.db, file_id)
    }

    pub fn solver_step(&self, file_id: FileId) -> Option<SalsaSemanticSolverStepSummary> {
        tracked::file_semantic_solver_step(self.db, file_id)
    }

    pub fn solver_task(
        &self,
        file_id: FileId,
        component_id: usize,
    ) -> Option<SalsaSemanticSolverComponentTaskSummary> {
        tracked::file_semantic_solver_task(self.db, file_id, component_id)
    }

    pub fn solver_ready_tasks(
        &self,
        file_id: FileId,
    ) -> Option<Vec<SalsaSemanticSolverComponentTaskSummary>> {
        tracked::file_semantic_solver_ready_tasks(self.db, file_id)
    }

    pub fn signature_return_value_shell(
        &self,
        file_id: FileId,
        signature_offset: TextSize,
    ) -> Option<SalsaSemanticValueShellSummary> {
        tracked::file_semantic_signature_return_value_shell(self.db, file_id, signature_offset)
    }

    pub fn signature_return_component_summary(
        &self,
        file_id: FileId,
        component_id: usize,
    ) -> Option<SalsaSemanticSignatureReturnComponentSummary> {
        tracked::file_semantic_signature_return_component_summary(self.db, file_id, component_id)
    }

    pub fn signature_return_summary(
        &self,
        file_id: FileId,
        signature_offset: TextSize,
    ) -> Option<SalsaSemanticSignatureReturnSummary> {
        tracked::file_semantic_signature_return_summary(self.db, file_id, signature_offset)
    }

    pub fn signature_return_component_result_summary(
        &self,
        file_id: FileId,
        signature_offset: TextSize,
    ) -> Option<SalsaSemanticSolverComponentResultSummary> {
        tracked::file_semantic_signature_return_component_result_summary(
            self.db,
            file_id,
            signature_offset,
        )
    }

    pub fn decl_value_shell(
        &self,
        file_id: FileId,
        decl_id: SalsaDeclId,
    ) -> Option<SalsaSemanticValueShellSummary> {
        tracked::file_semantic_decl_value_shell(self.db, file_id, decl_id)
    }

    pub fn decl_component_result_summary(
        &self,
        file_id: FileId,
        decl_id: SalsaDeclId,
    ) -> Option<SalsaSemanticSolverComponentResultSummary> {
        tracked::file_semantic_decl_component_result_summary(self.db, file_id, decl_id)
    }

    pub fn decl_summary(
        &self,
        file_id: FileId,
        decl_id: SalsaDeclId,
    ) -> Option<SalsaSemanticDeclSummary> {
        tracked::file_semantic_decl_summary(self.db, file_id, decl_id)
    }

    pub fn decl_summary_by_syntax_id(
        &self,
        file_id: FileId,
        syntax_id: SalsaSyntaxIdSummary,
    ) -> Option<SalsaSemanticDeclSummary> {
        tracked::file_semantic_decl_summary_by_syntax_id(self.db, file_id, syntax_id)
    }

    pub fn member_value_shell(
        &self,
        file_id: FileId,
        member_target: SalsaMemberTargetId,
    ) -> Option<SalsaSemanticValueShellSummary> {
        tracked::file_semantic_member_value_shell(self.db, file_id, member_target)
    }

    pub fn member_component_result_summary(
        &self,
        file_id: FileId,
        member_target: SalsaMemberTargetId,
    ) -> Option<SalsaSemanticSolverComponentResultSummary> {
        tracked::file_semantic_member_component_result_summary(self.db, file_id, member_target)
    }

    pub fn member_summary(
        &self,
        file_id: FileId,
        member_target: SalsaMemberTargetId,
    ) -> Option<SalsaSemanticMemberSummary> {
        tracked::file_semantic_member_summary(self.db, file_id, member_target)
    }

    pub fn member_summary_by_syntax_id(
        &self,
        file_id: FileId,
        syntax_id: SalsaSyntaxIdSummary,
    ) -> Option<SalsaSemanticMemberSummary> {
        tracked::file_semantic_member_summary_by_syntax_id(self.db, file_id, syntax_id)
    }

    pub fn solver_component_result_summary(
        &self,
        file_id: FileId,
        component_id: usize,
    ) -> Option<SalsaSemanticSolverComponentResultSummary> {
        tracked::file_semantic_solver_component_result_summary(self.db, file_id, component_id)
    }

    pub fn for_range_iter_value_shell(
        &self,
        file_id: FileId,
        loop_offset: TextSize,
    ) -> Option<SalsaSemanticValueShellSummary> {
        tracked::file_semantic_for_range_iter_value_shell(self.db, file_id, loop_offset)
    }

    pub fn for_range_iter_component_result_summary(
        &self,
        file_id: FileId,
        loop_offset: TextSize,
    ) -> Option<SalsaSemanticSolverComponentResultSummary> {
        tracked::file_semantic_for_range_iter_component_result_summary(
            self.db,
            file_id,
            loop_offset,
        )
    }

    pub fn for_range_iter_component_summary(
        &self,
        file_id: FileId,
        loop_offset: TextSize,
    ) -> Option<SalsaSemanticForRangeIterComponentSummary> {
        tracked::file_semantic_for_range_iter_component_summary(self.db, file_id, loop_offset)
    }

    pub fn module_export_value_shell(
        &self,
        file_id: FileId,
    ) -> Option<SalsaSemanticValueShellSummary> {
        tracked::file_semantic_module_export_value_shell(self.db, file_id)
    }

    pub fn module_export_component_result_summary(
        &self,
        file_id: FileId,
    ) -> Option<SalsaSemanticSolverComponentResultSummary> {
        tracked::file_semantic_module_export_component_result_summary(self.db, file_id)
    }

    pub fn module_export_component_summary(
        &self,
        file_id: FileId,
    ) -> Option<SalsaSemanticModuleExportComponentSummary> {
        tracked::file_semantic_module_export_component_summary(self.db, file_id)
    }

    pub fn graph_outgoing_edges(
        &self,
        file_id: FileId,
        node: SalsaSemanticGraphNodeSummary,
    ) -> Option<Vec<SalsaSemanticGraphEdgeSummary>> {
        tracked::file_semantic_graph_outgoing_edges(self.db, file_id, node)
    }

    pub fn graph_incoming_edges(
        &self,
        file_id: FileId,
        node: SalsaSemanticGraphNodeSummary,
    ) -> Option<Vec<SalsaSemanticGraphEdgeSummary>> {
        tracked::file_semantic_graph_incoming_edges(self.db, file_id, node)
    }

    pub fn graph_successors(
        &self,
        file_id: FileId,
        node: SalsaSemanticGraphNodeSummary,
    ) -> Option<Vec<SalsaSemanticGraphNodeSummary>> {
        tracked::file_semantic_graph_successors(self.db, file_id, node)
    }

    pub fn graph_predecessors(
        &self,
        file_id: FileId,
        node: SalsaSemanticGraphNodeSummary,
    ) -> Option<Vec<SalsaSemanticGraphNodeSummary>> {
        tracked::file_semantic_graph_predecessors(self.db, file_id, node)
    }
}

#[derive(Clone, Copy)]
pub struct SalsaSummarySemanticTargetQueries<'db> {
    db: &'db SalsaSummaryDatabase,
}

impl<'db> SalsaSummarySemanticTargetQueries<'db> {
    pub(crate) fn new(db: &'db SalsaSummaryDatabase) -> Self {
        Self { db }
    }

    pub fn index(&self, file_id: FileId) -> Option<Arc<SalsaSemanticTargetQueryIndex>> {
        tracked::file_semantic_target_query_index(self.db, file_id)
    }

    pub fn decl(
        &self,
        file_id: FileId,
        decl_id: SalsaDeclId,
    ) -> Option<SalsaSemanticTargetInfoSummary> {
        tracked::file_semantic_decl(self.db, file_id, decl_id)
    }

    pub fn member(
        &self,
        file_id: FileId,
        member_target: impl Into<SalsaMemberTargetId>,
    ) -> Option<SalsaSemanticTargetInfoSummary> {
        tracked::file_semantic_member(self.db, file_id, member_target.into())
    }

    pub fn signature(
        &self,
        file_id: FileId,
        signature_offset: TextSize,
    ) -> Option<SalsaSemanticTargetInfoSummary> {
        tracked::file_semantic_signature(self.db, file_id, signature_offset)
    }

    pub fn signature_explain(
        &self,
        file_id: FileId,
        signature_offset: TextSize,
    ) -> Option<SalsaSignatureExplainSummary> {
        tracked::file_signature_explain(self.db, file_id, signature_offset)
    }
}
