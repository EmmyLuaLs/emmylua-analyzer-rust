use std::collections::BTreeSet;

use emmylua_parser::LuaSyntaxKind;

use super::{
    SalsaDeclTypeInfoSummary, SalsaDeclTypeQueryIndex, SalsaMemberTypeInfoSummary,
    SalsaMemberTypeQueryIndex, SalsaModuleExportSemanticSummary, SalsaSignatureExplainIndex,
    SalsaSignatureReturnQuerySummary, SalsaTypeCandidateSummary, call_explain_is_resolved,
    find_call_explain_at, find_call_explain_by_syntax_id, find_decl_type_info,
    find_for_range_iter_query_at, find_member_type_info, find_member_use_at, find_name_use_at,
    find_signature_return_query_at,
};
use crate::{
    SalsaDocTypeRef, SalsaForRangeIterQueryIndex, SalsaForRangeIterQuerySummary,
    SalsaForRangeIterResolveStateSummary, SalsaNameUseResolutionSummary, SalsaSemanticDeclSummary,
    SalsaSemanticForRangeIterComponentSummary, SalsaSemanticGraphNodeSummary,
    SalsaSemanticGraphSccComponentSummary, SalsaSemanticGraphSccIndex, SalsaSemanticMemberSummary,
    SalsaSemanticResolveStateSummary, SalsaSemanticSignatureReturnComponentSummary,
    SalsaSemanticSignatureReturnSummary, SalsaSemanticSolverComponentResultSummary,
    SalsaSemanticSolverComponentTaskSummary, SalsaSemanticSolverExecutionSummary,
    SalsaSemanticSolverExecutionTaskSummary, SalsaSemanticSolverStepSummary,
    SalsaSemanticSolverTaskStateSummary, SalsaSemanticSolverWorklistSummary,
    SalsaSemanticValueShellSummary, SalsaSignatureReturnExprKindSummary,
    SalsaSignatureReturnQueryIndex, SalsaSignatureReturnResolveStateSummary,
    SalsaSignatureReturnValueSummary, SalsaUseSiteIndexSummary,
};

fn merge_solver_value_shells(
    shells: &[SalsaSemanticValueShellSummary],
) -> SalsaSemanticValueShellSummary {
    let mut candidate_type_offsets = shells
        .iter()
        .flat_map(|shell| shell.candidate_type_offsets.iter().copied())
        .collect::<Vec<_>>();
    candidate_type_offsets.sort();
    candidate_type_offsets.dedup();

    let state = if shells.iter().any(|shell| {
        matches!(
            shell.state,
            SalsaSemanticResolveStateSummary::RecursiveDependency
        )
    }) {
        SalsaSemanticResolveStateSummary::RecursiveDependency
    } else if shells
        .iter()
        .any(|shell| matches!(shell.state, SalsaSemanticResolveStateSummary::Resolved))
    {
        SalsaSemanticResolveStateSummary::Resolved
    } else if shells
        .iter()
        .any(|shell| matches!(shell.state, SalsaSemanticResolveStateSummary::Partial))
    {
        SalsaSemanticResolveStateSummary::Partial
    } else {
        SalsaSemanticResolveStateSummary::Unknown
    };

    SalsaSemanticValueShellSummary {
        state,
        candidate_type_offsets,
    }
}

pub fn build_semantic_solver_worklist(
    scc_index: &SalsaSemanticGraphSccIndex,
) -> SalsaSemanticSolverWorklistSummary {
    let tasks = scc_index
        .components
        .iter()
        .map(|component| SalsaSemanticSolverComponentTaskSummary {
            component_id: component.component_id,
            predecessor_component_ids: component.predecessor_component_ids.clone(),
            successor_component_ids: component.successor_component_ids.clone(),
            is_cycle: component.is_cycle,
        })
        .collect::<Vec<_>>();
    let ready_component_ids = tasks
        .iter()
        .filter(|task| task.predecessor_component_ids.is_empty())
        .map(|task| task.component_id)
        .collect::<Vec<_>>();

    SalsaSemanticSolverWorklistSummary {
        tasks,
        ready_component_ids,
        topo_order: scc_index.topo_order.clone(),
    }
}

pub fn find_semantic_solver_task(
    worklist: &SalsaSemanticSolverWorklistSummary,
    component_id: usize,
) -> Option<SalsaSemanticSolverComponentTaskSummary> {
    worklist
        .tasks
        .iter()
        .find(|task| task.component_id == component_id)
        .cloned()
}

pub fn collect_semantic_solver_ready_tasks(
    worklist: &SalsaSemanticSolverWorklistSummary,
) -> Vec<SalsaSemanticSolverComponentTaskSummary> {
    worklist
        .ready_component_ids
        .iter()
        .filter_map(|component_id| find_semantic_solver_task(worklist, *component_id))
        .collect()
}

pub fn build_semantic_solver_execution(
    worklist: &SalsaSemanticSolverWorklistSummary,
) -> SalsaSemanticSolverExecutionSummary {
    SalsaSemanticSolverExecutionSummary {
        tasks: worklist
            .tasks
            .iter()
            .map(|task| SalsaSemanticSolverExecutionTaskSummary {
                component_id: task.component_id,
                state: if task.predecessor_component_ids.is_empty() {
                    SalsaSemanticSolverTaskStateSummary::Ready
                } else {
                    SalsaSemanticSolverTaskStateSummary::Blocked
                },
                pending_predecessor_component_ids: task.predecessor_component_ids.clone(),
                successor_component_ids: task.successor_component_ids.clone(),
                is_cycle: task.is_cycle,
            })
            .collect(),
        ready_component_ids: worklist.ready_component_ids.clone(),
        completed_component_ids: Vec::new(),
        component_results: Vec::new(),
    }
}

pub fn find_semantic_solver_component_result(
    execution: &SalsaSemanticSolverExecutionSummary,
    component_id: usize,
) -> Option<SalsaSemanticSolverComponentResultSummary> {
    execution
        .component_results
        .iter()
        .find(|result| result.component_id == component_id)
        .cloned()
}

pub fn find_semantic_solver_execution_task(
    execution: &SalsaSemanticSolverExecutionSummary,
    component_id: usize,
) -> Option<SalsaSemanticSolverExecutionTaskSummary> {
    execution
        .tasks
        .iter()
        .find(|task| task.component_id == component_id)
        .cloned()
}

pub fn find_next_ready_semantic_solver_execution_task(
    execution: &SalsaSemanticSolverExecutionSummary,
) -> Option<SalsaSemanticSolverExecutionTaskSummary> {
    execution
        .ready_component_ids
        .iter()
        .copied()
        .min()
        .and_then(|component_id| find_semantic_solver_execution_task(execution, component_id))
}

pub fn semantic_solver_execution_is_complete(
    execution: &SalsaSemanticSolverExecutionSummary,
) -> bool {
    execution
        .tasks
        .iter()
        .all(|task| matches!(task.state, SalsaSemanticSolverTaskStateSummary::Completed))
}

pub fn step_semantic_solver_execution(
    execution: &SalsaSemanticSolverExecutionSummary,
) -> Option<SalsaSemanticSolverStepSummary> {
    let next_ready_task = find_next_ready_semantic_solver_execution_task(execution)?;
    Some(SalsaSemanticSolverStepSummary {
        completed_component_id: next_ready_task.component_id,
        component_result: None,
        next_execution: complete_semantic_solver_execution_task(
            execution,
            next_ready_task.component_id,
        ),
    })
}

pub fn build_semantic_solver_step_summary(
    execution: &SalsaSemanticSolverExecutionSummary,
    component_id: usize,
    scc_index: &SalsaSemanticGraphSccIndex,
    decl_types: &SalsaDeclTypeQueryIndex,
    member_types: &SalsaMemberTypeQueryIndex,
    signature_returns: &SalsaSignatureReturnQueryIndex,
    for_range_iters: &SalsaForRangeIterQueryIndex,
    use_sites: &SalsaUseSiteIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    module_export: Option<&SalsaModuleExportSemanticSummary>,
) -> Option<SalsaSemanticSolverStepSummary> {
    let component = scc_index
        .components
        .iter()
        .find(|component| component.component_id == component_id)?;
    let predecessor_results = component
        .predecessor_component_ids
        .iter()
        .filter_map(|predecessor_component_id| {
            find_semantic_solver_component_result(execution, *predecessor_component_id)
        })
        .collect::<Vec<_>>();
    let component_result = build_semantic_solver_component_result_summary(
        component,
        decl_types,
        member_types,
        signature_returns,
        for_range_iters,
        use_sites,
        signature_explain_index,
        module_export,
        &predecessor_results,
    );

    Some(SalsaSemanticSolverStepSummary {
        completed_component_id: component_id,
        component_result: Some(component_result.clone()),
        next_execution: complete_semantic_solver_execution_task_with_result(
            execution,
            component_id,
            component_result,
        ),
    })
}

pub fn step_semantic_solver_execution_with_signature_returns(
    execution: &SalsaSemanticSolverExecutionSummary,
    scc_index: &SalsaSemanticGraphSccIndex,
    decl_types: &SalsaDeclTypeQueryIndex,
    member_types: &SalsaMemberTypeQueryIndex,
    signature_returns: &SalsaSignatureReturnQueryIndex,
    for_range_iters: &SalsaForRangeIterQueryIndex,
    use_sites: &SalsaUseSiteIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    module_export: Option<&SalsaModuleExportSemanticSummary>,
) -> Option<SalsaSemanticSolverStepSummary> {
    let next_ready_task = find_next_ready_semantic_solver_execution_task(execution)?;
    build_semantic_solver_step_summary(
        execution,
        next_ready_task.component_id,
        scc_index,
        decl_types,
        member_types,
        signature_returns,
        for_range_iters,
        use_sites,
        signature_explain_index,
        module_export,
    )
}

fn build_semantic_value_shell_from_type_candidates(
    explicit_type_offsets: Vec<crate::SalsaDocTypeNodeKey>,
    has_partial_evidence: bool,
    has_resolved_evidence: bool,
) -> SalsaSemanticValueShellSummary {
    let state = if !explicit_type_offsets.is_empty() || has_resolved_evidence {
        SalsaSemanticResolveStateSummary::Resolved
    } else if has_partial_evidence {
        SalsaSemanticResolveStateSummary::Partial
    } else {
        SalsaSemanticResolveStateSummary::Unknown
    };

    SalsaSemanticValueShellSummary {
        state,
        candidate_type_offsets: explicit_type_offsets,
    }
}

fn has_stable_initializer_evidence(
    value_expr_syntax_id: Option<crate::SalsaSyntaxIdSummary>,
) -> bool {
    let kind = value_expr_syntax_id.map(|syntax_id| syntax_id.kind);
    matches!(
        kind,
        Some(
            LuaSyntaxKind::LiteralExpr
                | LuaSyntaxKind::ClosureExpr
                | LuaSyntaxKind::TableArrayExpr
                | LuaSyntaxKind::TableObjectExpr
                | LuaSyntaxKind::TableEmptyExpr
        )
    )
}

fn build_transfer_value_shell(
    shell: &SalsaSemanticValueShellSummary,
) -> SalsaSemanticValueShellSummary {
    match shell.state {
        SalsaSemanticResolveStateSummary::Partial if shell.candidate_type_offsets.is_empty() => {
            SalsaSemanticValueShellSummary {
                state: SalsaSemanticResolveStateSummary::Unknown,
                candidate_type_offsets: Vec::new(),
            }
        }
        _ => shell.clone(),
    }
}

fn build_effective_transfer_shell(
    shell: &SalsaSemanticValueShellSummary,
) -> Option<SalsaSemanticValueShellSummary> {
    let shell = build_transfer_value_shell(shell);
    (!matches!(shell.state, SalsaSemanticResolveStateSummary::Unknown)
        || !shell.candidate_type_offsets.is_empty())
    .then_some(shell)
}

fn build_propagated_value_shell(
    input_value_shell: &SalsaSemanticValueShellSummary,
    has_input: bool,
) -> SalsaSemanticValueShellSummary {
    if !has_input {
        return SalsaSemanticValueShellSummary {
            state: SalsaSemanticResolveStateSummary::Unknown,
            candidate_type_offsets: Vec::new(),
        };
    }

    build_effective_transfer_shell(input_value_shell).unwrap_or(SalsaSemanticValueShellSummary {
        state: SalsaSemanticResolveStateSummary::Unknown,
        candidate_type_offsets: Vec::new(),
    })
}

fn apply_component_transfer_step(
    propagated_value_shell: &SalsaSemanticValueShellSummary,
    local_value_shell: &SalsaSemanticValueShellSummary,
    current_value_shell: Option<&SalsaSemanticValueShellSummary>,
) -> SalsaSemanticValueShellSummary {
    let propagated = build_effective_transfer_shell(propagated_value_shell);
    let local = build_effective_transfer_shell(local_value_shell);
    let current = current_value_shell.and_then(build_effective_transfer_shell);

    let next = merge_component_iteration_shells(propagated, local, current);
    build_transfer_value_shell(&next)
}

fn build_semantic_value_shell_from_decl_type(
    decl_type: Option<&SalsaDeclTypeInfoSummary>,
) -> SalsaSemanticValueShellSummary {
    let Some(decl_type) = decl_type else {
        return SalsaSemanticValueShellSummary {
            state: SalsaSemanticResolveStateSummary::Unknown,
            candidate_type_offsets: Vec::new(),
        };
    };

    let mut explicit_type_offsets = decl_type.explicit_type_offsets.clone();
    explicit_type_offsets.sort();
    explicit_type_offsets.dedup();
    let has_partial_evidence =
        decl_type.initializer_offset.is_some() || decl_type.signature_offset.is_some();
    let has_resolved_evidence = !decl_type.explicit_type_offsets.is_empty()
        || !decl_type.named_type_names.is_empty()
        || decl_type.value_signature_offset.is_some()
        || has_stable_initializer_evidence(decl_type.value_expr_syntax_id);
    build_semantic_value_shell_from_type_candidates(
        explicit_type_offsets,
        has_partial_evidence,
        has_resolved_evidence,
    )
}

pub fn build_semantic_value_shell_from_member_type(
    member_type: Option<&SalsaMemberTypeInfoSummary>,
) -> SalsaSemanticValueShellSummary {
    let Some(member_type) = member_type else {
        return SalsaSemanticValueShellSummary {
            state: SalsaSemanticResolveStateSummary::Unknown,
            candidate_type_offsets: Vec::new(),
        };
    };

    build_semantic_value_shell_from_candidates(&member_type.candidates)
}

pub fn build_semantic_value_shell_from_candidates(
    candidates: &[SalsaTypeCandidateSummary],
) -> SalsaSemanticValueShellSummary {
    if candidates.is_empty() {
        return SalsaSemanticValueShellSummary {
            state: SalsaSemanticResolveStateSummary::Unknown,
            candidate_type_offsets: Vec::new(),
        };
    }

    let mut explicit_type_offsets = candidates
        .iter()
        .flat_map(|candidate| candidate.explicit_type_offsets.iter().copied())
        .collect::<Vec<_>>();
    explicit_type_offsets.sort();
    explicit_type_offsets.dedup();
    let has_partial_evidence = candidates
        .iter()
        .any(|candidate| candidate.initializer_offset.is_some());
    let has_resolved_evidence = candidates.iter().any(|candidate| {
        !candidate.explicit_type_offsets.is_empty()
            || !candidate.named_type_names.is_empty()
            || candidate.signature_offset.is_some()
            || has_stable_initializer_evidence(candidate.value_expr_syntax_id)
    });
    build_semantic_value_shell_from_type_candidates(
        explicit_type_offsets,
        has_partial_evidence,
        has_resolved_evidence,
    )
}

pub fn complete_semantic_solver_execution_task(
    execution: &SalsaSemanticSolverExecutionSummary,
    component_id: usize,
) -> SalsaSemanticSolverExecutionSummary {
    complete_semantic_solver_execution_task_with_result(execution, component_id, None)
}

pub fn complete_semantic_solver_execution_task_with_result(
    execution: &SalsaSemanticSolverExecutionSummary,
    component_id: usize,
    component_result: impl Into<Option<SalsaSemanticSolverComponentResultSummary>>,
) -> SalsaSemanticSolverExecutionSummary {
    let mut next = execution.clone();
    if next.completed_component_ids.contains(&component_id) {
        return next;
    }

    let Some(task_index) = next
        .tasks
        .iter()
        .position(|task| task.component_id == component_id)
    else {
        return next;
    };

    next.tasks[task_index].state = SalsaSemanticSolverTaskStateSummary::Completed;
    next.tasks[task_index]
        .pending_predecessor_component_ids
        .clear();
    next.ready_component_ids
        .retain(|ready_component_id| *ready_component_id != component_id);
    next.completed_component_ids.push(component_id);
    next.completed_component_ids.sort_unstable();
    if let Some(component_result) = component_result.into() {
        next.component_results
            .retain(|result| result.component_id != component_id);
        next.component_results.push(component_result);
        next.component_results
            .sort_by_key(|result| result.component_id);
    }

    for task in &mut next.tasks {
        if task.component_id == component_id
            || matches!(task.state, SalsaSemanticSolverTaskStateSummary::Completed)
        {
            continue;
        }

        task.pending_predecessor_component_ids
            .retain(|pending_component_id| *pending_component_id != component_id);
        if task.pending_predecessor_component_ids.is_empty() {
            task.state = SalsaSemanticSolverTaskStateSummary::Ready;
            if !next.ready_component_ids.contains(&task.component_id) {
                next.ready_component_ids.push(task.component_id);
            }
        }
    }

    next.ready_component_ids.sort_unstable();
    next
}

pub fn semantic_resolve_state_from_signature_return(
    state: SalsaSignatureReturnResolveStateSummary,
) -> SalsaSemanticResolveStateSummary {
    match state {
        SalsaSignatureReturnResolveStateSummary::Resolved => {
            SalsaSemanticResolveStateSummary::Resolved
        }
        SalsaSignatureReturnResolveStateSummary::Partial => {
            SalsaSemanticResolveStateSummary::Partial
        }
        SalsaSignatureReturnResolveStateSummary::RecursiveDependency => {
            SalsaSemanticResolveStateSummary::RecursiveDependency
        }
    }
}

pub fn signature_return_resolve_state_from_semantic(
    state: SalsaSemanticResolveStateSummary,
) -> Option<SalsaSignatureReturnResolveStateSummary> {
    match state {
        SalsaSemanticResolveStateSummary::Resolved => {
            Some(SalsaSignatureReturnResolveStateSummary::Resolved)
        }
        SalsaSemanticResolveStateSummary::Partial => {
            Some(SalsaSignatureReturnResolveStateSummary::Partial)
        }
        SalsaSemanticResolveStateSummary::RecursiveDependency => {
            Some(SalsaSignatureReturnResolveStateSummary::RecursiveDependency)
        }
        SalsaSemanticResolveStateSummary::Unknown => None,
    }
}

pub fn semantic_resolve_state_from_for_range_iter(
    state: SalsaForRangeIterResolveStateSummary,
) -> SalsaSemanticResolveStateSummary {
    match state {
        SalsaForRangeIterResolveStateSummary::Resolved => {
            SalsaSemanticResolveStateSummary::Resolved
        }
        SalsaForRangeIterResolveStateSummary::Partial => SalsaSemanticResolveStateSummary::Partial,
        SalsaForRangeIterResolveStateSummary::RecursiveDependency => {
            SalsaSemanticResolveStateSummary::RecursiveDependency
        }
    }
}

pub fn for_range_iter_resolve_state_from_semantic(
    state: SalsaSemanticResolveStateSummary,
) -> Option<SalsaForRangeIterResolveStateSummary> {
    match state {
        SalsaSemanticResolveStateSummary::Resolved => {
            Some(SalsaForRangeIterResolveStateSummary::Resolved)
        }
        SalsaSemanticResolveStateSummary::Partial => {
            Some(SalsaForRangeIterResolveStateSummary::Partial)
        }
        SalsaSemanticResolveStateSummary::RecursiveDependency => {
            Some(SalsaForRangeIterResolveStateSummary::RecursiveDependency)
        }
        SalsaSemanticResolveStateSummary::Unknown => None,
    }
}

pub fn semantic_resolve_state_from_module_export(
    module_export: Option<&SalsaModuleExportSemanticSummary>,
) -> SalsaSemanticResolveStateSummary {
    match module_export {
        Some(module_export) if module_export.semantic_target.is_some() => {
            SalsaSemanticResolveStateSummary::Resolved
        }
        Some(_) => SalsaSemanticResolveStateSummary::Partial,
        None => SalsaSemanticResolveStateSummary::Unknown,
    }
}

pub fn module_export_resolve_state_from_semantic(
    state: SalsaSemanticResolveStateSummary,
) -> Option<crate::SalsaModuleExportResolveStateSummary> {
    match state {
        SalsaSemanticResolveStateSummary::Resolved => {
            Some(crate::SalsaModuleExportResolveStateSummary::Resolved)
        }
        SalsaSemanticResolveStateSummary::Partial => {
            Some(crate::SalsaModuleExportResolveStateSummary::Partial)
        }
        SalsaSemanticResolveStateSummary::RecursiveDependency => {
            Some(crate::SalsaModuleExportResolveStateSummary::RecursiveDependency)
        }
        SalsaSemanticResolveStateSummary::Unknown => None,
    }
}

pub fn build_semantic_value_shell_from_signature_return(
    summary: &SalsaSignatureReturnQuerySummary,
) -> SalsaSemanticValueShellSummary {
    let mut candidate_type_offsets = BTreeSet::new();
    if let Some(value) = summary.values.first() {
        for type_offset in &value.doc_return_type_offsets {
            candidate_type_offsets.insert(*type_offset);
        }
    }

    SalsaSemanticValueShellSummary {
        state: semantic_resolve_state_from_signature_return(summary.state.clone()),
        candidate_type_offsets: candidate_type_offsets.into_iter().collect(),
    }
}

fn build_semantic_value_shell_from_signature_return_slot(
    summary: &SalsaSignatureReturnQuerySummary,
    result_index: usize,
) -> SalsaSemanticValueShellSummary {
    let mut candidate_type_offsets = BTreeSet::new();
    let state = if let Some(value) = summary.values.get(result_index) {
        collect_signature_return_value_candidate_type_offsets(
            value,
            result_index,
            &mut candidate_type_offsets,
        );

        semantic_resolve_state_from_signature_return_value(value)
    } else {
        semantic_resolve_state_from_signature_return(summary.state.clone())
    };

    SalsaSemanticValueShellSummary {
        state,
        candidate_type_offsets: candidate_type_offsets.into_iter().collect(),
    }
}

pub fn collect_signature_return_value_candidate_type_offsets(
    value: &SalsaSignatureReturnValueSummary,
    result_index: usize,
    candidate_type_offsets: &mut BTreeSet<crate::SalsaDocTypeNodeKey>,
) {
    for type_offset in &value.doc_return_type_offsets {
        candidate_type_offsets.insert(*type_offset);
    }

    if let Some(name_type) = &value.name_type {
        for candidate in &name_type.candidates {
            for type_offset in &candidate.explicit_type_offsets {
                candidate_type_offsets.insert(*type_offset);
            }
        }
    }

    if let Some(member_type) = &value.member_type {
        for candidate in &member_type.candidates {
            for type_offset in &candidate.explicit_type_offsets {
                candidate_type_offsets.insert(*type_offset);
            }
        }
    }

    if let Some(call) = &value.call {
        for return_row in &call.returns {
            if let Some(item) = return_row.items.get(result_index)
                && let SalsaDocTypeRef::Node(type_offset) = item.doc_type.type_ref
            {
                candidate_type_offsets.insert(type_offset);
            }
        }
    }
}

pub fn signature_return_value_candidate_type_offsets(
    value: &SalsaSignatureReturnValueSummary,
    result_index: usize,
) -> Vec<crate::SalsaDocTypeNodeKey> {
    let mut candidate_type_offsets = BTreeSet::new();
    collect_signature_return_value_candidate_type_offsets(
        value,
        result_index,
        &mut candidate_type_offsets,
    );
    candidate_type_offsets.into_iter().collect()
}

fn semantic_resolve_state_from_signature_return_value(
    value: &SalsaSignatureReturnValueSummary,
) -> SalsaSemanticResolveStateSummary {
    if !value.doc_return_type_offsets.is_empty() {
        return SalsaSemanticResolveStateSummary::Resolved;
    }

    match value.kind {
        SalsaSignatureReturnExprKindSummary::Name => value
            .name_type
            .as_ref()
            .map(|info| build_semantic_value_shell_from_candidates(&info.candidates).state)
            .unwrap_or(SalsaSemanticResolveStateSummary::Unknown),
        SalsaSignatureReturnExprKindSummary::Member => value
            .member_type
            .as_ref()
            .map(|info| build_semantic_value_shell_from_candidates(&info.candidates).state)
            .unwrap_or(SalsaSemanticResolveStateSummary::Unknown),
        SalsaSignatureReturnExprKindSummary::Call => value
            .call
            .as_ref()
            .map(semantic_resolve_state_from_call_explain)
            .unwrap_or(SalsaSemanticResolveStateSummary::Unknown),
        SalsaSignatureReturnExprKindSummary::Literal
        | SalsaSignatureReturnExprKindSummary::Closure
        | SalsaSignatureReturnExprKindSummary::Table => SalsaSemanticResolveStateSummary::Resolved,
        SalsaSignatureReturnExprKindSummary::Other => SalsaSemanticResolveStateSummary::Unknown,
    }
}

fn semantic_resolve_state_from_call_explain(
    call: &crate::SalsaCallExplainSummary,
) -> SalsaSemanticResolveStateSummary {
    if call_explain_is_resolved(call) {
        SalsaSemanticResolveStateSummary::Resolved
    } else if !call.candidate_signature_offsets.is_empty() {
        SalsaSemanticResolveStateSummary::Partial
    } else {
        SalsaSemanticResolveStateSummary::Unknown
    }
}

pub fn build_semantic_value_shell_from_for_range_iter(
    summary: &SalsaForRangeIterQuerySummary,
) -> SalsaSemanticValueShellSummary {
    build_semantic_value_shell_from_for_range_iter_source(summary, None)
}

pub fn build_semantic_value_shell_from_for_range_iter_source(
    summary: &SalsaForRangeIterQuerySummary,
    signature_returns: Option<&SalsaSignatureReturnQueryIndex>,
) -> SalsaSemanticValueShellSummary {
    let state = semantic_resolve_state_from_for_range_iter(summary.state.clone());
    let Some(source) = summary.source.as_ref() else {
        return SalsaSemanticValueShellSummary {
            state,
            candidate_type_offsets: Vec::new(),
        };
    };

    let mut shell = match source.kind {
        crate::SalsaForRangeIterSourceKindSummary::Name => {
            build_semantic_value_shell_from_for_range_name_type(source.name_type.as_ref())
        }
        crate::SalsaForRangeIterSourceKindSummary::Member => {
            build_semantic_value_shell_from_for_range_member_type(source.member_type.as_ref())
        }
        crate::SalsaForRangeIterSourceKindSummary::Call => source
            .call
            .as_ref()
            .map(|call| {
                signature_returns
                    .map(|signature_returns| {
                        build_semantic_value_shell_from_call_explain_slot(
                            signature_returns,
                            call,
                            0,
                        )
                    })
                    .unwrap_or_else(|| SalsaSemanticValueShellSummary {
                        state: semantic_resolve_state_from_call_explain(call),
                        candidate_type_offsets: Vec::new(),
                    })
            })
            .unwrap_or_else(|| SalsaSemanticValueShellSummary {
                state: SalsaSemanticResolveStateSummary::Unknown,
                candidate_type_offsets: Vec::new(),
            }),
        crate::SalsaForRangeIterSourceKindSummary::Other => SalsaSemanticValueShellSummary {
            state: SalsaSemanticResolveStateSummary::Unknown,
            candidate_type_offsets: Vec::new(),
        },
    };
    shell.state = join_semantic_resolve_states([shell.state, state]);
    shell
}

pub fn for_range_iter_slot_candidate_type_offsets(
    summary: &SalsaForRangeIterQuerySummary,
    slot_index: usize,
) -> Vec<crate::SalsaDocTypeNodeKey> {
    summary
        .iter_vars
        .iter()
        .find(|iter_var| iter_var.slot_index == slot_index)
        .map(|iter_var| iter_var.type_offsets.clone())
        .unwrap_or_default()
}

fn build_semantic_value_shell_from_for_range_name_type(
    name_type: Option<&crate::SalsaProgramPointTypeInfoSummary>,
) -> SalsaSemanticValueShellSummary {
    let Some(name_type) = name_type else {
        return SalsaSemanticValueShellSummary {
            state: SalsaSemanticResolveStateSummary::Unknown,
            candidate_type_offsets: Vec::new(),
        };
    };

    build_semantic_value_shell_from_candidates(&name_type.candidates)
}

fn build_semantic_value_shell_from_for_range_member_type(
    member_type: Option<&crate::SalsaProgramPointMemberTypeInfoSummary>,
) -> SalsaSemanticValueShellSummary {
    let Some(member_type) = member_type else {
        return SalsaSemanticValueShellSummary {
            state: SalsaSemanticResolveStateSummary::Unknown,
            candidate_type_offsets: Vec::new(),
        };
    };

    build_semantic_value_shell_from_candidates(&member_type.candidates)
}

pub fn build_semantic_value_shell_from_module_export(
    module_export: Option<&SalsaModuleExportSemanticSummary>,
) -> SalsaSemanticValueShellSummary {
    SalsaSemanticValueShellSummary {
        state: semantic_resolve_state_from_module_export(module_export),
        candidate_type_offsets: Vec::new(),
    }
}

fn build_semantic_value_shell_from_call_explain_slot(
    signature_returns: &SalsaSignatureReturnQueryIndex,
    call: &crate::SalsaCallExplainSummary,
    result_index: usize,
) -> SalsaSemanticValueShellSummary {
    let mut shells = Vec::new();
    let mut seen_signature_offsets = BTreeSet::new();
    for signature_offset in call
        .resolved_signature_offset
        .into_iter()
        .chain(call.candidate_signature_offsets.iter().copied())
    {
        if !seen_signature_offsets.insert(signature_offset) {
            continue;
        }
        if let Some(summary) = find_signature_return_query_at(signature_returns, signature_offset) {
            shells.push(build_semantic_value_shell_from_signature_return_slot(
                &summary,
                result_index,
            ));
        }
    }

    let mut candidate_type_offsets = BTreeSet::new();
    for return_row in &call.returns {
        if let Some(item) = return_row.items.get(result_index)
            && let SalsaDocTypeRef::Node(type_offset) = item.doc_type.type_ref
        {
            candidate_type_offsets.insert(type_offset);
        }
    }

    if !candidate_type_offsets.is_empty() {
        shells.push(SalsaSemanticValueShellSummary {
            state: SalsaSemanticResolveStateSummary::Resolved,
            candidate_type_offsets: candidate_type_offsets.iter().copied().collect(),
        });
    }

    if !shells.is_empty() {
        return merge_solver_value_shells(&shells);
    }

    SalsaSemanticValueShellSummary {
        state: semantic_resolve_state_from_call_explain(call),
        candidate_type_offsets: Vec::new(),
    }
}

fn build_semantic_value_shell_from_initializer_expr(
    expr_offset: rowan::TextSize,
    result_index: usize,
    source_call_syntax_id: Option<crate::SalsaSyntaxIdSummary>,
    decl_types: &SalsaDeclTypeQueryIndex,
    member_types: &SalsaMemberTypeQueryIndex,
    signature_returns: &SalsaSignatureReturnQueryIndex,
    use_sites: &SalsaUseSiteIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
) -> Option<SalsaSemanticValueShellSummary> {
    if let Some(call) = source_call_syntax_id
        .and_then(|syntax_id| find_call_explain_by_syntax_id(signature_explain_index, syntax_id))
        .or_else(|| find_call_explain_at(signature_explain_index, expr_offset))
    {
        return Some(build_semantic_value_shell_from_call_explain_slot(
            signature_returns,
            &call,
            result_index,
        ));
    }

    if let Some(member_use) = find_member_use_at(use_sites, expr_offset) {
        return Some(build_member_value_shell(member_types, &member_use.target));
    }

    if let Some(name_use) = find_name_use_at(use_sites, expr_offset)
        && let SalsaNameUseResolutionSummary::LocalDecl(decl_id) = name_use.resolution
    {
        return Some(build_decl_value_shell(decl_types, decl_id));
    }

    None
}

pub fn build_decl_value_shell(
    decl_types: &SalsaDeclTypeQueryIndex,
    decl_id: crate::SalsaDeclId,
) -> SalsaSemanticValueShellSummary {
    build_semantic_value_shell_from_decl_type(find_decl_type_info(decl_types, decl_id).as_ref())
}

pub fn build_member_value_shell(
    member_types: &SalsaMemberTypeQueryIndex,
    member_target: &crate::SalsaMemberTargetId,
) -> SalsaSemanticValueShellSummary {
    build_semantic_value_shell_from_member_type(
        find_member_type_info(member_types, member_target).as_ref(),
    )
}

pub fn build_signature_return_component_summary(
    component: &SalsaSemanticGraphSccComponentSummary,
    signature_returns: &SalsaSignatureReturnQueryIndex,
) -> Option<SalsaSemanticSignatureReturnComponentSummary> {
    let mut signature_offsets = component
        .nodes
        .iter()
        .filter_map(|node| match node {
            SalsaSemanticGraphNodeSummary::SignatureReturn(signature_offset) => {
                Some(*signature_offset)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    signature_offsets.sort();
    signature_offsets.dedup();

    if signature_offsets.is_empty() {
        return None;
    }

    let shells = signature_offsets
        .iter()
        .filter_map(|signature_offset| {
            find_signature_return_query_at(signature_returns, *signature_offset)
                .map(|summary| build_semantic_value_shell_from_signature_return(&summary))
        })
        .collect::<Vec<_>>();
    let value_shell = merge_semantic_value_shells(&shells);

    Some(SalsaSemanticSignatureReturnComponentSummary {
        component_id: component.component_id,
        signature_offsets,
        propagated_value_shell: SalsaSemanticValueShellSummary {
            state: SalsaSemanticResolveStateSummary::Unknown,
            candidate_type_offsets: Vec::new(),
        },
        local_value_shell: value_shell.clone(),
        fixedpoint_value_shell: value_shell.clone(),
        value_shell,
        is_cycle: component.is_cycle,
    })
}

pub fn build_signature_return_summary(
    component: &SalsaSemanticGraphSccComponentSummary,
    summary: &SalsaSignatureReturnQuerySummary,
) -> Option<SalsaSemanticSignatureReturnSummary> {
    if !component.nodes.iter().any(|node| {
        matches!(
            node,
            SalsaSemanticGraphNodeSummary::SignatureReturn(signature_offset)
                if *signature_offset == summary.signature_offset
        )
    }) {
        return None;
    }

    let value_shell = build_semantic_value_shell_from_signature_return(summary);
    Some(SalsaSemanticSignatureReturnSummary {
        component_id: component.component_id,
        signature_offset: summary.signature_offset,
        state: summary.state.clone(),
        doc_returns: summary.doc_returns.clone(),
        values: summary.values.clone(),
        propagated_value_shell: SalsaSemanticValueShellSummary {
            state: SalsaSemanticResolveStateSummary::Unknown,
            candidate_type_offsets: Vec::new(),
        },
        local_value_shell: value_shell.clone(),
        fixedpoint_value_shell: value_shell.clone(),
        value_shell,
        is_cycle: component.is_cycle,
    })
}

pub fn build_for_range_iter_component_summary(
    component: &SalsaSemanticGraphSccComponentSummary,
    summary: &SalsaForRangeIterQuerySummary,
) -> Option<SalsaSemanticForRangeIterComponentSummary> {
    if !component.nodes.iter().any(|node| {
        matches!(
            node,
            SalsaSemanticGraphNodeSummary::ForRangeIter(loop_offset)
                if *loop_offset == summary.loop_offset
        )
    }) {
        return None;
    }

    let value_shell = build_semantic_value_shell_from_for_range_iter_source(summary, None);
    Some(SalsaSemanticForRangeIterComponentSummary {
        component_id: component.component_id,
        loop_offset: summary.loop_offset,
        iter_expr_offsets: summary.iter_expr_offsets.clone(),
        state: summary.state.clone(),
        source: summary.source.clone(),
        iter_vars: summary.iter_vars.clone(),
        propagated_value_shell: SalsaSemanticValueShellSummary {
            state: SalsaSemanticResolveStateSummary::Unknown,
            candidate_type_offsets: Vec::new(),
        },
        local_value_shell: value_shell.clone(),
        fixedpoint_value_shell: value_shell.clone(),
        value_shell,
        is_cycle: component.is_cycle,
    })
}

pub fn build_semantic_decl_summary(
    component: &SalsaSemanticGraphSccComponentSummary,
    decl_type: &SalsaDeclTypeInfoSummary,
) -> Option<SalsaSemanticDeclSummary> {
    if !component.nodes.iter().any(|node| {
        matches!(
            node,
            SalsaSemanticGraphNodeSummary::DeclValue(decl_id) if *decl_id == decl_type.decl_id
        )
    }) {
        return None;
    }

    let value_shell = build_semantic_value_shell_from_decl_type(Some(decl_type));
    Some(SalsaSemanticDeclSummary {
        component_id: component.component_id,
        decl_type: decl_type.clone(),
        propagated_value_shell: SalsaSemanticValueShellSummary {
            state: SalsaSemanticResolveStateSummary::Unknown,
            candidate_type_offsets: Vec::new(),
        },
        local_value_shell: value_shell.clone(),
        fixedpoint_value_shell: value_shell.clone(),
        value_shell,
        is_cycle: component.is_cycle,
    })
}

pub fn build_semantic_member_summary(
    component: &SalsaSemanticGraphSccComponentSummary,
    member_type: &SalsaMemberTypeInfoSummary,
) -> Option<SalsaSemanticMemberSummary> {
    if !component.nodes.iter().any(|node| {
        matches!(
            node,
            SalsaSemanticGraphNodeSummary::MemberValue(member_target)
                if *member_target == member_type.target
        )
    }) {
        return None;
    }

    let value_shell = build_semantic_value_shell_from_member_type(Some(member_type));
    Some(SalsaSemanticMemberSummary {
        component_id: component.component_id,
        member_type: member_type.clone(),
        propagated_value_shell: SalsaSemanticValueShellSummary {
            state: SalsaSemanticResolveStateSummary::Unknown,
            candidate_type_offsets: Vec::new(),
        },
        local_value_shell: value_shell.clone(),
        fixedpoint_value_shell: value_shell.clone(),
        value_shell,
        is_cycle: component.is_cycle,
    })
}

pub fn build_semantic_solver_component_result_summary(
    component: &SalsaSemanticGraphSccComponentSummary,
    decl_types: &SalsaDeclTypeQueryIndex,
    member_types: &SalsaMemberTypeQueryIndex,
    signature_returns: &SalsaSignatureReturnQueryIndex,
    for_range_iters: &SalsaForRangeIterQueryIndex,
    use_sites: &SalsaUseSiteIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    module_export: Option<&SalsaModuleExportSemanticSummary>,
    predecessor_results: &[SalsaSemanticSolverComponentResultSummary],
) -> SalsaSemanticSolverComponentResultSummary {
    let mut decl_ids = component
        .nodes
        .iter()
        .filter_map(|node| match node {
            SalsaSemanticGraphNodeSummary::DeclValue(decl_id) => Some(*decl_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    decl_ids.sort();
    decl_ids.dedup();

    let mut member_targets = component
        .nodes
        .iter()
        .filter_map(|node| match node {
            SalsaSemanticGraphNodeSummary::MemberValue(member_target) => {
                Some(member_target.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    member_targets.sort();
    member_targets.dedup();

    let mut signature_offsets = component
        .nodes
        .iter()
        .filter_map(|node| match node {
            SalsaSemanticGraphNodeSummary::SignatureReturn(signature_offset) => {
                Some(*signature_offset)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    signature_offsets.sort();
    signature_offsets.dedup();

    let mut for_range_loop_offsets = component
        .nodes
        .iter()
        .filter_map(|node| match node {
            SalsaSemanticGraphNodeSummary::ForRangeIter(loop_offset) => Some(*loop_offset),
            _ => None,
        })
        .collect::<Vec<_>>();
    for_range_loop_offsets.sort();
    for_range_loop_offsets.dedup();

    let includes_module_export = component
        .nodes
        .iter()
        .any(|node| matches!(node, SalsaSemanticGraphNodeSummary::ModuleExport));

    let mut local_shells = decl_ids
        .iter()
        .map(|decl_id| build_decl_value_shell(decl_types, *decl_id))
        .collect::<Vec<_>>();
    local_shells.extend(decl_ids.iter().filter_map(|decl_id| {
        find_decl_type_info(decl_types, *decl_id).and_then(|decl_type| {
            decl_type.initializer_offset.and_then(|expr_offset| {
                build_semantic_value_shell_from_initializer_expr(
                    expr_offset,
                    decl_type.value_result_index,
                    decl_type.source_call_syntax_id,
                    decl_types,
                    member_types,
                    signature_returns,
                    use_sites,
                    signature_explain_index,
                )
            })
        })
    }));
    local_shells.extend(
        member_targets
            .iter()
            .map(|member_target| build_member_value_shell(member_types, member_target)),
    );
    local_shells.extend(member_targets.iter().flat_map(|member_target| {
        find_member_type_info(member_types, member_target)
            .into_iter()
            .flat_map(|member_type| {
                member_type.candidates.into_iter().filter_map(|candidate| {
                    candidate.initializer_offset.and_then(|expr_offset| {
                        build_semantic_value_shell_from_initializer_expr(
                            expr_offset,
                            candidate.value_result_index,
                            candidate.source_call_syntax_id,
                            decl_types,
                            member_types,
                            signature_returns,
                            use_sites,
                            signature_explain_index,
                        )
                    })
                })
            })
    }));
    local_shells.extend(
        signature_offsets
            .iter()
            .filter_map(|signature_offset| {
                find_signature_return_query_at(signature_returns, *signature_offset)
                    .map(|summary| build_semantic_value_shell_from_signature_return(&summary))
            })
            .collect::<Vec<_>>(),
    );
    local_shells.extend(for_range_loop_offsets.iter().filter_map(|loop_offset| {
        find_for_range_iter_query_at(for_range_iters, *loop_offset).map(|summary| {
            build_semantic_value_shell_from_for_range_iter_source(&summary, Some(signature_returns))
        })
    }));
    if includes_module_export {
        local_shells.push(build_semantic_value_shell_from_module_export(module_export));
    }
    let input_shells = predecessor_results
        .iter()
        .map(|result| result.fixedpoint_value_shell.clone())
        .collect::<Vec<_>>();
    let mut consumed_predecessor_component_ids = predecessor_results
        .iter()
        .map(|result| result.component_id)
        .collect::<Vec<_>>();
    consumed_predecessor_component_ids.sort_unstable();
    consumed_predecessor_component_ids.dedup();

    let input_value_shell = merge_solver_value_shells(&input_shells);
    let propagated_value_shell =
        build_propagated_value_shell(&input_value_shell, !predecessor_results.is_empty());
    let local_value_shell = merge_solver_value_shells(&local_shells);
    let mut merged_shells = input_shells;
    merged_shells.extend(local_shells);
    let value_shell = merge_solver_value_shells(&merged_shells);
    let (fixedpoint_value_shell, fixedpoint_iterations) = solve_component_fixedpoint(
        &input_value_shell,
        &local_value_shell,
        component.is_cycle,
        !predecessor_results.is_empty(),
    );

    SalsaSemanticSolverComponentResultSummary {
        component_id: component.component_id,
        decl_ids,
        member_targets,
        signature_offsets,
        for_range_loop_offsets,
        includes_module_export,
        consumed_predecessor_component_ids,
        input_value_shell,
        propagated_value_shell,
        local_value_shell,
        value_shell,
        fixedpoint_value_shell,
        fixedpoint_iterations,
        is_cycle: component.is_cycle,
    }
}

pub fn solve_component_fixedpoint(
    input_value_shell: &SalsaSemanticValueShellSummary,
    local_value_shell: &SalsaSemanticValueShellSummary,
    is_cycle: bool,
    has_input: bool,
) -> (SalsaSemanticValueShellSummary, usize) {
    let propagated_value_shell = build_propagated_value_shell(input_value_shell, has_input);
    let seed = apply_component_transfer_step(&propagated_value_shell, local_value_shell, None);
    if !is_cycle {
        return (seed, 0);
    }

    let mut current = seed.clone();
    let mut iterations = 0;
    loop {
        iterations += 1;
        let next = apply_component_transfer_step(
            &propagated_value_shell,
            local_value_shell,
            Some(&current),
        );
        if next == current {
            return (current, iterations);
        }
        current = next;
    }
}

fn merge_component_iteration_shells(
    input: Option<SalsaSemanticValueShellSummary>,
    local: Option<SalsaSemanticValueShellSummary>,
    current: Option<SalsaSemanticValueShellSummary>,
) -> SalsaSemanticValueShellSummary {
    let shells = input
        .into_iter()
        .chain(local)
        .chain(current)
        .collect::<Vec<_>>();
    merge_solver_value_shells(&shells)
}

pub fn join_semantic_resolve_states<I>(states: I) -> SalsaSemanticResolveStateSummary
where
    I: IntoIterator<Item = SalsaSemanticResolveStateSummary>,
{
    let states = states.into_iter().collect::<Vec<_>>();
    if states.is_empty() {
        return SalsaSemanticResolveStateSummary::Unknown;
    }
    if states
        .iter()
        .any(|state| matches!(state, SalsaSemanticResolveStateSummary::RecursiveDependency))
    {
        return SalsaSemanticResolveStateSummary::RecursiveDependency;
    }
    if states
        .iter()
        .all(|state| matches!(state, SalsaSemanticResolveStateSummary::Resolved))
    {
        return SalsaSemanticResolveStateSummary::Resolved;
    }
    if states
        .iter()
        .any(|state| matches!(state, SalsaSemanticResolveStateSummary::Resolved))
    {
        return SalsaSemanticResolveStateSummary::Partial;
    }
    if states
        .iter()
        .any(|state| matches!(state, SalsaSemanticResolveStateSummary::Partial))
    {
        return SalsaSemanticResolveStateSummary::Partial;
    }

    SalsaSemanticResolveStateSummary::Unknown
}

pub fn merge_semantic_value_shells(
    shells: &[SalsaSemanticValueShellSummary],
) -> SalsaSemanticValueShellSummary {
    let mut candidate_type_offsets = shells
        .iter()
        .flat_map(|shell| shell.candidate_type_offsets.iter().copied())
        .collect::<Vec<_>>();
    candidate_type_offsets.sort();
    candidate_type_offsets.dedup();

    SalsaSemanticValueShellSummary {
        state: join_semantic_resolve_states(shells.iter().map(|shell| shell.state)),
        candidate_type_offsets,
    }
}
