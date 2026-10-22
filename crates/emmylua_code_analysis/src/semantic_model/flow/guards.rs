//! Shared traversal and syntax matching for guarded array reads.

use std::collections::HashSet;

use emmylua_parser::{BinaryOperator, LuaAstNode, LuaExpr, LuaIndexExpr, UnaryOperator};
use rowan::TextSize;

use crate::LuaType;
use crate::semantic_db::flow::{FlowAntecedent, FlowId, FlowNodeKind, FlowTree, LuaClosureId};

use super::{SemanticModel, flow_condition_expr, literal_type};

/// Each incoming path must reach a guard in the same function before a loop,
/// goto boundary, or entry point. Callers decide which assignments are safe.
pub(super) fn guarded_on_all_paths(
    model: &SemanticModel,
    index: &LuaIndexExpr,
    offset: TextSize,
    mut is_guard: impl FnMut(LuaExpr, bool) -> bool,
    check_assignment: impl Fn(&FlowTree, FlowId) -> Option<()>,
) -> Option<()> {
    let tree = model.flow_tree()?;
    if !tree.has_condition() {
        return None;
    }
    let closure = LuaClosureId::from_node(index.syntax());
    let mut pending = vec![tree.get_flow_id_at(offset)?];
    let mut visited = HashSet::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        let node = tree.get_flow_node(current)?;
        match &node.kind {
            FlowNodeKind::Assignment(ptr) if !ptr.get_syntax_id().get_range().contains(offset) => {
                check_assignment(&tree, current)?;
            }
            FlowNodeKind::TrueCondition(ptr) | FlowNodeKind::FalseCondition(ptr) => {
                let cond = flow_condition_expr(model, ptr)?;
                if LuaClosureId::from_node(cond.syntax()) != closure {
                    return None;
                }
                if is_guard(cond, matches!(node.kind, FlowNodeKind::TrueCondition(_))) {
                    continue;
                }
            }
            FlowNodeKind::Start | FlowNodeKind::LoopLabel | FlowNodeKind::NamedLabel(_) => {
                return None;
            }
            FlowNodeKind::Unreachable => continue,
            _ => {}
        }
        match node.antecedent.as_ref()? {
            FlowAntecedent::Single(next) => pending.push(*next),
            FlowAntecedent::Multiple(multi) => pending.extend(tree.get_multi_antecedents(*multi)?),
        }
    }
    Some(())
}

pub(super) fn unparenthesized(mut expr: LuaExpr) -> Option<LuaExpr> {
    while let LuaExpr::ParenExpr(paren) = expr {
        expr = paren.get_expr()?;
    }
    Some(expr)
}

/// Remove outer parentheses and negations, preserving the selected branch.
pub(super) fn normalize_condition(mut cond: LuaExpr, mut truthy: bool) -> Option<(LuaExpr, bool)> {
    loop {
        cond = unparenthesized(cond)?;
        if let LuaExpr::UnaryExpr(unary) = &cond
            && unary.get_op_token()?.get_op() == UnaryOperator::OpNot
        {
            cond = unary.get_expr()?;
            truthy = !truthy;
        } else {
            return Some((cond, truthy));
        }
    }
}

/// Put the literal on the right and reverse relational operators as needed.
/// Callers control whether parentheses around operands are accepted.
pub(super) fn literal_comparison(
    left: LuaExpr,
    right: LuaExpr,
    op: BinaryOperator,
) -> Option<(LuaExpr, LuaType, BinaryOperator)> {
    match (left, right) {
        (value, LuaExpr::LiteralExpr(lit)) => Some((value, literal_type(&lit)?, op)),
        (LuaExpr::LiteralExpr(lit), value) => {
            let reversed = match op {
                BinaryOperator::OpLt => BinaryOperator::OpGt,
                BinaryOperator::OpLe => BinaryOperator::OpGe,
                BinaryOperator::OpGt => BinaryOperator::OpLt,
                BinaryOperator::OpGe => BinaryOperator::OpLe,
                other => other,
            };
            Some((value, literal_type(&lit)?, reversed))
        }
        _ => None,
    }
}
