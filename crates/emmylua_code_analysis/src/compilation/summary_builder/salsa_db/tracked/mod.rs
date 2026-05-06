use crate::{
    FileId, SalsaForRangeIterQueryIndex, SalsaForRangeIterQuerySummary,
    build_module_summary_with_index,
};
use crate::{
    SalsaSemanticDeclSummary, SalsaSemanticForRangeIterComponentSummary,
    SalsaSemanticMemberSummary, SalsaSemanticModuleExportComponentSummary,
    SalsaSemanticSignatureReturnComponentSummary, SalsaSemanticSignatureReturnSummary,
    SalsaSemanticSignatureSummary, SalsaSemanticSolverComponentResultSummary,
    SalsaSemanticSolverExecutionSummary, SalsaSemanticSolverWorklistSummary,
    SalsaSemanticValueShellSummary,
};
use rowan::TextSize;
use smol_str::SmolStr;
use std::sync::Arc;

use super::{
    super::{
        analysis::{
            SummaryDeclAnalysis, analyze_decl_summary, analyze_doc_owner_binding_summary,
            analyze_doc_summary, analyze_doc_type_summary, analyze_file_summary,
            analyze_flow_summary, analyze_property_summary, analyze_signature_summary,
            analyze_table_shape_summary, analyze_use_site_summary,
        },
        query::{
            SalsaCallExplainSummary, SalsaDeclTypeInfoSummary, SalsaDeclTypeQueryIndex,
            SalsaDocOwnerResolveIndex, SalsaDocOwnerResolveSummary, SalsaDocTagPropertySummary,
            SalsaDocTagQueryIndex, SalsaDocTypeLoweredIndex, SalsaDocTypeLoweredNode,
            SalsaDocTypeResolvedIndex, SalsaDocTypeResolvedSummary, SalsaGlobalTypeInfoSummary,
            SalsaGlobalTypeQueryIndex, SalsaLexicalUseIndex, SalsaLexicalUseSummary,
            SalsaLocalAssignmentQueryIndex, SalsaMemberTypeInfoSummary, SalsaMemberTypeQueryIndex,
            SalsaModuleExportSemanticSummary, SalsaModuleResolveIndex, SalsaNameTypeInfoSummary,
            SalsaProgramPointMemberTypeInfoSummary, SalsaProgramPointTypeInfoSummary,
            SalsaPropertyQueryIndex, SalsaSemanticGraphQueryIndex,
            SalsaSemanticGraphSccComponentSummary, SalsaSemanticGraphSccIndex,
            SalsaSemanticTargetInfoSummary, SalsaSemanticTargetQueryIndex,
            SalsaSignatureExplainIndex, SalsaSignatureExplainSummary,
            SalsaSignatureReturnQueryIndex, SalsaSignatureReturnQuerySummary,
            SalsaSingleFileSemanticSummary, build_branch_graph_summary,
            build_condition_graph_summary, build_decl_type_query_index, build_decl_value_shell,
            build_doc_owner_resolve_index, build_doc_tag_query_index, build_flow_query_summary,
            build_for_range_iter_component_summary, build_for_range_iter_query_index,
            build_global_type_query_index, build_lexical_use_index,
            build_local_assignment_query_index, build_loop_graph_summary,
            build_lowered_doc_type_index, build_member_type_query_index, build_member_value_shell,
            build_module_resolve_index, build_property_query_index, build_resolved_doc_type_index,
            build_semantic_decl_summary, build_semantic_graph_query_index,
            build_semantic_graph_scc_index, build_semantic_graph_summary,
            build_semantic_member_summary, build_semantic_solver_execution,
            build_semantic_solver_worklist, build_semantic_target_query_index,
            build_semantic_value_shell_from_for_range_iter_source,
            build_semantic_value_shell_from_module_export,
            build_semantic_value_shell_from_signature_return, build_signature_explain_index,
            build_signature_return_component_summary, build_signature_return_query_index,
            build_signature_return_summary, build_single_file_semantic_summary,
            build_terminal_graph_summary, can_reach_node, collect_active_type_narrows,
            collect_call_references_for_member_in_index, collect_call_references_for_name_in_index,
            collect_decl_references_in_index, collect_doc_owner_resolves_for_decl,
            collect_doc_owner_resolves_for_member, collect_doc_owner_resolves_for_signature,
            collect_doc_tags_for_kind_in_index, collect_doc_tags_for_owner_in_index,
            collect_global_name_references_in_index, collect_incoming_edges,
            collect_incoming_semantic_graph_edges, collect_member_references_by_role_in_index,
            collect_member_references_in_index, collect_name_references_by_role_in_index,
            collect_outgoing_edges, collect_outgoing_semantic_graph_edges,
            collect_predecessor_nodes, collect_properties_for_decl_and_key_in_index,
            collect_properties_for_decl_in_index, collect_properties_for_key_in_index,
            collect_properties_for_member_and_key_in_index, collect_properties_for_member_in_index,
            collect_properties_for_source_in_index, collect_properties_for_type_and_key_in_index,
            collect_properties_for_type_in_index, collect_reachable_nodes,
            collect_resolved_doc_tag_diagnostics_for_property,
            collect_semantic_graph_predecessor_nodes,
            collect_semantic_graph_scc_predecessor_components,
            collect_semantic_graph_scc_successor_components,
            collect_semantic_graph_successor_nodes, collect_semantic_solver_ready_tasks,
            collect_successor_nodes, find_call_explain_at, find_call_use_by_syntax_id,
            find_call_use_in_index, find_decl_type_info, find_doc_owner_resolve_at,
            find_doc_tag_at_in_index, find_doc_tag_property_in_index, find_for_range_iter_query_at,
            find_global_name_type_info, find_global_type_info, find_lexical_use_at,
            find_lowered_doc_type_at, find_lowered_doc_type_by_key,
            find_member_type_at_program_point, find_member_type_info, find_member_use_at,
            find_member_use_by_syntax_id, find_member_use_type_info, find_module_export,
            find_module_export_target, find_module_exported_global_function,
            find_module_exported_global_variable, find_name_type_at_program_point,
            find_name_type_info, find_name_use_at, find_name_use_by_syntax_id,
            find_next_ready_semantic_solver_execution_task, find_property_at_in_index,
            find_property_by_value_expr_syntax_id_in_index, find_resolved_doc_type_at,
            find_resolved_doc_type_by_key, find_semantic_decl_in_index,
            find_semantic_graph_scc_component, find_semantic_graph_scc_component_by_id,
            find_semantic_member_in_index, find_semantic_signature_in_index,
            find_semantic_solver_execution_task, find_semantic_solver_task,
            find_signature_explain_at, find_signature_return_query_at,
            for_range_iter_resolve_state_from_semantic, module_export_resolve_state_from_semantic,
            module_exports_closure, module_exports_decl, module_exports_global_function,
            module_exports_global_variable, module_exports_member, module_exports_table,
            semantic_solver_execution_is_complete, signature_return_resolve_state_from_semantic,
            signature_return_value_candidate_type_offsets,
            step_semantic_solver_execution_with_signature_returns,
        },
        summary::*,
    },
    SalsaSummaryDatabase, SummaryDb,
    inputs::{
        SummaryConfigInput, SummarySourceFileInput, SummaryWorkspaceInput, config_input,
        file_input, parse_chunk, workspace_input,
    },
};
use crate::SalsaSemanticGraphSummary;

mod doc;
mod lexical;

pub(crate) use doc::*;
pub(crate) use lexical::*;

#[salsa::tracked]
fn tracked_file_decl_analysis(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> SummaryDeclAnalysis {
    let parsed = parse_chunk(file.file_id(db), &file.text(db), &config.config(db));
    analyze_decl_summary(file.file_id(db), parsed.value)
}

#[salsa::tracked]
fn tracked_file_decl_tree_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaDeclTreeSummary> {
    Arc::new(tracked_file_decl_analysis(db, file, config).decl_tree)
}

#[salsa::tracked]
fn tracked_file_global_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaGlobalSummary> {
    Arc::new(tracked_file_decl_analysis(db, file, config).globals)
}

#[salsa::tracked]
fn tracked_file_doc_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaDocSummary> {
    let parsed = parse_chunk(file.file_id(db), &file.text(db), &config.config(db));
    Arc::new(analyze_doc_summary(parsed.value))
}

#[salsa::tracked]
fn tracked_file_flow_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaFlowSummary> {
    let parsed = parse_chunk(file.file_id(db), &file.text(db), &config.config(db));
    Arc::new(analyze_flow_summary(parsed.value))
}

#[salsa::tracked]
fn tracked_file_flow_block(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    block_offset: TextSize,
) -> Option<SalsaFlowBlockSummary> {
    let flow = tracked_file_flow_summary(db, file, config);
    flow.blocks
        .iter()
        .find(|block| block.syntax_offset == block_offset)
        .cloned()
}

#[salsa::tracked]
fn tracked_file_flow_branch(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    branch_offset: TextSize,
) -> Option<SalsaFlowBranchSummary> {
    let flow = tracked_file_flow_summary(db, file, config);
    flow.branches
        .iter()
        .find(|branch| branch.syntax_offset == branch_offset)
        .cloned()
}

#[salsa::tracked]
fn tracked_file_flow_loop(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    loop_offset: TextSize,
) -> Option<SalsaFlowLoopSummary> {
    let flow = tracked_file_flow_summary(db, file, config);
    flow.loops
        .iter()
        .find(|loop_summary| loop_summary.syntax_offset == loop_offset)
        .cloned()
}

#[salsa::tracked]
fn tracked_file_flow_return(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    return_offset: TextSize,
) -> Option<SalsaFlowReturnSummary> {
    let flow = tracked_file_flow_summary(db, file, config);
    flow.returns
        .iter()
        .find(|return_summary| return_summary.syntax_offset == return_offset)
        .cloned()
}

#[salsa::tracked]
fn tracked_file_flow_break(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    break_offset: TextSize,
) -> Option<SalsaFlowBreakSummary> {
    let flow = tracked_file_flow_summary(db, file, config);
    flow.breaks
        .iter()
        .find(|break_summary| break_summary.syntax_offset == break_offset)
        .cloned()
}

#[salsa::tracked]
fn tracked_file_flow_goto(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    goto_offset: TextSize,
) -> Option<SalsaFlowGotoSummary> {
    let flow = tracked_file_flow_summary(db, file, config);
    flow.gotos
        .iter()
        .find(|goto_summary| goto_summary.syntax_offset == goto_offset)
        .cloned()
}

#[salsa::tracked]
fn tracked_file_flow_label(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    label_offset: TextSize,
) -> Option<SalsaFlowLabelSummary> {
    let flow = tracked_file_flow_summary(db, file, config);
    flow.labels
        .iter()
        .find(|label| label.syntax_offset == label_offset)
        .cloned()
}

#[salsa::tracked]
fn tracked_file_flow_query_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaFlowQuerySummary> {
    let flow = tracked_file_flow_summary(db, file, config);
    Arc::new(build_flow_query_summary(flow.as_ref()))
}

#[salsa::tracked]
fn tracked_file_for_range_iter_query_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaForRangeIterQueryIndex> {
    let parsed = parse_chunk(file.file_id(db), &file.text(db), &config.config(db));
    let flow = tracked_file_flow_summary(db, file, config);
    let decl_tree = tracked_file_decl_tree_summary(db, file, config);
    let decl_index = tracked_file_decl_type_query_index(db, file, config);
    let member_index = tracked_file_member_type_query_index(db, file, config);
    let assignments = tracked_file_local_assignment_query_index(db, file, config);
    let property_index = tracked_file_property_summary(db, file, config);
    let doc = tracked_file_doc_summary(db, file, config);
    let doc_types = tracked_file_doc_type_summary(db, file, config);
    let lowered_types = tracked_file_doc_type_lowered_index(db, file, config);
    let signature_explain_index = tracked_file_signature_explain_index(db, file, config);
    let signature_return_index = tracked_file_signature_return_query_index(db, file, config);
    let doc_tag_query_index = tracked_file_doc_tag_query_index(db, file, config);
    let use_sites = tracked_file_use_site_summary(db, file, config);

    Arc::new(build_for_range_iter_query_index(
        flow.as_ref(),
        decl_tree.as_ref(),
        decl_index.as_ref(),
        member_index.as_ref(),
        assignments.as_ref(),
        property_index.as_ref(),
        doc.as_ref(),
        doc_types.as_ref(),
        lowered_types.as_ref(),
        signature_explain_index.as_ref(),
        signature_return_index.as_ref(),
        doc_tag_query_index.as_ref(),
        use_sites.as_ref(),
        &parsed.value,
    ))
}

#[salsa::tracked]
fn tracked_file_for_range_iter_query(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    loop_offset: TextSize,
) -> Option<SalsaForRangeIterQuerySummary> {
    if let Some(component_summary) =
        tracked_file_semantic_for_range_iter_component_summary(db, file, config, loop_offset)
    {
        return Some(project_for_range_iter_query_from_semantic_summary(
            &component_summary,
        ));
    }

    let index = tracked_file_for_range_iter_query_index(db, file, config);
    find_for_range_iter_query_at(index.as_ref(), loop_offset)
}

fn project_signature_return_query_from_semantic_summary(
    summary: &SalsaSemanticSignatureReturnSummary,
) -> SalsaSignatureReturnQuerySummary {
    SalsaSignatureReturnQuerySummary {
        signature_offset: summary.signature_offset,
        state: summary.state.clone(),
        doc_returns: summary.doc_returns.clone(),
        values: summary.values.clone(),
    }
}

fn project_for_range_iter_query_from_semantic_summary(
    summary: &SalsaSemanticForRangeIterComponentSummary,
) -> SalsaForRangeIterQuerySummary {
    SalsaForRangeIterQuerySummary {
        loop_offset: summary.loop_offset,
        iter_expr_offsets: summary.iter_expr_offsets.clone(),
        state: summary.state.clone(),
        source: summary.source.clone(),
        iter_vars: summary.iter_vars.clone(),
    }
}

#[salsa::tracked]
fn tracked_file_semantic_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaSingleFileSemanticSummary> {
    let properties = tracked_file_property_summary(db, file, config);
    let owner_resolves = tracked_file_doc_owner_resolve_index(db, file, config);
    let lexical_uses = tracked_file_lexical_use_index(db, file, config);
    let module = tracked_file_module_summary(db, file, config);
    let tag_properties = tracked_file_doc_tag_properties(db, file, config);
    Arc::new(build_single_file_semantic_summary(
        properties.as_ref(),
        owner_resolves.as_ref(),
        lexical_uses.as_ref(),
        &tag_properties,
        module.as_deref(),
    ))
}

#[salsa::tracked]
fn tracked_file_semantic_graph(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaSemanticGraphSummary> {
    let decl_tree = tracked_file_decl_tree_summary(db, file, config);
    let member_types = tracked_file_member_type_query_index(db, file, config);
    let signatures = tracked_file_signature_summary(db, file, config);
    let for_range_iters = tracked_file_for_range_iter_query_index(db, file, config);
    let use_sites = tracked_file_use_site_summary(db, file, config);
    let signature_explain_index = tracked_file_signature_explain_index(db, file, config);
    let signature_returns = tracked_file_signature_return_query_index(db, file, config);
    let semantic = tracked_file_semantic_summary(db, file, config);

    Arc::new(build_semantic_graph_summary(
        decl_tree.as_ref(),
        member_types.as_ref(),
        signatures.as_ref(),
        for_range_iters.as_ref(),
        use_sites.as_ref(),
        signature_explain_index.as_ref(),
        signature_returns.as_ref(),
        semantic.as_ref(),
    ))
}

#[salsa::tracked]
fn tracked_file_semantic_graph_query_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaSemanticGraphQueryIndex> {
    let graph = tracked_file_semantic_graph(db, file, config);
    Arc::new(build_semantic_graph_query_index(graph.as_ref()))
}

#[salsa::tracked]
fn tracked_file_semantic_graph_scc_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaSemanticGraphSccIndex> {
    let graph = tracked_file_semantic_graph(db, file, config);
    Arc::new(build_semantic_graph_scc_index(graph.as_ref()))
}

#[salsa::tracked]
fn tracked_file_semantic_graph_scc_component(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    node: SalsaSemanticGraphNodeSummary,
) -> Option<SalsaSemanticGraphSccComponentSummary> {
    let index = tracked_file_semantic_graph_scc_index(db, file, config);
    find_semantic_graph_scc_component(index.as_ref(), &node)
}

#[salsa::tracked]
fn tracked_file_semantic_graph_scc_component_by_id(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    component_id: usize,
) -> Option<SalsaSemanticGraphSccComponentSummary> {
    let index = tracked_file_semantic_graph_scc_index(db, file, config);
    find_semantic_graph_scc_component_by_id(index.as_ref(), component_id)
}

#[salsa::tracked]
fn tracked_file_semantic_graph_scc_successors(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    component_id: usize,
) -> Vec<SalsaSemanticGraphSccComponentSummary> {
    let index = tracked_file_semantic_graph_scc_index(db, file, config);
    collect_semantic_graph_scc_successor_components(index.as_ref(), component_id)
}

#[salsa::tracked]
fn tracked_file_semantic_graph_scc_predecessors(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    component_id: usize,
) -> Vec<SalsaSemanticGraphSccComponentSummary> {
    let index = tracked_file_semantic_graph_scc_index(db, file, config);
    collect_semantic_graph_scc_predecessor_components(index.as_ref(), component_id)
}

#[salsa::tracked]
fn tracked_file_semantic_solver_worklist(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaSemanticSolverWorklistSummary> {
    let scc_index = tracked_file_semantic_graph_scc_index(db, file, config);
    Arc::new(build_semantic_solver_worklist(scc_index.as_ref()))
}

#[salsa::tracked]
fn tracked_file_semantic_solver_execution(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaSemanticSolverExecutionSummary> {
    let worklist = tracked_file_semantic_solver_worklist(db, file, config);
    Arc::new(build_semantic_solver_execution(worklist.as_ref()))
}

#[salsa::tracked]
fn tracked_file_semantic_solver_execution_task(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    component_id: usize,
) -> Option<crate::SalsaSemanticSolverExecutionTaskSummary> {
    let execution = tracked_file_semantic_solver_execution(db, file, config);
    find_semantic_solver_execution_task(execution.as_ref(), component_id)
}

#[salsa::tracked]
fn tracked_file_semantic_solver_next_ready_execution_task(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Option<crate::SalsaSemanticSolverExecutionTaskSummary> {
    let execution = tracked_file_semantic_solver_execution(db, file, config);
    find_next_ready_semantic_solver_execution_task(execution.as_ref())
}

#[salsa::tracked]
fn tracked_file_semantic_solver_execution_is_complete(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> bool {
    let execution = tracked_file_semantic_solver_execution(db, file, config);
    semantic_solver_execution_is_complete(execution.as_ref())
}

#[salsa::tracked]
fn tracked_file_semantic_solver_step(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Option<crate::SalsaSemanticSolverStepSummary> {
    let execution = tracked_file_semantic_solver_execution(db, file, config);
    let scc_index = tracked_file_semantic_graph_scc_index(db, file, config);
    let decl_types = tracked_file_decl_type_query_index(db, file, config);
    let member_types = tracked_file_member_type_query_index(db, file, config);
    let signature_returns = tracked_file_signature_return_query_index(db, file, config);
    let for_range_iters = tracked_file_for_range_iter_query_index(db, file, config);
    let use_sites = tracked_file_use_site_summary(db, file, config);
    let signature_explain_index = tracked_file_signature_explain_index(db, file, config);
    let module_export = tracked_file_semantic_module_export(db, file, config);
    step_semantic_solver_execution_with_signature_returns(
        execution.as_ref(),
        scc_index.as_ref(),
        decl_types.as_ref(),
        member_types.as_ref(),
        signature_returns.as_ref(),
        for_range_iters.as_ref(),
        use_sites.as_ref(),
        signature_explain_index.as_ref(),
        module_export.as_ref(),
    )
}

#[salsa::tracked]
fn tracked_file_semantic_solver_task(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    component_id: usize,
) -> Option<crate::SalsaSemanticSolverComponentTaskSummary> {
    let worklist = tracked_file_semantic_solver_worklist(db, file, config);
    find_semantic_solver_task(worklist.as_ref(), component_id)
}

#[salsa::tracked]
fn tracked_file_semantic_solver_ready_tasks(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Vec<crate::SalsaSemanticSolverComponentTaskSummary> {
    let worklist = tracked_file_semantic_solver_worklist(db, file, config);
    collect_semantic_solver_ready_tasks(worklist.as_ref())
}

#[salsa::tracked]
fn tracked_file_semantic_signature_return_value_shell(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    signature_offset: TextSize,
) -> Option<SalsaSemanticValueShellSummary> {
    if let Some(component) = tracked_file_semantic_graph_scc_component(
        db,
        file,
        config,
        SalsaSemanticGraphNodeSummary::SignatureReturn(signature_offset),
    ) && let Some(component_result) = tracked_file_semantic_solver_component_result_summary(
        db,
        file,
        config,
        component.component_id,
    ) {
        return Some(component_result.fixedpoint_value_shell);
    }

    let summary = tracked_file_signature_return_query(db, file, config, signature_offset)?;
    Some(build_semantic_value_shell_from_signature_return(&summary))
}

#[salsa::tracked]
fn tracked_file_semantic_signature_return_component_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    component_id: usize,
) -> Option<SalsaSemanticSignatureReturnComponentSummary> {
    let component =
        tracked_file_semantic_graph_scc_component_by_id(db, file, config, component_id)?;
    let signature_returns = tracked_file_signature_return_query_index(db, file, config);
    let mut summary = build_signature_return_component_summary(&component, &signature_returns)?;
    if let Some(component_result) =
        tracked_file_semantic_solver_component_result_summary(db, file, config, component_id)
    {
        summary.propagated_value_shell = component_result.propagated_value_shell;
        summary.local_value_shell = component_result.local_value_shell;
        let fixedpoint_value_shell = component_result.fixedpoint_value_shell;
        summary.fixedpoint_value_shell = fixedpoint_value_shell.clone();
        summary.value_shell = fixedpoint_value_shell;
    }
    Some(summary)
}

fn merge_signature_return_resolve_state(
    base_state: SalsaSignatureReturnResolveStateSummary,
    solver_state: Option<SalsaSignatureReturnResolveStateSummary>,
) -> SalsaSignatureReturnResolveStateSummary {
    let Some(solver_state) = solver_state else {
        return base_state;
    };

    match (base_state, solver_state) {
        (SalsaSignatureReturnResolveStateSummary::RecursiveDependency, _)
        | (_, SalsaSignatureReturnResolveStateSummary::RecursiveDependency) => {
            SalsaSignatureReturnResolveStateSummary::RecursiveDependency
        }
        (SalsaSignatureReturnResolveStateSummary::Resolved, _)
        | (_, SalsaSignatureReturnResolveStateSummary::Resolved) => {
            SalsaSignatureReturnResolveStateSummary::Resolved
        }
        _ => SalsaSignatureReturnResolveStateSummary::Partial,
    }
}

#[salsa::tracked]
fn tracked_file_semantic_signature_return_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    signature_offset: TextSize,
) -> Option<SalsaSemanticSignatureReturnSummary> {
    let index = tracked_file_signature_return_query_index(db, file, config);
    let query_summary = find_signature_return_query_at(index.as_ref(), signature_offset)?;
    let component = tracked_file_semantic_graph_scc_component(
        db,
        file,
        config,
        SalsaSemanticGraphNodeSummary::SignatureReturn(signature_offset),
    )?;
    let mut summary = build_signature_return_summary(&component, &query_summary)?;
    if let Some(component_result) = tracked_file_semantic_signature_return_component_result_summary(
        db,
        file,
        config,
        signature_offset,
    ) {
        summary.propagated_value_shell = component_result.propagated_value_shell;
        summary.local_value_shell = component_result.local_value_shell;
        let fixedpoint_value_shell = component_result.fixedpoint_value_shell;
        summary.state = merge_signature_return_resolve_state(
            query_summary.state,
            signature_return_resolve_state_from_semantic(fixedpoint_value_shell.state),
        );
        let is_single_slot = summary.values.len() == 1;
        for (result_index, value) in summary.values.iter_mut().enumerate() {
            if !value.doc_return_type_offsets.is_empty() {
                continue;
            }

            let candidate_type_offsets =
                if is_single_slot && !fixedpoint_value_shell.candidate_type_offsets.is_empty() {
                    fixedpoint_value_shell.candidate_type_offsets.clone()
                } else {
                    signature_return_value_candidate_type_offsets(value, result_index)
                };

            if !candidate_type_offsets.is_empty() {
                value.doc_return_type_offsets = candidate_type_offsets;
            }
        }
        summary.fixedpoint_value_shell = fixedpoint_value_shell.clone();
        summary.value_shell = fixedpoint_value_shell;
    }
    Some(summary)
}

#[salsa::tracked]
fn tracked_file_semantic_signature_return_component_result_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    signature_offset: TextSize,
) -> Option<SalsaSemanticSolverComponentResultSummary> {
    let component = tracked_file_semantic_graph_scc_component(
        db,
        file,
        config,
        SalsaSemanticGraphNodeSummary::SignatureReturn(signature_offset),
    )?;
    tracked_file_semantic_solver_component_result_summary(db, file, config, component.component_id)
}

#[salsa::tracked]
fn tracked_file_semantic_decl_value_shell(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    decl_id: crate::SalsaDeclId,
) -> SalsaSemanticValueShellSummary {
    if let Some(summary) = tracked_file_semantic_decl_summary(db, file, config, decl_id) {
        return summary.value_shell;
    }

    if let Some(component) = tracked_file_semantic_graph_scc_component(
        db,
        file,
        config,
        SalsaSemanticGraphNodeSummary::DeclValue(decl_id),
    ) && let Some(component_result) = tracked_file_semantic_solver_component_result_summary(
        db,
        file,
        config,
        component.component_id,
    ) {
        return component_result.fixedpoint_value_shell;
    }

    let decl_types = tracked_file_decl_type_query_index(db, file, config);
    build_decl_value_shell(&decl_types, decl_id)
}

#[salsa::tracked]
fn tracked_file_semantic_decl_component_result_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    decl_id: crate::SalsaDeclId,
) -> Option<SalsaSemanticSolverComponentResultSummary> {
    let component = tracked_file_semantic_graph_scc_component(
        db,
        file,
        config,
        SalsaSemanticGraphNodeSummary::DeclValue(decl_id),
    )?;
    tracked_file_semantic_solver_component_result_summary(db, file, config, component.component_id)
}

#[salsa::tracked]
fn tracked_file_semantic_decl_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    decl_id: crate::SalsaDeclId,
) -> Option<SalsaSemanticDeclSummary> {
    let decl_type = tracked_file_decl_type_info(db, file, config, decl_id)?;
    let component = tracked_file_semantic_graph_scc_component(
        db,
        file,
        config,
        SalsaSemanticGraphNodeSummary::DeclValue(decl_id),
    )?;
    let mut summary = build_semantic_decl_summary(&component, &decl_type)?;
    if let Some(component_result) =
        tracked_file_semantic_decl_component_result_summary(db, file, config, decl_id)
    {
        summary.propagated_value_shell = component_result.propagated_value_shell;
        summary.local_value_shell = component_result.local_value_shell;
        let fixedpoint_value_shell = component_result.fixedpoint_value_shell;
        summary.fixedpoint_value_shell = fixedpoint_value_shell.clone();
        summary.value_shell = fixedpoint_value_shell;
    }
    Some(summary)
}

#[salsa::tracked]
fn tracked_file_semantic_member_value_shell(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    member_target: crate::SalsaMemberTargetId,
) -> SalsaSemanticValueShellSummary {
    if let Some(summary) =
        tracked_file_semantic_member_summary(db, file, config, member_target.clone())
    {
        return summary.value_shell;
    }

    if let Some(component) = tracked_file_semantic_graph_scc_component(
        db,
        file,
        config,
        SalsaSemanticGraphNodeSummary::MemberValue(member_target.clone()),
    ) && let Some(component_result) = tracked_file_semantic_solver_component_result_summary(
        db,
        file,
        config,
        component.component_id,
    ) {
        return component_result.fixedpoint_value_shell;
    }

    let member_types = tracked_file_member_type_query_index(db, file, config);
    build_member_value_shell(member_types.as_ref(), &member_target)
}

#[salsa::tracked]
fn tracked_file_semantic_member_component_result_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    member_target: crate::SalsaMemberTargetId,
) -> Option<SalsaSemanticSolverComponentResultSummary> {
    let component = tracked_file_semantic_graph_scc_component(
        db,
        file,
        config,
        SalsaSemanticGraphNodeSummary::MemberValue(member_target),
    )?;
    tracked_file_semantic_solver_component_result_summary(db, file, config, component.component_id)
}

#[salsa::tracked]
fn tracked_file_semantic_member_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    member_target: crate::SalsaMemberTargetId,
) -> Option<SalsaSemanticMemberSummary> {
    let member_type = tracked_file_member_type_info(db, file, config, member_target.clone())?;
    let component = tracked_file_semantic_graph_scc_component(
        db,
        file,
        config,
        SalsaSemanticGraphNodeSummary::MemberValue(member_target.clone()),
    )?;
    let mut summary = build_semantic_member_summary(&component, &member_type)?;
    if let Some(component_result) =
        tracked_file_semantic_member_component_result_summary(db, file, config, member_target)
    {
        summary.propagated_value_shell = component_result.propagated_value_shell;
        summary.local_value_shell = component_result.local_value_shell;
        let fixedpoint_value_shell = component_result.fixedpoint_value_shell;
        summary.fixedpoint_value_shell = fixedpoint_value_shell.clone();
        summary.value_shell = fixedpoint_value_shell;
    }
    Some(summary)
}

#[salsa::tracked]
fn tracked_file_semantic_solver_component_result_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    component_id: usize,
) -> Option<SalsaSemanticSolverComponentResultSummary> {
    let decl_types = tracked_file_decl_type_query_index(db, file, config);
    let member_types = tracked_file_member_type_query_index(db, file, config);
    let signature_returns = tracked_file_signature_return_query_index(db, file, config);
    let for_range_iters = tracked_file_for_range_iter_query_index(db, file, config);
    let use_sites = tracked_file_use_site_summary(db, file, config);
    let signature_explain_index = tracked_file_signature_explain_index(db, file, config);
    let module_export = tracked_file_semantic_module_export(db, file, config);
    let component =
        tracked_file_semantic_graph_scc_component_by_id(db, file, config, component_id)?;
    let predecessor_results = component
        .predecessor_component_ids
        .iter()
        .filter_map(|predecessor_component_id| {
            tracked_file_semantic_solver_component_result_summary(
                db,
                file,
                config,
                *predecessor_component_id,
            )
        })
        .collect::<Vec<_>>();
    Some(crate::build_semantic_solver_component_result_summary(
        &component,
        decl_types.as_ref(),
        member_types.as_ref(),
        signature_returns.as_ref(),
        for_range_iters.as_ref(),
        use_sites.as_ref(),
        signature_explain_index.as_ref(),
        module_export.as_ref(),
        &predecessor_results,
    ))
}

#[salsa::tracked]
fn tracked_file_semantic_for_range_iter_value_shell(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    loop_offset: TextSize,
) -> Option<SalsaSemanticValueShellSummary> {
    if let Some(component) = tracked_file_semantic_graph_scc_component(
        db,
        file,
        config,
        SalsaSemanticGraphNodeSummary::ForRangeIter(loop_offset),
    ) && let Some(component_result) = tracked_file_semantic_solver_component_result_summary(
        db,
        file,
        config,
        component.component_id,
    ) {
        return Some(component_result.fixedpoint_value_shell);
    }

    let summary = tracked_file_for_range_iter_query(db, file, config, loop_offset)?;
    let signature_returns = tracked_file_signature_return_query_index(db, file, config);
    Some(build_semantic_value_shell_from_for_range_iter_source(
        &summary,
        Some(signature_returns.as_ref()),
    ))
}

#[salsa::tracked]
fn tracked_file_semantic_for_range_iter_component_result_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    loop_offset: TextSize,
) -> Option<SalsaSemanticSolverComponentResultSummary> {
    let component = tracked_file_semantic_graph_scc_component(
        db,
        file,
        config,
        SalsaSemanticGraphNodeSummary::ForRangeIter(loop_offset),
    )?;
    tracked_file_semantic_solver_component_result_summary(db, file, config, component.component_id)
}

fn merge_for_range_iter_resolve_state(
    base_state: SalsaForRangeIterResolveStateSummary,
    solver_state: Option<SalsaForRangeIterResolveStateSummary>,
) -> SalsaForRangeIterResolveStateSummary {
    let Some(solver_state) = solver_state else {
        return base_state;
    };

    match (base_state, solver_state) {
        (SalsaForRangeIterResolveStateSummary::RecursiveDependency, _)
        | (_, SalsaForRangeIterResolveStateSummary::RecursiveDependency) => {
            SalsaForRangeIterResolveStateSummary::RecursiveDependency
        }
        (SalsaForRangeIterResolveStateSummary::Resolved, _)
        | (_, SalsaForRangeIterResolveStateSummary::Resolved) => {
            SalsaForRangeIterResolveStateSummary::Resolved
        }
        _ => SalsaForRangeIterResolveStateSummary::Partial,
    }
}

#[salsa::tracked]
fn tracked_file_semantic_for_range_iter_component_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    loop_offset: TextSize,
) -> Option<SalsaSemanticForRangeIterComponentSummary> {
    let index = tracked_file_for_range_iter_query_index(db, file, config);
    let query_summary = find_for_range_iter_query_at(index.as_ref(), loop_offset)?;
    let component = tracked_file_semantic_graph_scc_component(
        db,
        file,
        config,
        SalsaSemanticGraphNodeSummary::ForRangeIter(loop_offset),
    )?;
    let mut summary = build_for_range_iter_component_summary(&component, &query_summary)?;
    if let Some(component_result) =
        tracked_file_semantic_for_range_iter_component_result_summary(db, file, config, loop_offset)
    {
        summary.propagated_value_shell = component_result.propagated_value_shell;
        summary.local_value_shell = component_result.local_value_shell;
        let fixedpoint_value_shell = component_result.fixedpoint_value_shell;
        summary.state = merge_for_range_iter_resolve_state(
            query_summary.state,
            for_range_iter_resolve_state_from_semantic(fixedpoint_value_shell.state),
        );
        summary.fixedpoint_value_shell = fixedpoint_value_shell.clone();
        summary.value_shell = fixedpoint_value_shell;
    }
    Some(summary)
}

#[salsa::tracked]
fn tracked_file_semantic_module_export_value_shell(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> SalsaSemanticValueShellSummary {
    if let Some(component) = tracked_file_semantic_graph_scc_component(
        db,
        file,
        config,
        SalsaSemanticGraphNodeSummary::ModuleExport,
    ) && let Some(component_result) = tracked_file_semantic_solver_component_result_summary(
        db,
        file,
        config,
        component.component_id,
    ) {
        return component_result.fixedpoint_value_shell;
    }

    let module_export = tracked_file_semantic_module_export(db, file, config);
    build_semantic_value_shell_from_module_export(module_export.as_ref())
}

#[salsa::tracked]
fn tracked_file_semantic_module_export_component_result_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Option<SalsaSemanticSolverComponentResultSummary> {
    let component = tracked_file_semantic_graph_scc_component(
        db,
        file,
        config,
        SalsaSemanticGraphNodeSummary::ModuleExport,
    )?;
    tracked_file_semantic_solver_component_result_summary(db, file, config, component.component_id)
}

fn build_module_export_query_summary_base(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Option<crate::SalsaModuleExportQuerySummary> {
    let module = tracked_file_module_summary(db, file, config)?;
    let export_target = module.export_target.clone()?;
    let export = module.export.clone();
    let export_semantic = tracked_file_semantic_module_export(db, file, config);
    let semantic_target = export_semantic
        .as_ref()
        .and_then(|summary| summary.semantic_target.clone());
    let doc_owners = export_semantic
        .as_ref()
        .map(|summary| summary.doc_owners.clone())
        .unwrap_or_default();
    let tag_properties = export_semantic
        .as_ref()
        .map(|summary| summary.tag_properties.clone())
        .unwrap_or_default();
    let properties = if let Some(semantic_target) = &semantic_target {
        let index = tracked_file_semantic_target_query_index(db, file, config);
        match semantic_target {
            crate::SalsaSemanticTargetSummary::Decl(decl_id) => {
                find_semantic_decl_in_index(index.as_ref(), *decl_id)
            }
            crate::SalsaSemanticTargetSummary::Member(member_target) => {
                find_semantic_member_in_index(index.as_ref(), member_target)
            }
            crate::SalsaSemanticTargetSummary::Signature(signature_offset) => {
                find_semantic_signature_in_index(index.as_ref(), *signature_offset)
            }
        }
        .map(|target| target.properties)
        .unwrap_or_default()
    } else {
        Vec::new()
    };

    Some(crate::SalsaModuleExportQuerySummary {
        export_target,
        export,
        semantic_target: semantic_target.clone(),
        doc_owners,
        tag_properties,
        properties,
        state: if semantic_target.is_some() {
            crate::SalsaModuleExportResolveStateSummary::Resolved
        } else {
            crate::SalsaModuleExportResolveStateSummary::Partial
        },
    })
}

fn merge_module_export_resolve_state(
    base_state: crate::SalsaModuleExportResolveStateSummary,
    solver_state: Option<crate::SalsaModuleExportResolveStateSummary>,
) -> crate::SalsaModuleExportResolveStateSummary {
    let Some(solver_state) = solver_state else {
        return base_state;
    };

    match (base_state, solver_state) {
        (crate::SalsaModuleExportResolveStateSummary::RecursiveDependency, _)
        | (_, crate::SalsaModuleExportResolveStateSummary::RecursiveDependency) => {
            crate::SalsaModuleExportResolveStateSummary::RecursiveDependency
        }
        (crate::SalsaModuleExportResolveStateSummary::Resolved, _)
        | (_, crate::SalsaModuleExportResolveStateSummary::Resolved) => {
            crate::SalsaModuleExportResolveStateSummary::Resolved
        }
        _ => crate::SalsaModuleExportResolveStateSummary::Partial,
    }
}

#[salsa::tracked]
fn tracked_file_semantic_module_export_component_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Option<SalsaSemanticModuleExportComponentSummary> {
    let component = tracked_file_semantic_graph_scc_component(
        db,
        file,
        config,
        SalsaSemanticGraphNodeSummary::ModuleExport,
    )?;
    let query_summary = build_module_export_query_summary_base(db, file, config)?;
    let local_value_shell = build_semantic_value_shell_from_module_export(
        tracked_file_semantic_module_export(db, file, config).as_ref(),
    );
    let mut summary = SalsaSemanticModuleExportComponentSummary {
        component_id: component.component_id,
        export_target: query_summary.export_target.clone(),
        export: query_summary.export.clone(),
        semantic_target: query_summary.semantic_target.clone(),
        doc_owners: query_summary.doc_owners.clone(),
        tag_properties: query_summary.tag_properties.clone(),
        properties: query_summary.properties.clone(),
        state: merge_module_export_resolve_state(
            query_summary.state.clone(),
            module_export_resolve_state_from_semantic(local_value_shell.state),
        ),
        propagated_value_shell: SalsaSemanticValueShellSummary {
            state: crate::SalsaSemanticResolveStateSummary::Unknown,
            candidate_type_offsets: Vec::new(),
        },
        local_value_shell: local_value_shell.clone(),
        fixedpoint_value_shell: local_value_shell.clone(),
        value_shell: local_value_shell,
        is_cycle: component.is_cycle,
    };
    if let Some(component_result) =
        tracked_file_semantic_module_export_component_result_summary(db, file, config)
    {
        summary.propagated_value_shell = component_result.propagated_value_shell;
        summary.local_value_shell = component_result.local_value_shell;
        let fixedpoint_value_shell = component_result.fixedpoint_value_shell;
        summary.state = merge_module_export_resolve_state(
            query_summary.state,
            module_export_resolve_state_from_semantic(fixedpoint_value_shell.state),
        );
        summary.fixedpoint_value_shell = fixedpoint_value_shell.clone();
        summary.value_shell = fixedpoint_value_shell;
    }
    Some(summary)
}

#[salsa::tracked]
fn tracked_file_semantic_graph_outgoing_edges(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    node: SalsaSemanticGraphNodeSummary,
) -> Vec<SalsaSemanticGraphEdgeSummary> {
    let index = tracked_file_semantic_graph_query_index(db, file, config);
    collect_outgoing_semantic_graph_edges(index.as_ref(), &node)
}

#[salsa::tracked]
fn tracked_file_semantic_graph_incoming_edges(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    node: SalsaSemanticGraphNodeSummary,
) -> Vec<SalsaSemanticGraphEdgeSummary> {
    let index = tracked_file_semantic_graph_query_index(db, file, config);
    collect_incoming_semantic_graph_edges(index.as_ref(), &node)
}

#[salsa::tracked]
fn tracked_file_semantic_graph_successors(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    node: SalsaSemanticGraphNodeSummary,
) -> Vec<SalsaSemanticGraphNodeSummary> {
    let index = tracked_file_semantic_graph_query_index(db, file, config);
    collect_semantic_graph_successor_nodes(index.as_ref(), &node)
}

#[salsa::tracked]
fn tracked_file_semantic_graph_predecessors(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    node: SalsaSemanticGraphNodeSummary,
) -> Vec<SalsaSemanticGraphNodeSummary> {
    let index = tracked_file_semantic_graph_query_index(db, file, config);
    collect_semantic_graph_predecessor_nodes(index.as_ref(), &node)
}

#[salsa::tracked]
fn tracked_file_semantic_tag_properties(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Vec<SalsaDocTagPropertySummary> {
    tracked_file_semantic_summary(db, file, config)
        .file_tag_properties
        .clone()
}

#[salsa::tracked]
fn tracked_file_semantic_required_modules(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Vec<SmolStr> {
    tracked_file_semantic_summary(db, file, config)
        .required_modules
        .clone()
}

#[salsa::tracked]
fn tracked_file_semantic_module_export(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Option<SalsaModuleExportSemanticSummary> {
    tracked_file_semantic_summary(db, file, config)
        .module_export
        .clone()
}

#[salsa::tracked]
fn tracked_file_semantic_module_export_query(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Option<crate::SalsaModuleExportQuerySummary> {
    if let Some(component_summary) =
        tracked_file_semantic_module_export_component_summary(db, file, config)
    {
        return Some(crate::SalsaModuleExportQuerySummary {
            export_target: component_summary.export_target,
            export: component_summary.export,
            semantic_target: component_summary.semantic_target,
            doc_owners: component_summary.doc_owners,
            tag_properties: component_summary.tag_properties,
            properties: component_summary.properties,
            state: component_summary.state,
        });
    }

    build_module_export_query_summary_base(db, file, config)
}

#[salsa::tracked]
fn tracked_file_semantic_target_query_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaSemanticTargetQueryIndex> {
    let summary = tracked_file_semantic_summary(db, file, config);
    Arc::new(build_semantic_target_query_index(summary.as_ref()))
}

#[salsa::tracked]
fn tracked_file_semantic_decl(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    decl_id: SalsaDeclId,
) -> Option<SalsaSemanticTargetInfoSummary> {
    let index = tracked_file_semantic_target_query_index(db, file, config);
    find_semantic_decl_in_index(index.as_ref(), decl_id)
}

#[salsa::tracked]
fn tracked_file_semantic_member(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    member_target: SalsaMemberTargetId,
) -> Option<SalsaSemanticTargetInfoSummary> {
    let index = tracked_file_semantic_target_query_index(db, file, config);
    find_semantic_member_in_index(index.as_ref(), &member_target)
}

#[salsa::tracked]
fn tracked_file_semantic_signature(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    signature_offset: TextSize,
) -> Option<SalsaSemanticTargetInfoSummary> {
    let index = tracked_file_semantic_target_query_index(db, file, config);
    find_semantic_signature_in_index(index.as_ref(), signature_offset)
}

#[salsa::tracked]
fn tracked_file_semantic_signature_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    signature_offset: TextSize,
) -> Option<SalsaSemanticSignatureSummary> {
    let semantic_target = tracked_file_semantic_signature(db, file, config, signature_offset)?;
    let explain = tracked_file_signature_explain(db, file, config, signature_offset)?;
    let return_summary =
        tracked_file_semantic_signature_return_summary(db, file, config, signature_offset);

    Some(SalsaSemanticSignatureSummary {
        signature: explain.signature.clone(),
        doc_owners: semantic_target.doc_owners,
        tag_properties: semantic_target.tag_properties,
        properties: semantic_target.properties,
        explain,
        return_summary,
    })
}

#[salsa::tracked]
fn tracked_file_doc_owner_resolve(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    owner_offset: TextSize,
) -> Option<SalsaDocOwnerResolveSummary> {
    let resolve_index = tracked_file_doc_owner_resolve_index(db, file, config);
    find_doc_owner_resolve_at(&resolve_index, owner_offset)
}

#[salsa::tracked]
fn tracked_file_doc_owner_resolves_for_decl(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    decl_id: SalsaDeclId,
) -> Vec<SalsaDocOwnerResolveSummary> {
    let resolve_index = tracked_file_doc_owner_resolve_index(db, file, config);
    collect_doc_owner_resolves_for_decl(&resolve_index, decl_id)
}

#[salsa::tracked]
fn tracked_file_doc_owner_resolves_for_signature(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    signature_offset: TextSize,
) -> Vec<SalsaDocOwnerResolveSummary> {
    let resolve_index = tracked_file_doc_owner_resolve_index(db, file, config);
    collect_doc_owner_resolves_for_signature(&resolve_index, signature_offset)
}

#[salsa::tracked]
fn tracked_file_member_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaMemberIndexSummary> {
    Arc::new(tracked_file_decl_analysis(db, file, config).members)
}

#[salsa::tracked]
fn tracked_file_decl_by_syntax_id(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<crate::SalsaDeclSummary> {
    let decl_tree = tracked_file_decl_tree_summary(db, file, config);
    decl_tree
        .decls
        .iter()
    .find(|decl| decl.syntax_id == Some(syntax_id))
    .cloned()
}

#[salsa::tracked]
fn tracked_file_member_by_syntax_id(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaMemberSummary> {
    let members = tracked_file_member_summary(db, file, config);
    members
        .members
        .iter()
    .find(|member| member.syntax_id == syntax_id)
    .cloned()
}

#[salsa::tracked]
fn tracked_file_property_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaPropertyIndexSummary> {
    let parsed = parse_chunk(file.file_id(db), &file.text(db), &config.config(db));
    let decl_tree = tracked_file_decl_tree_summary(db, file, config);
    let members = tracked_file_member_summary(db, file, config);
    Arc::new(analyze_property_summary(
        decl_tree.as_ref(),
        members.as_ref(),
        parsed.value,
    ))
}

#[salsa::tracked]
fn tracked_file_table_shape_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaTableShapeIndexSummary> {
    let parsed = parse_chunk(file.file_id(db), &file.text(db), &config.config(db));
    Arc::new(analyze_table_shape_summary(parsed.value))
}

#[salsa::tracked]
fn tracked_file_property_query_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaPropertyQueryIndex> {
    let properties = tracked_file_property_summary(db, file, config);
    Arc::new(build_property_query_index(properties.as_ref()))
}

#[salsa::tracked]
fn tracked_file_property_at(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_offset: TextSize,
) -> Option<SalsaPropertySummary> {
    let index = tracked_file_property_query_index(db, file, config);
    find_property_at_in_index(index.as_ref(), syntax_offset)
}

#[salsa::tracked]
fn tracked_file_property_by_value_expr_syntax_id(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaPropertySummary> {
    let index = tracked_file_property_query_index(db, file, config);
    find_property_by_value_expr_syntax_id_in_index(index.as_ref(), syntax_id)
}

#[salsa::tracked]
fn tracked_file_properties_for_decl(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    decl_id: SalsaDeclId,
) -> Vec<SalsaPropertySummary> {
    let index = tracked_file_property_query_index(db, file, config);
    collect_properties_for_decl_in_index(index.as_ref(), decl_id)
}

#[salsa::tracked]
fn tracked_file_properties_for_type(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    type_name: SmolStr,
) -> Vec<SalsaPropertySummary> {
    let index = tracked_file_property_query_index(db, file, config);
    collect_properties_for_type_in_index(index.as_ref(), type_name.as_str())
}

#[salsa::tracked]
fn tracked_file_module_resolve_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaModuleResolveIndex> {
    let decl_tree = tracked_file_decl_tree_summary(db, file, config);
    let globals = tracked_file_global_summary(db, file, config);
    let members = tracked_file_member_summary(db, file, config);
    Arc::new(build_module_resolve_index(
        decl_tree.as_ref(),
        globals.as_ref(),
        members.as_ref(),
    ))
}

#[salsa::tracked]
fn tracked_file_module_export_target(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Option<SalsaExportTargetSummary> {
    let module = tracked_file_module_summary(db, file, config)?;
    find_module_export_target(&module)
}

#[salsa::tracked]
fn tracked_file_module_export(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Option<SalsaModuleExportSummary> {
    let module = tracked_file_module_summary(db, file, config)?;
    find_module_export(&module)
}

#[salsa::tracked]
fn tracked_file_module_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Option<Arc<SalsaModuleSummary>> {
    let parsed = parse_chunk(file.file_id(db), &file.text(db), &config.config(db));
    let resolve_index = tracked_file_module_resolve_index(db, file, config);
    build_module_summary_with_index(file.file_id(db), resolve_index.as_ref(), parsed.value)
        .map(Arc::new)
}

#[salsa::tracked]
fn tracked_file_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    _workspaces: SummaryWorkspaceInput,
    config: SummaryConfigInput,
) -> Arc<SalsaFileSummary> {
    let parsed = parse_chunk(file.file_id(db), &file.text(db), &config.config(db));
    Arc::new(analyze_file_summary(file.file_id(db), parsed.value))
}

fn file_and_config(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<(SummarySourceFileInput, SummaryConfigInput)> {
    Some((file_input(db, file_id)?, config_input(db)?))
}

fn file_workspace_and_config(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<(
    SummarySourceFileInput,
    SummaryWorkspaceInput,
    SummaryConfigInput,
)> {
    Some((
        file_input(db, file_id)?,
        workspace_input(db)?,
        config_input(db)?,
    ))
}

pub(crate) fn file_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaFileSummary>> {
    let (file, workspaces, config) = file_workspace_and_config(db, file_id)?;
    Some(tracked_file_summary(db, file, workspaces, config))
}

pub(crate) fn file_decl_tree_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaDeclTreeSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_decl_tree_summary(db, file, config))
}

pub(crate) fn file_global_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaGlobalSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_global_summary(db, file, config))
}

pub(crate) fn file_doc_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaDocSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_doc_summary(db, file, config))
}

pub(crate) fn file_member_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaMemberIndexSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_member_summary(db, file, config))
}

pub(crate) fn file_decl_by_syntax_id(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<crate::SalsaDeclSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_decl_by_syntax_id(db, file, config, syntax_id)
}

pub(crate) fn file_member_by_syntax_id(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaMemberSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_member_by_syntax_id(db, file, config, syntax_id)
}

pub(crate) fn file_property_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaPropertyIndexSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_property_summary(db, file, config))
}

pub(crate) fn file_table_shape_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaTableShapeIndexSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_table_shape_summary(db, file, config))
}

fn file_property_query_index(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaPropertyQueryIndex>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_property_query_index(db, file, config))
}

fn file_doc_tag_query_index(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaDocTagQueryIndex>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_doc_tag_query_index(db, file, config))
}

pub(crate) fn file_module_resolve_index(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaModuleResolveIndex>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_module_resolve_index(db, file, config))
}

pub(crate) fn file_property_at(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_offset: TextSize,
) -> Option<SalsaPropertySummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_property_at(db, file, config, syntax_offset)
}

pub(crate) fn file_property_by_value_expr_syntax_id(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaPropertySummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_property_by_value_expr_syntax_id(db, file, config, syntax_id)
}

pub(crate) fn file_properties_for_decl(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    decl_id: SalsaDeclId,
) -> Option<Vec<SalsaPropertySummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_properties_for_decl(db, file, config, decl_id))
}

pub(crate) fn file_properties_for_type(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    type_name: SmolStr,
) -> Option<Vec<SalsaPropertySummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_properties_for_type(
        db, file, config, type_name,
    ))
}

pub(crate) fn file_properties_for_member(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    member_target: SalsaMemberTargetId,
) -> Option<Vec<SalsaPropertySummary>> {
    let index = file_property_query_index(db, file_id)?;
    Some(collect_properties_for_member_in_index(
        &index,
        &member_target,
    ))
}

pub(crate) fn file_properties_for_key(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    key: SalsaPropertyKeySummary,
) -> Option<Vec<SalsaPropertySummary>> {
    let index = file_property_query_index(db, file_id)?;
    Some(collect_properties_for_key_in_index(&index, &key))
}

pub(crate) fn file_properties_for_source(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    source: SalsaPropertySourceSummary,
) -> Option<Vec<SalsaPropertySummary>> {
    let index = file_property_query_index(db, file_id)?;
    Some(collect_properties_for_source_in_index(&index, &source))
}

pub(crate) fn file_properties_for_decl_and_key(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    decl_id: SalsaDeclId,
    key: SalsaPropertyKeySummary,
) -> Option<Vec<SalsaPropertySummary>> {
    let index = file_property_query_index(db, file_id)?;
    Some(collect_properties_for_decl_and_key_in_index(
        &index, decl_id, &key,
    ))
}

pub(crate) fn file_properties_for_member_and_key(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    member_target: SalsaMemberTargetId,
    key: SalsaPropertyKeySummary,
) -> Option<Vec<SalsaPropertySummary>> {
    let index = file_property_query_index(db, file_id)?;
    Some(collect_properties_for_member_and_key_in_index(
        &index,
        &member_target,
        &key,
    ))
}

pub(crate) fn file_properties_for_type_and_key(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    type_name: SmolStr,
    key: SalsaPropertyKeySummary,
) -> Option<Vec<SalsaPropertySummary>> {
    let index = file_property_query_index(db, file_id)?;
    Some(collect_properties_for_type_and_key_in_index(
        &index,
        type_name.as_str(),
        &key,
    ))
}

pub(crate) fn file_flow_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaFlowSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_flow_summary(db, file, config))
}

pub(crate) fn file_flow_block(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    block_offset: TextSize,
) -> Option<SalsaFlowBlockSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_flow_block(db, file, config, block_offset)
}

pub(crate) fn file_flow_branch(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    branch_offset: TextSize,
) -> Option<SalsaFlowBranchSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_flow_branch(db, file, config, branch_offset)
}

pub(crate) fn file_flow_loop(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    loop_offset: TextSize,
) -> Option<SalsaFlowLoopSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_flow_loop(db, file, config, loop_offset)
}

pub(crate) fn file_flow_return(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    return_offset: TextSize,
) -> Option<SalsaFlowReturnSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_flow_return(db, file, config, return_offset)
}

pub(crate) fn file_flow_break(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    break_offset: TextSize,
) -> Option<SalsaFlowBreakSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_flow_break(db, file, config, break_offset)
}

pub(crate) fn file_flow_goto(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    goto_offset: TextSize,
) -> Option<SalsaFlowGotoSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_flow_goto(db, file, config, goto_offset)
}

pub(crate) fn file_flow_label(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    label_offset: TextSize,
) -> Option<SalsaFlowLabelSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_flow_label(db, file, config, label_offset)
}

pub(crate) fn file_flow_condition(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    condition_node_offset: u32,
) -> Option<SalsaFlowConditionSummary> {
    let flow = file_flow_summary(db, file_id)?;
    flow.conditions
        .iter()
        .find(|condition| condition.node_offset == condition_node_offset)
        .cloned()
}

pub(crate) fn file_flow_query_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaFlowQuerySummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_flow_query_summary(db, file, config))
}

pub(crate) fn file_for_range_iter_query_index(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaForRangeIterQueryIndex>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_for_range_iter_query_index(db, file, config))
}

pub(crate) fn file_for_range_iter_query(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    loop_offset: TextSize,
) -> Option<SalsaForRangeIterQuerySummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_for_range_iter_query(db, file, config, loop_offset)
}

pub(crate) fn file_flow_successors(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    node: SalsaFlowNodeRefSummary,
) -> Option<Vec<SalsaFlowNodeRefSummary>> {
    let query = file_flow_query_summary(db, file_id)?;
    Some(collect_successor_nodes(query.as_ref(), &node))
}

pub(crate) fn file_flow_outgoing_edges(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    node: SalsaFlowNodeRefSummary,
) -> Option<Vec<SalsaFlowEdgeSummary>> {
    let query = file_flow_query_summary(db, file_id)?;
    Some(collect_outgoing_edges(query.as_ref(), &node))
}

pub(crate) fn file_flow_predecessors(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    node: SalsaFlowNodeRefSummary,
) -> Option<Vec<SalsaFlowNodeRefSummary>> {
    let query = file_flow_query_summary(db, file_id)?;
    Some(collect_predecessor_nodes(query.as_ref(), &node))
}

pub(crate) fn file_flow_incoming_edges(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    node: SalsaFlowNodeRefSummary,
) -> Option<Vec<SalsaFlowEdgeSummary>> {
    let query = file_flow_query_summary(db, file_id)?;
    Some(collect_incoming_edges(query.as_ref(), &node))
}

pub(crate) fn file_flow_reachable_nodes(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    start: SalsaFlowNodeRefSummary,
) -> Option<Vec<SalsaFlowNodeRefSummary>> {
    let query = file_flow_query_summary(db, file_id)?;
    Some(collect_reachable_nodes(query.as_ref(), &start))
}

pub(crate) fn file_flow_can_reach(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    from: SalsaFlowNodeRefSummary,
    to: SalsaFlowNodeRefSummary,
) -> Option<bool> {
    let query = file_flow_query_summary(db, file_id)?;
    Some(can_reach_node(query.as_ref(), &from, &to))
}

pub(crate) fn file_flow_condition_graph(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    condition_node_offset: u32,
) -> Option<SalsaFlowConditionGraphSummary> {
    let flow = file_flow_summary(db, file_id)?;
    let query = file_flow_query_summary(db, file_id)?;
    build_condition_graph_summary(flow.as_ref(), query.as_ref(), condition_node_offset)
}

pub(crate) fn file_flow_branch_graph(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    branch_offset: TextSize,
) -> Option<SalsaFlowBranchGraphSummary> {
    let flow = file_flow_summary(db, file_id)?;
    let query = file_flow_query_summary(db, file_id)?;
    build_branch_graph_summary(flow.as_ref(), query.as_ref(), branch_offset)
}

pub(crate) fn file_flow_loop_graph(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    loop_offset: TextSize,
) -> Option<SalsaFlowLoopGraphSummary> {
    let flow = file_flow_summary(db, file_id)?;
    let query = file_flow_query_summary(db, file_id)?;
    build_loop_graph_summary(flow.as_ref(), query.as_ref(), loop_offset)
}

pub(crate) fn file_flow_return_graph(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    return_offset: TextSize,
) -> Option<SalsaFlowTerminalGraphSummary> {
    file_flow_return(db, file_id, return_offset)?;
    let query = file_flow_query_summary(db, file_id)?;
    Some(build_terminal_graph_summary(
        query.as_ref(),
        SalsaFlowNodeRefSummary::Return(return_offset),
    ))
}

pub(crate) fn file_flow_break_graph(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    break_offset: TextSize,
) -> Option<SalsaFlowTerminalGraphSummary> {
    file_flow_break(db, file_id, break_offset)?;
    let query = file_flow_query_summary(db, file_id)?;
    Some(build_terminal_graph_summary(
        query.as_ref(),
        SalsaFlowNodeRefSummary::Break(break_offset),
    ))
}

pub(crate) fn file_flow_goto_graph(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    goto_offset: TextSize,
) -> Option<SalsaFlowTerminalGraphSummary> {
    file_flow_goto(db, file_id, goto_offset)?;
    let query = file_flow_query_summary(db, file_id)?;
    Some(build_terminal_graph_summary(
        query.as_ref(),
        SalsaFlowNodeRefSummary::Goto(goto_offset),
    ))
}

pub(crate) fn file_semantic_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaSingleFileSemanticSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_summary(db, file, config))
}

pub(crate) fn file_semantic_graph(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaSemanticGraphSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_graph(db, file, config))
}

pub(crate) fn file_semantic_graph_query_index(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaSemanticGraphQueryIndex>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_graph_query_index(db, file, config))
}

pub(crate) fn file_semantic_graph_scc_index(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaSemanticGraphSccIndex>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_graph_scc_index(db, file, config))
}

pub(crate) fn file_semantic_graph_scc_component(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    node: SalsaSemanticGraphNodeSummary,
) -> Option<SalsaSemanticGraphSccComponentSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_graph_scc_component(db, file, config, node)
}

pub(crate) fn file_semantic_graph_scc_component_by_id(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    component_id: usize,
) -> Option<SalsaSemanticGraphSccComponentSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_graph_scc_component_by_id(db, file, config, component_id)
}

pub(crate) fn file_semantic_graph_scc_successors(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    component_id: usize,
) -> Option<Vec<SalsaSemanticGraphSccComponentSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_graph_scc_successors(
        db,
        file,
        config,
        component_id,
    ))
}

pub(crate) fn file_semantic_graph_scc_predecessors(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    component_id: usize,
) -> Option<Vec<SalsaSemanticGraphSccComponentSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_graph_scc_predecessors(
        db,
        file,
        config,
        component_id,
    ))
}

pub(crate) fn file_semantic_solver_worklist(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaSemanticSolverWorklistSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_solver_worklist(db, file, config))
}

pub(crate) fn file_semantic_solver_execution(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaSemanticSolverExecutionSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_solver_execution(db, file, config))
}

pub(crate) fn file_semantic_solver_execution_task(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    component_id: usize,
) -> Option<crate::SalsaSemanticSolverExecutionTaskSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_solver_execution_task(db, file, config, component_id)
}

pub(crate) fn file_semantic_solver_next_ready_execution_task(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<crate::SalsaSemanticSolverExecutionTaskSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_solver_next_ready_execution_task(db, file, config)
}

pub(crate) fn file_semantic_solver_execution_is_complete(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<bool> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_solver_execution_is_complete(
        db, file, config,
    ))
}

pub(crate) fn file_semantic_solver_step(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<crate::SalsaSemanticSolverStepSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_solver_step(db, file, config)
}

pub(crate) fn file_semantic_solver_task(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    component_id: usize,
) -> Option<crate::SalsaSemanticSolverComponentTaskSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_solver_task(db, file, config, component_id)
}

pub(crate) fn file_semantic_solver_ready_tasks(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Vec<crate::SalsaSemanticSolverComponentTaskSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_solver_ready_tasks(db, file, config))
}

pub(crate) fn file_semantic_signature_return_value_shell(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    signature_offset: TextSize,
) -> Option<SalsaSemanticValueShellSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_signature_return_value_shell(db, file, config, signature_offset)
}

pub(crate) fn file_semantic_signature_return_component_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    component_id: usize,
) -> Option<SalsaSemanticSignatureReturnComponentSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_signature_return_component_summary(db, file, config, component_id)
}

pub(crate) fn file_semantic_signature_return_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    signature_offset: TextSize,
) -> Option<SalsaSemanticSignatureReturnSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_signature_return_summary(db, file, config, signature_offset)
}

pub(crate) fn file_semantic_signature_return_component_result_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    signature_offset: TextSize,
) -> Option<SalsaSemanticSolverComponentResultSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_signature_return_component_result_summary(
        db,
        file,
        config,
        signature_offset,
    )
}

pub(crate) fn file_semantic_decl_value_shell(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    decl_id: crate::SalsaDeclId,
) -> Option<SalsaSemanticValueShellSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_decl_value_shell(
        db, file, config, decl_id,
    ))
}

pub(crate) fn file_semantic_decl_component_result_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    decl_id: crate::SalsaDeclId,
) -> Option<SalsaSemanticSolverComponentResultSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_decl_component_result_summary(db, file, config, decl_id)
}

pub(crate) fn file_semantic_decl_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    decl_id: crate::SalsaDeclId,
) -> Option<SalsaSemanticDeclSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_decl_summary(db, file, config, decl_id)
}

pub(crate) fn file_semantic_decl_summary_by_syntax_id(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaSemanticDeclSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    let decl = tracked_file_decl_by_syntax_id(db, file, config, syntax_id)?;
    tracked_file_semantic_decl_summary(db, file, config, decl.id)
}

pub(crate) fn file_semantic_member_value_shell(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    member_target: crate::SalsaMemberTargetId,
) -> Option<SalsaSemanticValueShellSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_member_value_shell(
        db,
        file,
        config,
        member_target,
    ))
}

pub(crate) fn file_semantic_member_component_result_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    member_target: crate::SalsaMemberTargetId,
) -> Option<SalsaSemanticSolverComponentResultSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_member_component_result_summary(db, file, config, member_target)
}

pub(crate) fn file_semantic_member_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    member_target: crate::SalsaMemberTargetId,
) -> Option<SalsaSemanticMemberSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_member_summary(db, file, config, member_target)
}

pub(crate) fn file_semantic_member_summary_by_syntax_id(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaSemanticMemberSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    let member = tracked_file_member_by_syntax_id(db, file, config, syntax_id)?;
    tracked_file_semantic_member_summary(db, file, config, member.target)
}

pub(crate) fn file_semantic_solver_component_result_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    component_id: usize,
) -> Option<SalsaSemanticSolverComponentResultSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_solver_component_result_summary(db, file, config, component_id)
}

pub(crate) fn file_semantic_for_range_iter_value_shell(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    loop_offset: TextSize,
) -> Option<SalsaSemanticValueShellSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_for_range_iter_value_shell(db, file, config, loop_offset)
}

pub(crate) fn file_semantic_for_range_iter_component_result_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    loop_offset: TextSize,
) -> Option<SalsaSemanticSolverComponentResultSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_for_range_iter_component_result_summary(db, file, config, loop_offset)
}

pub(crate) fn file_semantic_for_range_iter_component_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    loop_offset: TextSize,
) -> Option<SalsaSemanticForRangeIterComponentSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_for_range_iter_component_summary(db, file, config, loop_offset)
}

pub(crate) fn file_semantic_module_export_value_shell(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<SalsaSemanticValueShellSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_module_export_value_shell(
        db, file, config,
    ))
}

pub(crate) fn file_semantic_module_export_component_result_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<SalsaSemanticSolverComponentResultSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_module_export_component_result_summary(db, file, config)
}

pub(crate) fn file_semantic_module_export_component_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<SalsaSemanticModuleExportComponentSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_module_export_component_summary(db, file, config)
}

pub(crate) fn file_semantic_graph_outgoing_edges(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    node: SalsaSemanticGraphNodeSummary,
) -> Option<Vec<SalsaSemanticGraphEdgeSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_graph_outgoing_edges(
        db, file, config, node,
    ))
}

pub(crate) fn file_semantic_graph_incoming_edges(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    node: SalsaSemanticGraphNodeSummary,
) -> Option<Vec<SalsaSemanticGraphEdgeSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_graph_incoming_edges(
        db, file, config, node,
    ))
}

pub(crate) fn file_semantic_graph_successors(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    node: SalsaSemanticGraphNodeSummary,
) -> Option<Vec<SalsaSemanticGraphNodeSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_graph_successors(
        db, file, config, node,
    ))
}

pub(crate) fn file_semantic_graph_predecessors(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    node: SalsaSemanticGraphNodeSummary,
) -> Option<Vec<SalsaSemanticGraphNodeSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_graph_predecessors(
        db, file, config, node,
    ))
}

pub(crate) fn file_semantic_tag_properties(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Vec<SalsaDocTagPropertySummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_tag_properties(db, file, config))
}

pub(crate) fn file_semantic_required_modules(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Vec<SmolStr>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_required_modules(db, file, config))
}

pub(crate) fn file_semantic_module_export(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaModuleExportSemanticSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_module_export(db, file, config).map(Arc::new)
}

pub(crate) fn file_semantic_module_export_query(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<crate::SalsaModuleExportQuerySummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_module_export_query(db, file, config)
}

pub(crate) fn file_semantic_target_query_index(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaSemanticTargetQueryIndex>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_semantic_target_query_index(db, file, config))
}

pub(crate) fn file_semantic_decl(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    decl_id: SalsaDeclId,
) -> Option<SalsaSemanticTargetInfoSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_decl(db, file, config, decl_id)
}

pub(crate) fn file_semantic_member(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    member_target: SalsaMemberTargetId,
) -> Option<SalsaSemanticTargetInfoSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_member(db, file, config, member_target)
}

pub(crate) fn file_semantic_signature(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    signature_offset: TextSize,
) -> Option<SalsaSemanticTargetInfoSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_signature(db, file, config, signature_offset)
}

pub(crate) fn file_semantic_signature_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    signature_offset: TextSize,
) -> Option<SalsaSemanticSignatureSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_semantic_signature_summary(db, file, config, signature_offset)
}

pub(crate) fn file_doc_owner_resolve(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    owner_offset: TextSize,
) -> Option<SalsaDocOwnerResolveSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_doc_owner_resolve(db, file, config, owner_offset)
}

pub(crate) fn file_doc_owner_resolves_for_decl(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    decl_id: SalsaDeclId,
) -> Option<Vec<SalsaDocOwnerResolveSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_doc_owner_resolves_for_decl(
        db, file, config, decl_id,
    ))
}

pub(crate) fn file_doc_owner_resolves_for_member(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    member_target: SalsaMemberTargetId,
) -> Option<Vec<SalsaDocOwnerResolveSummary>> {
    let resolve_index = file_doc_owner_resolve_index(db, file_id)?;
    Some(collect_doc_owner_resolves_for_member(
        &resolve_index,
        &member_target,
    ))
}

pub(crate) fn file_doc_owner_resolves_for_signature(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    signature_offset: TextSize,
) -> Option<Vec<SalsaDocOwnerResolveSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_doc_owner_resolves_for_signature(
        db,
        file,
        config,
        signature_offset,
    ))
}

pub(crate) fn file_module_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaModuleSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_module_summary(db, file, config)
}

pub(crate) fn file_module_export_target(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<SalsaExportTargetSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_module_export_target(db, file, config)
}

pub(crate) fn file_module_export(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<SalsaModuleExportSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_module_export(db, file, config)
}

pub(crate) fn file_module_exported_global_function(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<SalsaGlobalFunctionSummary> {
    let module = file_module_summary(db, file_id)?;
    find_module_exported_global_function(&module)
}

pub(crate) fn file_module_exported_global_variable(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<SalsaGlobalVariableSummary> {
    let module = file_module_summary(db, file_id)?;
    find_module_exported_global_variable(&module)
}

pub(crate) fn file_module_exports_decl(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    decl_id: SalsaDeclId,
) -> Option<bool> {
    let module = file_module_summary(db, file_id)?;
    Some(module_exports_decl(&module, decl_id))
}

pub(crate) fn file_module_exports_member(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    member_target: SalsaMemberTargetId,
) -> Option<bool> {
    let module = file_module_summary(db, file_id)?;
    Some(module_exports_member(&module, &member_target))
}

pub(crate) fn file_module_exports_closure(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    signature_offset: TextSize,
) -> Option<bool> {
    let module = file_module_summary(db, file_id)?;
    Some(module_exports_closure(&module, signature_offset))
}

pub(crate) fn file_module_exports_table(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    table_offset: TextSize,
) -> Option<bool> {
    let module = file_module_summary(db, file_id)?;
    Some(module_exports_table(&module, table_offset))
}

pub(crate) fn file_module_exports_global_function(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    name: SmolStr,
) -> Option<bool> {
    let module = file_module_summary(db, file_id)?;
    Some(module_exports_global_function(&module, name.as_str()))
}

pub(crate) fn file_module_exports_global_variable(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    name: SmolStr,
) -> Option<bool> {
    let module = file_module_summary(db, file_id)?;
    Some(module_exports_global_variable(&module, name.as_str()))
}
