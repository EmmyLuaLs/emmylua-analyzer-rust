mod doc_owner;
mod doc_tag;
mod doc_type;
mod flow;
mod lexical;
mod module;
mod property;
mod semantic;
mod semantic_graph;
mod semantic_solver;
mod signature;
mod type_system;

pub use doc_owner::{
    SalsaDocOwnerResolutionSummary, SalsaDocOwnerResolveIndex, SalsaDocOwnerResolveSummary,
    build_doc_owner_resolve_index, collect_doc_owner_resolves_for_decl,
    collect_doc_owner_resolves_for_member, collect_doc_owner_resolves_for_signature,
    find_doc_owner_resolve_at,
};
pub use doc_tag::{
    SalsaDocDiagnosticActionKindSummary, SalsaDocDiagnosticActionSummary,
    SalsaDocTagNameContentSummary, SalsaDocTagPropertyEntrySummary, SalsaDocTagPropertySummary,
    SalsaDocTagQueryIndex, SalsaResolvedDocDiagnosticActionSummary, build_doc_tag_query_index,
    build_doc_tag_query_index_from_properties, collect_doc_tag_properties,
    collect_doc_tag_properties_for_owners_in_index, collect_doc_tags_for_kind,
    collect_doc_tags_for_kind_in_index, collect_doc_tags_for_owner,
    collect_doc_tags_for_owner_in_index, collect_ownerless_doc_tag_properties_in_index,
    collect_resolved_doc_tag_diagnostics_for_property, find_doc_tag_at, find_doc_tag_at_in_index,
    find_doc_tag_property, find_doc_tag_property_in_index,
};
pub use doc_type::{
    SalsaDocTypeLoweredGenericParam, SalsaDocTypeLoweredIndex, SalsaDocTypeLoweredKind,
    SalsaDocTypeLoweredNode, SalsaDocTypeLoweredObjectField, SalsaDocTypeLoweredObjectFieldKey,
    SalsaDocTypeLoweredParam, SalsaDocTypeLoweredReturn, SalsaDocTypeRef,
    SalsaDocTypeResolvedIndex, SalsaDocTypeResolvedSummary, build_lowered_doc_type_index,
    build_resolved_doc_type_index, find_lowered_doc_type_at, find_lowered_doc_type_by_key,
    find_resolved_doc_type_at, find_resolved_doc_type_by_key,
    find_resolved_doc_type_by_key_from_parts,
};
pub use flow::{
    build_branch_graph_summary, build_condition_graph_summary, build_flow_query_summary,
    build_for_range_iter_query_index, build_loop_graph_summary, build_terminal_graph_summary,
    can_reach_node, collect_incoming_edges, collect_outgoing_edges, collect_predecessor_nodes,
    collect_reachable_nodes, collect_successor_nodes, find_for_range_iter_query_at,
};
pub use lexical::{
    SalsaLexicalUseIndex, SalsaLexicalUseSummary, build_lexical_use_index,
    collect_call_references_for_member, collect_call_references_for_member_in_index,
    collect_call_references_for_name, collect_call_references_for_name_in_index,
    collect_decl_references, collect_decl_references_in_index, collect_global_name_references,
    collect_global_name_references_in_index, collect_member_references,
    collect_member_references_by_role, collect_member_references_by_role_in_index,
    collect_member_references_in_index, collect_name_references_by_role,
    collect_name_references_by_role_in_index, find_call_use_at, find_call_use_by_syntax_id,
    find_call_use_in_index, find_lexical_use_at, find_member_use_at, find_member_use_by_syntax_id,
    find_name_use_at, find_name_use_by_syntax_id,
};
pub use module::{
    SalsaModuleDeclHandle, SalsaModuleGlobalFunctionHandle, SalsaModuleGlobalVariableHandle,
    SalsaModuleMemberHandle, SalsaModuleMemberPathKey, SalsaModuleNameKey, SalsaModuleResolveIndex,
    build_module_resolve_index, decl_at_handle, find_decl_handle_by_name,
    find_global_function_handle_by_name, find_global_variable_handle_by_name,
    find_member_handle_by_path, find_module_export, find_module_export_target,
    find_module_exported_global_function, find_module_exported_global_variable,
    global_function_at_handle, global_variable_at_handle, member_at_handle, module_exports_closure,
    module_exports_decl, module_exports_global_function, module_exports_global_variable,
    module_exports_member, module_exports_table, resolve_module_export,
    resolve_module_export_in_index,
};
pub use property::{
    SalsaPropertyQueryIndex, build_property_query_index, collect_properties_for_decl,
    collect_properties_for_decl_and_key, collect_properties_for_decl_and_key_in_index,
    collect_properties_for_decl_in_index, collect_properties_for_key,
    collect_properties_for_key_in_index, collect_properties_for_member,
    collect_properties_for_member_and_key, collect_properties_for_member_and_key_in_index,
    collect_properties_for_member_in_index, collect_properties_for_source,
    collect_properties_for_source_in_index, collect_properties_for_type,
    collect_properties_for_type_and_key, collect_properties_for_type_and_key_in_index,
    collect_properties_for_type_in_index, find_property_at, find_property_at_in_index,
    find_property_by_value_expr_syntax_id, find_property_by_value_expr_syntax_id_in_index,
};
pub use semantic::{
    SalsaModuleExportSemanticSummary, SalsaSemanticTargetInfoSummary,
    SalsaSemanticTargetQueryIndex, SalsaSemanticTargetSummary, SalsaSingleFileSemanticSummary,
    build_semantic_target_query_index, build_single_file_semantic_summary,
    find_module_export_semantic, find_semantic_decl, find_semantic_decl_in_index,
    find_semantic_member, find_semantic_member_in_index, find_semantic_signature,
    find_semantic_signature_in_index,
};
pub use semantic_graph::{
    SalsaSemanticGraphQueryIndex, SalsaSemanticGraphSccComponentSummary,
    SalsaSemanticGraphSccIndex, build_semantic_graph_query_index, build_semantic_graph_scc_index,
    build_semantic_graph_summary, collect_incoming_semantic_graph_edges,
    collect_outgoing_semantic_graph_edges, collect_semantic_graph_predecessor_nodes,
    collect_semantic_graph_scc_predecessor_components,
    collect_semantic_graph_scc_successor_components, collect_semantic_graph_successor_nodes,
    find_semantic_graph_scc_component, find_semantic_graph_scc_component_by_id,
};
pub use semantic_solver::{
    build_decl_value_shell, build_for_range_iter_component_summary, build_member_value_shell,
    build_semantic_decl_summary, build_semantic_member_summary,
    build_semantic_solver_component_result_summary, build_semantic_solver_execution,
    build_semantic_solver_step_summary, build_semantic_solver_worklist,
    build_semantic_value_shell_from_candidates, build_semantic_value_shell_from_for_range_iter,
    build_semantic_value_shell_from_for_range_iter_source,
    build_semantic_value_shell_from_member_type, build_semantic_value_shell_from_module_export,
    build_semantic_value_shell_from_signature_return, build_signature_return_component_summary,
    build_signature_return_summary, collect_semantic_solver_ready_tasks,
    collect_signature_return_value_candidate_type_offsets, complete_semantic_solver_execution_task,
    complete_semantic_solver_execution_task_with_result,
    find_next_ready_semantic_solver_execution_task, find_semantic_solver_component_result,
    find_semantic_solver_execution_task, find_semantic_solver_task,
    for_range_iter_resolve_state_from_semantic, for_range_iter_slot_candidate_type_offsets,
    join_semantic_resolve_states, merge_semantic_value_shells,
    module_export_resolve_state_from_semantic, semantic_resolve_state_from_for_range_iter,
    semantic_resolve_state_from_module_export, semantic_resolve_state_from_signature_return,
    semantic_solver_execution_is_complete, signature_return_resolve_state_from_semantic,
    signature_return_value_candidate_type_offsets, solve_component_fixedpoint,
    step_semantic_solver_execution, step_semantic_solver_execution_with_signature_returns,
};
pub use signature::{
    SalsaCallArgExplainSummary, SalsaCallExplainSummary, SalsaSignatureExplainIndex,
    SalsaSignatureExplainSummary, SalsaSignatureGenericExplainSummary,
    SalsaSignatureGenericParamExplainSummary, SalsaSignatureOperatorExplainSummary,
    SalsaSignatureParamExplainSummary, SalsaSignatureReturnExplainSummary,
    SalsaSignatureReturnItemExplainSummary, SalsaSignatureReturnQueryIndex,
    SalsaSignatureReturnQuerySummary, SalsaSignatureTypeExplainSummary,
    build_signature_explain_index, build_signature_return_query_index, call_explain_is_resolved,
    find_call_explain_at, find_call_explain_by_syntax_id, find_signature_explain_at,
    find_signature_return_query_at,
};
pub use type_system::{
    SalsaDeclTypeInfoSummary, SalsaDeclTypeQueryIndex, SalsaGlobalTypeInfoSummary,
    SalsaGlobalTypeQueryIndex, SalsaLocalAssignmentQueryIndex, SalsaLocalAssignmentSummary,
    SalsaMemberTypeInfoSummary, SalsaMemberTypeQueryIndex, SalsaNameTypeInfoSummary,
    SalsaProgramPointMemberTypeInfoSummary, SalsaProgramPointTypeInfoSummary,
    SalsaTypeCandidateOriginSummary, SalsaTypeCandidateSummary, SalsaTypeNarrowSummary,
    build_decl_type_query_index, build_global_type_query_index, build_local_assignment_query_index,
    build_member_type_query_index, collect_active_type_narrows, find_decl_type_info,
    find_global_name_type_info, find_global_type_info, find_latest_local_assignment,
    find_member_type_at_program_point, find_member_type_info, find_member_use_type_info,
    find_name_type_at_program_point, find_name_type_info,
};
