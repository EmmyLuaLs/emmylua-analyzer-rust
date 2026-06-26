use std::collections::HashMap;

use emmylua_parser::{
    BinaryOperator, LuaAst, LuaAstNode, LuaBlock, LuaChunk, LuaExpr, LuaFuncStat, LuaIfStat,
    LuaLocalFuncStat, LuaStat,
};
use rowan::TextSize;

use super::super::{
    SalsaFlowBlockOwnerKindSummary, SalsaFlowBlockSummary, SalsaFlowBranchClauseKindSummary,
    SalsaFlowBranchClauseSummary, SalsaFlowBranchSummary, SalsaFlowBreakSummary,
    SalsaFlowConditionKindSummary, SalsaFlowConditionSummary, SalsaFlowGotoSummary,
    SalsaFlowLabelSummary, SalsaFlowLoopKindSummary, SalsaFlowLoopSummary, SalsaFlowReturnSummary,
    SalsaFlowStatementKindSummary, SalsaFlowStatementSummary, SalsaFlowSummary,
};

pub fn analyze_flow_summary(chunk: LuaChunk) -> SalsaFlowSummary {
    let mut collector = FlowSummaryCollector::default();

    for node in chunk.descendants::<LuaAst>() {
        match node {
            LuaAst::LuaBlock(block) => {
                let (block_summary, statements) = build_block_summary(&block);
                collector.blocks.push(block_summary);
                collector.statements.extend(statements);
            }
            LuaAst::LuaIfStat(if_stat) => {
                let branch_summary = build_branch_summary(&mut collector, if_stat);
                collector.branches.push(branch_summary);
            }
            LuaAst::LuaWhileStat(while_stat) => {
                let condition_expr_offset = while_stat
                    .get_condition_expr()
                    .map(|expr| expr.get_position());
                let condition_node_offset = while_stat
                    .get_condition_expr()
                    .map(|expr| collect_condition_summaries(&mut collector, expr));
                collector.loops.push(SalsaFlowLoopSummary {
                    syntax_offset: while_stat.get_position(),
                    kind: SalsaFlowLoopKindSummary::While,
                    condition_expr_offset,
                    condition_node_offset,
                    iter_expr_offsets: Vec::new(),
                    block_offset: while_stat.get_block().map(|block| block.get_position()),
                })
            }
            LuaAst::LuaRepeatStat(repeat_stat) => {
                let condition_expr_offset = repeat_stat
                    .get_condition_expr()
                    .map(|expr| expr.get_position());
                let condition_node_offset = repeat_stat
                    .get_condition_expr()
                    .map(|expr| collect_condition_summaries(&mut collector, expr));
                collector.loops.push(SalsaFlowLoopSummary {
                    syntax_offset: repeat_stat.get_position(),
                    kind: SalsaFlowLoopKindSummary::Repeat,
                    condition_expr_offset,
                    condition_node_offset,
                    iter_expr_offsets: Vec::new(),
                    block_offset: repeat_stat.get_block().map(|block| block.get_position()),
                })
            }
            LuaAst::LuaForStat(for_stat) => collector.loops.push(SalsaFlowLoopSummary {
                syntax_offset: for_stat.get_position(),
                kind: SalsaFlowLoopKindSummary::For,
                condition_expr_offset: None,
                condition_node_offset: None,
                iter_expr_offsets: for_stat
                    .get_iter_expr()
                    .map(|expr| expr.get_position())
                    .collect(),
                block_offset: for_stat.get_block().map(|block| block.get_position()),
            }),
            LuaAst::LuaForRangeStat(for_range_stat) => collector.loops.push(SalsaFlowLoopSummary {
                syntax_offset: for_range_stat.get_position(),
                kind: SalsaFlowLoopKindSummary::ForRange,
                condition_expr_offset: None,
                condition_node_offset: None,
                iter_expr_offsets: for_range_stat
                    .get_expr_list()
                    .map(|expr| expr.get_position())
                    .collect(),
                block_offset: for_range_stat.get_block().map(|block| block.get_position()),
            }),
            LuaAst::LuaReturnStat(return_stat) => collector.returns.push(SalsaFlowReturnSummary {
                syntax_offset: return_stat.get_position(),
                block_offset: parent_block_offset_of(&return_stat),
                expr_offsets: return_stat
                    .get_expr_list()
                    .map(|expr| expr.get_position())
                    .collect(),
            }),
            LuaAst::LuaBreakStat(break_stat) => collector.breaks.push(SalsaFlowBreakSummary {
                syntax_offset: break_stat.get_position(),
                block_offset: parent_block_offset_of(&break_stat),
            }),
            LuaAst::LuaGotoStat(goto_stat) => {
                if let Some(label_name) = goto_stat.get_label_name_token() {
                    collector.gotos.push(SalsaFlowGotoSummary {
                        syntax_offset: goto_stat.get_position(),
                        block_offset: parent_block_offset_of(&goto_stat),
                        label_name: label_name.get_name_text().into(),
                    });
                }
            }
            LuaAst::LuaLabelStat(label_stat) => {
                if let Some(label_name) = label_stat.get_label_name_token() {
                    collector.labels.push(SalsaFlowLabelSummary {
                        syntax_offset: label_stat.get_position(),
                        block_offset: parent_block_offset_of(&label_stat),
                        name: label_name.get_name_text().into(),
                    });
                }
            }
            _ => {}
        }
    }

    populate_block_children(&mut collector.blocks);
    collector.finish()
}

#[derive(Default)]
struct FlowSummaryCollector {
    blocks: Vec<SalsaFlowBlockSummary>,
    statements: Vec<SalsaFlowStatementSummary>,
    conditions: Vec<SalsaFlowConditionSummary>,
    condition_ids: HashMap<(TextSize, TextSize), u32>,
    next_condition_id: u32,
    branches: Vec<SalsaFlowBranchSummary>,
    loops: Vec<SalsaFlowLoopSummary>,
    returns: Vec<SalsaFlowReturnSummary>,
    breaks: Vec<SalsaFlowBreakSummary>,
    gotos: Vec<SalsaFlowGotoSummary>,
    labels: Vec<SalsaFlowLabelSummary>,
}

impl FlowSummaryCollector {
    fn finish(self) -> SalsaFlowSummary {
        SalsaFlowSummary {
            branch_count: self.branches.len(),
            loop_count: self.loops.len(),
            return_count: self.returns.len(),
            block_count: self.blocks.len(),
            break_count: self.breaks.len(),
            goto_count: self.gotos.len(),
            label_count: self.labels.len(),
            blocks: self.blocks,
            statements: self.statements,
            conditions: self.conditions,
            branches: self.branches,
            loops: self.loops,
            returns: self.returns,
            breaks: self.breaks,
            gotos: self.gotos,
            labels: self.labels,
        }
    }
}

fn build_block_summary(
    block: &LuaBlock,
) -> (SalsaFlowBlockSummary, Vec<SalsaFlowStatementSummary>) {
    let block_offset = block.get_position();
    let statements = block
        .get_stats()
        .map(|stat| build_statement_summary(block_offset, stat))
        .collect::<Vec<_>>();
    (
        SalsaFlowBlockSummary {
            syntax_offset: block.get_position(),
            owner_kind: block_owner_kind(block),
            owner_offset: block_owner_offset(block),
            parent_block_offset: parent_block_offset(block),
            child_block_offsets: Vec::new(),
            statement_offsets: statements
                .iter()
                .map(|statement| statement.syntax_offset)
                .collect(),
        },
        statements,
    )
}

fn build_statement_summary(block_offset: TextSize, stat: LuaStat) -> SalsaFlowStatementSummary {
    let syntax_offset = stat.get_position();
    let kind = match stat {
        LuaStat::DoStat(_) => SalsaFlowStatementKindSummary::Do,
        LuaStat::IfStat(_) => SalsaFlowStatementKindSummary::Branch,
        LuaStat::WhileStat(_)
        | LuaStat::RepeatStat(_)
        | LuaStat::ForStat(_)
        | LuaStat::ForRangeStat(_) => SalsaFlowStatementKindSummary::Loop,
        LuaStat::ReturnStat(_) => SalsaFlowStatementKindSummary::Return,
        LuaStat::BreakStat(_) => SalsaFlowStatementKindSummary::Break,
        LuaStat::GotoStat(_) => SalsaFlowStatementKindSummary::Goto,
        LuaStat::LabelStat(_) => SalsaFlowStatementKindSummary::Label,
        LuaStat::LocalStat(_)
        | LuaStat::AssignStat(_)
        | LuaStat::CallExprStat(_)
        | LuaStat::FuncStat(_)
        | LuaStat::LocalFuncStat(_)
        | LuaStat::EmptyStat(_)
        | LuaStat::GlobalStat(_) => SalsaFlowStatementKindSummary::Linear,
    };

    SalsaFlowStatementSummary {
        syntax_offset,
        block_offset,
        kind,
    }
}

fn build_branch_summary(
    collector: &mut FlowSummaryCollector,
    if_stat: LuaIfStat,
) -> SalsaFlowBranchSummary {
    let mut clauses = vec![SalsaFlowBranchClauseSummary {
        kind: SalsaFlowBranchClauseKindSummary::If,
        syntax_offset: if_stat.get_position(),
        condition_expr_offset: if_stat.get_condition_expr().map(|expr| expr.get_position()),
        condition_node_offset: if_stat
            .get_condition_expr()
            .map(|expr| collect_condition_summaries(collector, expr)),
        block_offset: if_stat.get_block().map(|block| block.get_position()),
    }];

    clauses.extend(if_stat.get_else_if_clause_list().map(|clause| {
        SalsaFlowBranchClauseSummary {
            kind: SalsaFlowBranchClauseKindSummary::ElseIf,
            syntax_offset: clause.get_position(),
            condition_expr_offset: clause.get_condition_expr().map(|expr| expr.get_position()),
            condition_node_offset: clause
                .get_condition_expr()
                .map(|expr| collect_condition_summaries(collector, expr)),
            block_offset: clause.get_block().map(|block| block.get_position()),
        }
    }));

    if let Some(clause) = if_stat.get_else_clause() {
        clauses.push(SalsaFlowBranchClauseSummary {
            kind: SalsaFlowBranchClauseKindSummary::Else,
            syntax_offset: clause.get_position(),
            condition_expr_offset: None,
            condition_node_offset: None,
            block_offset: clause.get_block().map(|block| block.get_position()),
        });
    }

    SalsaFlowBranchSummary {
        syntax_offset: if_stat.get_position(),
        clauses,
    }
}

fn collect_condition_summaries(collector: &mut FlowSummaryCollector, expr: LuaExpr) -> u32 {
    let syntax_offset = expr.get_position();
    let end_offset = expr.syntax().text_range().end();
    if let Some(node_offset) = collector.condition_ids.get(&(syntax_offset, end_offset)) {
        return *node_offset;
    }
    collector.next_condition_id += 1;
    let node_offset = collector.next_condition_id;
    collector
        .condition_ids
        .insert((syntax_offset, end_offset), node_offset);

    let (kind, left_condition_offset, right_condition_offset) = match expr {
        LuaExpr::BinaryExpr(binary_expr) => {
            let op = binary_expr.get_op_token().map(|token| token.get_op());
            if matches!(op, Some(BinaryOperator::OpAnd | BinaryOperator::OpOr)) {
                if let Some((left, right)) = binary_expr.get_exprs() {
                    let left_offset = collect_condition_summaries(collector, left);
                    let right_offset = collect_condition_summaries(collector, right);
                    (
                        match op {
                            Some(BinaryOperator::OpAnd) => SalsaFlowConditionKindSummary::And,
                            Some(BinaryOperator::OpOr) => SalsaFlowConditionKindSummary::Or,
                            _ => SalsaFlowConditionKindSummary::Expr,
                        },
                        Some(left_offset),
                        Some(right_offset),
                    )
                } else {
                    (SalsaFlowConditionKindSummary::Expr, None, None)
                }
            } else {
                (SalsaFlowConditionKindSummary::Expr, None, None)
            }
        }
        LuaExpr::ParenExpr(paren_expr) => {
            if let Some(inner_expr) = paren_expr.get_expr() {
                return collect_condition_summaries(collector, inner_expr);
            }
            (SalsaFlowConditionKindSummary::Expr, None, None)
        }
        _ => (SalsaFlowConditionKindSummary::Expr, None, None),
    };

    collector.conditions.push(SalsaFlowConditionSummary {
        node_offset,
        syntax_offset,
        kind,
        left_condition_offset,
        right_condition_offset,
    });
    node_offset
}

fn parent_block_offset_of<T: LuaAstNode>(node: &T) -> Option<TextSize> {
    node.get_parent::<LuaBlock>()
        .map(|block| block.get_position())
}

fn populate_block_children(blocks: &mut [SalsaFlowBlockSummary]) {
    let offset_to_index = blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.syntax_offset, index))
        .collect::<HashMap<_, _>>();

    let relations = blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| Some((index, block.parent_block_offset?)))
        .collect::<Vec<_>>();

    for (index, parent_offset) in relations {
        if let Some(parent_index) = offset_to_index.get(&parent_offset) {
            blocks[*parent_index]
                .child_block_offsets
                .push(blocks[index].syntax_offset);
        }
    }
}

fn block_owner_kind(block: &LuaBlock) -> SalsaFlowBlockOwnerKindSummary {
    let Some(ast_node) = block.get_parent::<LuaAst>() else {
        return SalsaFlowBlockOwnerKindSummary::Other;
    };

    match ast_node {
        LuaAst::LuaChunk(_) => SalsaFlowBlockOwnerKindSummary::Chunk,
        LuaAst::LuaClosureExpr(closure) => {
            let closure_parent = closure.get_parent::<LuaAst>();
            match closure_parent {
                Some(parent) if parent.is::<LuaLocalFuncStat>() => {
                    SalsaFlowBlockOwnerKindSummary::LocalFuncStat
                }
                Some(parent) if parent.is::<LuaFuncStat>() => {
                    SalsaFlowBlockOwnerKindSummary::FuncStat
                }
                _ => SalsaFlowBlockOwnerKindSummary::ClosureExpr,
            }
        }
        LuaAst::LuaDoStat(_) => SalsaFlowBlockOwnerKindSummary::DoStat,
        LuaAst::LuaElseIfClauseStat(_) => SalsaFlowBlockOwnerKindSummary::ElseIfClause,
        LuaAst::LuaElseClauseStat(_) => SalsaFlowBlockOwnerKindSummary::ElseClause,
        LuaAst::LuaIfStat(_) => SalsaFlowBlockOwnerKindSummary::IfStat,
        LuaAst::LuaWhileStat(_) => SalsaFlowBlockOwnerKindSummary::WhileStat,
        LuaAst::LuaRepeatStat(_) => SalsaFlowBlockOwnerKindSummary::RepeatStat,
        LuaAst::LuaForStat(_) => SalsaFlowBlockOwnerKindSummary::ForStat,
        LuaAst::LuaForRangeStat(_) => SalsaFlowBlockOwnerKindSummary::ForRangeStat,
        _ => SalsaFlowBlockOwnerKindSummary::Other,
    }
}

fn block_owner_offset(block: &LuaBlock) -> TextSize {
    let Some(ast_node) = block.get_parent::<LuaAst>() else {
        return block.get_position();
    };

    match ast_node {
        LuaAst::LuaChunk(chunk) => chunk.get_position(),
        LuaAst::LuaClosureExpr(closure) => {
            let closure_parent = closure.get_parent::<LuaAst>();
            if let Some(parent) = closure_parent {
                if parent.is::<LuaLocalFuncStat>() || parent.is::<LuaFuncStat>() {
                    parent.get_position()
                } else {
                    closure.get_position()
                }
            } else {
                closure.get_position()
            }
        }
        LuaAst::LuaDoStat(do_stat) => do_stat.get_position(),
        LuaAst::LuaElseIfClauseStat(clause) => clause.get_position(),
        LuaAst::LuaElseClauseStat(clause) => clause.get_position(),
        LuaAst::LuaIfStat(if_stat) => if_stat.get_position(),
        LuaAst::LuaWhileStat(while_stat) => while_stat.get_position(),
        LuaAst::LuaRepeatStat(repeat_stat) => repeat_stat.get_position(),
        LuaAst::LuaForStat(for_stat) => for_stat.get_position(),
        LuaAst::LuaForRangeStat(for_range_stat) => for_range_stat.get_position(),
        _ => block.get_position(),
    }
}

fn parent_block_offset(block: &LuaBlock) -> Option<TextSize> {
    let current_offset = block.get_position();
    block
        .ancestors::<LuaBlock>()
        .find(|ancestor| ancestor.get_position() != current_offset)
        .map(|parent| parent.get_position())
}
