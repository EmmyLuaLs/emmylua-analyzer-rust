//! Invert-if checker — pure AST.

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaBlock, LuaIfStat, LuaStat, LuaSyntaxKind, LuaTokenKind,
};

use crate::semantic_model::SemanticModel;
use crate::DiagnosticCode;

use super::super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, _model: &SemanticModel) {
    let root = _model.get_root().clone();
    for if_stat in root.descendants::<LuaIfStat>() {
        check_early_return(context, &if_stat);
    }
}

#[derive(PartialEq)]
enum Exit {
    Return, Break,
}

fn check_early_return(context: &mut DiagnosticContext, if_stat: &LuaIfStat) {
    let Some(else_clause) = if_stat.get_else_clause() else { return };
    if if_stat.get_else_if_clause_list().next().is_some() { return }
    let Some(if_block) = if_stat.get_block() else { return };
    let Some(else_block) = else_clause.get_block() else { return };

    let in_loop = is_in_loop(if_stat);
    let exit_type = get_exit(&else_block);
    if exit_type.is_none() { return }
    if exit_type == Some(Exit::Break) && !in_loop { return }
    if block_ends_with_exit(&if_block) { return }
    if !has_code_after(if_stat) { return }
    if count_stmts(&if_block) < 3 { return }

    if let Some(tk) = if_stat.token_by_kind(LuaTokenKind::TkIf) {
        context.add_diagnostic(
            DiagnosticCode::InvertIf,
            tk.get_range(),
            t!("Consider inverting 'if' statement to reduce nesting").to_string(),
            None,
        );
    }
}

fn get_exit(block: &LuaBlock) -> Option<Exit> {
    let stats: Vec<_> = block.get_stats().collect();
    if stats.len() != 1 { return None }
    match &stats[0] {
        LuaStat::ReturnStat(r) if r.get_expr_list().count() <= 1 => Some(Exit::Return),
        LuaStat::BreakStat(_) => Some(Exit::Break),
        _ => None,
    }
}

fn block_ends_with_exit(block: &LuaBlock) -> bool {
    block.get_stats().last().is_some_and(|s| matches!(s, LuaStat::ReturnStat(_) | LuaStat::BreakStat(_)))
}

fn count_stmts(block: &LuaBlock) -> usize {
    block.get_stats().filter(|s| !matches!(s, LuaStat::EmptyStat(_))).count()
}

fn is_in_loop(if_stat: &LuaIfStat) -> bool {
    for a in if_stat.syntax().ancestors() {
        match a.kind().into() {
            LuaSyntaxKind::ClosureExpr | LuaSyntaxKind::FuncStat | LuaSyntaxKind::LocalFuncStat | LuaSyntaxKind::Chunk => return false,
            LuaSyntaxKind::WhileStat | LuaSyntaxKind::RepeatStat | LuaSyntaxKind::ForStat | LuaSyntaxKind::ForRangeStat => return true,
            _ => {}
        }
    }
    false
}

fn has_code_after(if_stat: &LuaIfStat) -> bool {
    let mut next = if_stat.syntax().next_sibling();
    while let Some(s) = next {
        if LuaStat::cast(s.clone()).is_some_and(|s| !matches!(s, LuaStat::EmptyStat(_))) {
            return true;
        }
        next = s.next_sibling();
    }
    false
}
