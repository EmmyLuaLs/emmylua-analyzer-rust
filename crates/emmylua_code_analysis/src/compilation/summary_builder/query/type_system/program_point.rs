use std::collections::{BTreeMap, BTreeSet};

use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaBlock, LuaCallExpr, LuaChunk, LuaExpr, LuaIfStat, LuaIndexExpr,
    LuaIndexKey, LuaLiteralToken, LuaStat, LuaTableExpr, NumberResult, UnaryOperator,
};
use rowan::TextSize;
use smol_str::SmolStr;

use super::assignments::{
    SalsaLocalAssignmentQueryIndex, SalsaLocalAssignmentSummary,
    latest_assignments_at_program_point,
};
use super::data::{
    SalsaDeclTypeQueryIndex, SalsaMemberTypeQueryIndex, SalsaProgramPointMemberTypeInfoSummary,
    SalsaProgramPointTypeInfoSummary, SalsaTypeCandidateSummary, SalsaTypeNarrowSummary,
    decl_type_to_candidate, dedupe_type_candidates, find_decl_type_info, find_member_use_type_info,
    property_to_candidate,
};
use super::narrow::{apply_narrows_to_candidate, narrow_type_offsets};
use super::shared::{innermost_block_at_offset, name_expr_targets_decl, nearest_ancestor_block};
use crate::compilation::summary_builder::analysis::analyze_table_expr_shape;
use crate::compilation::summary_builder::query::{
    SalsaDocTagQueryIndex, SalsaDocTypeLoweredIndex, SalsaDocTypeLoweredKind,
    SalsaDocTypeLoweredObjectFieldKey, SalsaPropertyQueryIndex, SalsaSignatureExplainIndex,
    build_property_query_index, collect_doc_tags_for_owner_in_index,
    collect_properties_for_type_and_key_in_index, collect_properties_for_type_in_index,
    find_call_explain_by_syntax_id, find_resolved_doc_type_by_key_from_parts,
};
use crate::{
    SalsaDeclId, SalsaDeclTreeSummary, SalsaDocObjectFieldKeySummary, SalsaDocOwnerSummary,
    SalsaDocSummary, SalsaDocTagKindSummary, SalsaDocTypeDefKindSummary, SalsaDocTypeIndexSummary,
    SalsaDocTypeKindSummary, SalsaDocTypeNodeKey, SalsaDocTypeRef,
    SalsaDocTypeUnaryOperatorSummary, SalsaMemberRootSummary, SalsaMemberUseSummary,
    SalsaNameUseResolutionSummary, SalsaNameUseSummary, SalsaPropertyIndexSummary,
    SalsaSequenceShapeKindSummary, SalsaSyntaxIdSummary, SalsaTableShapeKindSummary,
};

pub fn find_name_type_at_program_point(
    index: &SalsaDeclTypeQueryIndex,
    name_use: &SalsaNameUseSummary,
    program_point_offset: TextSize,
    assignments: &SalsaLocalAssignmentQueryIndex,
    property_index: &SalsaPropertyIndexSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    decl_tree: &SalsaDeclTreeSummary,
    chunk: &LuaChunk,
    lowered_types: &SalsaDocTypeLoweredIndex,
    active_narrows: &[SalsaTypeNarrowSummary],
) -> SalsaProgramPointTypeInfoSummary {
    let property_query_index = build_property_query_index(property_index);
    let base_decl_type = match name_use.resolution {
        SalsaNameUseResolutionSummary::LocalDecl(decl_id) => find_decl_type_info(index, decl_id),
        SalsaNameUseResolutionSummary::Global => None,
    };
    let candidates = match name_use.resolution {
        SalsaNameUseResolutionSummary::LocalDecl(decl_id) => {
            find_decl_type_candidates_at_program_point(
                index,
                assignments,
                &property_query_index,
                doc_types,
                signature_explain_index,
                doc_tag_query_index,
                decl_tree,
                chunk,
                decl_id,
                program_point_offset,
                lowered_types,
                active_narrows,
            )
            .unwrap_or_else(|| base_decl_type.iter().map(decl_type_to_candidate).collect())
        }
        SalsaNameUseResolutionSummary::Global => Vec::new(),
    };

    SalsaProgramPointTypeInfoSummary {
        syntax_offset: name_use.syntax_offset,
        program_point_offset,
        name_use: name_use.clone(),
        base_decl_type,
        candidates,
        active_narrows: active_narrows.to_vec(),
    }
}

pub fn find_member_type_at_program_point(
    member_index: &SalsaMemberTypeQueryIndex,
    property_index: &SalsaPropertyIndexSummary,
    decl_index: &SalsaDeclTypeQueryIndex,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    member_use: &SalsaMemberUseSummary,
    program_point_offset: TextSize,
    assignments: &SalsaLocalAssignmentQueryIndex,
    decl_tree: &SalsaDeclTreeSummary,
    chunk: &LuaChunk,
    lowered_types: &SalsaDocTypeLoweredIndex,
    signature_explain_index: &SalsaSignatureExplainIndex,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
) -> SalsaProgramPointMemberTypeInfoSummary {
    let base_member_type = find_member_use_type_info(member_index, member_use);
    let property_query_index = build_property_query_index(property_index);
    let (owner_decl_id, owner_candidates, active_narrows) = match &member_use.target.root {
        SalsaMemberRootSummary::LocalDecl { decl_id, .. } => {
            let active_narrows = collect_active_type_narrows(
                decl_tree,
                chunk.clone(),
                *decl_id,
                program_point_offset,
            );
            let candidates = find_decl_type_candidates_at_program_point(
                decl_index,
                assignments,
                &property_query_index,
                doc_types,
                signature_explain_index,
                doc_tag_query_index,
                decl_tree,
                chunk,
                *decl_id,
                program_point_offset,
                lowered_types,
                &active_narrows,
            )
            .unwrap_or_default();
            (Some(*decl_id), candidates, active_narrows)
        }
        _ => (None, Vec::new(), Vec::new()),
    };

    let mut candidates = base_member_type
        .as_ref()
        .map(|info| info.candidates.clone())
        .unwrap_or_default();
    let owner_member_candidates = collect_member_candidates_from_owner_types(
        &property_query_index,
        &member_use.target.member_name,
        &owner_candidates,
        chunk,
        doc,
        doc_types,
        lowered_types,
    );
    let prefers_correlated_owner_candidates = owner_decl_id.is_some()
        && active_narrows
            .iter()
            .any(|narrow| matches!(narrow, SalsaTypeNarrowSummary::FieldLiteral { .. }));
    if prefers_correlated_owner_candidates {
        candidates = owner_member_candidates;
    } else {
        candidates.extend(owner_member_candidates);
        candidates = dedupe_type_candidates(candidates);
    }

    SalsaProgramPointMemberTypeInfoSummary {
        syntax_offset: member_use.syntax_offset,
        program_point_offset,
        member_use: member_use.clone(),
        base_member_type,
        owner_decl_id,
        owner_candidates,
        candidates,
        active_narrows,
    }
}

pub fn collect_active_type_narrows(
    decl_tree: &SalsaDeclTreeSummary,
    chunk: LuaChunk,
    decl_id: SalsaDeclId,
    program_point_offset: TextSize,
) -> Vec<SalsaTypeNarrowSummary> {
    let mut narrows = Vec::new();

    for if_stat in chunk.descendants::<LuaIfStat>() {
        narrows.extend(collect_if_clause_narrows(
            decl_tree,
            if_stat,
            decl_id,
            program_point_offset,
        ));
    }

    let Some(point_block) = innermost_block_at_offset(&chunk, program_point_offset) else {
        return narrows;
    };
    narrows.extend(collect_guard_clause_narrows(
        decl_tree,
        &point_block,
        decl_id,
        program_point_offset,
    ));
    for call_expr in chunk.descendants::<LuaCallExpr>() {
        if !call_expr.is_assert() {
            continue;
        }
        let call_offset = TextSize::from(u32::from(call_expr.get_position()));
        if call_offset >= program_point_offset {
            continue;
        }
        let Some(call_block) = nearest_ancestor_block(&call_expr) else {
            continue;
        };
        if call_block.get_range() != point_block.get_range() {
            continue;
        }
        if let Some(arg) = call_expr
            .get_args_list()
            .and_then(|args| args.get_args().next())
        {
            narrows.extend(collect_condition_narrows_for_expr(
                decl_tree, arg, decl_id, true,
            ));
        }
    }

    narrows
}

fn collect_guard_clause_narrows(
    decl_tree: &SalsaDeclTreeSummary,
    point_block: &LuaBlock,
    decl_id: SalsaDeclId,
    program_point_offset: TextSize,
) -> Vec<SalsaTypeNarrowSummary> {
    let mut narrows = Vec::new();

    for stat in point_block.get_stats() {
        let stat_offset = TextSize::from(u32::from(stat.get_position()));
        if stat_offset >= program_point_offset {
            break;
        }

        let LuaStat::IfStat(if_stat) = stat else {
            continue;
        };
        if if_stat.get_else_if_clause_list().next().is_some() || if_stat.get_else_clause().is_some()
        {
            continue;
        }

        let if_end = TextSize::from(u32::from(if_stat.syntax().text_range().end()));
        if if_end >= program_point_offset {
            continue;
        }

        let Some(then_block) = if_stat.get_block() else {
            continue;
        };
        if !block_guarantees_exit(&then_block) {
            continue;
        }

        if let Some(condition_expr) = if_stat.get_condition_expr() {
            narrows.extend(collect_condition_narrows_for_expr(
                decl_tree,
                condition_expr,
                decl_id,
                false,
            ));
        }
    }

    narrows
}

fn block_guarantees_exit(block: &LuaBlock) -> bool {
    block.get_stats().last().is_some_and(stat_guarantees_exit)
}

fn stat_guarantees_exit(stat: LuaStat) -> bool {
    match stat {
        LuaStat::ReturnStat(_) | LuaStat::BreakStat(_) => true,
        LuaStat::CallExprStat(call_expr_stat) => call_expr_stat
            .get_call_expr()
            .is_some_and(|call_expr| call_expr.is_error()),
        LuaStat::DoStat(do_stat) => do_stat
            .get_block()
            .is_some_and(|block| block_guarantees_exit(&block)),
        LuaStat::IfStat(if_stat) => {
            let Some(then_block) = if_stat.get_block() else {
                return false;
            };
            if !block_guarantees_exit(&then_block) {
                return false;
            }
            if !if_stat.get_else_if_clause_list().all(|clause| {
                clause
                    .get_block()
                    .is_some_and(|block| block_guarantees_exit(&block))
            }) {
                return false;
            }
            if let Some(else_clause) = if_stat.get_else_clause() {
                else_clause
                    .get_block()
                    .is_some_and(|block| block_guarantees_exit(&block))
            } else {
                false
            }
        }
        _ => false,
    }
}

fn collect_if_clause_narrows(
    decl_tree: &SalsaDeclTreeSummary,
    if_stat: LuaIfStat,
    decl_id: SalsaDeclId,
    program_point_offset: TextSize,
) -> Vec<SalsaTypeNarrowSummary> {
    let mut previous_conditions = Vec::<LuaExpr>::new();

    if let Some(if_block) = if_stat.get_block()
        && if_block.get_range().contains(program_point_offset)
    {
        let mut narrows = previous_conditions
            .iter()
            .flat_map(|condition| {
                collect_condition_narrows_for_expr(decl_tree, condition.clone(), decl_id, false)
            })
            .collect::<Vec<_>>();
        if let Some(condition_expr) = if_stat.get_condition_expr() {
            narrows.extend(collect_condition_narrows_for_expr(
                decl_tree,
                condition_expr,
                decl_id,
                true,
            ));
        }
        return narrows;
    }
    if let Some(condition_expr) = if_stat.get_condition_expr() {
        previous_conditions.push(condition_expr);
    }

    for clause in if_stat.get_else_if_clause_list() {
        if let Some(block) = clause.get_block()
            && block.get_range().contains(program_point_offset)
        {
            let mut narrows = previous_conditions
                .iter()
                .flat_map(|condition| {
                    collect_condition_narrows_for_expr(decl_tree, condition.clone(), decl_id, false)
                })
                .collect::<Vec<_>>();
            if let Some(condition_expr) = clause.get_condition_expr() {
                narrows.extend(collect_condition_narrows_for_expr(
                    decl_tree,
                    condition_expr,
                    decl_id,
                    true,
                ));
            }
            return narrows;
        }

        if let Some(condition_expr) = clause.get_condition_expr() {
            previous_conditions.push(condition_expr);
        }
    }

    if let Some(else_clause) = if_stat.get_else_clause()
        && let Some(block) = else_clause.get_block()
        && block.get_range().contains(program_point_offset)
    {
        return previous_conditions
            .into_iter()
            .flat_map(|condition| {
                collect_condition_narrows_for_expr(decl_tree, condition, decl_id, false)
            })
            .collect();
    }

    Vec::new()
}

fn find_decl_type_candidates_at_program_point(
    index: &SalsaDeclTypeQueryIndex,
    assignments: &SalsaLocalAssignmentQueryIndex,
    property_query_index: &SalsaPropertyQueryIndex,
    doc_types: &SalsaDocTypeIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    decl_tree: &SalsaDeclTreeSummary,
    chunk: &LuaChunk,
    decl_id: SalsaDeclId,
    program_point_offset: TextSize,
    lowered_types: &SalsaDocTypeLoweredIndex,
    active_narrows: &[SalsaTypeNarrowSummary],
) -> Option<Vec<SalsaTypeCandidateSummary>> {
    let base_decl_type = find_decl_type_info(index, decl_id);
    let mut candidates = latest_assignments_at_program_point(
        decl_tree,
        assignments,
        chunk,
        decl_id,
        program_point_offset,
    )
    .map(|merged_assignments| {
        merged_assignments
            .into_iter()
            .flat_map(|assignment| {
                assignment_to_program_point_candidates(
                    index,
                    assignments,
                    assignment,
                    property_query_index,
                    doc_types,
                    signature_explain_index,
                    doc_tag_query_index,
                    decl_tree,
                    chunk,
                    program_point_offset,
                    lowered_types,
                )
            })
            .collect::<Vec<_>>()
    })
    .unwrap_or_else(|| base_decl_type.iter().map(decl_type_to_candidate).collect());
    if !active_narrows.is_empty() {
        candidates = candidates
            .into_iter()
            .map(|candidate| {
                apply_narrows_to_candidate(
                    candidate,
                    property_query_index,
                    doc_types,
                    lowered_types,
                    active_narrows,
                )
            })
            .filter(|candidate| {
                !candidate.explicit_type_offsets.is_empty()
                    || candidate.initializer_offset.is_some()
            })
            .collect();
    }
    (!candidates.is_empty()).then_some(candidates)
}

fn assignment_to_program_point_candidates(
    index: &SalsaDeclTypeQueryIndex,
    assignments: &SalsaLocalAssignmentQueryIndex,
    assignment: SalsaLocalAssignmentSummary,
    property_query_index: &SalsaPropertyQueryIndex,
    doc_types: &SalsaDocTypeIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    decl_tree: &SalsaDeclTreeSummary,
    chunk: &LuaChunk,
    program_point_offset: TextSize,
    lowered_types: &SalsaDocTypeLoweredIndex,
) -> Vec<SalsaTypeCandidateSummary> {
    let mut visited_source_decl_ids = BTreeSet::new();
    assignment_to_program_point_candidates_with_visited(
        index,
        assignments,
        assignment,
        property_query_index,
        doc_types,
        signature_explain_index,
        doc_tag_query_index,
        decl_tree,
        chunk,
        program_point_offset,
        lowered_types,
        &mut visited_source_decl_ids,
    )
}

fn assignment_to_program_point_candidates_with_visited(
    index: &SalsaDeclTypeQueryIndex,
    assignments: &SalsaLocalAssignmentQueryIndex,
    assignment: SalsaLocalAssignmentSummary,
    property_query_index: &SalsaPropertyQueryIndex,
    doc_types: &SalsaDocTypeIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    decl_tree: &SalsaDeclTreeSummary,
    chunk: &LuaChunk,
    program_point_offset: TextSize,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited_source_decl_ids: &mut BTreeSet<SalsaDeclId>,
) -> Vec<SalsaTypeCandidateSummary> {
    if let Some(candidates) = call_assignment_candidates(
        &assignment,
        assignments,
        property_query_index,
        doc_types,
        signature_explain_index,
        doc_tag_query_index,
        decl_tree,
        chunk,
        program_point_offset,
        lowered_types,
    ) {
        return candidates;
    }

    let source_decl_candidates = assignment.source_decl_id.and_then(|source_decl_id| {
        source_decl_candidates_at_program_point(
            index,
            assignments,
            property_query_index,
            doc_types,
            signature_explain_index,
            doc_tag_query_index,
            decl_tree,
            chunk,
            source_decl_id,
            program_point_offset,
            lowered_types,
            visited_source_decl_ids,
        )
    });

    if assignment.is_local_declaration {
        if let Some(decl_type) = find_decl_type_info(index, assignment.decl_id) {
            let candidate = decl_type_to_candidate(&decl_type);
            if candidate.signature_offset.is_none()
                && candidate.explicit_type_offsets.is_empty()
                && candidate.named_type_names.is_empty()
                && let Some(source_decl_candidates) = source_decl_candidates
            {
                return source_decl_candidates
                    .into_iter()
                    .map(|source_candidate| {
                        alias_assignment_candidate(&assignment, source_candidate)
                    })
                    .collect();
            }

            return vec![candidate];
        }
    }

    if let Some(source_decl_candidates) = source_decl_candidates {
        return source_decl_candidates
            .into_iter()
            .map(|source_candidate| alias_assignment_candidate(&assignment, source_candidate))
            .collect();
    }

    vec![SalsaTypeCandidateSummary {
        origin: super::data::SalsaTypeCandidateOriginSummary::Assignment(assignment.syntax_offset),
        explicit_type_offsets: Vec::new(),
        named_type_names: Vec::new(),
        initializer_offset: assignment.value_expr_offset,
        value_expr_syntax_id: assignment.value_expr_syntax_id,
        value_result_index: assignment.value_result_index,
        source_call_syntax_id: assignment.source_call_syntax_id,
        signature_offset: None,
    }]
}

fn source_decl_candidates_at_program_point(
    index: &SalsaDeclTypeQueryIndex,
    assignments: &SalsaLocalAssignmentQueryIndex,
    property_query_index: &SalsaPropertyQueryIndex,
    doc_types: &SalsaDocTypeIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    decl_tree: &SalsaDeclTreeSummary,
    chunk: &LuaChunk,
    decl_id: SalsaDeclId,
    program_point_offset: TextSize,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited_source_decl_ids: &mut BTreeSet<SalsaDeclId>,
) -> Option<Vec<SalsaTypeCandidateSummary>> {
    if !visited_source_decl_ids.insert(decl_id) {
        return None;
    }

    let candidates = latest_assignments_at_program_point(
        decl_tree,
        assignments,
        chunk,
        decl_id,
        program_point_offset,
    )
    .map(|merged_assignments| {
        merged_assignments
            .into_iter()
            .flat_map(|assignment| {
                assignment_to_program_point_candidates_with_visited(
                    index,
                    assignments,
                    assignment,
                    property_query_index,
                    doc_types,
                    signature_explain_index,
                    doc_tag_query_index,
                    decl_tree,
                    chunk,
                    program_point_offset,
                    lowered_types,
                    visited_source_decl_ids,
                )
            })
            .collect::<Vec<_>>()
    })
    .or_else(|| {
        find_decl_type_info(index, decl_id)
            .map(|decl_type| vec![decl_type_to_candidate(&decl_type)])
    });

    visited_source_decl_ids.remove(&decl_id);
    candidates.filter(|candidates| !candidates.is_empty())
}

fn alias_assignment_candidate(
    assignment: &SalsaLocalAssignmentSummary,
    source_candidate: SalsaTypeCandidateSummary,
) -> SalsaTypeCandidateSummary {
    SalsaTypeCandidateSummary {
        origin: super::data::SalsaTypeCandidateOriginSummary::Assignment(assignment.syntax_offset),
        explicit_type_offsets: source_candidate.explicit_type_offsets,
        named_type_names: source_candidate.named_type_names,
        initializer_offset: assignment.value_expr_offset,
        value_expr_syntax_id: assignment.value_expr_syntax_id,
        value_result_index: assignment.value_result_index,
        source_call_syntax_id: assignment.source_call_syntax_id,
        signature_offset: source_candidate.signature_offset,
    }
}

fn call_assignment_candidates(
    assignment: &SalsaLocalAssignmentSummary,
    assignments: &SalsaLocalAssignmentQueryIndex,
    property_query_index: &SalsaPropertyQueryIndex,
    doc_types: &SalsaDocTypeIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    decl_tree: &SalsaDeclTreeSummary,
    chunk: &LuaChunk,
    program_point_offset: TextSize,
    lowered_types: &SalsaDocTypeLoweredIndex,
) -> Option<Vec<SalsaTypeCandidateSummary>> {
    let call_syntax_id = assignment.source_call_syntax_id?;
    let call_explain = find_call_explain_by_syntax_id(signature_explain_index, call_syntax_id)?;
    let resolved_signature = call_explain.resolved_signature.as_ref()?;

    let overload_rows = resolved_signature
        .doc_owners
        .iter()
        .flat_map(|owner| {
            collect_doc_tags_for_owner_in_index(
                doc_tag_query_index,
                &SalsaDocOwnerSummary {
                    kind: owner.owner_kind.clone(),
                    syntax_offset: Some(owner.owner_offset),
                },
            )
        })
        .filter(|tag| tag.kind == SalsaDocTagKindSummary::ReturnOverload)
        .map(|tag| -> Vec<SalsaDocTypeNodeKey> { tag.type_offsets().to_vec() })
        .collect::<Vec<_>>();

    let group_assignments = collect_surviving_call_group_assignments(
        assignment,
        assignments,
        decl_tree,
        chunk,
        program_point_offset,
    );

    let mut explicit_type_offsets = if !overload_rows.is_empty() {
        collect_return_slot_offsets(&overload_rows, assignment.value_result_index)
    } else {
        collect_signature_return_slot_offsets(&call_explain.returns, assignment.value_result_index)
    };

    if !overload_rows.is_empty() && group_assignments.len() > 1 {
        let filtered_rows = overload_rows
            .into_iter()
            .filter(|row: &Vec<SalsaDocTypeNodeKey>| {
                group_assignments
                    .iter()
                    .filter(|sibling| sibling.decl_id != assignment.decl_id)
                    .all(|sibling| {
                        let Some(type_offset) = row.get(sibling.value_result_index).copied() else {
                            return false;
                        };
                        let sibling_narrows = collect_active_type_narrows(
                            decl_tree,
                            chunk.clone(),
                            sibling.decl_id,
                            program_point_offset,
                        );
                        sibling_narrows.iter().all(|narrow| {
                            narrow_matches_type_offset(
                                type_offset,
                                narrow,
                                property_query_index,
                                doc_types,
                                lowered_types,
                            )
                        })
                    })
            })
            .collect::<Vec<_>>();

        let correlated_offsets =
            collect_return_slot_offsets(&filtered_rows, assignment.value_result_index);
        if !correlated_offsets.is_empty() {
            explicit_type_offsets = correlated_offsets;
        }
    }

    if explicit_type_offsets.is_empty() && call_explain.resolved_signature_offset.is_none() {
        return None;
    }

    Some(vec![SalsaTypeCandidateSummary {
        origin: super::data::SalsaTypeCandidateOriginSummary::Assignment(assignment.syntax_offset),
        explicit_type_offsets,
        named_type_names: Vec::new(),
        initializer_offset: assignment.value_expr_offset,
        value_expr_syntax_id: assignment.value_expr_syntax_id,
        value_result_index: assignment.value_result_index,
        source_call_syntax_id: assignment.source_call_syntax_id,
        signature_offset: call_explain.resolved_signature_offset,
    }])
}

fn collect_surviving_call_group_assignments(
    assignment: &SalsaLocalAssignmentSummary,
    assignments: &SalsaLocalAssignmentQueryIndex,
    decl_tree: &SalsaDeclTreeSummary,
    chunk: &LuaChunk,
    program_point_offset: TextSize,
) -> Vec<SalsaLocalAssignmentSummary> {
    let Some(call_syntax_id) = assignment.source_call_syntax_id else {
        return Vec::new();
    };

    let seed_decl_ids = assignments
        .assignments
        .iter()
        .filter(|candidate| {
            candidate.syntax_offset == assignment.syntax_offset
                && candidate.source_call_syntax_id == Some(call_syntax_id)
        })
        .map(|candidate| candidate.decl_id)
        .collect::<BTreeSet<_>>();

    let mut surviving = Vec::new();
    for decl_id in seed_decl_ids {
        if let Some(latest_assignments) = latest_assignments_at_program_point(
            decl_tree,
            assignments,
            chunk,
            decl_id,
            program_point_offset,
        ) {
            surviving.extend(
                latest_assignments
                    .into_iter()
                    .filter(|candidate| candidate.source_call_syntax_id == Some(call_syntax_id)),
            );
        }
    }

    dedupe_local_assignments(surviving)
}

fn dedupe_local_assignments(
    assignments: Vec<SalsaLocalAssignmentSummary>,
) -> Vec<SalsaLocalAssignmentSummary> {
    let mut seen = BTreeSet::new();
    assignments
        .into_iter()
        .filter(|assignment| {
            seen.insert((
                assignment.decl_id,
                assignment.syntax_offset,
                assignment.is_local_declaration,
                assignment.value_expr_syntax_id,
                assignment.value_expr_offset,
                assignment.value_result_index,
                assignment.source_decl_id,
                assignment.source_call_syntax_id,
            ))
        })
        .collect()
}

fn collect_return_slot_offsets(
    rows: &[Vec<SalsaDocTypeNodeKey>],
    slot_index: usize,
) -> Vec<SalsaDocTypeNodeKey> {
    rows.iter()
        .filter_map(|row| row.get(slot_index).copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn collect_signature_return_slot_offsets(
    returns: &[super::super::signature::SalsaSignatureReturnExplainSummary],
    slot_index: usize,
) -> Vec<SalsaDocTypeNodeKey> {
    returns
        .iter()
        .filter_map(|return_row| return_row.items.get(slot_index))
        .filter_map(|item| match item.doc_type.type_ref {
            SalsaDocTypeRef::Node(offset) => Some(offset),
            SalsaDocTypeRef::Incomplete => None,
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn narrow_matches_type_offset(
    type_offset: SalsaDocTypeNodeKey,
    narrow: &SalsaTypeNarrowSummary,
    property_query_index: &SalsaPropertyQueryIndex,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
) -> bool {
    !narrow_type_offsets(
        vec![type_offset],
        property_query_index,
        doc_types,
        lowered_types,
        std::slice::from_ref(narrow),
    )
    .is_empty()
}

fn collect_member_candidates_from_owner_types(
    property_query_index: &SalsaPropertyQueryIndex,
    member_name: &SmolStr,
    owner_candidates: &[SalsaTypeCandidateSummary],
    chunk: &LuaChunk,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
) -> Vec<SalsaTypeCandidateSummary> {
    let mut candidates = Vec::new();
    for candidate in owner_candidates {
        if candidate.explicit_type_offsets.is_empty() {
            for type_name in &candidate.named_type_names {
                candidates.extend(collect_member_candidates_from_named_type(
                    type_name,
                    member_name,
                    property_query_index,
                    chunk,
                    doc,
                    doc_types,
                    lowered_types,
                    &mut BTreeSet::new(),
                    &mut BTreeSet::new(),
                ));
            }
        }

        for type_offset in &candidate.explicit_type_offsets {
            candidates.extend(collect_member_candidates_from_type_offset(
                *type_offset,
                member_name,
                property_query_index,
                chunk,
                doc,
                doc_types,
                lowered_types,
                &mut BTreeSet::new(),
                &mut BTreeSet::new(),
            ));
        }

        if let Some(initializer_offset) = candidate.initializer_offset {
            candidates.extend(collect_member_candidates_from_initializer_offset(
                chunk,
                initializer_offset,
                member_name,
            ));
        }
    }

    dedupe_type_candidates(candidates)
}

fn collect_member_candidates_from_index_access_type_offset(
    base_type_offset: SalsaDocTypeNodeKey,
    index_type_offset: SalsaDocTypeNodeKey,
    member_name: &SmolStr,
    property_query_index: &SalsaPropertyQueryIndex,
    chunk: &LuaChunk,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
) -> Vec<SalsaTypeCandidateSummary> {
    let index_keys = collect_index_access_member_keys_from_type_offset(
        index_type_offset,
        property_query_index,
        doc,
        doc_types,
        lowered_types,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
    );

    let mut candidates = Vec::new();
    for index_key in index_keys {
        let owner_candidates = collect_member_candidates_from_type_offset(
            base_type_offset,
            &index_key,
            property_query_index,
            chunk,
            doc,
            doc_types,
            lowered_types,
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
        );
        for owner_candidate in owner_candidates {
            for owner_type_offset in owner_candidate.explicit_type_offsets {
                candidates.extend(collect_member_candidates_from_type_offset(
                    owner_type_offset,
                    member_name,
                    property_query_index,
                    chunk,
                    doc,
                    doc_types,
                    lowered_types,
                    &mut BTreeSet::new(),
                    &mut BTreeSet::new(),
                ));
            }
        }
    }

    dedupe_type_candidates(candidates)
}

fn collect_index_access_member_keys_from_type_offset(
    type_offset: SalsaDocTypeNodeKey,
    property_query_index: &SalsaPropertyQueryIndex,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited_offsets: &mut BTreeSet<SalsaDocTypeNodeKey>,
    visited_type_names: &mut BTreeSet<SmolStr>,
) -> Vec<SmolStr> {
    collect_index_access_member_keys_from_type_offset_with_substitution(
        type_offset,
        property_query_index,
        doc,
        doc_types,
        lowered_types,
        visited_offsets,
        visited_type_names,
        None,
    )
}

fn collect_index_access_member_keys_from_type_offset_with_substitution(
    type_offset: SalsaDocTypeNodeKey,
    property_query_index: &SalsaPropertyQueryIndex,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited_offsets: &mut BTreeSet<SalsaDocTypeNodeKey>,
    visited_type_names: &mut BTreeSet<SmolStr>,
    generic_args: Option<&BTreeMap<SmolStr, Vec<SalsaDocTypeNodeKey>>>,
) -> Vec<SmolStr> {
    if !visited_offsets.insert(type_offset) {
        return Vec::new();
    }

    if let Some(doc_type) = doc_types
        .types
        .iter()
        .find(|doc_type| doc_type.node_key() == type_offset)
    {
        if let Some(generic_args) = generic_args {
            match &doc_type.kind {
                SalsaDocTypeKindSummary::Infer { generic_name } => {
                    if let Some(offsets) = generic_name
                        .as_ref()
                        .and_then(|name| generic_args.get(name))
                    {
                        return offsets
                            .iter()
                            .flat_map(|offset| {
                                collect_index_access_member_keys_from_type_offset(
                                    *offset,
                                    property_query_index,
                                    doc,
                                    doc_types,
                                    lowered_types,
                                    visited_offsets,
                                    visited_type_names,
                                )
                            })
                            .collect();
                    }
                }
                SalsaDocTypeKindSummary::Name { name } => {
                    if let Some(offsets) = name.as_ref().and_then(|name| generic_args.get(name)) {
                        return offsets
                            .iter()
                            .flat_map(|offset| {
                                collect_index_access_member_keys_from_type_offset(
                                    *offset,
                                    property_query_index,
                                    doc,
                                    doc_types,
                                    lowered_types,
                                    visited_offsets,
                                    visited_type_names,
                                )
                            })
                            .collect();
                    }
                }
                _ => {}
            }
        }

        return match &doc_type.kind {
            SalsaDocTypeKindSummary::Name { name } => name
                .as_ref()
                .map(|name| {
                    collect_index_access_member_keys_from_named_type(
                        name,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    )
                })
                .unwrap_or_default(),
            SalsaDocTypeKindSummary::Literal { text } => {
                literal_member_key_from_text(text).into_iter().collect()
            }
            SalsaDocTypeKindSummary::Binary {
                left_type_offset,
                right_type_offset,
                ..
            } => {
                let mut keys = Vec::new();
                if let Some(left) = left_type_offset {
                    keys.extend(collect_index_access_member_keys_from_type_offset(
                        *left,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
                if let Some(right) = right_type_offset {
                    keys.extend(collect_index_access_member_keys_from_type_offset(
                        *right,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
                keys
            }
            SalsaDocTypeKindSummary::Unary {
                op: SalsaDocTypeUnaryOperatorSummary::Keyof,
                inner_type_offset,
            } => inner_type_offset
                .iter()
                .copied()
                .flat_map(|inner| {
                    collect_keyof_member_keys_from_type_offset(
                        inner,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    )
                })
                .collect(),
            SalsaDocTypeKindSummary::Array {
                item_type_offset: inner_type_offset,
            }
            | SalsaDocTypeKindSummary::Nullable { inner_type_offset }
            | SalsaDocTypeKindSummary::Unary {
                inner_type_offset, ..
            } => inner_type_offset
                .iter()
                .copied()
                .flat_map(|inner| {
                    collect_index_access_member_keys_from_type_offset(
                        inner,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    )
                })
                .collect(),
            SalsaDocTypeKindSummary::Tuple { item_type_offsets }
            | SalsaDocTypeKindSummary::MultiLineUnion { item_type_offsets } => item_type_offsets
                .iter()
                .flat_map(|item| {
                    collect_index_access_member_keys_from_type_offset(
                        *item,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    )
                })
                .collect(),
            SalsaDocTypeKindSummary::Conditional {
                true_type_offset,
                false_type_offset,
                ..
            } => {
                let mut keys = Vec::new();
                if let Some(true_type) = true_type_offset {
                    keys.extend(collect_index_access_member_keys_from_type_offset(
                        *true_type,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
                if let Some(false_type) = false_type_offset {
                    keys.extend(collect_index_access_member_keys_from_type_offset(
                        *false_type,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
                keys
            }
            SalsaDocTypeKindSummary::Mapped {
                key_type_offsets, ..
            } => key_type_offsets
                .iter()
                .flat_map(|item| {
                    collect_index_access_member_keys_from_type_offset_with_substitution(
                        *item,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                        generic_args,
                    )
                })
                .collect(),
            _ => Vec::new(),
        };
    }

    find_resolved_doc_type_by_key_from_parts(doc_types, lowered_types, type_offset)
        .map(|resolved| match resolved.lowered.kind {
            SalsaDocTypeLoweredKind::Name { name } => {
                collect_index_access_member_keys_from_named_type(
                    &name,
                    property_query_index,
                    doc,
                    doc_types,
                    lowered_types,
                    visited_offsets,
                    visited_type_names,
                )
            }
            SalsaDocTypeLoweredKind::Literal { text } => {
                literal_member_key_from_text(&text).into_iter().collect()
            }
            SalsaDocTypeLoweredKind::Union { item_types }
            | SalsaDocTypeLoweredKind::Intersection { item_types }
            | SalsaDocTypeLoweredKind::Tuple { item_types }
            | SalsaDocTypeLoweredKind::MultiLineUnion { item_types } => item_types
                .into_iter()
                .filter_map(|item| match item {
                    SalsaDocTypeRef::Node(offset) => Some(offset),
                    SalsaDocTypeRef::Incomplete => None,
                })
                .flat_map(|item| {
                    collect_index_access_member_keys_from_type_offset(
                        item,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    )
                })
                .collect(),
            SalsaDocTypeLoweredKind::Unary {
                op: SalsaDocTypeUnaryOperatorSummary::Keyof,
                inner_type,
            } => match inner_type {
                SalsaDocTypeRef::Node(offset) => collect_keyof_member_keys_from_type_offset(
                    offset,
                    property_query_index,
                    doc,
                    doc_types,
                    lowered_types,
                    visited_offsets,
                    visited_type_names,
                ),
                SalsaDocTypeRef::Incomplete => Vec::new(),
            },
            SalsaDocTypeLoweredKind::Nullable { inner_type }
            | SalsaDocTypeLoweredKind::Array {
                item_type: inner_type,
            }
            | SalsaDocTypeLoweredKind::Variadic {
                item_type: inner_type,
            }
            | SalsaDocTypeLoweredKind::Unary { inner_type, .. } => match inner_type {
                SalsaDocTypeRef::Node(offset) => collect_index_access_member_keys_from_type_offset(
                    offset,
                    property_query_index,
                    doc,
                    doc_types,
                    lowered_types,
                    visited_offsets,
                    visited_type_names,
                ),
                SalsaDocTypeRef::Incomplete => Vec::new(),
            },
            SalsaDocTypeLoweredKind::Conditional {
                true_type,
                false_type,
                ..
            } => [true_type, false_type]
                .into_iter()
                .filter_map(|branch| match branch {
                    SalsaDocTypeRef::Node(offset) => Some(offset),
                    SalsaDocTypeRef::Incomplete => None,
                })
                .flat_map(|branch| {
                    collect_index_access_member_keys_from_type_offset(
                        branch,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    )
                })
                .collect(),
            SalsaDocTypeLoweredKind::Mapped { key_types, .. } => key_types
                .into_iter()
                .filter_map(|key| match key {
                    SalsaDocTypeRef::Node(offset) => Some(offset),
                    SalsaDocTypeRef::Incomplete => None,
                })
                .flat_map(|item| {
                    collect_index_access_member_keys_from_type_offset(
                        item,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    )
                })
                .collect(),
            SalsaDocTypeLoweredKind::Generic {
                base_type,
                arg_types,
            } => collect_index_access_member_keys_from_generic_instance(
                base_type,
                arg_types,
                property_query_index,
                doc,
                doc_types,
                lowered_types,
                visited_offsets,
                visited_type_names,
            ),
            _ => Vec::new(),
        })
        .unwrap_or_default()
}

fn collect_index_access_member_keys_from_generic_instance(
    base_type: SalsaDocTypeRef,
    arg_types: Vec<SalsaDocTypeRef>,
    property_query_index: &SalsaPropertyQueryIndex,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited_offsets: &mut BTreeSet<SalsaDocTypeNodeKey>,
    visited_type_names: &mut BTreeSet<SmolStr>,
) -> Vec<SmolStr> {
    let SalsaDocTypeRef::Node(base_offset) = base_type else {
        return Vec::new();
    };

    let Some(base_doc_type) = doc_types
        .types
        .iter()
        .find(|doc_type| doc_type.node_key() == base_offset)
    else {
        return collect_index_access_member_keys_from_type_offset(
            base_offset,
            property_query_index,
            doc,
            doc_types,
            lowered_types,
            visited_offsets,
            visited_type_names,
        );
    };

    let SalsaDocTypeKindSummary::Name {
        name: Some(type_name),
    } = &base_doc_type.kind
    else {
        return collect_index_access_member_keys_from_type_offset(
            base_offset,
            property_query_index,
            doc,
            doc_types,
            lowered_types,
            visited_offsets,
            visited_type_names,
        );
    };

    let Some(type_def) = doc
        .type_defs
        .iter()
        .find(|type_def| type_def.name == *type_name)
    else {
        return collect_index_access_member_keys_from_type_offset(
            base_offset,
            property_query_index,
            doc,
            doc_types,
            lowered_types,
            visited_offsets,
            visited_type_names,
        );
    };

    let generic_arg_map = build_generic_arg_type_map(&type_def.generic_params, &arg_types);
    let Some(value_type_offset) = type_def.value_type_offset else {
        return collect_index_access_member_keys_from_type_offset(
            base_offset,
            property_query_index,
            doc,
            doc_types,
            lowered_types,
            visited_offsets,
            visited_type_names,
        );
    };

    collect_index_access_member_keys_from_type_offset_with_substitution(
        value_type_offset,
        property_query_index,
        doc,
        doc_types,
        lowered_types,
        visited_offsets,
        visited_type_names,
        Some(&generic_arg_map),
    )
}

fn build_generic_arg_type_map(
    generic_params: &[crate::SalsaDocGenericParamSummary],
    arg_types: &[SalsaDocTypeRef],
) -> BTreeMap<SmolStr, Vec<SalsaDocTypeNodeKey>> {
    generic_params
        .iter()
        .zip(arg_types.iter())
        .filter_map(|(param, arg)| match arg {
            SalsaDocTypeRef::Node(offset) => Some((param.name.clone(), vec![*offset])),
            SalsaDocTypeRef::Incomplete => None,
        })
        .collect()
}

fn collect_index_access_member_keys_from_named_type(
    type_name: &SmolStr,
    property_query_index: &SalsaPropertyQueryIndex,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited_offsets: &mut BTreeSet<SalsaDocTypeNodeKey>,
    visited_type_names: &mut BTreeSet<SmolStr>,
) -> Vec<SmolStr> {
    if !visited_type_names.insert(type_name.clone()) {
        return Vec::new();
    }

    let result = doc
        .type_defs
        .iter()
        .find(|type_def| type_def.name == *type_name)
        .map(|type_def| match type_def.kind {
            SalsaDocTypeDefKindSummary::Alias | SalsaDocTypeDefKindSummary::Attribute => type_def
                .value_type_offset
                .map(|value_type_offset| {
                    collect_index_access_member_keys_from_type_offset(
                        value_type_offset,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    )
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        })
        .unwrap_or_default();

    visited_type_names.remove(type_name);
    result
}

fn collect_keyof_member_keys_from_type_offset(
    type_offset: SalsaDocTypeNodeKey,
    property_query_index: &SalsaPropertyQueryIndex,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited_offsets: &mut BTreeSet<SalsaDocTypeNodeKey>,
    visited_type_names: &mut BTreeSet<SmolStr>,
) -> Vec<SmolStr> {
    if !visited_offsets.insert(type_offset) {
        return Vec::new();
    }

    let result = if let Some(doc_type) = doc_types
        .types
        .iter()
        .find(|doc_type| doc_type.node_key() == type_offset)
    {
        match &doc_type.kind {
            SalsaDocTypeKindSummary::Name { name } => name
                .as_ref()
                .map(|name| {
                    collect_keyof_member_keys_from_named_type(
                        name,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    )
                })
                .unwrap_or_default(),
            SalsaDocTypeKindSummary::Object { fields } => fields
                .iter()
                .filter_map(|field| match &field.key {
                    SalsaDocObjectFieldKeySummary::Name(name)
                    | SalsaDocObjectFieldKeySummary::String(name) => Some(name.clone()),
                    _ => None,
                })
                .collect(),
            SalsaDocTypeKindSummary::Binary {
                left_type_offset,
                right_type_offset,
                ..
            } => {
                let mut keys = Vec::new();
                if let Some(left) = left_type_offset {
                    keys.extend(collect_keyof_member_keys_from_type_offset(
                        *left,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
                if let Some(right) = right_type_offset {
                    keys.extend(collect_keyof_member_keys_from_type_offset(
                        *right,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
                keys
            }
            SalsaDocTypeKindSummary::Array {
                item_type_offset: inner_type_offset,
            }
            | SalsaDocTypeKindSummary::Nullable { inner_type_offset }
            | SalsaDocTypeKindSummary::Unary {
                inner_type_offset, ..
            } => inner_type_offset
                .iter()
                .copied()
                .flat_map(|inner| {
                    collect_keyof_member_keys_from_type_offset(
                        inner,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    )
                })
                .collect(),
            SalsaDocTypeKindSummary::Tuple { item_type_offsets }
            | SalsaDocTypeKindSummary::MultiLineUnion { item_type_offsets } => item_type_offsets
                .iter()
                .flat_map(|item| {
                    collect_keyof_member_keys_from_type_offset(
                        *item,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    )
                })
                .collect(),
            SalsaDocTypeKindSummary::Conditional {
                true_type_offset,
                false_type_offset,
                ..
            } => {
                let mut keys = Vec::new();
                if let Some(true_type) = true_type_offset {
                    keys.extend(collect_keyof_member_keys_from_type_offset(
                        *true_type,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
                if let Some(false_type) = false_type_offset {
                    keys.extend(collect_keyof_member_keys_from_type_offset(
                        *false_type,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
                keys
            }
            SalsaDocTypeKindSummary::Mapped {
                key_type_offsets, ..
            } => key_type_offsets
                .iter()
                .flat_map(|item| {
                    collect_index_access_member_keys_from_type_offset(
                        *item,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    )
                })
                .collect(),
            _ => Vec::new(),
        }
    } else {
        find_resolved_doc_type_by_key_from_parts(doc_types, lowered_types, type_offset)
            .map(|resolved| match resolved.lowered.kind {
                SalsaDocTypeLoweredKind::Name { name } => {
                    collect_keyof_member_keys_from_named_type(
                        &name,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    )
                }
                SalsaDocTypeLoweredKind::Object { fields } => fields
                    .into_iter()
                    .filter_map(|field| match field.key {
                        SalsaDocTypeLoweredObjectFieldKey::Name(name)
                        | SalsaDocTypeLoweredObjectFieldKey::String(name) => Some(name),
                        _ => None,
                    })
                    .collect(),
                SalsaDocTypeLoweredKind::Union { item_types }
                | SalsaDocTypeLoweredKind::Intersection { item_types }
                | SalsaDocTypeLoweredKind::Tuple { item_types }
                | SalsaDocTypeLoweredKind::MultiLineUnion { item_types } => item_types
                    .into_iter()
                    .filter_map(|item| match item {
                        SalsaDocTypeRef::Node(offset) => Some(offset),
                        SalsaDocTypeRef::Incomplete => None,
                    })
                    .flat_map(|item| {
                        collect_keyof_member_keys_from_type_offset(
                            item,
                            property_query_index,
                            doc,
                            doc_types,
                            lowered_types,
                            visited_offsets,
                            visited_type_names,
                        )
                    })
                    .collect(),
                SalsaDocTypeLoweredKind::Nullable { inner_type }
                | SalsaDocTypeLoweredKind::Array {
                    item_type: inner_type,
                }
                | SalsaDocTypeLoweredKind::Variadic {
                    item_type: inner_type,
                }
                | SalsaDocTypeLoweredKind::Unary { inner_type, .. } => match inner_type {
                    SalsaDocTypeRef::Node(offset) => collect_keyof_member_keys_from_type_offset(
                        offset,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ),
                    SalsaDocTypeRef::Incomplete => Vec::new(),
                },
                SalsaDocTypeLoweredKind::Conditional {
                    true_type,
                    false_type,
                    ..
                } => [true_type, false_type]
                    .into_iter()
                    .filter_map(|branch| match branch {
                        SalsaDocTypeRef::Node(offset) => Some(offset),
                        SalsaDocTypeRef::Incomplete => None,
                    })
                    .flat_map(|branch| {
                        collect_keyof_member_keys_from_type_offset(
                            branch,
                            property_query_index,
                            doc,
                            doc_types,
                            lowered_types,
                            visited_offsets,
                            visited_type_names,
                        )
                    })
                    .collect(),
                SalsaDocTypeLoweredKind::Mapped { key_types, .. } => key_types
                    .into_iter()
                    .filter_map(|key| match key {
                        SalsaDocTypeRef::Node(offset) => Some(offset),
                        SalsaDocTypeRef::Incomplete => None,
                    })
                    .flat_map(|item| {
                        collect_index_access_member_keys_from_type_offset(
                            item,
                            property_query_index,
                            doc,
                            doc_types,
                            lowered_types,
                            visited_offsets,
                            visited_type_names,
                        )
                    })
                    .collect(),
                _ => Vec::new(),
            })
            .unwrap_or_default()
    };

    visited_offsets.remove(&type_offset);
    dedupe_member_names(result)
}

fn collect_keyof_member_keys_from_named_type(
    type_name: &SmolStr,
    property_query_index: &SalsaPropertyQueryIndex,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited_offsets: &mut BTreeSet<SalsaDocTypeNodeKey>,
    visited_type_names: &mut BTreeSet<SmolStr>,
) -> Vec<SmolStr> {
    if !visited_type_names.insert(type_name.clone()) {
        return Vec::new();
    }

    let mut keys = collect_properties_for_type_in_index(property_query_index, type_name)
        .into_iter()
        .filter_map(|property| match property.key {
            crate::SalsaPropertyKeySummary::Name(name) => Some(name),
            _ => None,
        })
        .collect::<Vec<_>>();

    if let Some(type_def) = doc
        .type_defs
        .iter()
        .find(|type_def| type_def.name == *type_name)
    {
        match type_def.kind {
            SalsaDocTypeDefKindSummary::Alias | SalsaDocTypeDefKindSummary::Attribute => {
                if let Some(value_type_offset) = type_def.value_type_offset {
                    keys.extend(collect_keyof_member_keys_from_type_offset(
                        value_type_offset,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
            }
            SalsaDocTypeDefKindSummary::Class => {
                for super_type_offset in &type_def.super_type_offsets {
                    keys.extend(collect_keyof_member_keys_from_type_offset(
                        *super_type_offset,
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
            }
            SalsaDocTypeDefKindSummary::Enum => {}
        }
    }

    visited_type_names.remove(type_name);
    dedupe_member_names(keys)
}

fn dedupe_member_names(names: Vec<SmolStr>) -> Vec<SmolStr> {
    let mut seen = BTreeSet::new();
    names
        .into_iter()
        .filter(|name| seen.insert(name.clone()))
        .collect()
}

fn literal_member_key_from_text(text: &str) -> Option<SmolStr> {
    match text.as_bytes() {
        [b'"', .., b'"'] | [b'\'', .., b'\''] if text.len() >= 2 => {
            Some(SmolStr::new(&text[1..text.len() - 1]))
        }
        _ => None,
    }
}

fn collect_member_candidates_from_initializer_offset(
    chunk: &LuaChunk,
    initializer_offset: TextSize,
    member_name: &SmolStr,
) -> Vec<SalsaTypeCandidateSummary> {
    let Some(expr) = chunk
        .descendants::<LuaExpr>()
        .find(|expr| TextSize::from(u32::from(expr.get_position())) == initializer_offset)
    else {
        return Vec::new();
    };

    match expr {
        LuaExpr::TableExpr(table_expr) => {
            collect_member_candidates_from_table_expr(table_expr, member_name)
        }
        _ => Vec::new(),
    }
}

fn collect_member_candidates_from_table_expr(
    table_expr: LuaTableExpr,
    member_name: &SmolStr,
) -> Vec<SalsaTypeCandidateSummary> {
    let sequence_index = parse_numeric_member_index(member_name);
    let exact_named_or_integer_candidates = table_expr
        .get_fields_with_keys()
        .into_iter()
        .filter_map(|field| {
            let (field, key) = field;
            let matches = match key {
                LuaIndexKey::Name(name_token) => name_token.get_name_text() == member_name.as_str(),
                LuaIndexKey::String(string_token) => {
                    string_token.get_value() == member_name.as_str()
                }
                LuaIndexKey::Integer(number_token) => match number_token.get_number_value() {
                    NumberResult::Int(value) => member_name.parse::<i64>().ok() == Some(value),
                    _ => false,
                },
                _ => false,
            };
            if !matches {
                return None;
            }

            Some(table_field_to_candidate(field))
        })
        .collect::<Vec<_>>();
    if sequence_index.is_none() && !exact_named_or_integer_candidates.is_empty() {
        return exact_named_or_integer_candidates;
    }

    let shape = analyze_table_expr_shape(table_expr.clone());
    let Some(sequence_index) = sequence_index else {
        return exact_named_or_integer_candidates;
    };

    let exact_numeric_candidates =
        collect_table_field_candidates_by_numeric_index(table_expr.clone(), sequence_index);

    let candidates = match shape.kind {
        SalsaTableShapeKindSummary::SequenceLike => match shape.sequence_kind {
            SalsaSequenceShapeKindSummary::TupleLike => exact_numeric_candidates,
            SalsaSequenceShapeKindSummary::ArrayLike => {
                if exact_numeric_candidates
                    .iter()
                    .any(|candidate| candidate.value_result_index > 0)
                {
                    exact_numeric_candidates
                } else {
                    table_expr
                        .get_fields_with_keys()
                        .into_iter()
                        .filter_map(|(field, key)| match key {
                            LuaIndexKey::Idx(_) => Some(table_field_to_candidate(field)),
                            _ => None,
                        })
                        .collect()
                }
            }
            SalsaSequenceShapeKindSummary::None => exact_named_or_integer_candidates,
        },
        SalsaTableShapeKindSummary::Mixed => {
            let mut candidates = exact_named_or_integer_candidates;
            candidates.extend(exact_numeric_candidates);
            candidates
        }
        SalsaTableShapeKindSummary::ObjectLike | SalsaTableShapeKindSummary::Empty => {
            exact_named_or_integer_candidates
        }
    };

    dedupe_type_candidates(candidates)
}

fn collect_table_field_candidates_by_numeric_index(
    table_expr: LuaTableExpr,
    sequence_index: usize,
) -> Vec<SalsaTypeCandidateSummary> {
    let fields = table_expr.get_fields_with_keys();
    let last_field_index = fields.len().saturating_sub(1);

    fields
        .into_iter()
        .enumerate()
        .filter_map(|(field_index, (field, key))| match key {
            LuaIndexKey::Integer(number_token) => match number_token.get_number_value() {
                NumberResult::Int(value) if value == sequence_index as i64 => {
                    Some(table_field_to_candidate(field))
                }
                _ => None,
            },
            LuaIndexKey::Idx(index) if index == sequence_index => {
                Some(table_field_to_candidate(field))
            }
            LuaIndexKey::Idx(base_index)
                if field_index == last_field_index && sequence_index > base_index =>
            {
                let source_call_syntax_id = field
                    .get_value_expr()
                    .as_ref()
                    .and_then(call_expr_syntax_id_of_expr);
                source_call_syntax_id.map(|syntax_id| {
                    table_field_to_candidate_with_slot(
                        field,
                        sequence_index - base_index,
                        Some(syntax_id),
                    )
                })
            }
            _ => None,
        })
        .collect()
}

fn table_field_to_candidate(field: emmylua_parser::LuaTableField) -> SalsaTypeCandidateSummary {
    let source_call_syntax_id = field
        .get_value_expr()
        .as_ref()
        .and_then(call_expr_syntax_id_of_expr);
    table_field_to_candidate_with_slot(field, 0, source_call_syntax_id)
}

fn table_field_to_candidate_with_slot(
    field: emmylua_parser::LuaTableField,
    value_result_index: usize,
    source_call_syntax_id: Option<SalsaSyntaxIdSummary>,
) -> SalsaTypeCandidateSummary {
    let signature_offset = field
        .get_value_expr()
        .and_then(signature_initializer_offset_of_expr);
    SalsaTypeCandidateSummary {
        origin: super::data::SalsaTypeCandidateOriginSummary::Property(TextSize::from(u32::from(
            field.get_position(),
        ))),
        explicit_type_offsets: Vec::new(),
        named_type_names: Vec::new(),
        initializer_offset: field
            .get_value_expr()
            .map(|expr| TextSize::from(u32::from(expr.get_position()))),
        value_expr_syntax_id: field
            .get_value_expr()
            .map(|expr| expr.get_syntax_id().into()),
        value_result_index,
        source_call_syntax_id,
        signature_offset,
    }
}

fn call_expr_syntax_id_of_expr(expr: &emmylua_parser::LuaExpr) -> Option<SalsaSyntaxIdSummary> {
    match expr {
        emmylua_parser::LuaExpr::CallExpr(call_expr) => Some(call_expr.get_syntax_id().into()),
        _ => None,
    }
}

fn signature_initializer_offset_of_expr(expr: emmylua_parser::LuaExpr) -> Option<TextSize> {
    match expr {
        emmylua_parser::LuaExpr::ClosureExpr(closure) => {
            Some(TextSize::from(u32::from(closure.get_position())))
        }
        _ => None,
    }
}

fn collect_member_candidates_from_type_offset(
    type_offset: SalsaDocTypeNodeKey,
    member_name: &SmolStr,
    property_query_index: &SalsaPropertyQueryIndex,
    chunk: &LuaChunk,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited_offsets: &mut BTreeSet<SalsaDocTypeNodeKey>,
    visited_type_names: &mut BTreeSet<SmolStr>,
) -> Vec<SalsaTypeCandidateSummary> {
    collect_member_candidates_from_type_offset_with_substitution(
        type_offset,
        member_name,
        property_query_index,
        chunk,
        doc,
        doc_types,
        lowered_types,
        visited_offsets,
        visited_type_names,
        None,
    )
}

fn collect_member_candidates_from_type_offset_with_substitution(
    type_offset: SalsaDocTypeNodeKey,
    member_name: &SmolStr,
    property_query_index: &SalsaPropertyQueryIndex,
    chunk: &LuaChunk,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited_offsets: &mut BTreeSet<SalsaDocTypeNodeKey>,
    visited_type_names: &mut BTreeSet<SmolStr>,
    generic_args: Option<&BTreeMap<SmolStr, Vec<SalsaDocTypeNodeKey>>>,
) -> Vec<SalsaTypeCandidateSummary> {
    if !visited_offsets.insert(type_offset) {
        return Vec::new();
    }

    let mut candidates = Vec::new();

    if let Some(doc_type) = doc_types
        .types
        .iter()
        .find(|doc_type| doc_type.node_key() == type_offset)
    {
        if let Some(generic_args) = generic_args {
            match &doc_type.kind {
                SalsaDocTypeKindSummary::Infer { generic_name } => {
                    if let Some(offsets) = generic_name
                        .as_ref()
                        .and_then(|name| generic_args.get(name))
                    {
                        return offsets
                            .iter()
                            .flat_map(|offset| {
                                collect_member_candidates_from_type_offset(
                                    *offset,
                                    member_name,
                                    property_query_index,
                                    chunk,
                                    doc,
                                    doc_types,
                                    lowered_types,
                                    visited_offsets,
                                    visited_type_names,
                                )
                            })
                            .collect();
                    }
                }
                SalsaDocTypeKindSummary::Name { name } => {
                    if let Some(offsets) = name.as_ref().and_then(|name| generic_args.get(name)) {
                        return offsets
                            .iter()
                            .flat_map(|offset| {
                                collect_member_candidates_from_type_offset(
                                    *offset,
                                    member_name,
                                    property_query_index,
                                    chunk,
                                    doc,
                                    doc_types,
                                    lowered_types,
                                    visited_offsets,
                                    visited_type_names,
                                )
                            })
                            .collect();
                    }
                }
                _ => {}
            }
        }

        match &doc_type.kind {
            SalsaDocTypeKindSummary::Name { name } => {
                if let Some(type_name) = name {
                    candidates.extend(collect_member_candidates_from_named_type(
                        type_name,
                        member_name,
                        property_query_index,
                        chunk,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
            }
            SalsaDocTypeKindSummary::Object { fields } => {
                candidates.extend(collect_object_field_candidates(fields.iter().filter_map(
                    |field| match &field.key {
                        SalsaDocObjectFieldKeySummary::Name(name)
                        | SalsaDocObjectFieldKeySummary::String(name)
                            if name == member_name =>
                        {
                            Some((field.syntax_offset, field.value_type_offset))
                        }
                        _ => None,
                    },
                )));
            }
            SalsaDocTypeKindSummary::Binary {
                left_type_offset,
                right_type_offset,
                ..
            } => {
                if let Some(left) = left_type_offset {
                    candidates.extend(collect_member_candidates_from_type_offset(
                        *left,
                        member_name,
                        property_query_index,
                        chunk,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
                if let Some(right) = right_type_offset {
                    candidates.extend(collect_member_candidates_from_type_offset(
                        *right,
                        member_name,
                        property_query_index,
                        chunk,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
            }
            SalsaDocTypeKindSummary::Nullable { inner_type_offset }
            | SalsaDocTypeKindSummary::Unary {
                inner_type_offset, ..
            } => {
                if let Some(inner) = inner_type_offset {
                    candidates.extend(collect_member_candidates_from_type_offset(
                        *inner,
                        member_name,
                        property_query_index,
                        chunk,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
            }
            SalsaDocTypeKindSummary::Tuple { item_type_offsets }
            | SalsaDocTypeKindSummary::MultiLineUnion { item_type_offsets } => {
                for item in item_type_offsets {
                    candidates.extend(collect_member_candidates_from_type_offset(
                        *item,
                        member_name,
                        property_query_index,
                        chunk,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
            }
            SalsaDocTypeKindSummary::Generic {
                base_type_offset, ..
            } => {
                if let Some(resolved) =
                    find_resolved_doc_type_by_key_from_parts(doc_types, lowered_types, type_offset)
                {
                    candidates.extend(collect_member_candidates_from_lowered_kind(
                        type_offset.syntax_offset(),
                        resolved.lowered.kind,
                        member_name,
                        property_query_index,
                        chunk,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                } else if let Some(base) = base_type_offset {
                    candidates.extend(collect_member_candidates_from_type_offset(
                        *base,
                        member_name,
                        property_query_index,
                        chunk,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
            }
            SalsaDocTypeKindSummary::IndexAccess {
                base_type_offset,
                index_type_offset,
            } => {
                if let (Some(base), Some(index)) = (base_type_offset, index_type_offset) {
                    candidates.extend(collect_member_candidates_from_index_access_type_offset(
                        *base,
                        *index,
                        member_name,
                        property_query_index,
                        chunk,
                        doc,
                        doc_types,
                        lowered_types,
                    ));
                }
            }
            SalsaDocTypeKindSummary::Conditional {
                true_type_offset,
                false_type_offset,
                ..
            } => {
                if let Some(true_type) = true_type_offset {
                    candidates.extend(collect_member_candidates_from_type_offset(
                        *true_type,
                        member_name,
                        property_query_index,
                        chunk,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
                if let Some(false_type) = false_type_offset {
                    candidates.extend(collect_member_candidates_from_type_offset(
                        *false_type,
                        member_name,
                        property_query_index,
                        chunk,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
            }
            SalsaDocTypeKindSummary::Mapped {
                key_type_offsets,
                value_type_offset,
                ..
            } => {
                if mapped_doc_type_allows_member_name_with_substitution(
                    type_offset,
                    key_type_offsets.iter().copied(),
                    member_name,
                    property_query_index,
                    doc,
                    doc_types,
                    lowered_types,
                    generic_args,
                ) {
                    candidates.extend(collect_object_field_candidates(
                        value_type_offset
                            .iter()
                            .copied()
                            .map(|offset| (type_offset.syntax_offset(), Some(offset))),
                    ));
                }
            }
            _ => {}
        }
    } else if let Some(resolved) =
        find_resolved_doc_type_by_key_from_parts(doc_types, lowered_types, type_offset)
    {
        candidates.extend(collect_member_candidates_from_lowered_kind(
            type_offset.syntax_offset(),
            resolved.lowered.kind,
            member_name,
            property_query_index,
            chunk,
            doc,
            doc_types,
            lowered_types,
            visited_offsets,
            visited_type_names,
        ));
    }

    dedupe_type_candidates(candidates)
}

fn collect_member_candidates_from_lowered_kind(
    syntax_offset: TextSize,
    lowered_kind: SalsaDocTypeLoweredKind,
    member_name: &SmolStr,
    property_query_index: &SalsaPropertyQueryIndex,
    chunk: &LuaChunk,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited_offsets: &mut BTreeSet<SalsaDocTypeNodeKey>,
    visited_type_names: &mut BTreeSet<SmolStr>,
) -> Vec<SalsaTypeCandidateSummary> {
    let mut candidates = Vec::new();

    match lowered_kind {
        SalsaDocTypeLoweredKind::Name { name } => {
            candidates.extend(collect_member_candidates_from_named_type(
                &name,
                member_name,
                property_query_index,
                chunk,
                doc,
                doc_types,
                lowered_types,
                visited_offsets,
                visited_type_names,
            ));
        }
        SalsaDocTypeLoweredKind::Object { fields } => {
            candidates.extend(collect_object_field_candidates(
                fields.into_iter().filter_map(|field| match field.key {
                    SalsaDocTypeLoweredObjectFieldKey::Name(name)
                    | SalsaDocTypeLoweredObjectFieldKey::String(name)
                        if name == *member_name =>
                    {
                        Some((field.syntax_offset, field.value_type.doc_node_key()))
                    }
                    _ => None,
                }),
            ));
        }
        SalsaDocTypeLoweredKind::Union { item_types }
        | SalsaDocTypeLoweredKind::Intersection { item_types }
        | SalsaDocTypeLoweredKind::Tuple { item_types }
        | SalsaDocTypeLoweredKind::MultiLineUnion { item_types } => {
            for item in item_types {
                if let SalsaDocTypeRef::Node(offset) = item {
                    candidates.extend(collect_member_candidates_from_type_offset(
                        offset,
                        member_name,
                        property_query_index,
                        chunk,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
            }
        }
        SalsaDocTypeLoweredKind::Nullable { inner_type }
        | SalsaDocTypeLoweredKind::Array {
            item_type: inner_type,
        }
        | SalsaDocTypeLoweredKind::Variadic {
            item_type: inner_type,
        } => {
            if let SalsaDocTypeRef::Node(offset) = inner_type {
                candidates.extend(collect_member_candidates_from_type_offset(
                    offset,
                    member_name,
                    property_query_index,
                    chunk,
                    doc,
                    doc_types,
                    lowered_types,
                    visited_offsets,
                    visited_type_names,
                ));
            }
        }
        SalsaDocTypeLoweredKind::Generic {
            base_type,
            arg_types,
        } => {
            candidates.extend(collect_member_candidates_from_generic_instance(
                base_type,
                arg_types,
                member_name,
                property_query_index,
                chunk,
                doc,
                doc_types,
                lowered_types,
                visited_offsets,
                visited_type_names,
            ));
        }
        SalsaDocTypeLoweredKind::IndexAccess {
            base_type,
            index_type,
        } => {
            if let (SalsaDocTypeRef::Node(base_offset), SalsaDocTypeRef::Node(index_offset)) =
                (base_type, index_type)
            {
                candidates.extend(collect_member_candidates_from_index_access_type_offset(
                    base_offset,
                    index_offset,
                    member_name,
                    property_query_index,
                    chunk,
                    doc,
                    doc_types,
                    lowered_types,
                ));
            }
        }
        SalsaDocTypeLoweredKind::Conditional {
            true_type,
            false_type,
            ..
        } => {
            for branch in [true_type, false_type] {
                if let SalsaDocTypeRef::Node(offset) = branch {
                    candidates.extend(collect_member_candidates_from_type_offset(
                        offset,
                        member_name,
                        property_query_index,
                        chunk,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
            }
        }
        SalsaDocTypeLoweredKind::Mapped {
            key_types,
            value_type,
            ..
        } => {
            if mapped_key_types_allow_member_name(
                key_types.into_iter().filter_map(|key| match key {
                    SalsaDocTypeRef::Node(offset) => Some(offset),
                    SalsaDocTypeRef::Incomplete => None,
                }),
                member_name,
                property_query_index,
                doc,
                doc_types,
                lowered_types,
            ) {
                candidates.extend(collect_object_field_candidates(
                    value_type
                        .doc_node_key()
                        .into_iter()
                        .map(|offset| (syntax_offset, Some(offset))),
                ));
            }
        }
        _ => {}
    }

    candidates
}

fn mapped_doc_type_allows_member_name_with_substitution(
    type_offset: SalsaDocTypeNodeKey,
    raw_key_type_offsets: impl IntoIterator<Item = SalsaDocTypeNodeKey>,
    member_name: &SmolStr,
    property_query_index: &SalsaPropertyQueryIndex,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    generic_args: Option<&BTreeMap<SmolStr, Vec<SalsaDocTypeNodeKey>>>,
) -> bool {
    let raw_keys = collect_mapped_member_names_from_key_types(
        raw_key_type_offsets,
        property_query_index,
        doc,
        doc_types,
        lowered_types,
        generic_args,
    );
    if !raw_keys.is_empty() {
        return raw_keys.into_iter().any(|key| key == *member_name);
    }

    let lowered_keys =
        find_resolved_doc_type_by_key_from_parts(doc_types, lowered_types, type_offset)
            .and_then(|resolved| match resolved.lowered.kind {
                SalsaDocTypeLoweredKind::Mapped { key_types, .. } => {
                    Some(collect_mapped_member_names_from_key_types(
                        key_types.into_iter().filter_map(|key| match key {
                            SalsaDocTypeRef::Node(offset) => Some(offset),
                            SalsaDocTypeRef::Incomplete => None,
                        }),
                        property_query_index,
                        doc,
                        doc_types,
                        lowered_types,
                        generic_args,
                    ))
                }
                _ => None,
            })
            .unwrap_or_default();

    lowered_keys.is_empty() || lowered_keys.into_iter().any(|key| key == *member_name)
}

fn mapped_key_types_allow_member_name(
    key_type_offsets: impl IntoIterator<Item = SalsaDocTypeNodeKey>,
    member_name: &SmolStr,
    property_query_index: &SalsaPropertyQueryIndex,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
) -> bool {
    let keys = collect_mapped_member_names_from_key_types(
        key_type_offsets,
        property_query_index,
        doc,
        doc_types,
        lowered_types,
        None,
    );

    keys.is_empty() || keys.into_iter().any(|key| key == *member_name)
}

fn collect_mapped_member_names_from_key_types(
    key_type_offsets: impl IntoIterator<Item = SalsaDocTypeNodeKey>,
    property_query_index: &SalsaPropertyQueryIndex,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    generic_args: Option<&BTreeMap<SmolStr, Vec<SalsaDocTypeNodeKey>>>,
) -> Vec<SmolStr> {
    let keys = key_type_offsets
        .into_iter()
        .flat_map(|key_type_offset| {
            collect_index_access_member_keys_from_type_offset_with_substitution(
                key_type_offset,
                property_query_index,
                doc,
                doc_types,
                lowered_types,
                &mut BTreeSet::new(),
                &mut BTreeSet::new(),
                generic_args,
            )
        })
        .collect();
    dedupe_member_names(keys)
}

fn collect_member_candidates_from_generic_instance(
    base_type: SalsaDocTypeRef,
    arg_types: Vec<SalsaDocTypeRef>,
    member_name: &SmolStr,
    property_query_index: &SalsaPropertyQueryIndex,
    chunk: &LuaChunk,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited_offsets: &mut BTreeSet<SalsaDocTypeNodeKey>,
    visited_type_names: &mut BTreeSet<SmolStr>,
) -> Vec<SalsaTypeCandidateSummary> {
    let SalsaDocTypeRef::Node(base_offset) = base_type else {
        return Vec::new();
    };

    let Some(base_doc_type) = doc_types
        .types
        .iter()
        .find(|doc_type| doc_type.node_key() == base_offset)
    else {
        return collect_member_candidates_from_type_offset(
            base_offset,
            member_name,
            property_query_index,
            chunk,
            doc,
            doc_types,
            lowered_types,
            visited_offsets,
            visited_type_names,
        );
    };

    let SalsaDocTypeKindSummary::Name {
        name: Some(type_name),
    } = &base_doc_type.kind
    else {
        return collect_member_candidates_from_type_offset(
            base_offset,
            member_name,
            property_query_index,
            chunk,
            doc,
            doc_types,
            lowered_types,
            visited_offsets,
            visited_type_names,
        );
    };

    let Some(type_def) = doc
        .type_defs
        .iter()
        .find(|type_def| type_def.name == *type_name)
    else {
        return collect_member_candidates_from_type_offset(
            base_offset,
            member_name,
            property_query_index,
            chunk,
            doc,
            doc_types,
            lowered_types,
            visited_offsets,
            visited_type_names,
        );
    };

    let Some(value_type_offset) = type_def.value_type_offset else {
        return collect_member_candidates_from_type_offset(
            base_offset,
            member_name,
            property_query_index,
            chunk,
            doc,
            doc_types,
            lowered_types,
            visited_offsets,
            visited_type_names,
        );
    };

    let generic_arg_map = build_generic_arg_type_map(&type_def.generic_params, &arg_types);
    collect_member_candidates_from_type_offset_with_substitution(
        value_type_offset,
        member_name,
        property_query_index,
        chunk,
        doc,
        doc_types,
        lowered_types,
        visited_offsets,
        visited_type_names,
        Some(&generic_arg_map),
    )
}

fn collect_member_candidates_from_named_type(
    type_name: &SmolStr,
    member_name: &SmolStr,
    property_query_index: &SalsaPropertyQueryIndex,
    chunk: &LuaChunk,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited_offsets: &mut BTreeSet<SalsaDocTypeNodeKey>,
    visited_type_names: &mut BTreeSet<SmolStr>,
) -> Vec<SalsaTypeCandidateSummary> {
    let mut candidates =
        collect_direct_type_member_candidates(type_name, member_name, property_query_index, chunk);

    if !visited_type_names.insert(type_name.clone()) {
        return dedupe_type_candidates(candidates);
    }

    if let Some(type_def) = doc
        .type_defs
        .iter()
        .find(|type_def| type_def.name == *type_name)
    {
        match type_def.kind {
            SalsaDocTypeDefKindSummary::Alias | SalsaDocTypeDefKindSummary::Attribute => {
                if let Some(value_type_offset) = type_def.value_type_offset {
                    candidates.extend(collect_member_candidates_from_type_offset(
                        value_type_offset,
                        member_name,
                        property_query_index,
                        chunk,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
            }
            SalsaDocTypeDefKindSummary::Class => {
                for super_type_offset in &type_def.super_type_offsets {
                    candidates.extend(collect_member_candidates_from_type_offset(
                        *super_type_offset,
                        member_name,
                        property_query_index,
                        chunk,
                        doc,
                        doc_types,
                        lowered_types,
                        visited_offsets,
                        visited_type_names,
                    ));
                }
            }
            SalsaDocTypeDefKindSummary::Enum => {}
        }
    }

    dedupe_type_candidates(candidates)
}

fn collect_direct_type_member_candidates(
    type_name: &SmolStr,
    member_name: &SmolStr,
    property_query_index: &SalsaPropertyQueryIndex,
    chunk: &LuaChunk,
) -> Vec<SalsaTypeCandidateSummary> {
    let Some(sequence_index) = parse_numeric_member_index(member_name) else {
        return collect_properties_for_type_and_key_in_index(
            property_query_index,
            type_name,
            &crate::SalsaPropertyKeySummary::Name(member_name.clone()),
        )
        .into_iter()
        .map(property_to_candidate)
        .collect();
    };

    let exact_numeric_candidates =
        collect_numeric_type_property_candidates(property_query_index, type_name, sequence_index);

    let Some(shape) = find_type_owner_table_shape(property_query_index, type_name, chunk) else {
        return exact_numeric_candidates;
    };

    let candidates = match shape.kind {
        SalsaTableShapeKindSummary::SequenceLike => match shape.sequence_kind {
            SalsaSequenceShapeKindSummary::ArrayLike => {
                if exact_numeric_candidates
                    .iter()
                    .any(|candidate| candidate.value_result_index > 0)
                {
                    exact_numeric_candidates
                } else {
                    collect_sequence_type_property_candidates(property_query_index, type_name)
                }
            }
            SalsaSequenceShapeKindSummary::TupleLike => exact_numeric_candidates,
            SalsaSequenceShapeKindSummary::None => exact_numeric_candidates,
        },
        SalsaTableShapeKindSummary::Mixed => exact_numeric_candidates,
        SalsaTableShapeKindSummary::ObjectLike | SalsaTableShapeKindSummary::Empty => {
            exact_numeric_candidates
        }
    };

    dedupe_type_candidates(candidates)
}

fn collect_numeric_type_property_candidates(
    property_query_index: &SalsaPropertyQueryIndex,
    type_name: &SmolStr,
    sequence_index: usize,
) -> Vec<SalsaTypeCandidateSummary> {
    let mut properties = collect_properties_for_type_and_key_in_index(
        property_query_index,
        type_name,
        &crate::SalsaPropertyKeySummary::Integer(sequence_index as i64),
    );
    properties.extend(collect_properties_for_type_and_key_in_index(
        property_query_index,
        type_name,
        &crate::SalsaPropertyKeySummary::Sequence(sequence_index),
    ));
    properties.into_iter().map(property_to_candidate).collect()
}

fn collect_sequence_type_property_candidates(
    property_query_index: &SalsaPropertyQueryIndex,
    type_name: &SmolStr,
) -> Vec<SalsaTypeCandidateSummary> {
    collect_properties_for_type_in_index(property_query_index, type_name)
        .into_iter()
        .filter(|property| {
            matches!(
                property.source,
                crate::SalsaPropertySourceSummary::TableField
            ) && matches!(property.key, crate::SalsaPropertyKeySummary::Sequence(_))
        })
        .map(property_to_candidate)
        .collect()
}

fn find_type_owner_table_shape(
    property_query_index: &SalsaPropertyQueryIndex,
    type_name: &SmolStr,
    chunk: &LuaChunk,
) -> Option<crate::SalsaTableShapeSummary> {
    let first_table_field_offset =
        collect_properties_for_type_in_index(property_query_index, type_name)
            .into_iter()
            .find(|property| {
                matches!(
                    property.source,
                    crate::SalsaPropertySourceSummary::TableField
                )
            })?
            .syntax_offset;

    chunk.descendants::<LuaTableExpr>().find_map(|table_expr| {
        table_expr
            .get_fields()
            .any(|field| {
                TextSize::from(u32::from(field.get_position())) == first_table_field_offset
            })
            .then(|| analyze_table_expr_shape(table_expr))
    })
}

fn parse_numeric_member_index(member_name: &SmolStr) -> Option<usize> {
    member_name.parse::<usize>().ok().filter(|index| *index > 0)
}

fn collect_object_field_candidates(
    fields: impl IntoIterator<Item = (TextSize, Option<SalsaDocTypeNodeKey>)>,
) -> Vec<SalsaTypeCandidateSummary> {
    fields
        .into_iter()
        .map(
            |(syntax_offset, value_type_offset)| SalsaTypeCandidateSummary {
                origin: super::data::SalsaTypeCandidateOriginSummary::Property(syntax_offset),
                explicit_type_offsets: value_type_offset.into_iter().collect(),
                named_type_names: Vec::new(),
                initializer_offset: None,
                value_expr_syntax_id: None,
                value_result_index: 0,
                source_call_syntax_id: None,
                signature_offset: None,
            },
        )
        .collect()
}

trait SalsaDocTypeRefExt {
    fn doc_node_key(self) -> Option<SalsaDocTypeNodeKey>;
}

impl SalsaDocTypeRefExt for SalsaDocTypeRef {
    fn doc_node_key(self) -> Option<SalsaDocTypeNodeKey> {
        match self {
            SalsaDocTypeRef::Node(offset) => Some(offset),
            SalsaDocTypeRef::Incomplete => None,
        }
    }
}

fn collect_condition_narrows_for_expr(
    decl_tree: &SalsaDeclTreeSummary,
    expr: LuaExpr,
    decl_id: SalsaDeclId,
    positive: bool,
) -> Vec<SalsaTypeNarrowSummary> {
    match expr {
        LuaExpr::NameExpr(name_expr) => name_expr_targets_decl(decl_tree, &name_expr, decl_id)
            .then_some(if positive {
                SalsaTypeNarrowSummary::Truthy
            } else {
                SalsaTypeNarrowSummary::Falsey
            })
            .into_iter()
            .collect(),
        LuaExpr::ParenExpr(paren_expr) => paren_expr
            .get_expr()
            .map(|inner| collect_condition_narrows_for_expr(decl_tree, inner, decl_id, positive))
            .unwrap_or_default(),
        LuaExpr::UnaryExpr(unary_expr) => {
            let Some(op_token) = unary_expr.get_op_token() else {
                return Vec::new();
            };
            match op_token.get_op() {
                UnaryOperator::OpNot => unary_expr
                    .get_expr()
                    .map(|inner| {
                        collect_condition_narrows_for_expr(decl_tree, inner, decl_id, !positive)
                    })
                    .unwrap_or_default(),
                _ => Vec::new(),
            }
        }
        LuaExpr::BinaryExpr(binary_expr) => {
            let Some(op_token) = binary_expr.get_op_token() else {
                return Vec::new();
            };
            match op_token.get_op() {
                BinaryOperator::OpAnd if positive => binary_expr
                    .get_exprs()
                    .map(|(left, right)| {
                        let mut narrows =
                            collect_condition_narrows_for_expr(decl_tree, left, decl_id, true);
                        narrows.extend(collect_condition_narrows_for_expr(
                            decl_tree, right, decl_id, true,
                        ));
                        narrows
                    })
                    .unwrap_or_default(),
                BinaryOperator::OpOr if !positive => binary_expr
                    .get_exprs()
                    .map(|(left, right)| {
                        let mut narrows =
                            collect_condition_narrows_for_expr(decl_tree, left, decl_id, false);
                        narrows.extend(collect_condition_narrows_for_expr(
                            decl_tree, right, decl_id, false,
                        ));
                        narrows
                    })
                    .unwrap_or_default(),
                BinaryOperator::OpEq => match binary_expr.get_exprs() {
                    Some((left, right)) => {
                        let mut narrows = literal_narrow_from_binary_expr(
                            decl_tree,
                            left.clone(),
                            right.clone(),
                            decl_id,
                            positive,
                        );
                        narrows.extend(field_literal_narrow_from_binary_expr(
                            decl_tree,
                            left.clone(),
                            right.clone(),
                            decl_id,
                            positive,
                        ));
                        narrows.extend(type_guard_narrow_from_binary_expr(
                            decl_tree, left, right, decl_id, positive,
                        ));
                        narrows
                    }
                    None => Vec::new(),
                },
                BinaryOperator::OpNe => match binary_expr.get_exprs() {
                    Some((left, right)) => {
                        let mut narrows = literal_narrow_from_binary_expr(
                            decl_tree,
                            left.clone(),
                            right.clone(),
                            decl_id,
                            !positive,
                        );
                        narrows.extend(field_literal_narrow_from_binary_expr(
                            decl_tree,
                            left.clone(),
                            right.clone(),
                            decl_id,
                            !positive,
                        ));
                        narrows.extend(type_guard_narrow_from_binary_expr(
                            decl_tree, left, right, decl_id, !positive,
                        ));
                        narrows
                    }
                    None => Vec::new(),
                },
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

fn literal_narrow_from_binary_expr(
    decl_tree: &SalsaDeclTreeSummary,
    left: LuaExpr,
    right: LuaExpr,
    decl_id: SalsaDeclId,
    positive: bool,
) -> Vec<SalsaTypeNarrowSummary> {
    let (name_expr, literal_expr) = match (left, right) {
        (LuaExpr::NameExpr(name_expr), LuaExpr::LiteralExpr(literal_expr)) => {
            (Some(name_expr), Some(literal_expr))
        }
        (LuaExpr::LiteralExpr(literal_expr), LuaExpr::NameExpr(name_expr)) => {
            (Some(name_expr), Some(literal_expr))
        }
        _ => (None, None),
    };
    let Some(name_expr) = name_expr else {
        return Vec::new();
    };
    let Some(literal_expr) = literal_expr else {
        return Vec::new();
    };
    if !name_expr_targets_decl(decl_tree, &name_expr, decl_id) {
        return Vec::new();
    }
    vec![SalsaTypeNarrowSummary::Literal {
        literal_text: literal_expr.syntax().text().to_string().into(),
        positive,
    }]
}

fn field_literal_narrow_from_binary_expr(
    decl_tree: &SalsaDeclTreeSummary,
    left: LuaExpr,
    right: LuaExpr,
    decl_id: SalsaDeclId,
    positive: bool,
) -> Vec<SalsaTypeNarrowSummary> {
    let (index_expr, literal_expr) = match (left, right) {
        (LuaExpr::IndexExpr(index_expr), LuaExpr::LiteralExpr(literal_expr)) => {
            (Some(index_expr), Some(literal_expr))
        }
        (LuaExpr::LiteralExpr(literal_expr), LuaExpr::IndexExpr(index_expr)) => {
            (Some(index_expr), Some(literal_expr))
        }
        _ => (None, None),
    };
    let Some(index_expr) = index_expr else {
        return Vec::new();
    };
    let Some(literal_expr) = literal_expr else {
        return Vec::new();
    };
    if !index_expr_prefix_targets_decl(decl_tree, &index_expr, decl_id) {
        return Vec::new();
    }
    let Some(member_name) = simple_member_name(&index_expr) else {
        return Vec::new();
    };
    vec![SalsaTypeNarrowSummary::FieldLiteral {
        member_name,
        literal_text: literal_expr.syntax().text().to_string().into(),
        positive,
    }]
}

fn index_expr_prefix_targets_decl(
    decl_tree: &SalsaDeclTreeSummary,
    index_expr: &LuaIndexExpr,
    decl_id: SalsaDeclId,
) -> bool {
    match index_expr.get_prefix_expr() {
        Some(LuaExpr::NameExpr(name_expr)) => {
            name_expr_targets_decl(decl_tree, &name_expr, decl_id)
        }
        Some(LuaExpr::ParenExpr(paren_expr)) => paren_expr
            .get_expr()
            .and_then(|expr| match expr {
                LuaExpr::NameExpr(name_expr) => {
                    Some(name_expr_targets_decl(decl_tree, &name_expr, decl_id))
                }
                _ => None,
            })
            .unwrap_or(false),
        _ => false,
    }
}

fn simple_member_name(index_expr: &LuaIndexExpr) -> Option<SmolStr> {
    match index_expr.get_index_key()? {
        emmylua_parser::LuaIndexKey::Name(name) => Some(name.get_name_text().into()),
        emmylua_parser::LuaIndexKey::String(string) => Some(string.get_value().into()),
        _ => None,
    }
}

fn type_guard_narrow_from_binary_expr(
    decl_tree: &SalsaDeclTreeSummary,
    left: LuaExpr,
    right: LuaExpr,
    decl_id: SalsaDeclId,
    positive: bool,
) -> Vec<SalsaTypeNarrowSummary> {
    let (call_expr, literal_expr) = match (left, right) {
        (LuaExpr::CallExpr(call_expr), LuaExpr::LiteralExpr(literal_expr))
            if call_expr.is_type() =>
        {
            (Some(call_expr), Some(literal_expr))
        }
        (LuaExpr::LiteralExpr(literal_expr), LuaExpr::CallExpr(call_expr))
            if call_expr.is_type() =>
        {
            (Some(call_expr), Some(literal_expr))
        }
        _ => (None, None),
    };
    let Some(call_expr) = call_expr else {
        return Vec::new();
    };
    let Some(LuaLiteralToken::String(string_token)) =
        literal_expr.and_then(|expr| expr.get_literal())
    else {
        return Vec::new();
    };
    let Some(arg_expr) = call_expr
        .get_args_list()
        .and_then(|args| args.get_args().next())
    else {
        return Vec::new();
    };
    match arg_expr {
        LuaExpr::NameExpr(name_expr) if name_expr_targets_decl(decl_tree, &name_expr, decl_id) => {
            vec![if positive {
                SalsaTypeNarrowSummary::TypeGuard {
                    type_name: string_token.get_value().into(),
                }
            } else {
                SalsaTypeNarrowSummary::ExcludeTypeGuard {
                    type_name: string_token.get_value().into(),
                }
            }]
        }
        _ => Vec::new(),
    }
}
