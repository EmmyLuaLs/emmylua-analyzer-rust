//! invert_if — suggests reversing if statements to reduce nesting (purely syntactic check).

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaBlock, LuaIfStat, LuaStat, LuaSyntaxKind, LuaTokenKind,
};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::super::{CheckContext, Checker};

pub struct InvertIfChecker;

impl Checker for InvertIfChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::InvertIf];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for if_statement in root.descendants().filter_map(LuaIfStat::cast) {
            check_early_return_pattern(context, &if_statement);
        }
    }
}

/// Target pattern:
/// ```lua
/// if condition then
///     -- main logic with multiple statements
/// else
///     return
/// end
/// -- code after the if
/// ```
fn check_early_return_pattern(context: &mut CheckContext<'_>, if_statement: &LuaIfStat) {
    let Some(else_clause) = if_statement.get_else_clause() else {
        return;
    };
    // Do not handle if-elseif-else chains.
    if if_statement.get_else_if_clause_list().next().is_some() {
        return;
    }
    let Some(if_block) = if_statement.get_block() else {
        return;
    };
    let Some(else_block) = else_clause.get_block() else {
        return;
    };
    let in_loop = is_in_loop(if_statement);

    let else_exit_type = get_early_exit_type(&else_block);
    if else_exit_type == EarlyExitType::None {
        return;
    }
    if else_exit_type == EarlyExitType::Break && !in_loop {
        return;
    }
    // Main branch also exits → inverting provides no benefit.
    if block_ends_with_exit(&if_block) {
        return;
    }
    // No code after the if → inverting provides no benefit.
    if !has_code_after_if(if_statement) {
        return;
    }
    let if_stmt_count = count_meaningful_statements(&if_block);
    if if_stmt_count < 3 {
        return;
    }

    if let Some(if_token) = if_statement.token_by_kind(LuaTokenKind::TkIf) {
        context.add_diagnostic(
            DiagnosticCode::InvertIf,
            if_token.syntax().text_range(),
            t!("Consider inverting 'if' statement to reduce nesting"),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EarlyExitType {
    None,
    Return,
    Break,
}

/// Simple early-exit block: a single `return` (≤1 value) or `break`.
fn get_early_exit_type(block: &LuaBlock) -> EarlyExitType {
    let stats: Vec<_> = block.get_stats().collect();
    if stats.len() != 1 {
        return EarlyExitType::None;
    }
    match &stats[0] {
        LuaStat::ReturnStat(return_stat) => {
            if return_stat.get_expr_list().count() <= 1 {
                EarlyExitType::Return
            } else {
                EarlyExitType::None
            }
        }
        LuaStat::BreakStat(_) => EarlyExitType::Break,
        _ => EarlyExitType::None,
    }
}

fn block_ends_with_exit(block: &LuaBlock) -> bool {
    let stats: Vec<_> = block.get_stats().collect();
    matches!(
        stats.last(),
        Some(LuaStat::ReturnStat(_) | LuaStat::BreakStat(_))
    )
}

fn count_meaningful_statements(block: &LuaBlock) -> usize {
    block
        .get_stats()
        .filter(|s| !matches!(s, LuaStat::EmptyStat(_)))
        .count()
}

/// Whether inside a loop (stopping at function boundaries).
fn is_in_loop(if_statement: &LuaIfStat) -> bool {
    for ancestor in if_statement.syntax().ancestors() {
        let kind: LuaSyntaxKind = ancestor.kind().into();
        match kind {
            LuaSyntaxKind::ClosureExpr
            | LuaSyntaxKind::FuncStat
            | LuaSyntaxKind::LocalFuncStat
            | LuaSyntaxKind::Chunk => return false,
            LuaSyntaxKind::WhileStat
            | LuaSyntaxKind::RepeatStat
            | LuaSyntaxKind::ForStat
            | LuaSyntaxKind::ForRangeStat => return true,
            _ => {}
        }
    }
    false
}

/// Whether there are statements after the if in the same block.
fn has_code_after_if(if_statement: &LuaIfStat) -> bool {
    let mut next = if_statement.syntax().next_sibling();
    while let Some(sibling) = next {
        if let Some(stat) = LuaStat::cast(sibling.clone())
            && !matches!(stat, LuaStat::EmptyStat(_))
        {
            return true;
        }
        next = sibling.next_sibling();
    }
    false
}
