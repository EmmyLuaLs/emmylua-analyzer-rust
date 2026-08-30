use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaExpr, LuaIndexKey, LuaLiteralToken, LuaLocalName, LuaVarExpr,
    NumberResult, UnaryOperator,
};
use rowan::TextSize;
use smol_str::SmolStr;

use super::super::def::{LuaMemberKey, SemanticId};
use super::{DeclMultiReturnRef, DeclMultiReturnRefAt, FlowId, binder::FlowBinder};

pub(super) fn check_local_immutable(_binder: &mut FlowBinder, _decl_id: &SemanticId) -> bool {
    // M0: no reference index, so assume immutable.
    true
}

pub(super) fn check_value_expr_is_check_expr(value_expr: LuaExpr) -> bool {
    match value_expr {
        LuaExpr::BinaryExpr(binary_expr) => {
            let Some(op) = binary_expr.get_op_token() else {
                return false;
            };

            matches!(op.get_op(), BinaryOperator::OpEq | BinaryOperator::OpNe)
        }
        LuaExpr::CallExpr(call) => call.is_type(),
        _ => false, // Other expressions can be checked
    }
}

pub(super) fn get_local_decl_ids(
    binder: &FlowBinder<'_>,
    local_names: &[LuaLocalName],
) -> Vec<Option<SemanticId>> {
    local_names
        .iter()
        .map(|name| Some(SemanticId::decl(binder.file_id, name.get_range())))
        .collect()
}

pub(super) fn get_var_decl_ids(
    binder: &FlowBinder<'_>,
    vars: &[LuaVarExpr],
) -> Vec<Option<SemanticId>> {
    vars.iter()
        .map(|var| match var {
            LuaVarExpr::NameExpr(name_expr) => {
                let name = name_expr.get_name_text()?;
                binder
                    .facts
                    .find_visible_decl_before_offset(&name, name_expr.get_position())
                    .map(|decl| decl.id.clone())
            }
            _ => None,
        })
        .collect()
}

/// Converts an `IndexExpr` assignment lvalue into a member effect (owner + key + member identity).
/// Only accepts members already registered in facts; undefined fields are handled by `type_of_expr` as a fallback during backtracking.
pub(super) fn get_var_member_ids(
    binder: &FlowBinder<'_>,
    vars: &[LuaVarExpr],
) -> Vec<Option<(SemanticId, LuaMemberKey, SemanticId)>> {
    vars.iter()
        .map(|var| {
            let LuaVarExpr::IndexExpr(index_expr) = var else {
                return None;
            };
            let key = index_expr.get_index_key()?;
            let key_offset = key
                .get_range()
                .unwrap_or_else(|| index_expr.get_range())
                .start();
            let member = binder.facts.members.iter().find(|member| {
                member
                    .id
                    .member_key_range()
                    .is_some_and(|range| range.contains(key_offset))
            })?;
            Some((member.owner.clone(), member.key.clone(), member.id.clone()))
        })
        .collect()
}

/// Converts `LuaIndexKey` into a file-agnostic member key (matching the facts collection rules).
#[allow(dead_code)]
pub(super) fn member_key_from_index_key(key: LuaIndexKey) -> Option<LuaMemberKey> {
    match key {
        LuaIndexKey::Name(_) | LuaIndexKey::String(_) => {
            Some(LuaMemberKey::Name(SmolStr::new(key.get_path_part())))
        }
        LuaIndexKey::Integer(num) => match num.get_number_value() {
            NumberResult::Int(i) => Some(LuaMemberKey::Integer(i)),
            _ => None,
        },
        LuaIndexKey::Idx(idx) => Some(LuaMemberKey::Integer(idx as i64)),
        LuaIndexKey::Expr(_) => None,
    }
}

pub(super) fn bind_multi_return_refs(
    binder: &mut FlowBinder,
    decl_ids: &[Option<SemanticId>],
    values: &[LuaExpr],
    position: TextSize,
    flow_id: FlowId,
) {
    let last_value_idx = values.len().saturating_sub(1);

    for (i, decl_id) in decl_ids.iter().enumerate() {
        let Some(decl_id) = decl_id else {
            continue;
        };

        let reference = if i < last_value_idx {
            // Not the last expression: only take the first return value of that call.
            match values.get(i) {
                Some(LuaExpr::CallExpr(call_expr)) => Some(DeclMultiReturnRef {
                    call_expr: call_expr.to_ptr(),
                    return_index: 0,
                }),
                _ => None,
            }
        } else if let Some(LuaExpr::CallExpr(call_expr)) = values.last() {
            Some(DeclMultiReturnRef {
                call_expr: call_expr.to_ptr(),
                return_index: i - last_value_idx,
            })
        } else {
            None
        };

        binder
            .decl_multi_return_ref
            .entry(decl_id.clone())
            .or_default()
            .push(DeclMultiReturnRefAt {
                position,
                flow_id,
                reference,
            });
    }
}

pub(super) fn finish_entered_loop_post_flow(
    binder: &mut FlowBinder,
    after_loop_label: FlowId,
    block_flow: FlowId,
) -> FlowId {
    // Use pessimistic merging: only merge the loop body flow after the loop when the body is statically known to execute.
    binder.add_antecedent(after_loop_label, block_flow);
    if binder
        .get_flow(after_loop_label)
        .is_some_and(|flow_node| flow_node.antecedent.is_some())
    {
        after_loop_label
    } else {
        binder.unreachable
    }
}

/// Static reachability judgement for loops; only accepts straightforward literal truthiness.
///
/// This is not full constant evaluation or path inference. Dynamic expressions and complex
/// constant expressions return unknown, and are later treated as not confirmed to enter the loop.
pub(super) fn static_literal_truthiness(expr: &LuaExpr) -> Option<bool> {
    match expr {
        LuaExpr::LiteralExpr(literal_expr) => match literal_expr.get_literal()? {
            LuaLiteralToken::Bool(bool_token) => Some(bool_token.is_true()),
            LuaLiteralToken::Nil(_) => Some(false),
            LuaLiteralToken::String(_) | LuaLiteralToken::Number(_) => Some(true),
            LuaLiteralToken::Dots(_) | LuaLiteralToken::Question(_) => None,
        },
        LuaExpr::ParenExpr(paren_expr) => static_literal_truthiness(&paren_expr.get_expr()?),
        LuaExpr::UnaryExpr(unary_expr)
            if unary_expr
                .get_op_token()
                .is_some_and(|op| op.get_op() == UnaryOperator::OpNot) =>
        {
            static_literal_truthiness(&unary_expr.get_expr()?).map(|truthy| !truthy)
        }
        _ => None,
    }
}

pub(super) fn static_number_value(expr: &LuaExpr) -> Option<f64> {
    match expr {
        LuaExpr::LiteralExpr(literal_expr) => match literal_expr.get_literal()? {
            LuaLiteralToken::Number(number_token) => match number_token.get_number_value() {
                NumberResult::Int(value) => Some(value as f64),
                NumberResult::Uint(value) => Some(value as f64),
                NumberResult::Float(value) => Some(value),
                NumberResult::Number => None,
            },
            _ => None,
        },
        LuaExpr::ParenExpr(paren_expr) => static_number_value(&paren_expr.get_expr()?),
        _ => None,
    }
}
