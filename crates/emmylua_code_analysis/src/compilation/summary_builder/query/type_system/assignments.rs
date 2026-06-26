use std::collections::BTreeSet;

use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaAstToken, LuaBlock, LuaChunk, LuaExpr, LuaIfStat, LuaLocalStat,
    LuaStat,
};
use rowan::TextSize;

use super::shared::{find_visible_decl_before_offset, innermost_block_at_offset};
use crate::compilation::summary_builder::{
    SalsaLookupBucket, build_lookup_buckets, find_bucket_indices,
};
use crate::{SalsaDeclId, SalsaDeclKindSummary, SalsaDeclTreeSummary, SalsaSyntaxIdSummary};

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaLocalAssignmentSummary {
    pub decl_id: SalsaDeclId,
    pub syntax_offset: TextSize,
    pub is_local_declaration: bool,
    pub value_expr_syntax_id: Option<SalsaSyntaxIdSummary>,
    pub value_expr_offset: Option<TextSize>,
    pub value_result_index: usize,
    pub source_decl_id: Option<SalsaDeclId>,
    pub source_call_syntax_id: Option<SalsaSyntaxIdSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaLocalAssignmentQueryIndex {
    pub assignments: Vec<SalsaLocalAssignmentSummary>,
    pub(crate) by_decl_id: Vec<SalsaLookupBucket<SalsaDeclId>>,
}

pub fn build_local_assignment_query_index(
    decl_tree: &SalsaDeclTreeSummary,
    chunk: LuaChunk,
) -> SalsaLocalAssignmentQueryIndex {
    let assignments = chunk
        .descendants::<LuaStat>()
        .flat_map(|stat| match stat {
            LuaStat::AssignStat(assign_stat) => {
                build_local_assignments_for_assign_stat(decl_tree, assign_stat)
            }
            LuaStat::LocalStat(local_stat) => {
                build_local_assignments_for_local_stat(decl_tree, local_stat)
            }
            _ => Vec::new(),
        })
        .collect::<Vec<_>>();
    let by_decl_id = build_lookup_buckets(
        assignments
            .iter()
            .enumerate()
            .map(|(index, assignment)| (assignment.decl_id, index))
            .collect(),
    );

    SalsaLocalAssignmentQueryIndex {
        assignments,
        by_decl_id,
    }
}

pub fn find_latest_local_assignment(
    index: &SalsaLocalAssignmentQueryIndex,
    decl_id: SalsaDeclId,
    program_point_offset: TextSize,
) -> Option<SalsaLocalAssignmentSummary> {
    find_bucket_indices(&index.by_decl_id, &decl_id)
        .into_iter()
        .flatten()
        .filter_map(|entry_index| index.assignments.get(*entry_index))
        .filter(|assignment| assignment.syntax_offset < program_point_offset)
        .max_by_key(|assignment| assignment.syntax_offset)
        .cloned()
}

pub(crate) fn latest_assignments_at_program_point(
    decl_tree: &SalsaDeclTreeSummary,
    assignments: &SalsaLocalAssignmentQueryIndex,
    chunk: &LuaChunk,
    decl_id: SalsaDeclId,
    program_point_offset: TextSize,
) -> Option<Vec<SalsaLocalAssignmentSummary>> {
    let point_block = innermost_block_at_offset(chunk, program_point_offset)?;
    let merged_assignments = collect_assignments_in_block_until_point(
        decl_tree,
        &point_block,
        decl_id,
        program_point_offset,
        assignments,
        Vec::new(),
    );
    if merged_assignments.is_empty() {
        return find_latest_local_assignment(assignments, decl_id, program_point_offset)
            .map(|assignment| vec![assignment]);
    }

    Some(merged_assignments)
}

fn build_local_assignments_for_assign_stat(
    decl_tree: &SalsaDeclTreeSummary,
    assign_stat: LuaAssignStat,
) -> Vec<SalsaLocalAssignmentSummary> {
    let (vars, exprs) = assign_stat.get_var_and_expr_list();
    vars.into_iter()
        .enumerate()
        .filter_map(|(index, var)| {
            let name_expr = match var {
                emmylua_parser::LuaVarExpr::NameExpr(name_expr) => name_expr,
                _ => return None,
            };
            let name = name_expr.get_name_text()?;
            let decl = find_visible_decl_before_offset(decl_tree, &name, name_expr.get_position())?;
            if matches!(decl.kind, SalsaDeclKindSummary::Global) {
                return None;
            }

            let last_expr_index = exprs.len().saturating_sub(1);
            let (expr, value_result_index) = if let Some(expr) = exprs.get(index).cloned() {
                (Some(expr), 0)
            } else {
                (exprs.last().cloned(), index.saturating_sub(last_expr_index))
            };
            let value_expr_syntax_id = expr.as_ref().map(|expr| expr.get_syntax_id().into());
            let value_expr_offset = expr
                .as_ref()
                .map(|expr| TextSize::from(u32::from(expr.get_position())));
            let source_call_syntax_id = expr.as_ref().and_then(call_expr_syntax_id_of_expr);
            let source_decl_id = expr.and_then(|expr| source_decl_id_of_expr(decl_tree, expr));

            Some(SalsaLocalAssignmentSummary {
                decl_id: decl.id,
                syntax_offset: TextSize::from(u32::from(assign_stat.get_position())),
                is_local_declaration: false,
                value_expr_syntax_id,
                value_expr_offset,
                value_result_index,
                source_decl_id,
                source_call_syntax_id,
            })
        })
        .collect()
}

fn build_local_assignments_for_local_stat(
    decl_tree: &SalsaDeclTreeSummary,
    local_stat: LuaLocalStat,
) -> Vec<SalsaLocalAssignmentSummary> {
    let local_names = local_stat.get_local_name_list().collect::<Vec<_>>();
    let exprs = local_stat.get_value_exprs().collect::<Vec<_>>();
    local_names
        .into_iter()
        .enumerate()
        .filter_map(|(index, local_name)| {
            let name_token = local_name.get_name_token()?;
            let name = name_token.get_name_text();
            let decl =
                find_visible_decl_before_offset(decl_tree, &name, name_token.get_position())?;
            if matches!(decl.kind, SalsaDeclKindSummary::Global) {
                return None;
            }

            let last_expr_index = exprs.len().saturating_sub(1);
            let (expr, value_result_index) = if let Some(expr) = exprs.get(index).cloned() {
                (Some(expr), 0)
            } else {
                (exprs.last().cloned(), index.saturating_sub(last_expr_index))
            };
            let value_expr_syntax_id = expr.as_ref().map(|expr| expr.get_syntax_id().into());
            let value_expr_offset = expr
                .as_ref()
                .map(|expr| TextSize::from(u32::from(expr.get_position())));
            let source_call_syntax_id = expr.as_ref().and_then(call_expr_syntax_id_of_expr);
            let source_decl_id = expr.and_then(|expr| source_decl_id_of_expr(decl_tree, expr));

            Some(SalsaLocalAssignmentSummary {
                decl_id: decl.id,
                syntax_offset: TextSize::from(u32::from(local_stat.get_position())),
                is_local_declaration: true,
                value_expr_syntax_id,
                value_expr_offset,
                value_result_index,
                source_decl_id,
                source_call_syntax_id,
            })
        })
        .collect()
}

fn call_expr_syntax_id_of_expr(expr: &LuaExpr) -> Option<SalsaSyntaxIdSummary> {
    match expr {
        LuaExpr::CallExpr(call_expr) => Some(call_expr.get_syntax_id().into()),
        LuaExpr::ParenExpr(paren_expr) => call_expr_syntax_id_of_expr(&paren_expr.get_expr()?),
        _ => None,
    }
}

fn source_decl_id_of_expr(decl_tree: &SalsaDeclTreeSummary, expr: LuaExpr) -> Option<SalsaDeclId> {
    match expr {
        LuaExpr::NameExpr(name_expr) => {
            let name = name_expr.get_name_text()?;
            let decl = find_visible_decl_before_offset(decl_tree, &name, name_expr.get_position())?;
            (!matches!(decl.kind, SalsaDeclKindSummary::Global)).then_some(decl.id)
        }
        LuaExpr::ParenExpr(paren_expr) => source_decl_id_of_expr(decl_tree, paren_expr.get_expr()?),
        _ => None,
    }
}

fn collect_assignments_in_block_until_point(
    decl_tree: &SalsaDeclTreeSummary,
    block: &LuaBlock,
    decl_id: SalsaDeclId,
    program_point_offset: TextSize,
    assignments: &SalsaLocalAssignmentQueryIndex,
    incoming: Vec<SalsaLocalAssignmentSummary>,
) -> Vec<SalsaLocalAssignmentSummary> {
    let mut current = incoming;

    for stat in block.get_stats() {
        let stat_offset = TextSize::from(u32::from(stat.get_position()));
        if stat_offset >= program_point_offset {
            break;
        }

        match stat {
            LuaStat::AssignStat(assign_stat) => {
                let replacements =
                    assignments_for_decl_in_assign_stat(assignments, decl_id, &assign_stat);
                if !replacements.is_empty() {
                    current = replacements;
                }
            }
            LuaStat::LocalStat(local_stat) => {
                let replacements =
                    assignments_for_decl_in_local_stat(assignments, decl_id, &local_stat);
                if !replacements.is_empty() {
                    current = replacements;
                }
            }
            LuaStat::IfStat(if_stat) => {
                if if_stat.get_range().contains(program_point_offset) {
                    if let Some(current_clause_block) =
                        find_active_if_clause_block(&if_stat, program_point_offset)
                    {
                        return collect_assignments_in_block_until_point(
                            decl_tree,
                            &current_clause_block,
                            decl_id,
                            program_point_offset,
                            assignments,
                            current,
                        );
                    }
                } else if TextSize::from(u32::from(if_stat.syntax().text_range().end()))
                    < program_point_offset
                {
                    current = merge_assignments_after_if(
                        decl_tree,
                        &if_stat,
                        decl_id,
                        assignments,
                        current,
                    );
                }
            }
            LuaStat::DoStat(do_stat) => {
                if let Some(inner_block) = do_stat.get_block() {
                    if inner_block.get_range().contains(program_point_offset) {
                        return collect_assignments_in_block_until_point(
                            decl_tree,
                            &inner_block,
                            decl_id,
                            program_point_offset,
                            assignments,
                            current,
                        );
                    }
                    let nested = collect_assignments_in_block_until_point(
                        decl_tree,
                        &inner_block,
                        decl_id,
                        TextSize::from(u32::from(inner_block.syntax().text_range().end())),
                        assignments,
                        current.clone(),
                    );
                    if !nested.is_empty() {
                        current = nested;
                    }
                }
            }
            _ => {}
        }
    }

    current
}

fn assignments_for_decl_in_local_stat(
    assignments: &SalsaLocalAssignmentQueryIndex,
    decl_id: SalsaDeclId,
    local_stat: &LuaLocalStat,
) -> Vec<SalsaLocalAssignmentSummary> {
    let syntax_offset = TextSize::from(u32::from(local_stat.get_position()));
    assignments
        .assignments
        .iter()
        .filter(|assignment| {
            assignment.decl_id == decl_id && assignment.syntax_offset == syntax_offset
        })
        .cloned()
        .collect()
}

fn assignments_for_decl_in_assign_stat(
    assignments: &SalsaLocalAssignmentQueryIndex,
    decl_id: SalsaDeclId,
    assign_stat: &LuaAssignStat,
) -> Vec<SalsaLocalAssignmentSummary> {
    let syntax_offset = TextSize::from(u32::from(assign_stat.get_position()));
    assignments
        .assignments
        .iter()
        .filter(|assignment| {
            assignment.decl_id == decl_id && assignment.syntax_offset == syntax_offset
        })
        .cloned()
        .collect()
}

fn merge_assignments_after_if(
    decl_tree: &SalsaDeclTreeSummary,
    if_stat: &LuaIfStat,
    decl_id: SalsaDeclId,
    assignments: &SalsaLocalAssignmentQueryIndex,
    incoming: Vec<SalsaLocalAssignmentSummary>,
) -> Vec<SalsaLocalAssignmentSummary> {
    let mut merged = Vec::<SalsaLocalAssignmentSummary>::new();

    if let Some(block) = if_stat.get_block() {
        merged.extend(final_assignments_in_block(
            decl_tree,
            &block,
            decl_id,
            assignments,
            incoming.clone(),
        ));
    }
    for clause in if_stat.get_else_if_clause_list() {
        if let Some(block) = clause.get_block() {
            merged.extend(final_assignments_in_block(
                decl_tree,
                &block,
                decl_id,
                assignments,
                incoming.clone(),
            ));
        }
    }
    if let Some(else_clause) = if_stat.get_else_clause() {
        if let Some(block) = else_clause.get_block() {
            merged.extend(final_assignments_in_block(
                decl_tree,
                &block,
                decl_id,
                assignments,
                incoming.clone(),
            ));
        }
    } else {
        merged.extend(incoming);
    }

    dedupe_assignments(merged)
}

fn final_assignments_in_block(
    decl_tree: &SalsaDeclTreeSummary,
    block: &LuaBlock,
    decl_id: SalsaDeclId,
    assignments: &SalsaLocalAssignmentQueryIndex,
    incoming: Vec<SalsaLocalAssignmentSummary>,
) -> Vec<SalsaLocalAssignmentSummary> {
    let block_end = TextSize::from(u32::from(block.syntax().text_range().end()));
    let resolved = collect_assignments_in_block_until_point(
        decl_tree,
        block,
        decl_id,
        block_end,
        assignments,
        incoming.clone(),
    );
    if resolved.is_empty() {
        incoming
    } else {
        resolved
    }
}

fn dedupe_assignments(
    assignments: Vec<SalsaLocalAssignmentSummary>,
) -> Vec<SalsaLocalAssignmentSummary> {
    let mut seen = BTreeSet::new();
    assignments
        .into_iter()
        .filter(|assignment| {
            seen.insert((
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

fn find_active_if_clause_block(
    if_stat: &LuaIfStat,
    program_point_offset: TextSize,
) -> Option<LuaBlock> {
    if let Some(block) = if_stat.get_block()
        && block.get_range().contains(program_point_offset)
    {
        return Some(block);
    }
    for clause in if_stat.get_else_if_clause_list() {
        if let Some(block) = clause.get_block()
            && block.get_range().contains(program_point_offset)
        {
            return Some(block);
        }
    }
    if let Some(else_clause) = if_stat.get_else_clause()
        && let Some(block) = else_clause.get_block()
        && block.get_range().contains(program_point_offset)
    {
        return Some(block);
    }
    None
}
