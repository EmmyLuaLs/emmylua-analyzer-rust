use emmylua_parser::LuaSyntaxKind;

use super::*;

#[test]
fn test_summary_builder_semantic_solver_worklist_tracks_ready_components() {
    let mut compilation = setup_compilation();
    let source = r#"local function leaf()
  return 1
end

local function mid()
  return leaf()
end

local function a()
  return b()
end

local function b()
  return a()
end
"#;
    set_test_file(
        &mut compilation,
        601,
        "C:/ws/semantic_solver_worklist.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(601))
        .expect("signature summary")
        .signatures
        .clone();
    let leaf_signature = signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("leaf"))
        .expect("leaf signature");
    let mid_signature = signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("mid"))
        .expect("mid signature");
    let a_signature = signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("a"))
        .expect("a signature");

    let leaf_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(601),
            crate::SalsaSemanticGraphNodeSummary::SignatureReturn(leaf_signature.syntax_offset),
        )
        .expect("leaf component");
    let mid_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(601),
            crate::SalsaSemanticGraphNodeSummary::SignatureReturn(mid_signature.syntax_offset),
        )
        .expect("mid component");
    let cycle_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(601),
            crate::SalsaSemanticGraphNodeSummary::SignatureReturn(a_signature.syntax_offset),
        )
        .expect("cycle component");

    let worklist = compilation
        .semantic()
        .file()
        .solver_worklist(FileId::new(601))
        .expect("solver worklist");
    assert_eq!(worklist.tasks.len(), worklist.topo_order.len());
    assert!(
        worklist
            .ready_component_ids
            .contains(&mid_component.component_id)
    );
    assert!(
        worklist
            .ready_component_ids
            .contains(&cycle_component.component_id)
    );
    assert!(
        !worklist
            .ready_component_ids
            .contains(&leaf_component.component_id)
    );

    let leaf_task = compilation
        .semantic()
        .file()
        .solver_task(FileId::new(601), leaf_component.component_id)
        .expect("leaf solver task");
    assert!(
        leaf_task
            .predecessor_component_ids
            .contains(&mid_component.component_id)
    );

    let ready_tasks = compilation
        .semantic()
        .file()
        .solver_ready_tasks(FileId::new(601))
        .expect("ready tasks");
    assert!(
        ready_tasks
            .iter()
            .all(|task| task.predecessor_component_ids.is_empty())
    );

    let execution = compilation
        .semantic()
        .file()
        .solver_execution(FileId::new(601))
        .expect("solver execution");
    assert!(
        !compilation
            .semantic()
            .file()
            .solver_execution_is_complete(FileId::new(601))
            .expect("solver execution completion state")
    );
    assert!(
        execution
            .ready_component_ids
            .contains(&mid_component.component_id)
    );
    assert!(
        execution
            .ready_component_ids
            .contains(&cycle_component.component_id)
    );

    let mid_execution_task = compilation
        .semantic()
        .file()
        .solver_execution_task(FileId::new(601), mid_component.component_id)
        .expect("mid execution task");
    assert_eq!(
        mid_execution_task.state,
        crate::SalsaSemanticSolverTaskStateSummary::Ready
    );
    let next_ready_task = compilation
        .semantic()
        .file()
        .solver_next_ready_execution_task(FileId::new(601))
        .expect("next ready execution task");
    assert!(
        execution
            .ready_component_ids
            .contains(&next_ready_task.component_id)
    );
    assert_eq!(
        next_ready_task.state,
        crate::SalsaSemanticSolverTaskStateSummary::Ready
    );
    let step = compilation
        .semantic()
        .file()
        .solver_step(FileId::new(601))
        .expect("solver step");
    assert_eq!(step.completed_component_id, next_ready_task.component_id);
    let step_component_result = step.component_result.expect("step component result");
    assert_eq!(
        step_component_result.component_id,
        step.completed_component_id
    );
    let tracked_component_result = compilation
        .semantic()
        .file()
        .solver_component_result_summary(FileId::new(601), step.completed_component_id)
        .expect("tracked component result");
    assert_eq!(step_component_result, tracked_component_result);
    let completed_with_result = crate::complete_semantic_solver_execution_task_with_result(
        &execution,
        step.completed_component_id,
        step_component_result.clone(),
    );
    assert_eq!(step.next_execution, completed_with_result);
    assert!(
        step.next_execution
            .completed_component_ids
            .contains(&step.completed_component_id)
    );
    assert!(
        step.next_execution
            .component_results
            .iter()
            .any(|result| result.component_id == step.completed_component_id)
    );
    let completed_step_task = crate::find_semantic_solver_execution_task(
        &step.next_execution,
        step.completed_component_id,
    )
    .expect("completed step task");
    assert_eq!(
        completed_step_task.state,
        crate::SalsaSemanticSolverTaskStateSummary::Completed
    );
    assert!(
        step.next_execution
            .ready_component_ids
            .iter()
            .all(|component_id| matches!(
                crate::find_semantic_solver_execution_task(&step.next_execution, *component_id),
                Some(task) if task.state == crate::SalsaSemanticSolverTaskStateSummary::Ready
            ))
    );

    let scc_index = compilation
        .semantic()
        .file()
        .graph_scc_index(FileId::new(601))
        .expect("graph scc index");
    let signature_return_index = compilation
        .doc()
        .signature_return_index(FileId::new(601))
        .expect("signature return index");
    let decl_type_index = compilation
        .types()
        .decl_index(FileId::new(601))
        .expect("decl type index");
    let member_type_index = compilation
        .types()
        .member_index(FileId::new(601))
        .expect("member type index");
    let for_range_iter_index = compilation
        .flow()
        .for_range_iter_index(FileId::new(601))
        .expect("for range iter index");
    let use_sites = compilation
        .lexical()
        .use_sites(FileId::new(601))
        .expect("use sites");
    let signature_explain_index = compilation
        .doc()
        .signature_explain_index(FileId::new(601))
        .expect("signature explain index");
    let module_export = compilation
        .semantic()
        .file()
        .summary(FileId::new(601))
        .expect("semantic summary")
        .module_export
        .clone();
    let mid_step = crate::build_semantic_solver_step_summary(
        &execution,
        mid_component.component_id,
        &scc_index,
        &decl_type_index,
        &member_type_index,
        &signature_return_index,
        &for_range_iter_index,
        &use_sites,
        &signature_explain_index,
        module_export.as_ref(),
    )
    .expect("mid step");
    let leaf_step = crate::build_semantic_solver_step_summary(
        &mid_step.next_execution,
        leaf_component.component_id,
        &scc_index,
        &decl_type_index,
        &member_type_index,
        &signature_return_index,
        &for_range_iter_index,
        &use_sites,
        &signature_explain_index,
        module_export.as_ref(),
    )
    .expect("leaf step");
    let leaf_step_result = leaf_step.component_result.expect("leaf step result");
    assert!(
        leaf_step_result
            .consumed_predecessor_component_ids
            .contains(&mid_component.component_id)
    );

    let next_execution =
        crate::complete_semantic_solver_execution_task(&execution, mid_component.component_id);
    let next_leaf_task =
        crate::find_semantic_solver_execution_task(&next_execution, leaf_component.component_id)
            .expect("next leaf execution task");
    assert_eq!(
        next_leaf_task.state,
        crate::SalsaSemanticSolverTaskStateSummary::Ready
    );
    assert!(
        next_execution
            .completed_component_ids
            .contains(&mid_component.component_id)
    );
    assert!(!crate::semantic_solver_execution_is_complete(
        &next_execution
    ));

    let final_execution = execution.tasks.iter().map(|task| task.component_id).fold(
        next_execution.clone(),
        |execution_state, component_id| {
            crate::complete_semantic_solver_execution_task(&execution_state, component_id)
        },
    );
    assert!(crate::semantic_solver_execution_is_complete(
        &final_execution
    ));
}

#[test]
fn test_summary_builder_semantic_solver_value_shells_and_merge() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string
local value = pick()

local holder = {}
holder.alias = value

---@type fun(): string, integer
local iter

---@return string
local function resolved()
  return value
end

local function recursive()
  return recursive()
end

for key, extra in iter do
  print(key, extra)
end

return resolved
"#;
    set_test_file(
        &mut compilation,
        602,
        "C:/ws/semantic_solver_shells.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(602))
        .expect("signature summary")
        .signatures
        .clone();
    let decl_tree = compilation
        .file()
        .decl_tree(FileId::new(602))
        .expect("decl tree");
    let members = compilation
        .file()
        .members(FileId::new(602))
        .expect("member summary");
    let value_decl = decl_tree
        .decls
        .iter()
        .find(|decl| decl.name.as_str() == "value")
        .expect("value decl");
    let alias_member = members
        .members
        .iter()
        .find(|member| member.target.member_name.as_str() == "alias")
        .expect("alias member");
    let resolved_signature = signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("resolved"))
        .expect("resolved signature");
    let recursive_signature = signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("recursive"))
        .expect("recursive signature");
    let loop_offset = compilation
        .flow()
        .summary(FileId::new(602))
        .expect("flow summary")
        .loops
        .iter()
        .find(|loop_summary| matches!(loop_summary.kind, crate::SalsaFlowLoopKindSummary::ForRange))
        .map(|loop_summary| loop_summary.syntax_offset)
        .expect("for range loop");

    let resolved_shell = compilation
        .semantic()
        .file()
        .signature_return_value_shell(FileId::new(602), resolved_signature.syntax_offset)
        .expect("resolved value shell");
    assert_eq!(
        resolved_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert!(!resolved_shell.candidate_type_offsets.is_empty());

    let recursive_shell = compilation
        .semantic()
        .file()
        .signature_return_value_shell(FileId::new(602), recursive_signature.syntax_offset)
        .expect("recursive value shell");
    assert_eq!(
        recursive_shell.state,
        crate::SalsaSemanticResolveStateSummary::RecursiveDependency
    );

    let iter_shell = compilation
        .semantic()
        .file()
        .for_range_iter_value_shell(FileId::new(602), loop_offset)
        .expect("for range value shell");
    assert_eq!(
        iter_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert!(!iter_shell.candidate_type_offsets.is_empty());

    let export_shell = compilation
        .semantic()
        .file()
        .module_export_value_shell(FileId::new(602))
        .expect("module export value shell");
    assert_eq!(
        export_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );

    let merged = crate::merge_semantic_value_shells(&[
        resolved_shell.clone(),
        iter_shell.clone(),
        recursive_shell.clone(),
    ]);
    assert_eq!(
        merged.state,
        crate::SalsaSemanticResolveStateSummary::RecursiveDependency
    );
    assert!(merged.candidate_type_offsets.len() >= resolved_shell.candidate_type_offsets.len());

    assert_eq!(
        crate::join_semantic_resolve_states([
            crate::SalsaSemanticResolveStateSummary::Unknown,
            crate::SalsaSemanticResolveStateSummary::Resolved,
        ]),
        crate::SalsaSemanticResolveStateSummary::Partial
    );
    let recursive_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(602),
            crate::SalsaSemanticGraphNodeSummary::SignatureReturn(
                recursive_signature.syntax_offset,
            ),
        )
        .expect("recursive component");
    let recursive_component_summary = compilation
        .semantic()
        .file()
        .signature_return_component_summary(FileId::new(602), recursive_component.component_id)
        .expect("recursive component summary");
    assert!(recursive_component_summary.is_cycle);
    assert_eq!(
        recursive_component_summary.value_shell.state,
        crate::SalsaSemanticResolveStateSummary::RecursiveDependency
    );
    assert_eq!(
        recursive_component_summary.fixedpoint_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::RecursiveDependency
    );
    assert!(
        recursive_component_summary
            .signature_offsets
            .contains(&recursive_signature.syntax_offset)
    );

    let resolved_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(602),
            crate::SalsaSemanticGraphNodeSummary::SignatureReturn(resolved_signature.syntax_offset),
        )
        .expect("resolved component");
    let resolved_component_summary = compilation
        .semantic()
        .file()
        .signature_return_component_summary(FileId::new(602), resolved_component.component_id)
        .expect("resolved component summary");
    assert_eq!(
        resolved_component_summary.value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        resolved_component_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert!(
        !resolved_component_summary
            .value_shell
            .candidate_type_offsets
            .is_empty()
    );
    let resolved_component_result = compilation
        .semantic()
        .file()
        .solver_component_result_summary(FileId::new(602), resolved_component.component_id)
        .expect("resolved component result");
    let resolved_component_result_by_signature = compilation
        .semantic()
        .file()
        .signature_return_component_result_summary(
            FileId::new(602),
            resolved_signature.syntax_offset,
        )
        .expect("resolved component result by signature");
    assert_eq!(
        resolved_component_summary.propagated_value_shell,
        resolved_component_result.propagated_value_shell
    );
    assert_eq!(
        resolved_component_summary.local_value_shell,
        resolved_component_result.local_value_shell
    );
    assert_eq!(
        resolved_component_summary.fixedpoint_value_shell,
        resolved_component_result.fixedpoint_value_shell
    );
    assert_eq!(
        resolved_component_summary.value_shell,
        resolved_component_result.fixedpoint_value_shell
    );
    assert_eq!(
        resolved_component_result_by_signature,
        resolved_component_result
    );
    assert_eq!(
        resolved_shell,
        resolved_component_result.fixedpoint_value_shell
    );

    let value_decl_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(602),
            crate::SalsaSemanticGraphNodeSummary::DeclValue(value_decl.id),
        )
        .expect("value decl component");
    let value_decl_component_result = compilation
        .semantic()
        .file()
        .solver_component_result_summary(FileId::new(602), value_decl_component.component_id)
        .expect("value decl component result");
    let value_decl_component_result_by_decl = compilation
        .semantic()
        .file()
        .decl_component_result_summary(FileId::new(602), value_decl.id)
        .expect("value decl component result by decl");
    let value_decl_summary = compilation
        .semantic()
        .file()
        .decl_summary(FileId::new(602), value_decl.id)
        .expect("value decl summary");
    assert!(
        value_decl_component_result
            .decl_ids
            .contains(&value_decl.id)
    );
    assert_eq!(
        value_decl_component_result_by_decl,
        value_decl_component_result
    );
    assert_eq!(
        value_decl_summary.component_id,
        value_decl_component.component_id
    );
    assert_eq!(value_decl_summary.decl_type.decl_id, value_decl.id);
    assert_eq!(
        value_decl_summary.propagated_value_shell,
        value_decl_component_result.propagated_value_shell
    );
    assert_eq!(
        value_decl_summary.local_value_shell,
        value_decl_component_result.local_value_shell
    );
    assert_eq!(
        value_decl_summary.fixedpoint_value_shell,
        value_decl_component_result.fixedpoint_value_shell
    );
    assert_eq!(
        value_decl_summary.value_shell,
        value_decl_component_result.fixedpoint_value_shell
    );
    assert_eq!(
        value_decl_component_result.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert!(
        !value_decl_component_result
            .local_value_shell
            .candidate_type_offsets
            .is_empty()
    );
    let alias_member_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(602),
            crate::SalsaSemanticGraphNodeSummary::MemberValue(alias_member.target.clone()),
        )
        .expect("alias member component");
    let alias_member_component_result = compilation
        .semantic()
        .file()
        .solver_component_result_summary(FileId::new(602), alias_member_component.component_id)
        .expect("alias member component result");
    let alias_member_component_result_by_member = compilation
        .semantic()
        .file()
        .member_component_result_summary(FileId::new(602), alias_member.target.clone())
        .expect("alias member component result by member");
    let alias_member_summary = compilation
        .semantic()
        .file()
        .member_summary(FileId::new(602), alias_member.target.clone())
        .expect("alias member summary");
    assert!(
        alias_member_component_result
            .member_targets
            .contains(&alias_member.target)
    );
    assert_eq!(
        alias_member_component_result_by_member,
        alias_member_component_result
    );
    assert_eq!(
        alias_member_summary.component_id,
        alias_member_component.component_id
    );
    assert_eq!(alias_member_summary.member_type.target, alias_member.target);
    assert_eq!(
        alias_member_summary.propagated_value_shell,
        alias_member_component_result.propagated_value_shell
    );
    assert_eq!(
        alias_member_summary.local_value_shell,
        alias_member_component_result.local_value_shell
    );
    assert_eq!(
        alias_member_summary.fixedpoint_value_shell,
        alias_member_component_result.fixedpoint_value_shell
    );
    assert_eq!(
        alias_member_summary.value_shell,
        alias_member_component_result.fixedpoint_value_shell
    );
    assert_eq!(
        alias_member_component_result.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );

    let for_range_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(602),
            crate::SalsaSemanticGraphNodeSummary::ForRangeIter(loop_offset),
        )
        .expect("for range component");
    let for_range_component_result = compilation
        .semantic()
        .file()
        .solver_component_result_summary(FileId::new(602), for_range_component.component_id)
        .expect("for range component result");
    let for_range_component_result_by_loop = compilation
        .semantic()
        .file()
        .for_range_iter_component_result_summary(FileId::new(602), loop_offset)
        .expect("for range component result by loop");
    let for_range_component_summary = compilation
        .semantic()
        .file()
        .for_range_iter_component_summary(FileId::new(602), loop_offset)
        .expect("for range component summary");
    assert!(
        for_range_component_result
            .for_range_loop_offsets
            .contains(&loop_offset)
    );
    assert_eq!(
        for_range_component_result_by_loop,
        for_range_component_result
    );
    assert_eq!(
        for_range_component_summary.component_id,
        for_range_component.component_id
    );
    assert_eq!(
        for_range_component_summary.propagated_value_shell,
        for_range_component_result.propagated_value_shell
    );
    assert_eq!(
        for_range_component_summary.local_value_shell,
        for_range_component_result.local_value_shell
    );
    assert_eq!(
        for_range_component_summary.fixedpoint_value_shell,
        for_range_component_result.fixedpoint_value_shell
    );
    assert_eq!(
        for_range_component_summary.value_shell,
        for_range_component_result.fixedpoint_value_shell
    );
    assert_eq!(
        for_range_component_result.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );

    let module_export_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(602),
            crate::SalsaSemanticGraphNodeSummary::ModuleExport,
        )
        .expect("module export component");
    let module_export_component_result = compilation
        .semantic()
        .file()
        .solver_component_result_summary(FileId::new(602), module_export_component.component_id)
        .expect("module export component result");
    let module_export_component_result_by_export = compilation
        .semantic()
        .file()
        .module_export_component_result_summary(FileId::new(602))
        .expect("module export component result by export");
    let module_export_component_summary = compilation
        .semantic()
        .file()
        .module_export_component_summary(FileId::new(602))
        .expect("module export component summary");
    let module_export_query = compilation
        .semantic()
        .file()
        .module_export_query(FileId::new(602))
        .expect("module export query");
    assert!(module_export_component_result.includes_module_export);
    assert_eq!(
        module_export_component_result_by_export,
        module_export_component_result
    );
    assert_eq!(
        module_export_component_summary.component_id,
        module_export_component.component_id
    );
    assert_eq!(
        module_export_component_summary.propagated_value_shell,
        module_export_component_result.propagated_value_shell
    );
    assert_eq!(
        module_export_component_summary.local_value_shell,
        module_export_component_result.local_value_shell
    );
    assert_eq!(
        module_export_component_summary.fixedpoint_value_shell,
        module_export_component_result.fixedpoint_value_shell
    );
    assert_eq!(
        module_export_component_summary.value_shell,
        module_export_component_result.fixedpoint_value_shell
    );
    assert_eq!(
        module_export_component_summary.state,
        module_export_query.state
    );
    assert_eq!(
        module_export_component_summary.semantic_target,
        module_export_query.semantic_target
    );
    assert_eq!(
        module_export_component_result.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        export_shell,
        module_export_component_result.fixedpoint_value_shell
    );

    let execution = compilation
        .semantic()
        .file()
        .solver_execution(FileId::new(602))
        .expect("solver execution");
    let decl_type_index = compilation
        .types()
        .decl_index(FileId::new(602))
        .expect("decl type index");
    let member_type_index = compilation
        .types()
        .member_index(FileId::new(602))
        .expect("member type index");
    let scc_index = compilation
        .semantic()
        .file()
        .graph_scc_index(FileId::new(602))
        .expect("scc index");
    let signature_return_index = compilation
        .doc()
        .signature_return_index(FileId::new(602))
        .expect("signature return index");
    let use_sites = compilation
        .lexical()
        .use_sites(FileId::new(602))
        .expect("use sites");
    let signature_explain_index = compilation
        .doc()
        .signature_explain_index(FileId::new(602))
        .expect("signature explain index");
    let module_export = compilation
        .semantic()
        .file()
        .summary(FileId::new(602))
        .expect("semantic summary")
        .module_export
        .clone();
    let for_range_iter_index = compilation
        .flow()
        .for_range_iter_index(FileId::new(602))
        .expect("for range iter index");
    let resolved_ready_execution = resolved_component.predecessor_component_ids.iter().fold(
        execution,
        |execution_state, predecessor_component_id| {
            crate::complete_semantic_solver_execution_task(
                &execution_state,
                *predecessor_component_id,
            )
            .into()
        },
    );
    let resolved_step = crate::build_semantic_solver_step_summary(
        &resolved_ready_execution,
        resolved_component.component_id,
        &scc_index,
        &decl_type_index,
        &member_type_index,
        &signature_return_index,
        &for_range_iter_index,
        &use_sites,
        &signature_explain_index,
        module_export.as_ref(),
    )
    .expect("resolved step");
    let resolved_step_component = resolved_step
        .component_result
        .expect("resolved step component result");
    assert_eq!(
        resolved_step_component.component_id,
        resolved_component.component_id
    );
    assert_ne!(
        resolved_step_component.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Unknown
    );
    assert_eq!(resolved_step_component.fixedpoint_iterations, 0);
    assert_eq!(
        resolved_step_component.fixedpoint_value_shell,
        resolved_step_component.value_shell
    );
    assert!(
        resolved_step_component
            .signature_offsets
            .contains(&resolved_signature.syntax_offset)
    );
    assert!(
        resolved_step
            .next_execution
            .completed_component_ids
            .contains(&resolved_component.component_id)
    );

    let propagated_execution = crate::complete_semantic_solver_execution_task_with_result(
        &resolved_ready_execution,
        resolved_component.component_id,
        resolved_step_component.clone(),
    );
    let module_export_step = crate::build_semantic_solver_step_summary(
        &propagated_execution,
        module_export_component.component_id,
        &scc_index,
        &decl_type_index,
        &member_type_index,
        &signature_return_index,
        &for_range_iter_index,
        &use_sites,
        &signature_explain_index,
        module_export.as_ref(),
    )
    .expect("module export step");
    let module_export_step_result = module_export_step
        .component_result
        .expect("module export step result");
    assert!(module_export_step_result.includes_module_export);
    assert_eq!(
        module_export_step_result.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );

    let recursive_component_result = compilation
        .semantic()
        .file()
        .solver_component_result_summary(FileId::new(602), recursive_component.component_id)
        .expect("recursive component result");
    assert!(recursive_component_result.is_cycle);
    assert!(recursive_component_result.fixedpoint_iterations >= 1);
    assert_eq!(
        recursive_component_result.fixedpoint_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::RecursiveDependency
    );
    assert_eq!(
        recursive_shell,
        recursive_component_result.fixedpoint_value_shell
    );

    let for_range_component_result = compilation
        .semantic()
        .file()
        .solver_component_result_summary(FileId::new(602), for_range_component.component_id)
        .expect("for range component result again");
    assert_eq!(
        iter_shell,
        for_range_component_result.fixedpoint_value_shell
    );
}

#[test]
fn test_summary_builder_semantic_solver_module_export_value_shell_consumes_decl_candidates() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string
local value = pick()

return value
"#;
    set_test_file(
        &mut compilation,
        604,
        "C:/ws/semantic_solver_module_export_value_shell.lua",
        source,
    );

    let export_shell = compilation
        .semantic()
        .file()
        .module_export_value_shell(FileId::new(604))
        .expect("module export value shell");
    let module_export_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(604),
            crate::SalsaSemanticGraphNodeSummary::ModuleExport,
        )
        .expect("module export component");
    let module_export_component_result = compilation
        .semantic()
        .file()
        .solver_component_result_summary(FileId::new(604), module_export_component.component_id)
        .expect("module export component result");

    assert_eq!(
        export_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        export_shell,
        module_export_component_result.fixedpoint_value_shell
    );
}

#[test]
fn test_summary_builder_semantic_solver_resolves_nested_property_member_initializer_chain() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string
local source

local box = { value = source }
local holder = {
  nested = {
    enabled = box.value,
  },
}
"#;
    set_test_file(
        &mut compilation,
        619,
        "C:/ws/semantic_solver_nested_property_member_initializer.lua",
        source,
    );

    let decls = compilation
        .file()
        .decl_tree(FileId::new(619))
        .expect("decl tree")
        .decls
        .clone();
    let box_decl = decls
        .iter()
        .find(|decl| decl.name == "box" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. }))
        .expect("box decl");
    let holder_decl = decls
        .iter()
        .find(|decl| {
            decl.name == "holder" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("holder decl");
    let source_member = crate::SalsaMemberTargetId::from(&crate::SalsaMemberTargetSummary {
        root: crate::SalsaMemberRootSummary::LocalDecl {
            name: "box".into(),
            decl_id: box_decl.id,
        },
        owner_segments: Vec::new().into(),
        member_name: "value".into(),
    });
    let nested_member = crate::SalsaMemberTargetId::from(&crate::SalsaMemberTargetSummary {
        root: crate::SalsaMemberRootSummary::LocalDecl {
            name: "holder".into(),
            decl_id: holder_decl.id,
        },
        owner_segments: vec!["nested".into()].into(),
        member_name: "enabled".into(),
    });

    let source_member_result = compilation
        .semantic()
        .file()
        .member_summary(FileId::new(619), source_member.clone())
        .expect("source member component result");
    let nested_member_result = compilation
        .semantic()
        .file()
        .member_summary(FileId::new(619), nested_member.clone())
        .expect("nested member component result");
    let nested_member_shell = compilation
        .semantic()
        .file()
        .member_summary(FileId::new(619), nested_member.clone())
        .expect("nested member shell");

    assert_eq!(
        source_member_result.fixedpoint_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        nested_member_result.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Partial
    );
    assert_eq!(
        nested_member_result.fixedpoint_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        nested_member_shell.value_shell,
        nested_member_result.fixedpoint_value_shell
    );
    assert_eq!(
        nested_member_result
            .fixedpoint_value_shell
            .candidate_type_offsets,
        source_member_result
            .fixedpoint_value_shell
            .candidate_type_offsets
    );
}

#[test]
fn test_summary_builder_semantic_solver_resolves_nested_property_call_and_multihop_member_initializer_chain()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@type string
local source

---@return string
local function make()
  return source
end

local outer = {
  nested = {
    value = source,
  },
}

local holder = {
  nested = {
    made = make(),
    forwarded = outer.nested.value,
  },
}
"#;
    set_test_file(
        &mut compilation,
        620,
        "C:/ws/semantic_solver_nested_property_call_multihop.lua",
        source,
    );

    let decls = compilation
        .file()
        .decl_tree(FileId::new(620))
        .expect("decl tree")
        .decls
        .clone();
    let outer_decl = decls
        .iter()
        .find(|decl| {
            decl.name == "outer" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("outer decl");
    let holder_decl = decls
        .iter()
        .find(|decl| {
            decl.name == "holder" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("holder decl");
    let multihop_source_member =
        crate::SalsaMemberTargetId::from(&crate::SalsaMemberTargetSummary {
            root: crate::SalsaMemberRootSummary::LocalDecl {
                name: "outer".into(),
                decl_id: outer_decl.id,
            },
            owner_segments: vec!["nested".into()].into(),
            member_name: "value".into(),
        });
    let forwarded_member = crate::SalsaMemberTargetId::from(&crate::SalsaMemberTargetSummary {
        root: crate::SalsaMemberRootSummary::LocalDecl {
            name: "holder".into(),
            decl_id: holder_decl.id,
        },
        owner_segments: vec!["nested".into()].into(),
        member_name: "forwarded".into(),
    });
    let made_member = crate::SalsaMemberTargetId::from(&crate::SalsaMemberTargetSummary {
        root: crate::SalsaMemberRootSummary::LocalDecl {
            name: "holder".into(),
            decl_id: holder_decl.id,
        },
        owner_segments: vec!["nested".into()].into(),
        member_name: "made".into(),
    });

    let source_member_result = compilation
        .semantic()
        .file()
        .member_summary(FileId::new(620), multihop_source_member.clone())
        .expect("multihop source member component result");
    let forwarded_member_result = compilation
        .semantic()
        .file()
        .member_summary(FileId::new(620), forwarded_member.clone())
        .expect("forwarded member component result");
    let made_member_result = compilation
        .semantic()
        .file()
        .member_summary(FileId::new(620), made_member.clone())
        .expect("made member component result");
    let forwarded_member_shell = compilation
        .semantic()
        .file()
        .member_summary(FileId::new(620), forwarded_member.clone())
        .expect("forwarded member shell");
    let made_member_shell = compilation
        .semantic()
        .file()
        .member_summary(FileId::new(620), made_member.clone())
        .expect("made member shell");

    assert_eq!(
        source_member_result.fixedpoint_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        forwarded_member_result.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Partial
    );
    assert_eq!(
        forwarded_member_result.fixedpoint_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        forwarded_member_shell.value_shell,
        forwarded_member_result.fixedpoint_value_shell
    );
    assert_eq!(
        forwarded_member_result
            .fixedpoint_value_shell
            .candidate_type_offsets,
        source_member_result
            .fixedpoint_value_shell
            .candidate_type_offsets
    );

    assert_eq!(
        made_member_result.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        made_member_result.fixedpoint_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        made_member_shell.value_shell,
        made_member_result.fixedpoint_value_shell
    );
    assert!(
        !made_member_result
            .fixedpoint_value_shell
            .candidate_type_offsets
            .is_empty()
    );
}

#[test]
fn test_summary_builder_semantic_solver_member_summary_resolves_signature_backed_member() {
    let mut compilation = setup_compilation();
    let source = r#"local holder = {}

function holder.run()
  return 1
end
"#;
    set_test_file(
        &mut compilation,
        605,
        "C:/ws/semantic_solver_member_signature.lua",
        source,
    );

    let run_member = compilation
        .file()
        .members(FileId::new(605))
        .expect("member summary")
        .members
        .clone()
        .into_iter()
        .find(|member| member.target.member_name.as_str() == "run")
        .expect("run member");

    let member_summary = compilation
        .semantic()
        .file()
        .member_summary(FileId::new(605), run_member.target.clone())
        .expect("member summary");

    assert_eq!(
        member_summary.value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        member_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        member_summary.value_shell,
        member_summary.fixedpoint_value_shell
    );
}

#[test]
fn test_summary_builder_semantic_solver_decl_summary_resolves_closure_initializer() {
    let mut compilation = setup_compilation();
    let source = r#"local fn = function()
  return 1
end
"#;
    set_test_file(
        &mut compilation,
        606,
        "C:/ws/semantic_solver_decl_signature.lua",
        source,
    );

    let fn_decl = compilation
        .file()
        .decl_tree(FileId::new(606))
        .expect("decl tree")
        .decls
        .clone()
        .into_iter()
        .find(|decl| decl.name == "fn" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. }))
        .expect("fn decl");

    let decl_summary = compilation
        .semantic()
        .file()
        .decl_summary(FileId::new(606), fn_decl.id)
        .expect("decl summary");

    assert_eq!(
        decl_summary.value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        decl_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        decl_summary.value_shell,
        decl_summary.fixedpoint_value_shell
    );
}

#[test]
fn test_summary_builder_semantic_solver_decl_summary_resolves_literal_initializer() {
    let mut compilation = setup_compilation();
    let source = r#"local value = 1
"#;
    set_test_file(
        &mut compilation,
        609,
        "C:/ws/semantic_solver_decl_literal.lua",
        source,
    );

    let value_decl = compilation
        .file()
        .decl_tree(FileId::new(609))
        .expect("decl tree")
        .decls
        .clone()
        .into_iter()
        .find(|decl| {
            decl.name == "value" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("value decl");

    let decl_summary = compilation
        .semantic()
        .file()
        .decl_summary(FileId::new(609), value_decl.id)
        .expect("decl summary");

    assert_eq!(
        decl_summary.value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        decl_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        decl_summary.value_shell,
        decl_summary.fixedpoint_value_shell
    );
}

#[test]
fn test_summary_builder_semantic_solver_decl_value_shell_resolves_name_initializer() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string
local source
local alias = source
"#;
    set_test_file(
        &mut compilation,
        612,
        "C:/ws/semantic_solver_decl_name_initializer.lua",
        source,
    );

    let alias_decl = compilation
        .file()
        .decl_tree(FileId::new(612))
        .expect("decl tree")
        .decls
        .clone()
        .into_iter()
        .find(|decl| {
            decl.name == "alias" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("alias decl");

    let decl_summary = compilation
        .semantic()
        .file()
        .decl_summary(FileId::new(612), alias_decl.id)
        .expect("decl summary");

    assert_eq!(
        decl_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert!(
        !decl_summary
            .local_value_shell
            .candidate_type_offsets
            .is_empty()
    );
}

#[test]
fn test_summary_builder_semantic_solver_decl_value_shell_resolves_call_initializer() {
    let mut compilation = setup_compilation();
    let source = r#"---@return string
local function make()
  return "x"
end

local value = make()
"#;
    set_test_file(
        &mut compilation,
        613,
        "C:/ws/semantic_solver_decl_call_initializer.lua",
        source,
    );

    let value_decl = compilation
        .file()
        .decl_tree(FileId::new(613))
        .expect("decl tree")
        .decls
        .clone()
        .into_iter()
        .find(|decl| {
            decl.name == "value" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("value decl");

    let decl_summary = compilation
        .semantic()
        .file()
        .decl_summary(FileId::new(613), value_decl.id)
        .expect("decl summary");

    assert_eq!(
        decl_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert!(
        !decl_summary
            .local_value_shell
            .candidate_type_offsets
            .is_empty()
    );
}

#[test]
fn test_summary_builder_semantic_solver_decl_value_shell_consumes_overload_return_rows() {
    let mut compilation = setup_compilation();
    let source = r#"---@return_overload string
---@return_overload integer
local function make()
  return "x"
end

local value = make()
"#;
    set_test_file(
        &mut compilation,
        615,
        "C:/ws/semantic_solver_decl_overload_return_rows.lua",
        source,
    );

    let value_decl = compilation
        .file()
        .decl_tree(FileId::new(615))
        .expect("decl tree")
        .decls
        .clone()
        .into_iter()
        .find(|decl| {
            decl.name == "value" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("value decl");

    let decl_summary = compilation
        .semantic()
        .file()
        .decl_summary(FileId::new(615), value_decl.id)
        .expect("decl summary");

    assert_eq!(
        decl_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        decl_summary.local_value_shell.candidate_type_offsets.len(),
        2
    );
}

#[test]
fn test_summary_builder_semantic_solver_decl_value_shell_prefers_best_call_candidate() {
    let mut compilation = setup_compilation();
    let source = r#"---@return integer
function make(a, b)
  return 1
end

---@return string
local function make(a)
  return "x"
end

local value = make(1)
"#;
    set_test_file(
        &mut compilation,
        614,
        "C:/ws/semantic_solver_decl_best_call_candidate.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(614))
        .expect("signature summary")
        .signatures
        .clone();
    let selected_signature = signatures
        .iter()
        .filter(|signature| signature.name.as_deref() == Some("make"))
        .find(|signature| signature.params.len() == 1)
        .expect("selected signature");

    let expected_shell = compilation
        .semantic()
        .file()
        .signature_return_value_shell(FileId::new(614), selected_signature.syntax_offset)
        .expect("selected signature shell");

    let value_decl = compilation
        .file()
        .decl_tree(FileId::new(614))
        .expect("decl tree")
        .decls
        .clone()
        .into_iter()
        .find(|decl| {
            decl.name == "value" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("value decl");

    let decl_summary = compilation
        .semantic()
        .file()
        .decl_summary(FileId::new(614), value_decl.id)
        .expect("decl summary");

    assert_eq!(
        decl_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        decl_summary.local_value_shell.candidate_type_offsets,
        expected_shell.candidate_type_offsets
    );
}

#[test]
fn test_summary_builder_semantic_solver_decl_value_shell_uses_multi_return_slot() {
    let mut compilation = setup_compilation();
    let source = r#"---@return string, integer
local function pair()
  return "x", 1
end

local first, second = pair()
"#;
    set_test_file(
        &mut compilation,
        616,
        "C:/ws/semantic_solver_decl_multi_return_slot.lua",
        source,
    );

    let signature = compilation
        .doc()
        .signatures(FileId::new(616))
        .expect("signature summary")
        .signatures
        .clone()
        .into_iter()
        .find(|signature| signature.name.as_deref() == Some("pair"))
        .expect("pair signature");
    let signature_summary = compilation
        .semantic()
        .file()
        .signature_return_summary(FileId::new(616), signature.syntax_offset)
        .expect("signature return summary");

    let decls = compilation
        .file()
        .decl_tree(FileId::new(616))
        .expect("decl tree")
        .decls
        .clone();
    let first_decl = decls
        .iter()
        .find(|decl| {
            decl.name == "first" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("first decl");
    let second_decl = decls
        .iter()
        .find(|decl| {
            decl.name == "second" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("second decl");

    let first_decl_summary = compilation
        .semantic()
        .file()
        .decl_summary(FileId::new(616), first_decl.id)
        .expect("first decl summary");
    let second_decl_summary = compilation
        .semantic()
        .file()
        .decl_summary(FileId::new(616), second_decl.id)
        .expect("second decl summary");

    assert_eq!(
        first_decl_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        second_decl_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        first_decl_summary.local_value_shell.candidate_type_offsets,
        signature_summary.values[0].doc_return_type_offsets
    );
    assert_eq!(
        second_decl_summary.local_value_shell.candidate_type_offsets,
        signature_summary.values[1].doc_return_type_offsets
    );
    assert_ne!(
        first_decl_summary.local_value_shell.candidate_type_offsets,
        second_decl_summary.local_value_shell.candidate_type_offsets
    );
}

#[test]
fn test_summary_builder_semantic_solver_decl_value_shell_keeps_resolved_first_slot_in_partial_multi_return()
 {
    let mut compilation = setup_compilation();
    let source = r#"local function pair()
  return 1, missing
end

local first, second = pair()
"#;
    set_test_file(
        &mut compilation,
        617,
        "C:/ws/semantic_solver_decl_partial_multi_return_slot.lua",
        source,
    );

    let decls = compilation
        .file()
        .decl_tree(FileId::new(617))
        .expect("decl tree")
        .decls
        .clone();
    let first_decl = decls
        .iter()
        .find(|decl| {
            decl.name == "first" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("first decl");
    let second_decl = decls
        .iter()
        .find(|decl| {
            decl.name == "second" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("second decl");

    let first_decl_summary = compilation
        .semantic()
        .file()
        .decl_summary(FileId::new(617), first_decl.id)
        .expect("first decl summary");
    let second_decl_summary = compilation
        .semantic()
        .file()
        .decl_summary(FileId::new(617), second_decl.id)
        .expect("second decl summary");

    assert_eq!(
        first_decl_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        second_decl_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Partial
    );
}

#[test]
fn test_summary_builder_semantic_solver_member_summary_uses_multi_return_slot() {
    let mut compilation = setup_compilation();
    let source = r#"---@return integer
---@return string
local function pair()
  return 1, "two"
end

local holder = {}
holder.first, holder.second = pair()
"#;
    set_test_file(
        &mut compilation,
        621,
        "C:/ws/semantic_solver_member_multi_return_slot.lua",
        source,
    );

    let signature = compilation
        .doc()
        .signatures(FileId::new(621))
        .expect("signature summary")
        .signatures
        .clone()
        .into_iter()
        .find(|signature| signature.name.as_deref() == Some("pair"))
        .expect("pair signature");
    let signature_summary = compilation
        .semantic()
        .file()
        .signature_return_summary(FileId::new(621), signature.syntax_offset)
        .expect("pair signature return summary");

    let members = compilation
        .file()
        .members(FileId::new(621))
        .expect("member summary")
        .members
        .clone();
    let first_member = members
        .iter()
        .find(|member| member.target.member_name.as_str() == "first")
        .expect("first member");
    let second_member = members
        .iter()
        .find(|member| member.target.member_name.as_str() == "second")
        .expect("second member");

    let first_summary = compilation
        .semantic()
        .file()
        .member_summary(FileId::new(621), first_member.target.clone())
        .expect("first member summary");
    let second_summary = compilation
        .semantic()
        .file()
        .member_summary(FileId::new(621), second_member.target.clone())
        .expect("second member summary");

    assert_eq!(
        first_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        second_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        first_summary.local_value_shell.candidate_type_offsets,
        signature_summary.values[0].doc_return_type_offsets
    );
    assert_eq!(
        second_summary.local_value_shell.candidate_type_offsets,
        signature_summary.values[1].doc_return_type_offsets
    );
    assert_ne!(
        first_summary.local_value_shell.candidate_type_offsets,
        second_summary.local_value_shell.candidate_type_offsets
    );
}

#[test]
fn test_summary_builder_semantic_solver_decl_summary_resolves_local_function_statement() {
    let mut compilation = setup_compilation();
    let source = r#"local function fn()
  return 1
end
"#;
    set_test_file(
        &mut compilation,
        608,
        "C:/ws/semantic_solver_decl_local_func.lua",
        source,
    );

    let fn_decl = compilation
        .file()
        .decl_tree(FileId::new(608))
        .expect("decl tree")
        .decls
        .clone()
        .into_iter()
        .find(|decl| decl.name == "fn" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. }))
        .expect("fn decl");

    let decl_type = compilation
        .types()
        .decl(FileId::new(608), fn_decl.id)
        .expect("decl type");
    let decl_summary = compilation
        .semantic()
        .file()
        .decl_summary(FileId::new(608), fn_decl.id)
        .expect("decl summary");

    assert!(decl_type.initializer_offset.is_some());
    assert!(decl_type.value_signature_offset.is_some());
    assert_eq!(
        decl_summary.value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        decl_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        decl_summary.value_shell,
        decl_summary.fixedpoint_value_shell
    );
}

#[test]
fn test_summary_builder_semantic_solver_member_summary_resolves_property_closure_initializer() {
    let mut compilation = setup_compilation();
    let source = r#"local holder = {
  run = function()
    return 1
  end,
}
"#;
    set_test_file(
        &mut compilation,
        607,
        "C:/ws/semantic_solver_property_member_signature.lua",
        source,
    );

    let holder_decl = compilation
        .file()
        .decl_tree(FileId::new(607))
        .expect("decl tree")
        .decls
        .clone()
        .into_iter()
        .find(|decl| {
            decl.name == "holder" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("holder decl");
    let run_member_target = crate::SalsaMemberTargetSummary {
        root: crate::SalsaMemberRootSummary::LocalDecl {
            name: "holder".into(),
            decl_id: holder_decl.id,
        },
        owner_segments: Vec::new().into(),
        member_name: "run".into(),
    };
    let run_member_target_handle = crate::SalsaMemberTargetId::from(&run_member_target);

    let member_summary = compilation
        .semantic()
        .file()
        .member_summary(FileId::new(607), run_member_target_handle)
        .expect("member summary");

    assert_eq!(
        member_summary.value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        member_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        member_summary.value_shell,
        member_summary.fixedpoint_value_shell
    );
}

#[test]
fn test_summary_builder_semantic_solver_member_summary_resolves_property_table_initializer() {
    let mut compilation = setup_compilation();
    let source = r#"local holder = {
  config = {},
}
"#;
    set_test_file(
        &mut compilation,
        610,
        "C:/ws/semantic_solver_property_member_table.lua",
        source,
    );

    let holder_decl = compilation
        .file()
        .decl_tree(FileId::new(610))
        .expect("decl tree")
        .decls
        .clone()
        .into_iter()
        .find(|decl| {
            decl.name == "holder" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("holder decl");
    let config_member_target = crate::SalsaMemberTargetSummary {
        root: crate::SalsaMemberRootSummary::LocalDecl {
            name: "holder".into(),
            decl_id: holder_decl.id,
        },
        owner_segments: Vec::new().into(),
        member_name: "config".into(),
    };
    let config_member_target_handle = crate::SalsaMemberTargetId::from(&config_member_target);

    let member_summary = compilation
        .semantic()
        .file()
        .member_summary(FileId::new(610), config_member_target_handle)
        .expect("member summary");

    assert_eq!(
        member_summary.value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        member_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        member_summary.value_shell,
        member_summary.fixedpoint_value_shell
    );
}

#[test]
fn test_summary_builder_semantic_solver_decl_summary_resolves_named_type_only_decl() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Box
local Box
"#;
    set_test_file(
        &mut compilation,
        618,
        "C:/ws/semantic_solver_decl_named_type_only.lua",
        source,
    );

    let box_decl = compilation
        .file()
        .decl_tree(FileId::new(618))
        .expect("decl tree")
        .decls
        .clone()
        .into_iter()
        .find(|decl| decl.name == "Box" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. }))
        .expect("Box decl");

    let decl_type = compilation
        .types()
        .decl(FileId::new(618), box_decl.id)
        .expect("Box decl type");
    let decl_summary = compilation
        .semantic()
        .file()
        .decl_summary(FileId::new(618), box_decl.id)
        .expect("decl summary");

    assert_eq!(decl_type.named_type_names, vec!["Box"]);
    assert_eq!(
        decl_summary.value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert_eq!(
        decl_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert!(decl_summary.value_shell.candidate_type_offsets.is_empty());
}

#[test]
fn test_summary_builder_semantic_solver_weak_partial_cycle_drops_out_of_fixedpoint() {
    let input_shell = crate::SalsaSemanticValueShellSummary {
        state: crate::SalsaSemanticResolveStateSummary::Unknown,
        candidate_type_offsets: Vec::new(),
    };
    let local_shell = crate::SalsaSemanticValueShellSummary {
        state: crate::SalsaSemanticResolveStateSummary::Partial,
        candidate_type_offsets: Vec::new(),
    };

    let (fixedpoint_shell, iterations) =
        crate::solve_component_fixedpoint(&input_shell, &local_shell, true, false);

    assert_eq!(
        fixedpoint_shell.state,
        crate::SalsaSemanticResolveStateSummary::Unknown
    );
    assert!(iterations >= 1);
}

#[test]
fn test_summary_builder_semantic_solver_weak_partial_input_drops_out_of_propagation() {
    let input_shell = crate::SalsaSemanticValueShellSummary {
        state: crate::SalsaSemanticResolveStateSummary::Partial,
        candidate_type_offsets: Vec::new(),
    };
    let local_shell = crate::SalsaSemanticValueShellSummary {
        state: crate::SalsaSemanticResolveStateSummary::Unknown,
        candidate_type_offsets: Vec::new(),
    };

    let (fixedpoint_shell, iterations) =
        crate::solve_component_fixedpoint(&input_shell, &local_shell, true, true);

    assert_eq!(
        fixedpoint_shell.state,
        crate::SalsaSemanticResolveStateSummary::Unknown
    );
    assert!(fixedpoint_shell.candidate_type_offsets.is_empty());
    assert!(iterations >= 1);
}

#[test]
fn test_summary_builder_semantic_solver_preserves_weak_local_shell_before_transfer() {
    let mut compilation = setup_compilation();
    let source = r#"local value = missing_value
"#;
    set_test_file(
        &mut compilation,
        640,
        "C:/ws/semantic_solver_weak_local_shell.lua",
        source,
    );

    let value_decl = compilation
        .file()
        .decl_tree(FileId::new(640))
        .expect("decl tree")
        .decls
        .clone()
        .into_iter()
        .find(|decl| {
            decl.name == "value" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("value decl");

    let decl_summary = compilation
        .semantic()
        .file()
        .decl_summary(FileId::new(640), value_decl.id)
        .expect("decl summary");

    assert_eq!(
        decl_summary.local_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Partial
    );
    assert!(
        decl_summary
            .local_value_shell
            .candidate_type_offsets
            .is_empty()
    );
    assert_eq!(
        decl_summary.fixedpoint_value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Unknown
    );
    assert_eq!(
        decl_summary.value_shell,
        decl_summary.fixedpoint_value_shell
    );
}

#[test]
fn test_summary_builder_semantic_solver_consumes_signature_name_and_member_predecessors() {
    let mut compilation = setup_compilation();
    let source = r#"local value = 1
local holder = {}
holder.alias = 1

local function from_name()
  return value
end

local function from_member()
  return holder.alias
end
"#;
    set_test_file(
        &mut compilation,
        603,
        "C:/ws/semantic_solver_signature_targets.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(603))
        .expect("signature summary")
        .signatures
        .clone();
    let decl_tree = compilation
        .file()
        .decl_tree(FileId::new(603))
        .expect("decl tree");
    let members = compilation
        .file()
        .members(FileId::new(603))
        .expect("member summary");
    let from_name_signature = signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("from_name"))
        .expect("from_name signature");
    let from_member_signature = signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("from_member"))
        .expect("from_member signature");
    let value_decl = decl_tree
        .decls
        .iter()
        .find(|decl| decl.name.as_str() == "value")
        .expect("value decl");
    let alias_member = members
        .members
        .iter()
        .find(|member| member.target.member_name.as_str() == "alias")
        .expect("alias member");

    let scc_index = compilation
        .semantic()
        .file()
        .graph_scc_index(FileId::new(603))
        .expect("graph scc index");
    let decl_type_index = compilation
        .types()
        .decl_index(FileId::new(603))
        .expect("decl type index");
    let member_type_index = compilation
        .types()
        .member_index(FileId::new(603))
        .expect("member type index");
    let signature_return_index = compilation
        .doc()
        .signature_return_index(FileId::new(603))
        .expect("signature return index");
    let for_range_iter_index = compilation
        .flow()
        .for_range_iter_index(FileId::new(603))
        .expect("for range iter index");
    let use_sites = compilation
        .lexical()
        .use_sites(FileId::new(603))
        .expect("use sites");
    let signature_explain_index = compilation
        .doc()
        .signature_explain_index(FileId::new(603))
        .expect("signature explain index");
    let module_export = compilation
        .semantic()
        .file()
        .summary(FileId::new(603))
        .expect("semantic summary")
        .module_export
        .clone();

    let from_name_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(603),
            crate::SalsaSemanticGraphNodeSummary::SignatureReturn(
                from_name_signature.syntax_offset,
            ),
        )
        .expect("from_name component");
    let from_member_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(603),
            crate::SalsaSemanticGraphNodeSummary::SignatureReturn(
                from_member_signature.syntax_offset,
            ),
        )
        .expect("from_member component");
    let value_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(603),
            crate::SalsaSemanticGraphNodeSummary::DeclValue(value_decl.id),
        )
        .expect("value component");
    let alias_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(603),
            crate::SalsaSemanticGraphNodeSummary::MemberValue(alias_member.target.clone()),
        )
        .expect("alias component");

    let execution = compilation
        .semantic()
        .file()
        .solver_execution(FileId::new(603))
        .expect("solver execution");
    let after_name = crate::build_semantic_solver_step_summary(
        &execution,
        from_name_component.component_id,
        &scc_index,
        &decl_type_index,
        &member_type_index,
        &signature_return_index,
        &for_range_iter_index,
        &use_sites,
        &signature_explain_index,
        module_export.as_ref(),
    )
    .expect("from_name step")
    .next_execution;
    let value_step = crate::build_semantic_solver_step_summary(
        &after_name,
        value_component.component_id,
        &scc_index,
        &decl_type_index,
        &member_type_index,
        &signature_return_index,
        &for_range_iter_index,
        &use_sites,
        &signature_explain_index,
        module_export.as_ref(),
    )
    .expect("value step");
    let value_result = value_step.component_result.expect("value result");
    assert!(
        value_result
            .consumed_predecessor_component_ids
            .contains(&from_name_component.component_id)
    );

    let after_member = crate::build_semantic_solver_step_summary(
        &execution,
        from_member_component.component_id,
        &scc_index,
        &decl_type_index,
        &member_type_index,
        &signature_return_index,
        &for_range_iter_index,
        &use_sites,
        &signature_explain_index,
        module_export.as_ref(),
    )
    .expect("from_member step")
    .next_execution;
    let alias_step = crate::build_semantic_solver_step_summary(
        &after_member,
        alias_component.component_id,
        &scc_index,
        &decl_type_index,
        &member_type_index,
        &signature_return_index,
        &for_range_iter_index,
        &use_sites,
        &signature_explain_index,
        module_export.as_ref(),
    )
    .expect("alias step");
    let alias_result = alias_step.component_result.expect("alias result");
    assert!(
        alias_result
            .consumed_predecessor_component_ids
            .contains(&from_member_component.component_id)
    );

    let tracked_value_result = compilation
        .semantic()
        .file()
        .solver_component_result_summary(FileId::new(603), value_component.component_id)
        .expect("tracked value component result");
    let tracked_value_summary = compilation
        .semantic()
        .file()
        .decl_summary(FileId::new(603), value_decl.id)
        .expect("tracked value summary");
    let tracked_from_name_result = compilation
        .semantic()
        .file()
        .solver_component_result_summary(FileId::new(603), from_name_component.component_id)
        .expect("tracked from_name component result");
    assert!(
        tracked_value_result
            .consumed_predecessor_component_ids
            .contains(&from_name_component.component_id)
    );
    assert_eq!(
        tracked_value_result.input_value_shell,
        tracked_from_name_result.fixedpoint_value_shell
    );
    assert_eq!(
        tracked_value_result.propagated_value_shell,
        tracked_from_name_result.fixedpoint_value_shell
    );
    assert_eq!(
        tracked_value_summary.value_shell,
        tracked_value_result.fixedpoint_value_shell
    );

    let tracked_alias_result = compilation
        .semantic()
        .file()
        .solver_component_result_summary(FileId::new(603), alias_component.component_id)
        .expect("tracked alias component result");
    let tracked_alias_summary = compilation
        .semantic()
        .file()
        .member_summary(FileId::new(603), alias_member.target.clone())
        .expect("tracked alias summary");
    let tracked_from_member_result = compilation
        .semantic()
        .file()
        .solver_component_result_summary(FileId::new(603), from_member_component.component_id)
        .expect("tracked from_member component result");
    assert!(
        tracked_alias_result
            .consumed_predecessor_component_ids
            .contains(&from_member_component.component_id)
    );
    assert_eq!(
        tracked_alias_result.input_value_shell,
        tracked_from_member_result.fixedpoint_value_shell
    );
    assert_eq!(
        tracked_alias_result.propagated_value_shell,
        tracked_from_member_result.fixedpoint_value_shell
    );
    assert_eq!(
        tracked_alias_summary.value_shell,
        tracked_alias_result.fixedpoint_value_shell
    );
}

#[test]
fn test_summary_builder_semantic_solver_member_shell_ignores_empty_candidates() {
    let empty_member_target = crate::SalsaMemberTargetSummary {
        root: crate::SalsaMemberRootSummary::LocalDecl {
            name: "holder".into(),
            decl_id: crate::SalsaDeclId::new(TextSize::from(1)),
        },
        owner_segments: Vec::new().into(),
        member_name: "value".into(),
    };
    let empty_member_type = crate::SalsaMemberTypeInfoSummary {
        target: empty_member_target.clone().into(),
        candidates: vec![crate::SalsaTypeCandidateSummary {
            origin: crate::SalsaTypeCandidateOriginSummary::Member(empty_member_target.into()),
            explicit_type_offsets: Vec::new(),
            named_type_names: Vec::new(),
            initializer_offset: None,
            value_expr_syntax_id: None,
            value_result_index: 0,
            source_call_syntax_id: None,
            signature_offset: None,
        }],
    };
    let empty_shell = crate::build_semantic_value_shell_from_member_type(Some(&empty_member_type));
    assert_eq!(
        empty_shell.state,
        crate::SalsaSemanticResolveStateSummary::Unknown
    );

    let initialized_member_target = crate::SalsaMemberTargetSummary {
        root: crate::SalsaMemberRootSummary::LocalDecl {
            name: "holder".into(),
            decl_id: crate::SalsaDeclId::new(TextSize::from(2)),
        },
        owner_segments: Vec::new().into(),
        member_name: "value".into(),
    };
    let initialized_member_type = crate::SalsaMemberTypeInfoSummary {
        target: initialized_member_target.clone().into(),
        candidates: vec![crate::SalsaTypeCandidateSummary {
            origin: crate::SalsaTypeCandidateOriginSummary::Member(
                initialized_member_target.into(),
            ),
            explicit_type_offsets: Vec::new(),
            named_type_names: Vec::new(),
            initializer_offset: Some(rowan::TextSize::from(12)),
            value_expr_syntax_id: None,
            value_result_index: 0,
            source_call_syntax_id: None,
            signature_offset: None,
        }],
    };
    let initialized_shell =
        crate::build_semantic_value_shell_from_member_type(Some(&initialized_member_type));
    assert_eq!(
        initialized_shell.state,
        crate::SalsaSemanticResolveStateSummary::Partial
    );

    let named_member_target = crate::SalsaMemberTargetSummary {
        root: crate::SalsaMemberRootSummary::LocalDecl {
            name: "holder".into(),
            decl_id: crate::SalsaDeclId::new(TextSize::from(3)),
        },
        owner_segments: Vec::new().into(),
        member_name: "value".into(),
    };
    let named_member_type = crate::SalsaMemberTypeInfoSummary {
        target: named_member_target.clone().into(),
        candidates: vec![crate::SalsaTypeCandidateSummary {
            origin: crate::SalsaTypeCandidateOriginSummary::Member(named_member_target.into()),
            explicit_type_offsets: Vec::new(),
            named_type_names: vec!["Box".into()],
            initializer_offset: None,
            value_expr_syntax_id: None,
            value_result_index: 0,
            source_call_syntax_id: None,
            signature_offset: None,
        }],
    };
    let named_shell = crate::build_semantic_value_shell_from_member_type(Some(&named_member_type));
    assert_eq!(
        named_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
}

#[test]
fn test_summary_builder_semantic_solver_candidate_shell_supports_property_like_candidates() {
    let empty_shell = crate::build_semantic_value_shell_from_candidates(&[]);
    assert_eq!(
        empty_shell.state,
        crate::SalsaSemanticResolveStateSummary::Unknown
    );

    let partial_candidates = vec![crate::SalsaTypeCandidateSummary {
        origin: crate::SalsaTypeCandidateOriginSummary::Property(rowan::TextSize::from(10)),
        explicit_type_offsets: Vec::new(),
        named_type_names: Vec::new(),
        initializer_offset: Some(rowan::TextSize::from(12)),
        value_expr_syntax_id: None,
        value_result_index: 1,
        source_call_syntax_id: None,
        signature_offset: None,
    }];
    let partial_shell = crate::build_semantic_value_shell_from_candidates(&partial_candidates);
    assert_eq!(
        partial_shell.state,
        crate::SalsaSemanticResolveStateSummary::Partial
    );

    let resolved_candidates = vec![crate::SalsaTypeCandidateSummary {
        origin: crate::SalsaTypeCandidateOriginSummary::Property(rowan::TextSize::from(20)),
        explicit_type_offsets: Vec::new(),
        named_type_names: Vec::new(),
        initializer_offset: Some(rowan::TextSize::from(22)),
        value_expr_syntax_id: None,
        value_result_index: 1,
        source_call_syntax_id: None,
        signature_offset: Some(rowan::TextSize::from(24)),
    }];
    let resolved_shell = crate::build_semantic_value_shell_from_candidates(&resolved_candidates);
    assert_eq!(
        resolved_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
}

#[test]
fn test_summary_builder_semantic_solver_for_range_name_source_uses_shared_candidate_shell() {
    let summary = crate::SalsaForRangeIterQuerySummary {
        loop_offset: rowan::TextSize::from(10),
        iter_expr_offsets: vec![rowan::TextSize::from(11)],
        state: crate::SalsaForRangeIterResolveStateSummary::Resolved,
        source: Some(crate::SalsaForRangeIterSourceSummary {
            expr_offset: rowan::TextSize::from(11),
            kind: crate::SalsaForRangeIterSourceKindSummary::Name,
            name_type: Some(crate::SalsaProgramPointTypeInfoSummary {
                syntax_offset: rowan::TextSize::from(11),
                program_point_offset: rowan::TextSize::from(12),
                name_use: crate::SalsaNameUseSummary {
                    syntax_offset: rowan::TextSize::from(11),
                    syntax_id: crate::SalsaSyntaxIdSummary {
                        kind: LuaSyntaxKind::None,
                        start_offset: rowan::TextSize::from(11),
                        end_offset: rowan::TextSize::from(15),
                    },
                    name: "iter".into(),
                    role: crate::SalsaUseSiteRoleSummary::Read,
                    resolution: crate::SalsaNameUseResolutionSummary::Global,
                },
                base_decl_type: None,
                candidates: vec![crate::SalsaTypeCandidateSummary {
                    origin: crate::SalsaTypeCandidateOriginSummary::Assignment(
                        rowan::TextSize::from(13),
                    ),
                    explicit_type_offsets: Vec::new(),
                    named_type_names: vec!["Box".into()],
                    initializer_offset: None,
                    value_expr_syntax_id: None,
                    value_result_index: 0,
                    source_call_syntax_id: None,
                    signature_offset: None,
                }],
                active_narrows: Vec::new(),
            }),
            member_type: None,
            call: None,
        }),
        iter_vars: Vec::new(),
    };

    let shell = crate::build_semantic_value_shell_from_for_range_iter_source(&summary, None);
    assert_eq!(
        shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
}

#[test]
fn test_summary_builder_semantic_solver_for_range_member_source_uses_shared_candidate_shell() {
    let member_target = crate::SalsaMemberTargetSummary {
        root: crate::SalsaMemberRootSummary::LocalDecl {
            name: "holder".into(),
            decl_id: crate::SalsaDeclId::new(rowan::TextSize::from(9)),
        },
        owner_segments: Vec::new().into(),
        member_name: "value".into(),
    };
    let summary = crate::SalsaForRangeIterQuerySummary {
        loop_offset: rowan::TextSize::from(20),
        iter_expr_offsets: vec![rowan::TextSize::from(21)],
        state: crate::SalsaForRangeIterResolveStateSummary::Resolved,
        source: Some(crate::SalsaForRangeIterSourceSummary {
            expr_offset: rowan::TextSize::from(21),
            kind: crate::SalsaForRangeIterSourceKindSummary::Member,
            name_type: None,
            member_type: Some(crate::SalsaProgramPointMemberTypeInfoSummary {
                syntax_offset: rowan::TextSize::from(21),
                program_point_offset: rowan::TextSize::from(22),
                member_use: crate::SalsaMemberUseSummary {
                    syntax_offset: rowan::TextSize::from(21),
                    syntax_id: crate::SalsaSyntaxIdSummary {
                        kind: LuaSyntaxKind::None,
                        start_offset: rowan::TextSize::from(21),
                        end_offset: rowan::TextSize::from(27),
                    },
                    role: crate::SalsaUseSiteRoleSummary::Read,
                    target: member_target.clone().into(),
                },
                base_member_type: None,
                owner_decl_id: None,
                owner_candidates: Vec::new(),
                candidates: vec![crate::SalsaTypeCandidateSummary {
                    origin: crate::SalsaTypeCandidateOriginSummary::Member(member_target.into()),
                    explicit_type_offsets: Vec::new(),
                    named_type_names: vec!["Box".into()],
                    initializer_offset: None,
                    value_expr_syntax_id: None,
                    value_result_index: 0,
                    source_call_syntax_id: None,
                    signature_offset: None,
                }],
                active_narrows: Vec::new(),
            }),
            call: None,
        }),
        iter_vars: Vec::new(),
    };

    let shell = crate::build_semantic_value_shell_from_for_range_iter_source(&summary, None);
    assert_eq!(
        shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
}

#[test]
fn test_summary_builder_semantic_solver_for_range_call_source_treats_return_rows_as_resolved() {
    let summary = crate::SalsaForRangeIterQuerySummary {
        loop_offset: rowan::TextSize::from(30),
        iter_expr_offsets: vec![rowan::TextSize::from(31)],
        state: crate::SalsaForRangeIterResolveStateSummary::Resolved,
        source: Some(crate::SalsaForRangeIterSourceSummary {
            expr_offset: rowan::TextSize::from(31),
            kind: crate::SalsaForRangeIterSourceKindSummary::Call,
            name_type: None,
            member_type: None,
            call: Some(crate::SalsaCallExplainSummary {
                call: crate::SalsaCallSummary {
                    syntax_offset: rowan::TextSize::from(31),
                    syntax_id: crate::SalsaSyntaxIdSummary {
                        kind: LuaSyntaxKind::None,
                        start_offset: rowan::TextSize::from(31),
                        end_offset: rowan::TextSize::from(36),
                    },
                    callee_offset: rowan::TextSize::from(32),
                    kind: crate::SalsaCallKindSummary::Normal,
                    is_colon_call: false,
                    is_single_arg_no_parens: false,
                    arg_expr_offsets: Vec::new(),
                    call_generic_type_offsets: Vec::new(),
                },
                lexical_call: None,
                call_generic_types: Vec::new(),
                candidate_signature_offsets: Vec::new(),
                resolved_signature_offset: None,
                resolved_signature: None,
                args: Vec::new(),
                overload_returns: Vec::new(),
                returns: vec![crate::SalsaSignatureReturnExplainSummary {
                    syntax_offset: rowan::TextSize::from(33),
                    items: vec![crate::SalsaSignatureReturnItemExplainSummary {
                        name: None,
                        doc_type: crate::SalsaSignatureTypeExplainSummary {
                            type_ref: crate::SalsaDocTypeRef::Incomplete,
                            lowered: None,
                        },
                    }],
                }],
            }),
        }),
        iter_vars: Vec::new(),
    };

    let empty_signature_returns = crate::SalsaSignatureReturnQueryIndex {
        signatures: Vec::new(),
    };
    let shell = crate::build_semantic_value_shell_from_for_range_iter_source(
        &summary,
        Some(&empty_signature_returns),
    );
    assert_eq!(
        shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
}

#[test]
fn test_summary_builder_semantic_solver_for_range_call_source_keeps_candidate_only_partial() {
    let summary = crate::SalsaForRangeIterQuerySummary {
        loop_offset: rowan::TextSize::from(40),
        iter_expr_offsets: vec![rowan::TextSize::from(41)],
        state: crate::SalsaForRangeIterResolveStateSummary::Resolved,
        source: Some(crate::SalsaForRangeIterSourceSummary {
            expr_offset: rowan::TextSize::from(41),
            kind: crate::SalsaForRangeIterSourceKindSummary::Call,
            name_type: None,
            member_type: None,
            call: Some(crate::SalsaCallExplainSummary {
                call: crate::SalsaCallSummary {
                    syntax_offset: rowan::TextSize::from(41),
                    syntax_id: crate::SalsaSyntaxIdSummary {
                        kind: LuaSyntaxKind::None,
                        start_offset: rowan::TextSize::from(41),
                        end_offset: rowan::TextSize::from(46),
                    },
                    callee_offset: rowan::TextSize::from(42),
                    kind: crate::SalsaCallKindSummary::Normal,
                    is_colon_call: false,
                    is_single_arg_no_parens: false,
                    arg_expr_offsets: Vec::new(),
                    call_generic_type_offsets: Vec::new(),
                },
                lexical_call: None,
                call_generic_types: Vec::new(),
                candidate_signature_offsets: vec![rowan::TextSize::from(45)],
                resolved_signature_offset: None,
                resolved_signature: None,
                args: Vec::new(),
                overload_returns: Vec::new(),
                returns: Vec::new(),
            }),
        }),
        iter_vars: Vec::new(),
    };

    let empty_signature_returns = crate::SalsaSignatureReturnQueryIndex {
        signatures: Vec::new(),
    };
    let shell = crate::build_semantic_value_shell_from_for_range_iter_source(
        &summary,
        Some(&empty_signature_returns),
    );
    assert_eq!(
        shell.state,
        crate::SalsaSemanticResolveStateSummary::Partial
    );
}

#[test]
fn test_summary_builder_call_explain_is_resolved_when_return_rows_exist() {
    let call = crate::SalsaCallExplainSummary {
        call: crate::SalsaCallSummary {
            syntax_offset: rowan::TextSize::from(50),
            syntax_id: crate::SalsaSyntaxIdSummary {
                kind: LuaSyntaxKind::None,
                start_offset: rowan::TextSize::from(50),
                end_offset: rowan::TextSize::from(55),
            },
            callee_offset: rowan::TextSize::from(51),
            kind: crate::SalsaCallKindSummary::Normal,
            is_colon_call: false,
            is_single_arg_no_parens: false,
            arg_expr_offsets: Vec::new(),
            call_generic_type_offsets: Vec::new(),
        },
        lexical_call: None,
        call_generic_types: Vec::new(),
        candidate_signature_offsets: Vec::new(),
        resolved_signature_offset: None,
        resolved_signature: None,
        args: Vec::new(),
        overload_returns: Vec::new(),
        returns: vec![crate::SalsaSignatureReturnExplainSummary {
            syntax_offset: rowan::TextSize::from(52),
            items: vec![crate::SalsaSignatureReturnItemExplainSummary {
                name: None,
                doc_type: crate::SalsaSignatureTypeExplainSummary {
                    type_ref: crate::SalsaDocTypeRef::Incomplete,
                    lowered: None,
                },
            }],
        }],
    };

    assert!(crate::call_explain_is_resolved(&call));
}

#[test]
fn test_summary_builder_call_explain_is_not_resolved_for_candidate_only_calls() {
    let call = crate::SalsaCallExplainSummary {
        call: crate::SalsaCallSummary {
            syntax_offset: rowan::TextSize::from(60),
            syntax_id: crate::SalsaSyntaxIdSummary {
                kind: LuaSyntaxKind::None,
                start_offset: rowan::TextSize::from(60),
                end_offset: rowan::TextSize::from(65),
            },
            callee_offset: rowan::TextSize::from(61),
            kind: crate::SalsaCallKindSummary::Normal,
            is_colon_call: false,
            is_single_arg_no_parens: false,
            arg_expr_offsets: Vec::new(),
            call_generic_type_offsets: Vec::new(),
        },
        lexical_call: None,
        call_generic_types: Vec::new(),
        candidate_signature_offsets: vec![rowan::TextSize::from(64)],
        resolved_signature_offset: None,
        resolved_signature: None,
        args: Vec::new(),
        overload_returns: Vec::new(),
        returns: Vec::new(),
    };

    assert!(!crate::call_explain_is_resolved(&call));
}
