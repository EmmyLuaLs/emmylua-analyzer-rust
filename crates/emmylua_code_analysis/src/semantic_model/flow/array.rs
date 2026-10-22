//! Prove `a[#a]` exists only for an unchanged local table that never escapes.
//! This adds no numeric bounds or effects to the general flow model.

use emmylua_parser::{BinaryOperator, LuaAssignStat, LuaAstNode, LuaExpr, UnaryOperator};
use rowan::TextSize;

use crate::LuaType;
use crate::semantic_db::def::{DeclKind, SemanticId};
use crate::semantic_db::flow::LuaClosureId;

use super::guards::{
    guarded_on_all_paths, literal_comparison, normalize_condition, unparenthesized,
};
use super::{SemanticModel, literal_type};

pub(super) fn last_element_type(
    model: &SemanticModel,
    index: &emmylua_parser::LuaIndexExpr,
    offset: TextSize,
) -> Option<LuaType> {
    let emmylua_parser::LuaIndexKey::Expr(key) = index.get_index_key()? else {
        return None;
    };
    let LuaExpr::UnaryExpr(length) = unparenthesized(key)? else {
        return None;
    };
    if length.get_op_token()?.get_op() != UnaryOperator::OpLen {
        return None;
    }
    let prefix = index.get_prefix_expr()?;
    let decl = name_decl(model, prefix.clone())?;
    if name_decl(model, length.get_expr()?)? != decl || !length_is_stable(model, &decl) {
        return None;
    }
    guarded_on_all_paths(
        model,
        index,
        offset,
        |cond, truthy| nonempty_condition(model, &decl, cond, truthy).unwrap_or(false),
        |_, _| Some(()), // The whole-function stability check already excludes writes.
    )?;
    // Keep the current element type, including existing casts on the prefix.
    let components = match model.type_of_expr_at(prefix.get_syntax_id(), offset) {
        LuaType::Union(union) => union.into_vec(),
        ty => vec![ty],
    };
    let bases = components
        .into_iter()
        .map(|ty| match ty {
            LuaType::Array(array) => Some(array.get_base().clone()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    (!bases.is_empty()).then(|| LuaType::from_vec(bases))
}

fn name_decl(model: &SemanticModel, expr: LuaExpr) -> Option<SemanticId> {
    let LuaExpr::NameExpr(name) = unparenthesized(expr)? else {
        return None;
    };
    model.resolve_name(name.get_position())
}

fn length_is_stable(model: &SemanticModel, decl: &SemanticId) -> bool {
    let key = (model.file_id(), decl.clone());
    if let Some(stable) = model.cache.borrow().array_length_stable.get(&key) {
        return *stable;
    }
    let stable = (|| {
        let info = model.file_facts()?.decl_by_id(decl)?;
        if !matches!(info.kind, DeclKind::Local { is_iter: false, .. }) {
            return None;
        }
        let root = model.syntax_tree()?.get_red_root();
        let initializer = LuaExpr::cast(info.value_expr_syntax?.to_node_from_root(&root)?)?;
        if !matches!(unparenthesized(initializer)?, LuaExpr::TableExpr(_)) {
            return None;
        }
        let owner = info.owner_syntax?.to_node_from_root(&root)?;
        let closure = LuaClosureId::from_node(&owner);
        Some(model.decl_references(decl).iter().all(|syntax| {
            syntax.to_node_from_root(&root).is_some_and(|node| {
                LuaClosureId::from_node(&node) == closure && reference_is_read_only(node)
            })
        }))
    })()
    .unwrap_or(false);
    model
        .cache
        .borrow_mut()
        .array_length_stable
        .insert(key, stable);
    stable
}

/// Check every reference, including future writes and captures. An alias or
/// escape disqualifies the whole declaration, so calls need no effect tracking.
fn reference_is_read_only(mut node: emmylua_parser::LuaSyntaxNode) -> bool {
    while let Some(parent) = node.parent()
        && matches!(LuaExpr::cast(parent.clone()), Some(LuaExpr::ParenExpr(_)))
    {
        node = parent;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    match LuaExpr::cast(parent.clone()) {
        Some(LuaExpr::UnaryExpr(unary)) => unary
            .get_op_token()
            .is_some_and(|op| matches!(op.get_op(), UnaryOperator::OpLen | UnaryOperator::OpNot)),
        Some(LuaExpr::BinaryExpr(binary)) => {
            let Some((left, right)) = binary.get_exprs() else {
                return false;
            };
            let other = if left.get_range() == node.text_range() {
                right
            } else {
                left
            };
            binary.get_op_token().is_some_and(|op| {
                matches!(op.get_op(), BinaryOperator::OpEq | BinaryOperator::OpNe)
            }) && matches!(unparenthesized(other), Some(LuaExpr::LiteralExpr(lit))
                if literal_type(&lit).is_some())
        }
        Some(LuaExpr::IndexExpr(index)) => {
            index
                .get_index_token()
                .is_some_and(|token| !token.is_colon())
                && index
                    .get_prefix_expr()
                    .is_some_and(|prefix| prefix.get_range() == node.text_range())
                && !index.ancestors::<LuaAssignStat>().any(|assign| {
                    assign
                        .get_var_and_expr_list()
                        .0
                        .iter()
                        .any(|var| var.get_range().contains_range(index.get_range()))
                })
        }
        _ => matches!(
            parent.kind().into(),
            emmylua_parser::LuaSyntaxKind::IfStat
                | emmylua_parser::LuaSyntaxKind::ElseIfClauseStat
                | emmylua_parser::LuaSyntaxKind::WhileStat
                | emmylua_parser::LuaSyntaxKind::RepeatStat
        ),
    }
}

fn first_element(model: &SemanticModel, decl: &SemanticId, expr: LuaExpr) -> Option<bool> {
    let LuaExpr::IndexExpr(index) = unparenthesized(expr)? else {
        return Some(false);
    };
    let first = match index.get_index_key()? {
        emmylua_parser::LuaIndexKey::Integer(number) => matches!(
            number.get_number_value(),
            emmylua_parser::NumberResult::Int(1) | emmylua_parser::NumberResult::Uint(1)
        ),
        emmylua_parser::LuaIndexKey::Idx(1) => true,
        _ => false,
    };
    Some(first && name_decl(model, index.get_prefix_expr()?).as_ref() == Some(decl))
}

fn nonempty_condition(
    model: &SemanticModel,
    decl: &SemanticId,
    cond: LuaExpr,
    truthy: bool,
) -> Option<bool> {
    use BinaryOperator::*;
    let (cond, truthy) = normalize_condition(cond, truthy)?;
    if truthy && first_element(model, decl, cond.clone())? {
        return Some(true);
    }
    let LuaExpr::BinaryExpr(binary) = cond else {
        return Some(false);
    };
    let (left, right) = binary.get_exprs()?;
    let op = binary.get_op_token()?.get_op();
    let (value, literal, op) =
        literal_comparison(unparenthesized(left)?, unparenthesized(right)?, op)?;
    if literal.is_nil() {
        return Some(
            matches!(op, OpEq | OpNe)
                && truthy == (op == OpNe)
                && first_element(model, decl, value)?,
        );
    }
    let LuaType::IntegerConst(bound) = literal else {
        return Some(false);
    };
    let LuaExpr::UnaryExpr(length) = value else {
        return Some(false);
    };
    if length.get_op_token()?.get_op() != UnaryOperator::OpLen
        || name_decl(model, length.get_expr()?).as_ref() != Some(decl)
    {
        return Some(false);
    }
    Some(match (op, truthy) {
        (OpEq, true) | (OpNe, false) | (OpGe, true) | (OpLt, false) => bound > 0,
        (OpGt, true) | (OpLe, false) => bound >= 0,
        (OpEq, false) | (OpNe, true) => bound == 0,
        _ => false,
    })
}
