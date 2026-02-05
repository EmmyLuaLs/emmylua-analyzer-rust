use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaBlock, LuaIfStat, LuaStat, LuaSyntaxKind, LuaTokenKind,
};

use crate::{
    DiagnosticCode, SemanticModel,
    diagnostic::checker::{Checker, DiagnosticContext},
};

pub struct InvertIfChecker;

impl Checker for InvertIfChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::InvertIf];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for if_statement in root.descendants::<LuaIfStat>() {
            check_early_return_pattern(context, &if_statement);
        }
    }
}

/// Check if an if statement follows an early-return pattern that could benefit from inversion.
///
/// The pattern we're looking for is:
/// ```lua
/// if condition then
///     -- many statements (main logic)
/// else
///     return  -- or return nil, or break (in loops)
/// end
/// -- more code follows here (important!)
/// ```
///
/// This can be refactored to:
/// ```lua
/// if not condition then
///     return
/// end
/// -- main logic (now at lower nesting level)
/// -- more code follows
/// ```
fn check_early_return_pattern(context: &mut DiagnosticContext, if_statement: &LuaIfStat) {
    // Only check if statements that have an else clause
    let Some(else_clause) = if_statement.get_else_clause() else {
        return;
    };

    // Don't suggest inversion for if-elseif-else chains
    if if_statement.get_else_if_clause_list().next().is_some() {
        return;
    }

    let Some(if_block) = if_statement.get_block() else {
        return;
    };
    let Some(else_block) = else_clause.get_block() else {
        return;
    };

    // Check if this if statement is in a loop - if so, be more careful about break suggestions
    let in_loop = is_in_loop(if_statement);

    // The else block should be a simple early-exit (return or break in loops)
    let else_exit_type = get_early_exit_type(&else_block);
    if else_exit_type == EarlyExitType::None {
        return;
    }

    // Break is only meaningful in loops
    if else_exit_type == EarlyExitType::Break && !in_loop {
        return;
    }

    // The if block should NOT end with return/break (otherwise both branches exit, no benefit)
    if block_ends_with_exit(&if_block) {
        return;
    }

    // Check if there's code after this if statement (otherwise no real benefit from inversion)
    if !has_code_after_if(if_statement) {
        return;
    }

    // The if block should have substantial code to justify the suggestion
    let if_stmt_count = count_meaningful_statements(&if_block);
    if if_stmt_count < 3 {
        return;
    }

    // All conditions met - suggest inversion
    if let Some(if_token) = if_statement.token_by_kind(LuaTokenKind::TkIf) {
        context.add_diagnostic(
            DiagnosticCode::InvertIf,
            if_token.get_range(),
            t!("Consider inverting 'if' statement to reduce nesting").to_string(),
            None,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EarlyExitType {
    None,
    Return,
    Break,
}

/// Check if a block is a simple early exit (return with no value, return nil, or break)
fn get_early_exit_type(block: &LuaBlock) -> EarlyExitType {
    let stats: Vec<_> = block.get_stats().collect();

    // Should only have one statement
    if stats.len() != 1 {
        return EarlyExitType::None;
    }

    match &stats[0] {
        LuaStat::ReturnStat(return_stat) => {
            let expr_count = return_stat.get_expr_list().count();
            // Allow: return, return nil
            if expr_count <= 1 {
                EarlyExitType::Return
            } else {
                EarlyExitType::None
            }
        }
        LuaStat::BreakStat(_) => EarlyExitType::Break,
        _ => EarlyExitType::None,
    }
}

/// Check if a block ends with a return or break statement
fn block_ends_with_exit(block: &LuaBlock) -> bool {
    let stats: Vec<_> = block.get_stats().collect();
    if let Some(last) = stats.last() {
        matches!(last, LuaStat::ReturnStat(_) | LuaStat::BreakStat(_))
    } else {
        false
    }
}

/// Count meaningful statements (excluding empty statements)
fn count_meaningful_statements(block: &LuaBlock) -> usize {
    block
        .get_stats()
        .filter(|s| !matches!(s, LuaStat::EmptyStat(_)))
        .count()
}

/// Check if this if statement is inside a loop
fn is_in_loop(if_statement: &LuaIfStat) -> bool {
    for ancestor in if_statement.syntax().ancestors() {
        let kind: LuaSyntaxKind = ancestor.kind().into();
        match kind {
            // Stop at function boundaries
            LuaSyntaxKind::ClosureExpr
            | LuaSyntaxKind::FuncStat
            | LuaSyntaxKind::LocalFuncStat
            | LuaSyntaxKind::Chunk => {
                return false;
            }
            // Found a loop
            LuaSyntaxKind::WhileStat
            | LuaSyntaxKind::RepeatStat
            | LuaSyntaxKind::ForStat
            | LuaSyntaxKind::ForRangeStat => {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// Check if there's meaningful code after this if statement in the same block
fn has_code_after_if(if_statement: &LuaIfStat) -> bool {
    // Get the next sibling that is a statement
    let mut next = if_statement.syntax().next_sibling();
    while let Some(sibling) = next {
        if let Some(stat) = LuaStat::cast(sibling.clone()) {
            // Check if it's a meaningful statement (not just empty)
            if !matches!(stat, LuaStat::EmptyStat(_)) {
                return true;
            }
        }
        next = sibling.next_sibling();
    }
    false
}
