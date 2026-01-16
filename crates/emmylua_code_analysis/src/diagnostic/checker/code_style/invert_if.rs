use emmylua_parser::{LuaAstNode, LuaAstToken, LuaBlock, LuaIfStat, LuaStat, LuaTokenKind};

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
            check_if_statement(context, if_statement.clone());
            check_nested_if_depth(context, if_statement);
        }
    }
}

fn check_if_statement(context: &mut DiagnosticContext, if_statement: LuaIfStat) {
    // Only check if statements that have an else clause
    let Some(else_clause) = if_statement.get_else_clause() else {
        return;
    };

    // Check for elseif clauses; if present, do not suggest inversion
    if if_statement.get_else_if_clause_list().next().is_some() {
        return;
    }

    // Get the if body and else body
    let Some(if_block) = if_statement.get_block() else {
        return;
    };
    let Some(else_block) = else_clause.get_block() else {
        return;
    };

    // Check whether the else block contains only simple jump statements (return or break)
    if is_simple_jump_statement(&else_block) {
        // Check whether the if block has enough statements to recommend inversion
        let if_stmt_count = count_statements(&if_block);
        if if_stmt_count >= 2 {
            // Suggest inverting the if statement
            if let Some(if_token) = if_statement.token_by_kind(LuaTokenKind::TkIf) {
                context.add_diagnostic(
                    DiagnosticCode::InvertIf,
                    if_token.get_range(),
                    t!("Consider inverting 'if' statement to reduce nesting").to_string(),
                    None,
                );
            }
        }
    }
}

/// Check whether a block contains only simple jump statements (return or break)
fn is_simple_jump_statement(block: &emmylua_parser::LuaBlock) -> bool {
    let stats: Vec<_> = block.get_stats().collect();

    // Only one statement
    if stats.len() != 1 {
        return false;
    }

    // Check if it is a return or break statement
    match &stats[0] {
        LuaStat::ReturnStat(return_stat) => {
            // return statement has no return values, or only one simple return value
            return_stat.get_expr_list().count() == 0
        }
        LuaStat::BreakStat(_) => true,
        _ => false,
    }
}

/// Count the number of statements in a block
fn count_statements(block: &LuaBlock) -> usize {
    block.get_stats().count()
}

/// Check for deeply nested if statements
/// Reports diagnostics when nesting exceeds threshold (default: 3 levels)
/// Only warns if the if statement is at the beginning of a block (suitable for early returns)
fn check_nested_if_depth(context: &mut DiagnosticContext, if_statement: LuaIfStat) {
    const MAX_NESTING_DEPTH: usize = 3;
    // Calculate nesting depth
    let depth = calculate_if_nesting_depth(&if_statement);

    if depth >= MAX_NESTING_DEPTH {
        if let Some(if_token) = if_statement.token_by_kind(LuaTokenKind::TkIf) {
            let message = t!(
                "Deep nesting detected (level %{level}). Consider using early returns to reduce complexity",
                level = depth
            );
            context.add_diagnostic(
                DiagnosticCode::InvertIf,
                if_token.get_range(),
                message.to_string(),
                None,
            );
        }
    }
}

/// Calculate the nesting depth of an if statement
/// Returns the number of nested if statements from the function/file root
fn calculate_if_nesting_depth(if_statement: &LuaIfStat) -> usize {
    let mut depth = 1;
    let mut current_if = if_statement.clone();
    loop {
        let prev_stat: Option<LuaStat> = current_if.syntax().prev_sibling().and_then(LuaStat::cast);
        if prev_stat.is_some() {
            break;
        }
        depth += 1;
        let Some(parent_block) = current_if.get_parent::<LuaBlock>() else {
            return depth;
        };
        current_if = match parent_block.get_parent::<LuaIfStat>() {
            Some(parent) => parent,
            None => return depth,
        };
    }

    depth
}
