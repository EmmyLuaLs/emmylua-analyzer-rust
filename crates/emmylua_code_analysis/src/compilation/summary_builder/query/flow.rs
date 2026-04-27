#![allow(dead_code)]

use std::collections::{BTreeSet, HashMap};

use emmylua_parser::{LuaAstNode, LuaChunk, LuaExpr};
use rowan::TextSize;
use smol_str::SmolStr;

use super::super::summary::*;
use super::{
    SalsaCallExplainSummary, SalsaDeclTypeQueryIndex, SalsaDocTagQueryIndex,
    SalsaDocTypeLoweredIndex, SalsaDocTypeLoweredKind, SalsaDocTypeRef,
    SalsaLocalAssignmentQueryIndex, SalsaMemberTypeQueryIndex, SalsaSignatureExplainIndex,
    SalsaSignatureReturnQueryIndex, collect_active_type_narrows, find_call_explain_at,
    find_member_type_at_program_point, find_member_use_at, find_name_type_at_program_point,
    find_name_use_at, find_resolved_doc_type_by_key_from_parts, find_signature_return_query_at,
    signature_return_value_candidate_type_offsets,
};
use crate::{
    SalsaDeclId, SalsaDeclKindSummary, SalsaDeclTreeSummary, SalsaDocSummary,
    SalsaDocTypeIndexSummary, SalsaDocTypeNodeKey, SalsaNameUseResolutionSummary,
    SalsaProgramPointMemberTypeInfoSummary, SalsaProgramPointTypeInfoSummary,
    SalsaPropertyIndexSummary, SalsaTypeCandidateSummary, SalsaUseSiteIndexSummary,
};

pub fn build_flow_query_summary(flow: &SalsaFlowSummary) -> SalsaFlowQuerySummary {
    let index = FlowQueryIndex::new(flow);

    let root_block_offsets = flow
        .blocks
        .iter()
        .filter(|block| index.is_flow_region_root(block))
        .map(|block| block.syntax_offset)
        .collect();

    let branch_links: Vec<SalsaFlowBranchLinkSummary> = flow
        .branches
        .iter()
        .map(|branch| {
            let clause_block_offsets = branch
                .clauses
                .iter()
                .filter_map(|clause| clause.block_offset)
                .collect::<Vec<_>>();
            let entry_block_offset = clause_block_offsets
                .first()
                .and_then(|block_offset| index.find_block(*block_offset))
                .and_then(|block| block.parent_block_offset);

            SalsaFlowBranchLinkSummary {
                branch_offset: branch.syntax_offset,
                entry_block_offset,
                clause_block_offsets,
                exit_block_offset: entry_block_offset,
            }
        })
        .collect();

    let loop_links: Vec<SalsaFlowLoopLinkSummary> = flow
        .loops
        .iter()
        .map(|loop_summary| SalsaFlowLoopLinkSummary {
            loop_offset: loop_summary.syntax_offset,
            entry_block_offset: loop_summary
                .block_offset
                .and_then(|block_offset| index.find_block(block_offset))
                .and_then(|block| block.parent_block_offset),
            body_block_offset: loop_summary.block_offset,
            continue_block_offset: loop_summary.block_offset,
            exit_block_offset: loop_summary
                .block_offset
                .and_then(|block_offset| index.find_block(block_offset))
                .and_then(|block| block.parent_block_offset),
        })
        .collect();

    let return_links: Vec<SalsaFlowReturnLinkSummary> = flow
        .returns
        .iter()
        .map(|return_summary| SalsaFlowReturnLinkSummary {
            return_offset: return_summary.syntax_offset,
            block_offset: return_summary.block_offset,
            exit_root_block_offset: return_summary
                .block_offset
                .and_then(|block_offset| index.flow_region_root_offset_of(block_offset)),
        })
        .collect();

    let break_links: Vec<SalsaFlowBreakLinkSummary> = flow
        .breaks
        .iter()
        .map(|break_summary| {
            let enclosing_loop_offset = break_summary
                .block_offset
                .and_then(|block_offset| index.enclosing_loop_offset_of(block_offset));
            let exit_block_offset = enclosing_loop_offset
                .and_then(|loop_offset| index.find_loop(loop_offset))
                .and_then(|loop_summary| loop_summary.block_offset)
                .and_then(|block_offset| index.find_block(block_offset))
                .and_then(|block| block.parent_block_offset);

            SalsaFlowBreakLinkSummary {
                break_offset: break_summary.syntax_offset,
                block_offset: break_summary.block_offset,
                enclosing_loop_offset,
                exit_block_offset,
            }
        })
        .collect();

    let goto_links: Vec<SalsaFlowGotoLinkSummary> = flow
        .gotos
        .iter()
        .map(|goto_summary| {
            let target_label = goto_summary.block_offset.and_then(|block_offset| {
                index.resolve_goto_target_label(goto_summary, block_offset)
            });
            SalsaFlowGotoLinkSummary {
                goto_offset: goto_summary.syntax_offset,
                block_offset: goto_summary.block_offset,
                target_label_offset: target_label.map(|label| label.syntax_offset),
                target_block_offset: target_label.and_then(|label| label.block_offset),
            }
        })
        .collect();

    let mut terminal_edges = return_links
        .iter()
        .map(|link| SalsaFlowTerminalEdgeSummary {
            kind: SalsaFlowTerminalEdgeKindSummary::Return,
            syntax_offset: link.return_offset,
            source_block_offset: link.block_offset,
            target_block_offset: link.exit_root_block_offset,
            target_label_offset: None,
            flow_region_root_offset: link.exit_root_block_offset,
        })
        .collect::<Vec<_>>();
    terminal_edges.extend(break_links.iter().map(|link| {
        SalsaFlowTerminalEdgeSummary {
            kind: SalsaFlowTerminalEdgeKindSummary::Break,
            syntax_offset: link.break_offset,
            source_block_offset: link.block_offset,
            target_block_offset: link.exit_block_offset,
            target_label_offset: None,
            flow_region_root_offset: link
                .block_offset
                .and_then(|block_offset| index.flow_region_root_offset_of(block_offset)),
        }
    }));
    terminal_edges.extend(goto_links.iter().map(|link| {
        SalsaFlowTerminalEdgeSummary {
            kind: SalsaFlowTerminalEdgeKindSummary::Goto,
            syntax_offset: link.goto_offset,
            source_block_offset: link.block_offset,
            target_block_offset: link.target_block_offset,
            target_label_offset: link.target_label_offset,
            flow_region_root_offset: link
                .block_offset
                .and_then(|block_offset| index.flow_region_root_offset_of(block_offset)),
        }
    }));

    let branch_links_by_offset = branch_links
        .iter()
        .map(|link| (link.branch_offset, link))
        .collect::<HashMap<_, _>>();
    let loop_links_by_offset = loop_links
        .iter()
        .map(|link| (link.loop_offset, link))
        .collect::<HashMap<_, _>>();
    let return_links_by_offset = return_links
        .iter()
        .map(|link| (link.return_offset, link))
        .collect::<HashMap<_, _>>();
    let break_links_by_offset = break_links
        .iter()
        .map(|link| (link.break_offset, link))
        .collect::<HashMap<_, _>>();
    let goto_links_by_offset = goto_links
        .iter()
        .map(|link| (link.goto_offset, link))
        .collect::<HashMap<_, _>>();

    let mut edges = Vec::new();
    for block in &flow.blocks {
        if let Some(first_statement_offset) = block.statement_offsets.first() {
            edges.push(SalsaFlowEdgeSummary {
                kind: SalsaFlowEdgeKindSummary::BlockToStatement,
                from: SalsaFlowNodeRefSummary::Block(block.syntax_offset),
                to: SalsaFlowNodeRefSummary::Statement(*first_statement_offset),
            });
        }

        for statement_pair in block.statement_offsets.windows(2) {
            let Some(current_statement) = index.find_statement(statement_pair[0]) else {
                continue;
            };
            if statement_falls_through(current_statement) {
                edges.push(SalsaFlowEdgeSummary {
                    kind: SalsaFlowEdgeKindSummary::StatementFallthrough,
                    from: SalsaFlowNodeRefSummary::Statement(statement_pair[0]),
                    to: SalsaFlowNodeRefSummary::Statement(statement_pair[1]),
                });
            }
        }
    }

    for statement in &flow.statements {
        let statement_node = SalsaFlowNodeRefSummary::Statement(statement.syntax_offset);
        match statement.kind {
            SalsaFlowStatementKindSummary::Linear => {}
            SalsaFlowStatementKindSummary::Do => {
                if let Some(body_block) = index.find_owned_block(
                    SalsaFlowBlockOwnerKindSummary::DoStat,
                    statement.syntax_offset,
                ) {
                    edges.push(SalsaFlowEdgeSummary {
                        kind: SalsaFlowEdgeKindSummary::StatementToBlock,
                        from: statement_node,
                        to: SalsaFlowNodeRefSummary::Block(body_block.syntax_offset),
                    });
                }
            }
            SalsaFlowStatementKindSummary::Branch => {
                let merge_node = SalsaFlowNodeRefSummary::Merge(statement.syntax_offset);
                if let Some(next_target) = find_statement_successor_target(&index, statement) {
                    edges.push(SalsaFlowEdgeSummary {
                        kind: SalsaFlowEdgeKindSummary::MergeToNext,
                        from: merge_node.clone(),
                        to: next_target,
                    });
                }
                edges.push(SalsaFlowEdgeSummary {
                    kind: SalsaFlowEdgeKindSummary::StatementToBranch,
                    from: statement_node.clone(),
                    to: SalsaFlowNodeRefSummary::Branch(statement.syntax_offset),
                });
                if let Some(branch_summary) = index.find_branch(statement.syntax_offset)
                    && let Some(condition_offset) = branch_summary
                        .clauses
                        .first()
                        .and_then(|clause| clause.condition_node_offset)
                {
                    edges.push(SalsaFlowEdgeSummary {
                        kind: SalsaFlowEdgeKindSummary::StatementToCondition,
                        from: statement_node.clone(),
                        to: SalsaFlowNodeRefSummary::Condition(condition_offset),
                    });
                    append_branch_condition_edges(
                        &index,
                        branch_summary,
                        merge_node.clone(),
                        &mut edges,
                    );
                }
                if let Some(link) = branch_links_by_offset.get(&statement.syntax_offset) {
                    edges.extend(link.clause_block_offsets.iter().map(|block_offset| {
                        SalsaFlowEdgeSummary {
                            kind: SalsaFlowEdgeKindSummary::BranchToClause,
                            from: SalsaFlowNodeRefSummary::Branch(statement.syntax_offset),
                            to: SalsaFlowNodeRefSummary::Block(*block_offset),
                        }
                    }));
                    if find_statement_successor_target(&index, statement).is_some() {
                        edges.extend(link.clause_block_offsets.iter().map(|block_offset| {
                            SalsaFlowEdgeSummary {
                                kind: SalsaFlowEdgeKindSummary::ClauseToMerge,
                                from: SalsaFlowNodeRefSummary::Block(*block_offset),
                                to: SalsaFlowNodeRefSummary::Merge(statement.syntax_offset),
                            }
                        }));
                    }
                }
            }
            SalsaFlowStatementKindSummary::Loop => {
                let merge_node = SalsaFlowNodeRefSummary::Merge(statement.syntax_offset);
                if let Some(next_target) = find_statement_successor_target(&index, statement) {
                    edges.push(SalsaFlowEdgeSummary {
                        kind: SalsaFlowEdgeKindSummary::MergeToNext,
                        from: merge_node.clone(),
                        to: next_target,
                    });
                }
                edges.push(SalsaFlowEdgeSummary {
                    kind: SalsaFlowEdgeKindSummary::StatementToLoop,
                    from: statement_node.clone(),
                    to: SalsaFlowNodeRefSummary::Loop(statement.syntax_offset),
                });
                if let Some(loop_summary) = index.find_loop(statement.syntax_offset)
                    && let Some(condition_offset) = loop_summary.condition_node_offset
                {
                    edges.push(SalsaFlowEdgeSummary {
                        kind: SalsaFlowEdgeKindSummary::StatementToCondition,
                        from: statement_node.clone(),
                        to: SalsaFlowNodeRefSummary::Condition(condition_offset),
                    });
                    let true_target =
                        if matches!(loop_summary.kind, SalsaFlowLoopKindSummary::Repeat) {
                            merge_node.clone()
                        } else {
                            SalsaFlowNodeRefSummary::Loop(statement.syntax_offset)
                        };
                    let false_target =
                        if matches!(loop_summary.kind, SalsaFlowLoopKindSummary::Repeat) {
                            SalsaFlowNodeRefSummary::Loop(statement.syntax_offset)
                        } else {
                            merge_node.clone()
                        };
                    append_condition_edges(
                        &index,
                        condition_offset,
                        true_target,
                        false_target,
                        &mut edges,
                    );
                }
                if let Some(link) = loop_links_by_offset.get(&statement.syntax_offset) {
                    if let Some(body_block_offset) = link.body_block_offset {
                        edges.push(SalsaFlowEdgeSummary {
                            kind: SalsaFlowEdgeKindSummary::LoopToBody,
                            from: SalsaFlowNodeRefSummary::Loop(statement.syntax_offset),
                            to: SalsaFlowNodeRefSummary::Block(body_block_offset),
                        });
                    }
                    if let Some(continue_block_offset) = link.continue_block_offset {
                        let continue_target = index
                            .find_loop(statement.syntax_offset)
                            .and_then(|loop_summary| loop_summary.condition_node_offset)
                            .map(SalsaFlowNodeRefSummary::Condition)
                            .unwrap_or(SalsaFlowNodeRefSummary::Loop(statement.syntax_offset));
                        edges.push(SalsaFlowEdgeSummary {
                            kind: SalsaFlowEdgeKindSummary::LoopContinue,
                            from: SalsaFlowNodeRefSummary::Block(continue_block_offset),
                            to: continue_target,
                        });
                    }
                    if find_statement_successor_target(&index, statement).is_some() {
                        edges.push(SalsaFlowEdgeSummary {
                            kind: SalsaFlowEdgeKindSummary::LoopToMerge,
                            from: SalsaFlowNodeRefSummary::Loop(statement.syntax_offset),
                            to: merge_node,
                        });
                    }
                }
            }
            SalsaFlowStatementKindSummary::Return => {
                edges.push(SalsaFlowEdgeSummary {
                    kind: SalsaFlowEdgeKindSummary::StatementToTerminal,
                    from: statement_node,
                    to: SalsaFlowNodeRefSummary::Return(statement.syntax_offset),
                });
                if let Some(link) = return_links_by_offset.get(&statement.syntax_offset)
                    && let Some(target_block_offset) = link.exit_root_block_offset
                {
                    edges.push(SalsaFlowEdgeSummary {
                        kind: SalsaFlowEdgeKindSummary::TerminalToTarget,
                        from: SalsaFlowNodeRefSummary::Return(statement.syntax_offset),
                        to: SalsaFlowNodeRefSummary::Block(target_block_offset),
                    });
                }
                edges.push(SalsaFlowEdgeSummary {
                    kind: SalsaFlowEdgeKindSummary::TerminalToUnreachable,
                    from: SalsaFlowNodeRefSummary::Return(statement.syntax_offset),
                    to: SalsaFlowNodeRefSummary::Unreachable(statement.syntax_offset),
                });
            }
            SalsaFlowStatementKindSummary::Break => {
                edges.push(SalsaFlowEdgeSummary {
                    kind: SalsaFlowEdgeKindSummary::StatementToTerminal,
                    from: statement_node,
                    to: SalsaFlowNodeRefSummary::Break(statement.syntax_offset),
                });
                if let Some(link) = break_links_by_offset.get(&statement.syntax_offset)
                    && let Some(target_block_offset) = link.exit_block_offset
                {
                    edges.push(SalsaFlowEdgeSummary {
                        kind: SalsaFlowEdgeKindSummary::TerminalToTarget,
                        from: SalsaFlowNodeRefSummary::Break(statement.syntax_offset),
                        to: SalsaFlowNodeRefSummary::Block(target_block_offset),
                    });
                }
                edges.push(SalsaFlowEdgeSummary {
                    kind: SalsaFlowEdgeKindSummary::TerminalToUnreachable,
                    from: SalsaFlowNodeRefSummary::Break(statement.syntax_offset),
                    to: SalsaFlowNodeRefSummary::Unreachable(statement.syntax_offset),
                });
            }
            SalsaFlowStatementKindSummary::Goto => {
                edges.push(SalsaFlowEdgeSummary {
                    kind: SalsaFlowEdgeKindSummary::StatementToTerminal,
                    from: statement_node,
                    to: SalsaFlowNodeRefSummary::Goto(statement.syntax_offset),
                });
                if let Some(link) = goto_links_by_offset.get(&statement.syntax_offset)
                    && let Some(label_offset) = link.target_label_offset
                {
                    edges.push(SalsaFlowEdgeSummary {
                        kind: SalsaFlowEdgeKindSummary::GotoToLabel,
                        from: SalsaFlowNodeRefSummary::Goto(statement.syntax_offset),
                        to: SalsaFlowNodeRefSummary::Label(label_offset),
                    });
                }
                edges.push(SalsaFlowEdgeSummary {
                    kind: SalsaFlowEdgeKindSummary::TerminalToUnreachable,
                    from: SalsaFlowNodeRefSummary::Goto(statement.syntax_offset),
                    to: SalsaFlowNodeRefSummary::Unreachable(statement.syntax_offset),
                });
            }
            SalsaFlowStatementKindSummary::Label => {
                edges.push(SalsaFlowEdgeSummary {
                    kind: SalsaFlowEdgeKindSummary::StatementToLabel,
                    from: statement_node,
                    to: SalsaFlowNodeRefSummary::Label(statement.syntax_offset),
                });
                if let Some(next_target) = find_statement_successor_target(&index, statement) {
                    edges.push(SalsaFlowEdgeSummary {
                        kind: SalsaFlowEdgeKindSummary::LabelToNext,
                        from: SalsaFlowNodeRefSummary::Label(statement.syntax_offset),
                        to: next_target,
                    });
                }
            }
        }
    }

    SalsaFlowQuerySummary {
        root_block_offsets,
        branch_links,
        loop_links,
        return_links,
        break_links,
        goto_links,
        terminal_edges,
        edges,
    }
}

pub fn collect_successor_nodes(
    query: &SalsaFlowQuerySummary,
    node: &SalsaFlowNodeRefSummary,
) -> Vec<SalsaFlowNodeRefSummary> {
    query
        .edges
        .iter()
        .filter(|edge| &edge.from == node)
        .map(|edge| edge.to.clone())
        .collect()
}

pub fn collect_outgoing_edges(
    query: &SalsaFlowQuerySummary,
    node: &SalsaFlowNodeRefSummary,
) -> Vec<SalsaFlowEdgeSummary> {
    query
        .edges
        .iter()
        .filter(|edge| &edge.from == node)
        .cloned()
        .collect()
}

pub fn collect_predecessor_nodes(
    query: &SalsaFlowQuerySummary,
    node: &SalsaFlowNodeRefSummary,
) -> Vec<SalsaFlowNodeRefSummary> {
    query
        .edges
        .iter()
        .filter(|edge| &edge.to == node)
        .map(|edge| edge.from.clone())
        .collect()
}

pub fn collect_incoming_edges(
    query: &SalsaFlowQuerySummary,
    node: &SalsaFlowNodeRefSummary,
) -> Vec<SalsaFlowEdgeSummary> {
    query
        .edges
        .iter()
        .filter(|edge| &edge.to == node)
        .cloned()
        .collect()
}

pub fn collect_reachable_nodes(
    query: &SalsaFlowQuerySummary,
    start: &SalsaFlowNodeRefSummary,
) -> Vec<SalsaFlowNodeRefSummary> {
    let mut visited = Vec::new();
    let mut queue = vec![start.clone()];
    let mut index = 0;

    while let Some(node) = queue.get(index).cloned() {
        index += 1;
        for successor in collect_successor_nodes(query, &node) {
            if successor == *start || visited.contains(&successor) {
                continue;
            }
            visited.push(successor.clone());
            queue.push(successor);
        }
    }

    visited
}

pub fn can_reach_node(
    query: &SalsaFlowQuerySummary,
    from: &SalsaFlowNodeRefSummary,
    to: &SalsaFlowNodeRefSummary,
) -> bool {
    if from == to {
        return true;
    }

    collect_reachable_nodes(query, from).contains(to)
}

pub fn build_condition_graph_summary(
    flow: &SalsaFlowSummary,
    query: &SalsaFlowQuerySummary,
    condition_offset: u32,
) -> Option<SalsaFlowConditionGraphSummary> {
    let index = FlowQueryIndex::new(flow);
    let condition = index.find_condition(condition_offset)?;
    let condition_node = SalsaFlowNodeRefSummary::Condition(condition_offset);
    Some(SalsaFlowConditionGraphSummary {
        condition_node_offset: condition.node_offset,
        syntax_offset: condition.syntax_offset,
        kind: condition.kind.clone(),
        enter_targets: unique_nodes(collect_successor_nodes_by_kind(
            query,
            &condition_node,
            SalsaFlowEdgeKindSummary::ConditionEnter,
        )),
        true_targets: collect_condition_terminal_targets(
            query,
            condition_offset,
            SalsaFlowEdgeKindSummary::ConditionTrue,
        ),
        false_targets: collect_condition_terminal_targets(
            query,
            condition_offset,
            SalsaFlowEdgeKindSummary::ConditionFalse,
        ),
    })
}

pub fn build_branch_graph_summary(
    flow: &SalsaFlowSummary,
    query: &SalsaFlowQuerySummary,
    branch_offset: TextSize,
) -> Option<SalsaFlowBranchGraphSummary> {
    let index = FlowQueryIndex::new(flow);
    let branch = index.find_branch(branch_offset)?;
    let merge_node = SalsaFlowNodeRefSummary::Merge(branch_offset);
    let next_targets =
        collect_successor_nodes_by_kind(query, &merge_node, SalsaFlowEdgeKindSummary::MergeToNext);
    let has_merge = query
        .edges
        .iter()
        .any(|edge| edge.kind == SalsaFlowEdgeKindSummary::ClauseToMerge && edge.to == merge_node);
    Some(SalsaFlowBranchGraphSummary {
        branch_offset,
        condition_node_offset: branch
            .clauses
            .first()
            .and_then(|clause| clause.condition_node_offset),
        clause_block_offsets: branch
            .clauses
            .iter()
            .filter_map(|clause| clause.block_offset)
            .collect(),
        merge_node: has_merge.then_some(merge_node),
        next_targets: unique_nodes(next_targets),
    })
}

pub fn build_loop_graph_summary(
    flow: &SalsaFlowSummary,
    query: &SalsaFlowQuerySummary,
    loop_offset: TextSize,
) -> Option<SalsaFlowLoopGraphSummary> {
    let index = FlowQueryIndex::new(flow);
    let loop_summary = index.find_loop(loop_offset)?;
    let loop_node = SalsaFlowNodeRefSummary::Loop(loop_offset);
    let merge_node = SalsaFlowNodeRefSummary::Merge(loop_offset);
    let next_targets =
        collect_successor_nodes_by_kind(query, &merge_node, SalsaFlowEdgeKindSummary::MergeToNext);
    let has_merge =
        query.edges.iter().any(|edge| {
            edge.kind == SalsaFlowEdgeKindSummary::LoopToMerge && edge.to == merge_node
        }) || loop_summary
            .condition_node_offset
            .is_some_and(|condition_offset| {
                query.edges.iter().any(|edge| {
                    edge.kind == SalsaFlowEdgeKindSummary::ConditionFalse
                        && edge.from == SalsaFlowNodeRefSummary::Condition(condition_offset)
                        && edge.to == merge_node
                })
            });

    let mut continue_targets = unique_nodes(collect_predecessor_nodes(query, &loop_node));
    if let Some(condition_offset) = loop_summary.condition_node_offset {
        continue_targets.extend(unique_nodes(collect_predecessor_nodes(
            query,
            &SalsaFlowNodeRefSummary::Condition(condition_offset),
        )));
    }

    Some(SalsaFlowLoopGraphSummary {
        loop_offset,
        condition_node_offset: loop_summary.condition_node_offset,
        body_block_offset: collect_successor_nodes_by_kind(
            query,
            &loop_node,
            SalsaFlowEdgeKindSummary::LoopToBody,
        )
        .into_iter()
        .find_map(|node| match node {
            SalsaFlowNodeRefSummary::Block(offset) => Some(offset),
            _ => None,
        }),
        continue_targets: unique_nodes(continue_targets),
        merge_node: has_merge.then_some(merge_node),
        next_targets: unique_nodes(next_targets),
    })
}

pub fn build_terminal_graph_summary(
    query: &SalsaFlowQuerySummary,
    terminal_node: SalsaFlowNodeRefSummary,
) -> SalsaFlowTerminalGraphSummary {
    let target_nodes = unique_nodes(collect_successor_nodes_by_kind(
        query,
        &terminal_node,
        SalsaFlowEdgeKindSummary::TerminalToTarget,
    ));
    let unreachable_node = collect_successor_nodes_by_kind(
        query,
        &terminal_node,
        SalsaFlowEdgeKindSummary::TerminalToUnreachable,
    )
    .into_iter()
    .next();
    SalsaFlowTerminalGraphSummary {
        terminal_node,
        target_nodes,
        unreachable_node,
    }
}

fn collect_successor_nodes_by_kind(
    query: &SalsaFlowQuerySummary,
    node: &SalsaFlowNodeRefSummary,
    kind: SalsaFlowEdgeKindSummary,
) -> Vec<SalsaFlowNodeRefSummary> {
    query
        .edges
        .iter()
        .filter(|edge| edge.kind == kind && &edge.from == node)
        .map(|edge| edge.to.clone())
        .collect()
}

fn unique_nodes(nodes: Vec<SalsaFlowNodeRefSummary>) -> Vec<SalsaFlowNodeRefSummary> {
    let mut unique = Vec::new();
    for node in nodes {
        if !unique.contains(&node) {
            unique.push(node);
        }
    }
    unique
}

fn collect_condition_terminal_targets(
    query: &SalsaFlowQuerySummary,
    condition_offset: u32,
    edge_kind: SalsaFlowEdgeKindSummary,
) -> Vec<SalsaFlowNodeRefSummary> {
    let condition_node = SalsaFlowNodeRefSummary::Condition(condition_offset);
    let direct_targets = collect_successor_nodes_by_kind(query, &condition_node, edge_kind.clone());
    if !direct_targets.is_empty() {
        return unique_nodes(direct_targets);
    }

    let mut aggregated = Vec::new();
    for enter_target in collect_successor_nodes_by_kind(
        query,
        &condition_node,
        SalsaFlowEdgeKindSummary::ConditionEnter,
    ) {
        if let SalsaFlowNodeRefSummary::Condition(child_offset) = enter_target {
            aggregated.extend(collect_condition_terminal_targets(
                query,
                child_offset,
                edge_kind.clone(),
            ));
        }
    }

    unique_nodes(aggregated)
}

fn append_condition_edges(
    index: &FlowQueryIndex,
    condition_offset: u32,
    true_target: SalsaFlowNodeRefSummary,
    false_target: SalsaFlowNodeRefSummary,
    edges: &mut Vec<SalsaFlowEdgeSummary>,
) {
    let Some(condition) = index.find_condition(condition_offset) else {
        return;
    };

    match condition.kind {
        SalsaFlowConditionKindSummary::Expr => {
            edges.push(SalsaFlowEdgeSummary {
                kind: SalsaFlowEdgeKindSummary::ConditionTrue,
                from: SalsaFlowNodeRefSummary::Condition(condition_offset),
                to: true_target,
            });
            edges.push(SalsaFlowEdgeSummary {
                kind: SalsaFlowEdgeKindSummary::ConditionFalse,
                from: SalsaFlowNodeRefSummary::Condition(condition_offset),
                to: false_target,
            });
        }
        SalsaFlowConditionKindSummary::And => {
            let Some(left_offset) = condition.left_condition_offset else {
                return;
            };
            let Some(right_offset) = condition.right_condition_offset else {
                return;
            };
            edges.push(SalsaFlowEdgeSummary {
                kind: SalsaFlowEdgeKindSummary::ConditionEnter,
                from: SalsaFlowNodeRefSummary::Condition(condition_offset),
                to: SalsaFlowNodeRefSummary::Condition(left_offset),
            });
            append_condition_edges(
                index,
                left_offset,
                SalsaFlowNodeRefSummary::Condition(right_offset),
                false_target.clone(),
                edges,
            );
            append_condition_edges(index, right_offset, true_target, false_target, edges);
        }
        SalsaFlowConditionKindSummary::Or => {
            let Some(left_offset) = condition.left_condition_offset else {
                return;
            };
            let Some(right_offset) = condition.right_condition_offset else {
                return;
            };
            edges.push(SalsaFlowEdgeSummary {
                kind: SalsaFlowEdgeKindSummary::ConditionEnter,
                from: SalsaFlowNodeRefSummary::Condition(condition_offset),
                to: SalsaFlowNodeRefSummary::Condition(left_offset),
            });
            append_condition_edges(
                index,
                left_offset,
                true_target.clone(),
                SalsaFlowNodeRefSummary::Condition(right_offset),
                edges,
            );
            append_condition_edges(index, right_offset, true_target, false_target, edges);
        }
    }
}

fn append_branch_condition_edges(
    index: &FlowQueryIndex,
    branch: &SalsaFlowBranchSummary,
    merge_node: SalsaFlowNodeRefSummary,
    edges: &mut Vec<SalsaFlowEdgeSummary>,
) {
    for (entry_index, clause) in branch.clauses.iter().enumerate() {
        let Some(condition_offset) = clause.condition_node_offset else {
            continue;
        };
        let true_target = clause
            .block_offset
            .map(SalsaFlowNodeRefSummary::Block)
            .unwrap_or_else(|| merge_node.clone());
        let false_target = branch
            .clauses
            .get(entry_index + 1)
            .map(|next_clause| {
                if let Some(next_condition_offset) = next_clause.condition_node_offset {
                    SalsaFlowNodeRefSummary::Condition(next_condition_offset)
                } else if let Some(next_block_offset) = next_clause.block_offset {
                    SalsaFlowNodeRefSummary::Block(next_block_offset)
                } else {
                    merge_node.clone()
                }
            })
            .unwrap_or_else(|| merge_node.clone());
        append_condition_edges(index, condition_offset, true_target, false_target, edges);
    }
}

fn statement_falls_through(statement: &SalsaFlowStatementSummary) -> bool {
    matches!(
        statement.kind,
        SalsaFlowStatementKindSummary::Linear | SalsaFlowStatementKindSummary::Do
    )
}

fn find_statement_successor_target(
    index: &FlowQueryIndex,
    statement: &SalsaFlowStatementSummary,
) -> Option<SalsaFlowNodeRefSummary> {
    let block = index.find_block(statement.block_offset)?;
    let statement_index = block
        .statement_offsets
        .iter()
        .position(|offset| *offset == statement.syntax_offset)?;
    if let Some(next_statement_offset) = block.statement_offsets.get(statement_index + 1) {
        return Some(SalsaFlowNodeRefSummary::Statement(*next_statement_offset));
    }

    find_block_completion_target(index, block.syntax_offset)
}

fn find_block_completion_target(
    index: &FlowQueryIndex,
    block_offset: TextSize,
) -> Option<SalsaFlowNodeRefSummary> {
    let block = index.find_block(block_offset)?;
    match block.owner_kind {
        SalsaFlowBlockOwnerKindSummary::Chunk
        | SalsaFlowBlockOwnerKindSummary::FuncStat
        | SalsaFlowBlockOwnerKindSummary::LocalFuncStat
        | SalsaFlowBlockOwnerKindSummary::ClosureExpr => None,
        SalsaFlowBlockOwnerKindSummary::WhileStat
        | SalsaFlowBlockOwnerKindSummary::RepeatStat
        | SalsaFlowBlockOwnerKindSummary::ForStat
        | SalsaFlowBlockOwnerKindSummary::ForRangeStat => index
            .find_loop(block.owner_offset)
            .and_then(|loop_summary| {
                loop_summary
                    .condition_node_offset
                    .map(SalsaFlowNodeRefSummary::Condition)
            })
            .or(Some(SalsaFlowNodeRefSummary::Loop(block.owner_offset))),
        SalsaFlowBlockOwnerKindSummary::IfStat | SalsaFlowBlockOwnerKindSummary::DoStat => {
            let owner_statement = index.find_statement(block.owner_offset)?;
            find_statement_successor_target(index, owner_statement)
        }
        SalsaFlowBlockOwnerKindSummary::ElseIfClause
        | SalsaFlowBlockOwnerKindSummary::ElseClause => {
            let branch_offset = index.branch_offset_of_clause(block.owner_offset)?;
            let owner_statement = index.find_statement(branch_offset)?;
            find_statement_successor_target(index, owner_statement)
        }
        SalsaFlowBlockOwnerKindSummary::Other => {
            block.parent_block_offset.and_then(|parent_block_offset| {
                find_block_completion_target(index, parent_block_offset)
            })
        }
    }
}

struct FlowQueryIndex<'a> {
    flow: &'a SalsaFlowSummary,
    blocks: HashMap<TextSize, &'a SalsaFlowBlockSummary>,
    statements: HashMap<TextSize, &'a SalsaFlowStatementSummary>,
    conditions: HashMap<u32, &'a SalsaFlowConditionSummary>,
    branches: HashMap<TextSize, &'a SalsaFlowBranchSummary>,
    loops: HashMap<TextSize, &'a SalsaFlowLoopSummary>,
    branch_by_clause_offset: HashMap<TextSize, TextSize>,
    flow_region_root_by_block: HashMap<TextSize, TextSize>,
    enclosing_loop_by_block: HashMap<TextSize, TextSize>,
    label_by_root_and_name: HashMap<(TextSize, SmolStr), &'a SalsaFlowLabelSummary>,
}

impl<'a> FlowQueryIndex<'a> {
    fn new(flow: &'a SalsaFlowSummary) -> Self {
        let blocks = flow
            .blocks
            .iter()
            .map(|block| (block.syntax_offset, block))
            .collect::<HashMap<_, _>>();
        let statements = flow
            .statements
            .iter()
            .map(|statement| (statement.syntax_offset, statement))
            .collect::<HashMap<_, _>>();
        let conditions = flow
            .conditions
            .iter()
            .map(|condition| (condition.node_offset, condition))
            .collect::<HashMap<_, _>>();
        let branches = flow
            .branches
            .iter()
            .map(|branch| (branch.syntax_offset, branch))
            .collect::<HashMap<_, _>>();
        let loops = flow
            .loops
            .iter()
            .map(|loop_summary| (loop_summary.syntax_offset, loop_summary))
            .collect::<HashMap<_, _>>();

        let branch_by_clause_offset = flow
            .branches
            .iter()
            .flat_map(|branch| {
                branch
                    .clauses
                    .iter()
                    .map(move |clause| (clause.syntax_offset, branch.syntax_offset))
            })
            .collect::<HashMap<_, _>>();

        let mut flow_region_root_by_block = HashMap::new();
        let mut enclosing_loop_by_block = HashMap::new();
        for block in flow.blocks.iter().map(|block| block.syntax_offset) {
            let _ = compute_flow_region_root_offset(block, &blocks, &mut flow_region_root_by_block);
            let _ = compute_enclosing_loop_offset(block, &blocks, &mut enclosing_loop_by_block);
        }

        let mut label_by_root_and_name = HashMap::new();
        for label in &flow.labels {
            let Some(block_offset) = label.block_offset else {
                continue;
            };
            let Some(root_offset) = flow_region_root_by_block.get(&block_offset).copied() else {
                continue;
            };
            label_by_root_and_name.insert((root_offset, label.name.clone()), label);
        }

        Self {
            flow,
            blocks,
            statements,
            conditions,
            branches,
            loops,
            branch_by_clause_offset,
            flow_region_root_by_block,
            enclosing_loop_by_block,
            label_by_root_and_name,
        }
    }

    fn find_block(&self, block_offset: TextSize) -> Option<&'a SalsaFlowBlockSummary> {
        self.blocks.get(&block_offset).copied()
    }

    fn find_statement(&self, statement_offset: TextSize) -> Option<&'a SalsaFlowStatementSummary> {
        self.statements.get(&statement_offset).copied()
    }

    fn find_condition(&self, condition_offset: u32) -> Option<&'a SalsaFlowConditionSummary> {
        self.conditions.get(&condition_offset).copied()
    }

    fn find_branch(&self, branch_offset: TextSize) -> Option<&'a SalsaFlowBranchSummary> {
        self.branches.get(&branch_offset).copied()
    }

    fn find_loop(&self, loop_offset: TextSize) -> Option<&'a SalsaFlowLoopSummary> {
        self.loops.get(&loop_offset).copied()
    }

    fn branch_offset_of_clause(&self, clause_offset: TextSize) -> Option<TextSize> {
        self.branch_by_clause_offset.get(&clause_offset).copied()
    }

    fn flow_region_root_offset_of(&self, block_offset: TextSize) -> Option<TextSize> {
        self.flow_region_root_by_block.get(&block_offset).copied()
    }

    fn enclosing_loop_offset_of(&self, block_offset: TextSize) -> Option<TextSize> {
        self.enclosing_loop_by_block.get(&block_offset).copied()
    }

    fn resolve_goto_target_label(
        &self,
        goto_summary: &SalsaFlowGotoSummary,
        source_block_offset: TextSize,
    ) -> Option<&'a SalsaFlowLabelSummary> {
        let source_root = self.flow_region_root_offset_of(source_block_offset)?;
        self.label_by_root_and_name
            .get(&(source_root, goto_summary.label_name.clone()))
            .copied()
    }

    fn find_owned_block(
        &self,
        owner_kind: SalsaFlowBlockOwnerKindSummary,
        owner_offset: TextSize,
    ) -> Option<&'a SalsaFlowBlockSummary> {
        self.flow
            .blocks
            .iter()
            .find(|block| block.owner_kind == owner_kind && block.owner_offset == owner_offset)
    }

    fn is_flow_region_root(&self, block: &SalsaFlowBlockSummary) -> bool {
        block.parent_block_offset.is_none()
            || matches!(
                block.owner_kind,
                SalsaFlowBlockOwnerKindSummary::FuncStat
                    | SalsaFlowBlockOwnerKindSummary::LocalFuncStat
                    | SalsaFlowBlockOwnerKindSummary::ClosureExpr
            )
    }
}

fn compute_flow_region_root_offset(
    block_offset: TextSize,
    blocks: &HashMap<TextSize, &SalsaFlowBlockSummary>,
    cache: &mut HashMap<TextSize, TextSize>,
) -> Option<TextSize> {
    if let Some(root_offset) = cache.get(&block_offset).copied() {
        return Some(root_offset);
    }

    let block = blocks.get(&block_offset).copied()?;
    let root_offset = if block.parent_block_offset.is_none()
        || matches!(
            block.owner_kind,
            SalsaFlowBlockOwnerKindSummary::FuncStat
                | SalsaFlowBlockOwnerKindSummary::LocalFuncStat
                | SalsaFlowBlockOwnerKindSummary::ClosureExpr
        ) {
        block_offset
    } else {
        compute_flow_region_root_offset(block.parent_block_offset?, blocks, cache)?
    };

    cache.insert(block_offset, root_offset);
    Some(root_offset)
}

fn compute_enclosing_loop_offset(
    block_offset: TextSize,
    blocks: &HashMap<TextSize, &SalsaFlowBlockSummary>,
    cache: &mut HashMap<TextSize, TextSize>,
) -> Option<TextSize> {
    if let Some(loop_offset) = cache.get(&block_offset).copied() {
        return Some(loop_offset);
    }

    let block = blocks.get(&block_offset).copied()?;
    let loop_offset = match block.owner_kind {
        SalsaFlowBlockOwnerKindSummary::WhileStat
        | SalsaFlowBlockOwnerKindSummary::RepeatStat
        | SalsaFlowBlockOwnerKindSummary::ForStat
        | SalsaFlowBlockOwnerKindSummary::ForRangeStat => Some(block.owner_offset),
        _ => block.parent_block_offset.and_then(|parent_block_offset| {
            compute_enclosing_loop_offset(parent_block_offset, blocks, cache)
        }),
    }?;

    cache.insert(block_offset, loop_offset);
    Some(loop_offset)
}

pub fn build_for_range_iter_query_index(
    flow: &SalsaFlowSummary,
    decl_tree: &SalsaDeclTreeSummary,
    decl_index: &SalsaDeclTypeQueryIndex,
    member_index: &SalsaMemberTypeQueryIndex,
    assignments: &SalsaLocalAssignmentQueryIndex,
    property_index: &SalsaPropertyIndexSummary,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    signature_explain_index: &SalsaSignatureExplainIndex,
    signature_return_index: &SalsaSignatureReturnQueryIndex,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    use_sites: &SalsaUseSiteIndexSummary,
    chunk: &LuaChunk,
) -> SalsaForRangeIterQueryIndex {
    SalsaForRangeIterQueryIndex {
        loops: flow
            .loops
            .iter()
            .filter(|loop_summary| matches!(loop_summary.kind, SalsaFlowLoopKindSummary::ForRange))
            .map(|loop_summary| {
                build_for_range_iter_query(
                    loop_summary,
                    decl_tree,
                    decl_index,
                    member_index,
                    assignments,
                    property_index,
                    doc,
                    doc_types,
                    lowered_types,
                    signature_explain_index,
                    signature_return_index,
                    doc_tag_query_index,
                    use_sites,
                    chunk,
                )
            })
            .collect(),
    }
}

pub fn find_for_range_iter_query_at(
    index: &SalsaForRangeIterQueryIndex,
    loop_offset: TextSize,
) -> Option<SalsaForRangeIterQuerySummary> {
    index
        .loops
        .iter()
        .find(|summary| summary.loop_offset == loop_offset)
        .cloned()
}

fn build_for_range_iter_query(
    loop_summary: &SalsaFlowLoopSummary,
    decl_tree: &SalsaDeclTreeSummary,
    decl_index: &SalsaDeclTypeQueryIndex,
    member_index: &SalsaMemberTypeQueryIndex,
    assignments: &SalsaLocalAssignmentQueryIndex,
    property_index: &SalsaPropertyIndexSummary,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    signature_explain_index: &SalsaSignatureExplainIndex,
    signature_return_index: &SalsaSignatureReturnQueryIndex,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    use_sites: &SalsaUseSiteIndexSummary,
    chunk: &LuaChunk,
) -> SalsaForRangeIterQuerySummary {
    let iter_var_decls = collect_for_range_iter_decls(decl_tree, loop_summary.syntax_offset);
    let source = loop_summary
        .iter_expr_offsets
        .first()
        .copied()
        .and_then(|expr_offset| {
            build_for_range_iter_source(
                expr_offset,
                decl_tree,
                decl_index,
                member_index,
                assignments,
                property_index,
                doc,
                doc_types,
                lowered_types,
                signature_explain_index,
                doc_tag_query_index,
                use_sites,
                chunk,
            )
        });

    let mut recursive_dependency = false;
    let slot_offsets = if let Some(source) = &source {
        let (slot_offsets, has_recursive_dependency) = collect_for_range_slot_offsets(
            source,
            iter_var_decls.len(),
            doc_types,
            lowered_types,
            signature_return_index,
        );
        recursive_dependency = has_recursive_dependency;
        slot_offsets
    } else {
        vec![Vec::new(); iter_var_decls.len()]
    };

    let iter_vars = iter_var_decls
        .into_iter()
        .enumerate()
        .map(
            |(slot_index, (decl_id, name))| SalsaForRangeIterVarSummary {
                name,
                decl_id,
                slot_index,
                type_offsets: slot_offsets.get(slot_index).cloned().unwrap_or_default(),
            },
        )
        .collect::<Vec<_>>();

    let state = if recursive_dependency {
        SalsaForRangeIterResolveStateSummary::RecursiveDependency
    } else if iter_vars
        .iter()
        .all(|iter_var| !iter_var.type_offsets.is_empty())
    {
        SalsaForRangeIterResolveStateSummary::Resolved
    } else {
        SalsaForRangeIterResolveStateSummary::Partial
    };

    SalsaForRangeIterQuerySummary {
        loop_offset: loop_summary.syntax_offset,
        iter_expr_offsets: loop_summary.iter_expr_offsets.clone(),
        state,
        source,
        iter_vars,
    }
}

fn collect_for_range_iter_decls(
    decl_tree: &SalsaDeclTreeSummary,
    loop_offset: TextSize,
) -> Vec<(SalsaDeclId, SmolStr)> {
    let Some(scope) = decl_tree.scopes.iter().find(|scope| {
        matches!(scope.kind, SalsaScopeKindSummary::ForRange) && scope.start_offset == loop_offset
    }) else {
        return Vec::new();
    };

    let mut decls = decl_tree
        .decls
        .iter()
        .filter(|decl| {
            decl.scope_id == scope.id
                && matches!(
                    decl.kind,
                    SalsaDeclKindSummary::Local {
                        attrib: Some(SalsaLocalAttributeSummary::IterConst)
                    }
                )
        })
        .map(|decl| (decl.start_offset, decl.id, decl.name.clone()))
        .collect::<Vec<_>>();
    decls.sort_by_key(|(start_offset, _, _)| *start_offset);
    decls
        .into_iter()
        .map(|(_, decl_id, name)| (decl_id, name))
        .collect()
}

fn build_for_range_iter_source(
    expr_offset: TextSize,
    decl_tree: &SalsaDeclTreeSummary,
    decl_index: &SalsaDeclTypeQueryIndex,
    member_index: &SalsaMemberTypeQueryIndex,
    assignments: &SalsaLocalAssignmentQueryIndex,
    property_index: &SalsaPropertyIndexSummary,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    signature_explain_index: &SalsaSignatureExplainIndex,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    use_sites: &SalsaUseSiteIndexSummary,
    chunk: &LuaChunk,
) -> Option<SalsaForRangeIterSourceSummary> {
    let expr = find_expr_at_offset(chunk, expr_offset)?;
    match expr {
        LuaExpr::NameExpr(_) => Some(SalsaForRangeIterSourceSummary {
            expr_offset,
            kind: SalsaForRangeIterSourceKindSummary::Name,
            name_type: build_flow_name_type(
                expr_offset,
                decl_tree,
                decl_index,
                assignments,
                property_index,
                doc_types,
                lowered_types,
                signature_explain_index,
                doc_tag_query_index,
                use_sites,
                chunk,
            ),
            member_type: None,
            call: None,
        }),
        LuaExpr::IndexExpr(_) => Some(SalsaForRangeIterSourceSummary {
            expr_offset,
            kind: SalsaForRangeIterSourceKindSummary::Member,
            name_type: None,
            member_type: build_flow_member_type(
                expr_offset,
                decl_index,
                member_index,
                assignments,
                property_index,
                doc,
                doc_types,
                lowered_types,
                signature_explain_index,
                doc_tag_query_index,
                decl_tree,
                use_sites,
                chunk,
            ),
            call: None,
        }),
        LuaExpr::CallExpr(_) => Some(SalsaForRangeIterSourceSummary {
            expr_offset,
            kind: SalsaForRangeIterSourceKindSummary::Call,
            name_type: None,
            member_type: None,
            call: find_call_explain_at(signature_explain_index, expr_offset),
        }),
        _ => Some(SalsaForRangeIterSourceSummary {
            expr_offset,
            kind: SalsaForRangeIterSourceKindSummary::Other,
            name_type: None,
            member_type: None,
            call: None,
        }),
    }
}

fn build_flow_name_type(
    expr_offset: TextSize,
    decl_tree: &SalsaDeclTreeSummary,
    decl_index: &SalsaDeclTypeQueryIndex,
    assignments: &SalsaLocalAssignmentQueryIndex,
    property_index: &SalsaPropertyIndexSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    signature_explain_index: &SalsaSignatureExplainIndex,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    use_sites: &SalsaUseSiteIndexSummary,
    chunk: &LuaChunk,
) -> Option<SalsaProgramPointTypeInfoSummary> {
    let name_use = find_name_use_at(use_sites, expr_offset)?;
    let active_narrows = match name_use.resolution {
        SalsaNameUseResolutionSummary::LocalDecl(decl_id) => {
            collect_active_type_narrows(decl_tree, chunk.clone(), decl_id, expr_offset)
        }
        SalsaNameUseResolutionSummary::Global => Vec::new(),
    };

    Some(find_name_type_at_program_point(
        decl_index,
        &name_use,
        expr_offset,
        assignments,
        property_index,
        doc_types,
        signature_explain_index,
        doc_tag_query_index,
        decl_tree,
        chunk,
        lowered_types,
        &active_narrows,
    ))
}

fn build_flow_member_type(
    expr_offset: TextSize,
    decl_index: &SalsaDeclTypeQueryIndex,
    member_index: &SalsaMemberTypeQueryIndex,
    assignments: &SalsaLocalAssignmentQueryIndex,
    property_index: &SalsaPropertyIndexSummary,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    signature_explain_index: &SalsaSignatureExplainIndex,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    decl_tree: &SalsaDeclTreeSummary,
    use_sites: &SalsaUseSiteIndexSummary,
    chunk: &LuaChunk,
) -> Option<SalsaProgramPointMemberTypeInfoSummary> {
    let member_use = find_member_use_at(use_sites, expr_offset)?;
    Some(find_member_type_at_program_point(
        member_index,
        property_index,
        decl_index,
        doc,
        doc_types,
        &member_use,
        expr_offset,
        assignments,
        decl_tree,
        chunk,
        lowered_types,
        signature_explain_index,
        doc_tag_query_index,
    ))
}

fn collect_for_range_slot_offsets(
    source: &SalsaForRangeIterSourceSummary,
    slot_count: usize,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    signature_return_index: &SalsaSignatureReturnQueryIndex,
) -> (Vec<Vec<SalsaDocTypeNodeKey>>, bool) {
    let mut recursive_dependency = false;
    let mut all_slots = vec![Vec::new(); slot_count];

    #[allow(clippy::needless_range_loop)]
    for slot_index in 0..slot_count {
        let type_offsets = match source.kind {
            SalsaForRangeIterSourceKindSummary::Name => source
                .name_type
                .as_ref()
                .map(|name_type| {
                    collect_iterator_slot_offsets_from_candidates(
                        &name_type.candidates,
                        slot_index,
                        doc_types,
                        lowered_types,
                        signature_return_index,
                        &mut recursive_dependency,
                    )
                })
                .unwrap_or_default(),
            SalsaForRangeIterSourceKindSummary::Member => source
                .member_type
                .as_ref()
                .map(|member_type| {
                    collect_iterator_slot_offsets_from_candidates(
                        &member_type.candidates,
                        slot_index,
                        doc_types,
                        lowered_types,
                        signature_return_index,
                        &mut recursive_dependency,
                    )
                })
                .unwrap_or_default(),
            SalsaForRangeIterSourceKindSummary::Call => source
                .call
                .as_ref()
                .map(|call| {
                    collect_iterator_slot_offsets_from_call(
                        call,
                        slot_index,
                        doc_types,
                        lowered_types,
                        signature_return_index,
                        &mut recursive_dependency,
                    )
                })
                .unwrap_or_default(),
            SalsaForRangeIterSourceKindSummary::Other => Vec::new(),
        };
        all_slots[slot_index] = type_offsets;
    }

    (all_slots, recursive_dependency)
}

fn collect_iterator_slot_offsets_from_candidates(
    candidates: &[SalsaTypeCandidateSummary],
    slot_index: usize,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    signature_return_index: &SalsaSignatureReturnQueryIndex,
    recursive_dependency: &mut bool,
) -> Vec<SalsaDocTypeNodeKey> {
    let mut offsets = BTreeSet::new();
    for candidate in candidates {
        if let Some(signature_offset) = candidate.signature_offset
            && let Some(signature_return) =
                find_signature_return_query_at(signature_return_index, signature_offset)
        {
            if matches!(
                signature_return.state,
                SalsaSignatureReturnResolveStateSummary::RecursiveDependency
            ) {
                *recursive_dependency = true;
            }

            for type_offset in collect_iterator_slot_offsets_from_signature_return(
                &signature_return,
                slot_index,
                doc_types,
                lowered_types,
                signature_return_index,
                recursive_dependency,
            ) {
                offsets.insert(type_offset);
            }
        }

        for type_offset in &candidate.explicit_type_offsets {
            for return_type in collect_iterator_slot_offsets_from_type_offset(
                *type_offset,
                slot_index,
                doc_types,
                lowered_types,
                &mut BTreeSet::new(),
            ) {
                offsets.insert(return_type);
            }
        }
    }

    offsets.into_iter().collect()
}

fn collect_iterator_slot_offsets_from_call(
    call: &SalsaCallExplainSummary,
    slot_index: usize,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    signature_return_index: &SalsaSignatureReturnQueryIndex,
    recursive_dependency: &mut bool,
) -> Vec<SalsaDocTypeNodeKey> {
    collect_iterator_slot_offsets_from_call_with_visited(
        call,
        slot_index,
        doc_types,
        lowered_types,
        signature_return_index,
        recursive_dependency,
        &mut BTreeSet::new(),
    )
}

fn collect_iterator_slot_offsets_from_call_with_visited(
    call: &SalsaCallExplainSummary,
    slot_index: usize,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    signature_return_index: &SalsaSignatureReturnQueryIndex,
    recursive_dependency: &mut bool,
    visited_signature_offsets: &mut BTreeSet<TextSize>,
) -> Vec<SalsaDocTypeNodeKey> {
    let mut offsets = BTreeSet::new();
    for signature_offset in call
        .resolved_signature_offset
        .into_iter()
        .chain(call.candidate_signature_offsets.iter().copied())
    {
        if !visited_signature_offsets.insert(signature_offset) {
            continue;
        }

        let Some(signature_return) =
            find_signature_return_query_at(signature_return_index, signature_offset)
        else {
            continue;
        };

        if matches!(
            signature_return.state,
            SalsaSignatureReturnResolveStateSummary::RecursiveDependency
        ) {
            *recursive_dependency = true;
        }

        for return_row in &signature_return.doc_returns {
            let Some(iterator_item) = return_row.items.first() else {
                continue;
            };

            for type_offset in collect_iterator_slot_offsets_from_type_ref(
                iterator_item.doc_type.type_ref.clone(),
                slot_index,
                doc_types,
                lowered_types,
                &mut BTreeSet::new(),
            ) {
                offsets.insert(type_offset);
            }
        }

        if let Some(iterator_value) = signature_return.values.first() {
            for type_offset in collect_iterator_slot_offsets_from_iterator_value(
                iterator_value,
                slot_index,
                doc_types,
                lowered_types,
                signature_return_index,
                recursive_dependency,
                visited_signature_offsets,
            ) {
                offsets.insert(type_offset);
            }
        }

        visited_signature_offsets.remove(&signature_offset);
    }

    for return_row in &call.returns {
        let Some(iterator_type) = return_row.items.first() else {
            continue;
        };

        for type_offset in collect_iterator_slot_offsets_from_type_ref(
            iterator_type.doc_type.type_ref.clone(),
            slot_index,
            doc_types,
            lowered_types,
            &mut BTreeSet::new(),
        ) {
            offsets.insert(type_offset);
        }
    }

    offsets.into_iter().collect()
}

fn collect_iterator_slot_offsets_from_iterator_value(
    value: &SalsaSignatureReturnValueSummary,
    slot_index: usize,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    signature_return_index: &SalsaSignatureReturnQueryIndex,
    recursive_dependency: &mut bool,
    visited_signature_offsets: &mut BTreeSet<TextSize>,
) -> Vec<SalsaDocTypeNodeKey> {
    let mut offsets = BTreeSet::new();

    for type_offset in &value.doc_return_type_offsets {
        for return_type in collect_iterator_slot_offsets_from_type_offset(
            *type_offset,
            slot_index,
            doc_types,
            lowered_types,
            &mut BTreeSet::new(),
        ) {
            offsets.insert(return_type);
        }
    }

    if let Some(name_type) = &value.name_type {
        for type_offset in collect_iterator_slot_offsets_from_candidates(
            &name_type.candidates,
            slot_index,
            doc_types,
            lowered_types,
            signature_return_index,
            recursive_dependency,
        ) {
            offsets.insert(type_offset);
        }
    }

    if let Some(member_type) = &value.member_type {
        for type_offset in collect_iterator_slot_offsets_from_candidates(
            &member_type.candidates,
            slot_index,
            doc_types,
            lowered_types,
            signature_return_index,
            recursive_dependency,
        ) {
            offsets.insert(type_offset);
        }
    }

    if let Some(call) = &value.call {
        for type_offset in collect_iterator_slot_offsets_from_call_with_visited(
            call,
            slot_index,
            doc_types,
            lowered_types,
            signature_return_index,
            recursive_dependency,
            visited_signature_offsets,
        ) {
            offsets.insert(type_offset);
        }
    }

    offsets.into_iter().collect()
}

fn collect_iterator_slot_offsets_from_signature_return(
    signature_return: &SalsaSignatureReturnQuerySummary,
    slot_index: usize,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    signature_return_index: &SalsaSignatureReturnQueryIndex,
    recursive_dependency: &mut bool,
) -> Vec<SalsaDocTypeNodeKey> {
    let mut offsets = BTreeSet::new();

    for return_row in &signature_return.doc_returns {
        if let Some(item) = return_row.items.get(slot_index)
            && let SalsaDocTypeRef::Node(type_offset) = item.doc_type.type_ref
        {
            offsets.insert(type_offset);
        }
    }

    if offsets.is_empty()
        && let Some(value) = signature_return.values.get(slot_index)
    {
        for type_offset in signature_return_value_candidate_type_offsets(value, slot_index) {
            offsets.insert(type_offset);
        }
    }

    if offsets.is_empty()
        && let Some(value) = signature_return.values.first()
    {
        for type_offset in collect_iterator_slot_offsets_from_iterator_value(
            value,
            slot_index,
            doc_types,
            lowered_types,
            signature_return_index,
            recursive_dependency,
            &mut BTreeSet::new(),
        ) {
            offsets.insert(type_offset);
        }
    }

    offsets.into_iter().collect()
}

fn collect_iterator_slot_offsets_from_type_ref(
    type_ref: SalsaDocTypeRef,
    slot_index: usize,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited: &mut BTreeSet<SalsaDocTypeNodeKey>,
) -> Vec<SalsaDocTypeNodeKey> {
    match type_ref {
        SalsaDocTypeRef::Node(type_offset) => collect_iterator_slot_offsets_from_type_offset(
            type_offset,
            slot_index,
            doc_types,
            lowered_types,
            visited,
        ),
        SalsaDocTypeRef::Incomplete => Vec::new(),
    }
}

fn collect_iterator_slot_offsets_from_type_offset(
    type_offset: SalsaDocTypeNodeKey,
    slot_index: usize,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited: &mut BTreeSet<SalsaDocTypeNodeKey>,
) -> Vec<SalsaDocTypeNodeKey> {
    if !visited.insert(type_offset) {
        return Vec::new();
    }

    if let Some(doc_type) = doc_types
        .types
        .iter()
        .find(|doc_type| doc_type.node_key() == type_offset)
    {
        let result = match &doc_type.kind {
            SalsaDocTypeKindSummary::Function { returns, .. } => returns
                .get(slot_index)
                .and_then(|ret| ret.type_offset)
                .into_iter()
                .collect(),
            SalsaDocTypeKindSummary::Nullable { inner_type_offset }
            | SalsaDocTypeKindSummary::Array {
                item_type_offset: inner_type_offset,
            }
            | SalsaDocTypeKindSummary::Variadic {
                item_type_offset: inner_type_offset,
            }
            | SalsaDocTypeKindSummary::Unary {
                inner_type_offset, ..
            } => inner_type_offset
                .iter()
                .copied()
                .flat_map(|inner| {
                    collect_iterator_slot_offsets_from_type_offset(
                        inner,
                        slot_index,
                        doc_types,
                        lowered_types,
                        visited,
                    )
                })
                .collect(),
            SalsaDocTypeKindSummary::Generic {
                base_type_offset, ..
            } => base_type_offset
                .iter()
                .copied()
                .flat_map(|base_type| {
                    collect_iterator_slot_offsets_from_type_offset(
                        base_type,
                        slot_index,
                        doc_types,
                        lowered_types,
                        visited,
                    )
                })
                .collect(),
            SalsaDocTypeKindSummary::Binary {
                left_type_offset,
                right_type_offset,
                ..
            } => left_type_offset
                .iter()
                .chain(right_type_offset.iter())
                .copied()
                .flat_map(|inner| {
                    collect_iterator_slot_offsets_from_type_offset(
                        inner,
                        slot_index,
                        doc_types,
                        lowered_types,
                        visited,
                    )
                })
                .collect(),
            SalsaDocTypeKindSummary::Conditional {
                true_type_offset,
                false_type_offset,
                ..
            } => true_type_offset
                .iter()
                .chain(false_type_offset.iter())
                .copied()
                .flat_map(|inner| {
                    collect_iterator_slot_offsets_from_type_offset(
                        inner,
                        slot_index,
                        doc_types,
                        lowered_types,
                        visited,
                    )
                })
                .collect(),
            SalsaDocTypeKindSummary::Tuple { item_type_offsets }
            | SalsaDocTypeKindSummary::MultiLineUnion { item_type_offsets } => item_type_offsets
                .iter()
                .copied()
                .flat_map(|inner| {
                    collect_iterator_slot_offsets_from_type_offset(
                        inner,
                        slot_index,
                        doc_types,
                        lowered_types,
                        visited,
                    )
                })
                .collect(),
            _ => Vec::new(),
        };
        if !result.is_empty() {
            return result;
        }
    }

    find_resolved_doc_type_by_key_from_parts(doc_types, lowered_types, type_offset)
        .map(|resolved| match resolved.lowered.kind {
            SalsaDocTypeLoweredKind::Function { returns, .. } => returns
                .get(slot_index)
                .and_then(|ret| match ret.doc_type {
                    SalsaDocTypeRef::Node(type_offset) => Some(type_offset),
                    SalsaDocTypeRef::Incomplete => None,
                })
                .into_iter()
                .collect(),
            SalsaDocTypeLoweredKind::Nullable { inner_type }
            | SalsaDocTypeLoweredKind::Array {
                item_type: inner_type,
            }
            | SalsaDocTypeLoweredKind::Variadic {
                item_type: inner_type,
            }
            | SalsaDocTypeLoweredKind::Unary { inner_type, .. } => {
                collect_iterator_slot_offsets_from_type_ref(
                    inner_type,
                    slot_index,
                    doc_types,
                    lowered_types,
                    visited,
                )
            }
            SalsaDocTypeLoweredKind::Generic { base_type, .. } => {
                collect_iterator_slot_offsets_from_type_ref(
                    base_type,
                    slot_index,
                    doc_types,
                    lowered_types,
                    visited,
                )
            }
            SalsaDocTypeLoweredKind::Union { item_types }
            | SalsaDocTypeLoweredKind::Intersection { item_types }
            | SalsaDocTypeLoweredKind::Tuple { item_types }
            | SalsaDocTypeLoweredKind::MultiLineUnion { item_types } => item_types
                .into_iter()
                .flat_map(|inner_type| {
                    collect_iterator_slot_offsets_from_type_ref(
                        inner_type,
                        slot_index,
                        doc_types,
                        lowered_types,
                        visited,
                    )
                })
                .collect(),
            SalsaDocTypeLoweredKind::Conditional {
                true_type,
                false_type,
                ..
            } => collect_iterator_slot_offsets_from_type_ref(
                true_type,
                slot_index,
                doc_types,
                lowered_types,
                visited,
            )
            .into_iter()
            .chain(collect_iterator_slot_offsets_from_type_ref(
                false_type,
                slot_index,
                doc_types,
                lowered_types,
                visited,
            ))
            .collect(),
            _ => Vec::new(),
        })
        .unwrap_or_default()
}

fn find_expr_at_offset(chunk: &LuaChunk, expr_offset: TextSize) -> Option<LuaExpr> {
    chunk
        .descendants::<LuaExpr>()
        .find(|expr| TextSize::from(u32::from(expr.get_position())) == expr_offset)
}
