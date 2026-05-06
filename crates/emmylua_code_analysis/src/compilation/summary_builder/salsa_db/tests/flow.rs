use super::*;

#[test]
fn test_summary_builder_flow_structures() {
    let mut compilation = setup_compilation();
    let source = r#"local function run(flag, items)
  do
    local a = 1
  end

  if flag then
    return 1
  elseif #items > 0 then
    goto continue
  else
    while flag do
      break
    end
  end

  repeat
    flag = false
  until flag

  for i = 1, 3 do
    local x = i
  end

  for key, value in pairs(items) do
    print(key, value)
  end

  ::continue::
  return items
end"#;
    set_test_file(&mut compilation, 11, "C:/ws/flow.lua", source);

    let file = compilation.file();
    let flow_api = compilation.flow();

    let flow = flow_api.summary(FileId::new(11)).expect("flow summary");
    let flow_query = flow_api.query(FileId::new(11)).expect("flow query summary");
    let file_summary = file.summary(FileId::new(11)).expect("file summary");

    assert_eq!(flow.as_ref(), &file_summary.flow);
    assert_eq!(flow.branch_count, 1);
    assert_eq!(flow.loop_count, 4);
    assert_eq!(flow.return_count, 2);
    assert_eq!(flow.break_count, 1);
    assert_eq!(flow.goto_count, 1);
    assert_eq!(flow.label_count, 1);
    assert!(flow.block_count >= 8);
    assert!(flow.statements.len() >= 10);

    assert!(flow.blocks.iter().any(|block| matches!(
        block,
        crate::SalsaFlowBlockSummary {
            owner_kind: crate::SalsaFlowBlockOwnerKindSummary::LocalFuncStat,
            child_block_offsets,
            statement_offsets,
            ..
        } if !child_block_offsets.is_empty() && !statement_offsets.is_empty()
    )));
    assert!(flow.blocks.iter().any(|block| matches!(
        block,
        crate::SalsaFlowBlockSummary {
            owner_kind: crate::SalsaFlowBlockOwnerKindSummary::WhileStat,
            parent_block_offset: Some(_),
            ..
        }
    )));
    assert!(flow.blocks.iter().any(|block| matches!(
        block,
        crate::SalsaFlowBlockSummary {
            owner_kind: crate::SalsaFlowBlockOwnerKindSummary::ElseClause,
            ..
        }
    )));

    assert!(flow.branches.iter().any(|branch| {
        branch.clauses.len() == 3
            && matches!(
                branch.clauses[0].kind,
                crate::SalsaFlowBranchClauseKindSummary::If
            )
            && matches!(
                branch.clauses[1].kind,
                crate::SalsaFlowBranchClauseKindSummary::ElseIf
            )
            && matches!(
                branch.clauses[2].kind,
                crate::SalsaFlowBranchClauseKindSummary::Else
            )
    }));
    assert!(flow.loops.iter().any(|loop_summary| matches!(
        loop_summary,
        crate::SalsaFlowLoopSummary {
            kind: crate::SalsaFlowLoopKindSummary::While,
            condition_expr_offset: Some(_),
            block_offset: Some(_),
            ..
        }
    )));
    assert!(flow.loops.iter().any(|loop_summary| matches!(
        loop_summary,
        crate::SalsaFlowLoopSummary {
            kind: crate::SalsaFlowLoopKindSummary::Repeat,
            condition_expr_offset: Some(_),
            ..
        }
    )));
    assert!(flow.loops.iter().any(|loop_summary| matches!(
        loop_summary,
        crate::SalsaFlowLoopSummary {
            kind: crate::SalsaFlowLoopKindSummary::For,
            iter_expr_offsets,
            ..
        } if iter_expr_offsets.len() == 2
    )));
    assert!(flow.loops.iter().any(|loop_summary| matches!(
        loop_summary,
        crate::SalsaFlowLoopSummary {
            kind: crate::SalsaFlowLoopKindSummary::ForRange,
            iter_expr_offsets,
            ..
        } if iter_expr_offsets.len() == 1
    )));

    assert!(
        flow.returns
            .iter()
            .any(|return_summary| return_summary.expr_offsets.len() == 1)
    );
    assert!(
        flow.breaks
            .iter()
            .any(|break_summary| break_summary.block_offset.is_some())
    );
    assert!(
        flow.gotos
            .iter()
            .any(|goto_summary| goto_summary.label_name == "continue")
    );
    assert!(
        flow.labels
            .iter()
            .any(|label_summary| label_summary.name == "continue")
    );

    let func_block_offset = flow
        .blocks
        .iter()
        .find(|block| {
            matches!(
                block.owner_kind,
                crate::SalsaFlowBlockOwnerKindSummary::LocalFuncStat
            )
        })
        .map(|block| block.syntax_offset)
        .expect("function block offset");
    let func_first_statement_offset = flow
        .blocks
        .iter()
        .find(|block| block.syntax_offset == func_block_offset)
        .and_then(|block| block.statement_offsets.first().copied())
        .expect("function first statement offset");
    let while_loop_offset = flow
        .loops
        .iter()
        .find(|loop_summary| matches!(loop_summary.kind, crate::SalsaFlowLoopKindSummary::While))
        .map(|loop_summary| loop_summary.syntax_offset)
        .expect("while loop offset");
    let while_loop_link = flow_query
        .loop_links
        .iter()
        .find(|link| link.loop_offset == while_loop_offset)
        .expect("while loop link");
    let while_body_offset = while_loop_link.body_block_offset.expect("while body block");
    let while_continue_offset = while_loop_link
        .continue_block_offset
        .expect("while continue block");
    let while_condition_offset = flow
        .loops
        .iter()
        .find(|loop_summary| loop_summary.syntax_offset == while_loop_offset)
        .and_then(|loop_summary| loop_summary.condition_node_offset)
        .expect("while condition offset");
    let break_offset = flow.breaks[0].syntax_offset;
    let goto_offset = flow.gotos[0].syntax_offset;
    let label_offset = flow.labels[0].syntax_offset;
    let branch_offset = flow.branches[0].syntax_offset;
    let branch_link = flow_query
        .branch_links
        .iter()
        .find(|link| link.branch_offset == branch_offset)
        .expect("branch link");
    let first_clause_offset = *branch_link
        .clause_block_offsets
        .first()
        .expect("first clause block");
    let branch_statement_offset = flow
        .statements
        .iter()
        .find(|statement| {
            statement.syntax_offset == branch_offset
                && matches!(statement.kind, crate::SalsaFlowStatementKindSummary::Branch)
        })
        .map(|statement| statement.syntax_offset)
        .expect("branch statement offset");
    let branch_condition_offset = flow
        .branches
        .iter()
        .find(|branch| branch.syntax_offset == branch_offset)
        .and_then(|branch| branch.clauses.first())
        .and_then(|clause| clause.condition_node_offset)
        .expect("branch condition offset");
    let while_statement_offset = flow
        .statements
        .iter()
        .find(|statement| {
            statement.syntax_offset == while_loop_offset
                && matches!(statement.kind, crate::SalsaFlowStatementKindSummary::Loop)
        })
        .map(|statement| statement.syntax_offset)
        .expect("while statement offset");

    assert!(flow_query.root_block_offsets.contains(&TextSize::from(0)));
    assert!(flow_query.root_block_offsets.contains(&func_block_offset));
    assert!(matches!(
        flow_api.block_at(FileId::new(11), func_block_offset),
        Some(crate::SalsaFlowBlockSummary {
            owner_kind: crate::SalsaFlowBlockOwnerKindSummary::LocalFuncStat,
            ..
        })
    ));
    assert!(matches!(
        flow_api.branch_at(FileId::new(11), branch_offset),
        Some(crate::SalsaFlowBranchSummary { clauses, .. }) if clauses.len() == 3
    ));
    assert!(matches!(
        flow_api.loop_at(FileId::new(11), while_loop_offset),
        Some(crate::SalsaFlowLoopSummary {
            kind: crate::SalsaFlowLoopKindSummary::While,
            ..
        })
    ));
    assert!(flow_api.break_at(FileId::new(11), break_offset).is_some());
    assert!(flow_api.goto_at(FileId::new(11), goto_offset).is_some());
    assert!(flow_api.label(FileId::new(11), label_offset).is_some());

    assert!(flow_query.branch_links.iter().any(|link| {
        link.branch_offset == branch_offset
            && link.entry_block_offset == Some(func_block_offset)
            && link.clause_block_offsets.len() == 3
            && link.exit_block_offset == Some(func_block_offset)
    }));
    assert!(flow_query.loop_links.iter().any(|link| {
        link.loop_offset == while_loop_offset
            && link.entry_block_offset.is_some()
            && link.continue_block_offset.is_some()
            && link.exit_block_offset.is_some()
    }));
    assert!(flow_query.break_links.iter().any(|link| {
        link.break_offset == break_offset
            && link.enclosing_loop_offset == Some(while_loop_offset)
            && link.exit_block_offset.is_some()
    }));
    assert!(flow_query.goto_links.iter().any(|link| {
        link.goto_offset == goto_offset && link.target_label_offset == Some(label_offset)
    }));
    assert!(
        flow_query
            .return_links
            .iter()
            .all(|link| link.exit_root_block_offset.is_some())
    );
    assert!(flow_query.terminal_edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaFlowTerminalEdgeSummary {
            kind: crate::SalsaFlowTerminalEdgeKindSummary::Break,
            syntax_offset,
            ..
        } if *syntax_offset == break_offset
    )));
    assert!(flow_query.terminal_edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaFlowTerminalEdgeSummary {
            kind: crate::SalsaFlowTerminalEdgeKindSummary::Goto,
            syntax_offset,
            target_label_offset: Some(target),
            ..
        } if *syntax_offset == goto_offset && *target == label_offset
    )));
    assert!(flow_query.terminal_edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaFlowTerminalEdgeSummary {
            kind: crate::SalsaFlowTerminalEdgeKindSummary::Return,
            flow_region_root_offset: Some(_),
            ..
        }
    )));
    assert!(flow_query.edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaFlowEdgeSummary {
            kind: crate::SalsaFlowEdgeKindSummary::BlockToStatement,
            from: crate::SalsaFlowNodeRefSummary::Block(from),
            to: crate::SalsaFlowNodeRefSummary::Statement(to),
        } if *from == func_block_offset && *to == func_first_statement_offset
    )));
    assert!(flow_query.edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaFlowEdgeSummary {
            kind: crate::SalsaFlowEdgeKindSummary::StatementToBranch,
            from: crate::SalsaFlowNodeRefSummary::Statement(from),
            to: crate::SalsaFlowNodeRefSummary::Branch(to),
        } if *from == branch_statement_offset && *to == branch_offset
    )));
    assert!(flow_query.edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaFlowEdgeSummary {
            kind: crate::SalsaFlowEdgeKindSummary::StatementToLoop,
            from: crate::SalsaFlowNodeRefSummary::Statement(from),
            to: crate::SalsaFlowNodeRefSummary::Loop(to),
        } if *from == while_statement_offset && *to == while_loop_offset
    )));
    assert!(flow_query.edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaFlowEdgeSummary {
            kind: crate::SalsaFlowEdgeKindSummary::LoopToBody,
            from: crate::SalsaFlowNodeRefSummary::Loop(from),
            to: crate::SalsaFlowNodeRefSummary::Block(to),
        } if *from == while_loop_offset && *to == while_body_offset
    )));
    assert!(flow_query.edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaFlowEdgeSummary {
            kind: crate::SalsaFlowEdgeKindSummary::LoopContinue,
            from: crate::SalsaFlowNodeRefSummary::Block(from),
            to: crate::SalsaFlowNodeRefSummary::Condition(to),
        } if *from == while_continue_offset && *to == while_condition_offset
    )));

    let branch_successors = flow_api
        .successors(
            FileId::new(11),
            crate::SalsaFlowNodeRefSummary::Branch(branch_offset),
        )
        .expect("branch successors");
    assert!(
        branch_successors.contains(&crate::SalsaFlowNodeRefSummary::Block(first_clause_offset))
    );

    let branch_statement_successors = flow_api
        .successors(
            FileId::new(11),
            crate::SalsaFlowNodeRefSummary::Statement(branch_statement_offset),
        )
        .expect("branch statement successors");
    assert!(
        branch_statement_successors
            .contains(&crate::SalsaFlowNodeRefSummary::Branch(branch_offset))
    );

    let branch_statement_edges = flow_api
        .outgoing_edges(
            FileId::new(11),
            crate::SalsaFlowNodeRefSummary::Statement(branch_statement_offset),
        )
        .expect("branch statement outgoing edges");
    assert!(branch_statement_edges.iter().any(|edge| {
        matches!(
            edge,
            crate::SalsaFlowEdgeSummary {
                kind: crate::SalsaFlowEdgeKindSummary::StatementToBranch,
                to: crate::SalsaFlowNodeRefSummary::Branch(offset),
                ..
            } if *offset == branch_offset
        )
    }));

    let branch_graph = flow_api
        .branch_graph(FileId::new(11), branch_offset)
        .expect("branch graph");
    assert_eq!(
        branch_graph.condition_node_offset,
        Some(branch_condition_offset)
    );
    assert_eq!(branch_graph.clause_block_offsets.len(), 3);
    assert!(branch_graph.merge_node.is_some());
    assert!(!branch_graph.next_targets.is_empty());

    let while_successors = flow_api
        .successors(
            FileId::new(11),
            crate::SalsaFlowNodeRefSummary::Loop(while_loop_offset),
        )
        .expect("while successors");
    assert!(while_successors.contains(&crate::SalsaFlowNodeRefSummary::Block(while_body_offset)));

    let while_predecessors = flow_api
        .predecessors(
            FileId::new(11),
            crate::SalsaFlowNodeRefSummary::Condition(while_condition_offset),
        )
        .expect("while predecessors");
    assert!(
        while_predecessors.contains(&crate::SalsaFlowNodeRefSummary::Block(
            while_continue_offset
        ))
    );

    let loop_graph = flow_api
        .loop_graph(FileId::new(11), while_loop_offset)
        .expect("loop graph");
    assert_eq!(
        loop_graph.condition_node_offset,
        Some(while_condition_offset)
    );
    assert_eq!(loop_graph.body_block_offset, Some(while_body_offset));
    assert!(
        loop_graph
            .continue_targets
            .contains(&crate::SalsaFlowNodeRefSummary::Block(
                while_continue_offset
            ))
    );
    assert!(loop_graph.merge_node.is_some());
    assert!(!loop_graph.next_targets.is_empty());

    let break_graph = flow_api
        .break_graph(FileId::new(11), break_offset)
        .expect("break graph");
    assert!(break_graph.unreachable_node.is_some());
    assert!(!break_graph.target_nodes.is_empty());

    let goto_graph = flow_api
        .goto_graph(FileId::new(11), goto_offset)
        .expect("goto graph");
    assert!(goto_graph.unreachable_node.is_some());

    let label_successors = flow_api
        .successors(
            FileId::new(11),
            crate::SalsaFlowNodeRefSummary::Label(label_offset),
        )
        .expect("label successors");
    assert!(!label_successors.is_empty());

    let label_incoming_edges = flow_api
        .incoming_edges(
            FileId::new(11),
            crate::SalsaFlowNodeRefSummary::Label(label_offset),
        )
        .expect("label incoming edges");
    assert!(label_incoming_edges.iter().any(|edge| {
        matches!(
            edge,
            crate::SalsaFlowEdgeSummary {
                kind: crate::SalsaFlowEdgeKindSummary::GotoToLabel,
                from: crate::SalsaFlowNodeRefSummary::Goto(offset),
                ..
            } if *offset == goto_offset
        )
    }));

    let goto_reachable = flow_api
        .reachable_nodes(
            FileId::new(11),
            crate::SalsaFlowNodeRefSummary::Goto(goto_offset),
        )
        .expect("goto reachable");
    assert!(goto_reachable.contains(&crate::SalsaFlowNodeRefSummary::Label(label_offset)));

    let goto_can_reach_label = flow_api
        .can_reach(
            FileId::new(11),
            crate::SalsaFlowNodeRefSummary::Goto(goto_offset),
            crate::SalsaFlowNodeRefSummary::Label(label_offset),
        )
        .expect("goto can reach label");
    assert!(goto_can_reach_label);
}

#[test]
fn test_summary_builder_flow_condition_graph_structures() {
    let mut compilation = setup_compilation();
    let source = r#"local function run(a, b, c)
  if a and b or c then
    return 1
  end
  return 2
end"#;
    set_test_file(&mut compilation, 12, "C:/ws/flow_condition.lua", source);

    let flow_api = compilation.flow();
    let flow = flow_api.summary(FileId::new(12)).expect("flow summary");
    let flow_query = flow_api.query(FileId::new(12)).expect("flow query summary");

    assert!(
        flow.conditions
            .iter()
            .any(|condition| matches!(condition.kind, crate::SalsaFlowConditionKindSummary::And))
    );
    assert!(
        flow.conditions
            .iter()
            .any(|condition| matches!(condition.kind, crate::SalsaFlowConditionKindSummary::Or))
    );

    let branch_offset = flow.branches[0].syntax_offset;
    let branch_statement_offset = flow
        .statements
        .iter()
        .find(|statement| statement.syntax_offset == branch_offset)
        .map(|statement| statement.syntax_offset)
        .expect("branch statement");
    let root_condition_offset = flow.branches[0]
        .clauses
        .first()
        .and_then(|clause| clause.condition_node_offset)
        .expect("root condition");
    let root_condition = flow
        .conditions
        .iter()
        .find(|condition| condition.node_offset == root_condition_offset)
        .expect("root condition summary");
    assert!(
        root_condition.kind == crate::SalsaFlowConditionKindSummary::Expr
            || root_condition.left_condition_offset.is_some()
            || root_condition.right_condition_offset.is_some()
    );

    assert!(flow_query.edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaFlowEdgeSummary {
            kind: crate::SalsaFlowEdgeKindSummary::StatementToCondition,
            from: crate::SalsaFlowNodeRefSummary::Statement(from),
            to: crate::SalsaFlowNodeRefSummary::Condition(to),
        } if *from == branch_statement_offset && *to == root_condition_offset
    )));
    assert!(flow_query.edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaFlowEdgeSummary {
            kind: crate::SalsaFlowEdgeKindSummary::ConditionEnter,
            from: crate::SalsaFlowNodeRefSummary::Condition(_),
            to: crate::SalsaFlowNodeRefSummary::Condition(_),
        }
    )));
    assert!(flow_query.edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaFlowEdgeSummary {
            kind: crate::SalsaFlowEdgeKindSummary::ConditionFalse,
            from: crate::SalsaFlowNodeRefSummary::Condition(_),
            to: crate::SalsaFlowNodeRefSummary::Condition(_),
        }
    )));
    assert!(flow_query.edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaFlowEdgeSummary {
            kind: crate::SalsaFlowEdgeKindSummary::MergeToNext,
            from: crate::SalsaFlowNodeRefSummary::Merge(from),
            to: crate::SalsaFlowNodeRefSummary::Statement(_),
        } if *from == branch_offset
    )));
    assert!(flow_query.edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaFlowEdgeSummary {
            kind: crate::SalsaFlowEdgeKindSummary::TerminalToUnreachable,
            from: crate::SalsaFlowNodeRefSummary::Return(_),
            to: crate::SalsaFlowNodeRefSummary::Unreachable(_),
        }
    )));

    let condition_graph = flow_api
        .condition_graph(FileId::new(12), root_condition_offset)
        .expect("condition graph");
    assert_eq!(condition_graph.condition_node_offset, root_condition_offset);
    assert!(!condition_graph.true_targets.is_empty());
    assert!(!condition_graph.false_targets.is_empty());
    if !matches!(
        condition_graph.kind,
        crate::SalsaFlowConditionKindSummary::Expr
    ) {
        assert!(!condition_graph.enter_targets.is_empty());
    }

    let branch_graph = flow_api
        .branch_graph(FileId::new(12), branch_offset)
        .expect("branch graph");
    assert_eq!(
        branch_graph.condition_node_offset,
        Some(root_condition_offset)
    );
    assert_eq!(branch_graph.clause_block_offsets.len(), 1);
    assert!(branch_graph.merge_node.is_some());

    let first_return_offset = flow
        .returns
        .iter()
        .map(|return_summary| return_summary.syntax_offset)
        .min()
        .expect("first return offset");

    let return_graph = flow_api
        .return_graph(FileId::new(12), first_return_offset)
        .expect("return graph");
    assert!(return_graph.unreachable_node.is_some());

    let condition_reachable = flow_api
        .reachable_nodes(
            FileId::new(12),
            crate::SalsaFlowNodeRefSummary::Condition(root_condition_offset),
        )
        .expect("condition reachable nodes");
    assert!(
        condition_reachable
            .iter()
            .any(|node| matches!(node, crate::SalsaFlowNodeRefSummary::Block(_)))
    );
    assert!(
        condition_reachable
            .iter()
            .any(|node| matches!(node, crate::SalsaFlowNodeRefSummary::Unreachable(_)))
    );

    let condition_can_reach_unreachable = flow_api
        .can_reach(
            FileId::new(12),
            crate::SalsaFlowNodeRefSummary::Condition(root_condition_offset),
            return_graph
                .unreachable_node
                .clone()
                .expect("return unreachable node"),
        )
        .expect("condition can reach return unreachable");
    assert!(condition_can_reach_unreachable);
}

#[test]
fn test_summary_builder_flow_goto_respects_closure_scope() {
    let mut compilation = setup_compilation();
    let source = r#"local function outer(flag)
  ::shared::
  local function inner()
    if flag then
      goto shared
    end
    ::shared::
    return 1
  end

  return inner()
end"#;
    set_test_file(&mut compilation, 13, "C:/ws/flow_closure_goto.lua", source);

    let flow_api = compilation.flow();
    let flow = flow_api.summary(FileId::new(13)).expect("flow summary");
    let flow_query = flow_api.query(FileId::new(13)).expect("flow query summary");

    let goto_offset = flow.gotos[0].syntax_offset;
    let inner_label_offset = flow
        .labels
        .iter()
        .filter(|label| label.name == "shared")
        .max_by_key(|label| label.syntax_offset)
        .map(|label| label.syntax_offset)
        .expect("inner label offset");
    let outer_label_offset = flow
        .labels
        .iter()
        .filter(|label| label.name == "shared")
        .min_by_key(|label| label.syntax_offset)
        .map(|label| label.syntax_offset)
        .expect("outer label offset");

    let goto_link = flow_query
        .goto_links
        .iter()
        .find(|link| link.goto_offset == goto_offset)
        .expect("goto link");
    assert_eq!(goto_link.target_label_offset, Some(inner_label_offset));
    assert_ne!(goto_link.target_label_offset, Some(outer_label_offset));

    let goto_successors = flow_api
        .successors(
            FileId::new(13),
            crate::SalsaFlowNodeRefSummary::Goto(goto_offset),
        )
        .expect("goto successors");
    assert!(goto_successors.contains(&crate::SalsaFlowNodeRefSummary::Label(inner_label_offset)));
    assert!(!goto_successors.contains(&crate::SalsaFlowNodeRefSummary::Label(outer_label_offset)));
}

#[test]
fn test_summary_builder_flow_break_respects_nearest_loop() {
    let mut compilation = setup_compilation();
    let source = r#"local function run(flag)
  while flag do
    while flag do
      break
    end
    flag = false
  end
  return 1
end"#;
    set_test_file(&mut compilation, 14, "C:/ws/flow_nested_break.lua", source);

    let flow_api = compilation.flow();
    let flow = flow_api.summary(FileId::new(14)).expect("flow summary");
    let flow_query = flow_api.query(FileId::new(14)).expect("flow query summary");

    let break_offset = flow.breaks[0].syntax_offset;
    let mut loop_offsets = flow
        .loops
        .iter()
        .map(|loop_summary| loop_summary.syntax_offset)
        .collect::<Vec<_>>();
    loop_offsets.sort();
    let outer_loop_offset = loop_offsets[0];
    let inner_loop_offset = loop_offsets[1];

    let break_link = flow_query
        .break_links
        .iter()
        .find(|link| link.break_offset == break_offset)
        .expect("break link");
    assert_eq!(break_link.enclosing_loop_offset, Some(inner_loop_offset));
    assert_ne!(break_link.enclosing_loop_offset, Some(outer_loop_offset));

    let break_graph = flow_api
        .break_graph(FileId::new(14), break_offset)
        .expect("break graph");
    assert!(break_graph.unreachable_node.is_some());
    assert!(!break_graph.target_nodes.is_empty());

    let outer_loop_graph = flow_api
        .loop_graph(FileId::new(14), outer_loop_offset)
        .expect("outer loop graph");
    let inner_loop_graph = flow_api
        .loop_graph(FileId::new(14), inner_loop_offset)
        .expect("inner loop graph");
    assert_ne!(
        outer_loop_graph.body_block_offset,
        inner_loop_graph.body_block_offset
    );
}
