use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaExpr, LuaLiteralToken, LuaLocalName, LuaVarExpr, NumberResult,
    UnaryOperator,
};
use rowan::TextSize;

use crate::{
    DeclMultiReturnRef, DeclMultiReturnRefAt, FlowId, LuaDeclId,
    compilation::analyzer::flow::binder::FlowBinder,
};

pub(super) fn check_local_immutable(binder: &mut FlowBinder, decl_id: LuaDeclId) -> bool {
    let Some(decl_ref) = binder
        .db
        .get_reference_index()
        .get_decl_references(&binder.file_id, &decl_id)
    else {
        return true;
    };

    !decl_ref.mutable
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
) -> Vec<Option<LuaDeclId>> {
    local_names
        .iter()
        .map(|name| Some(LuaDeclId::new(binder.file_id, name.get_position())))
        .collect()
}

pub(super) fn get_var_decl_ids(
    binder: &FlowBinder<'_>,
    vars: &[LuaVarExpr],
) -> Vec<Option<LuaDeclId>> {
    vars.iter()
        .map(|var| {
            binder
                .db
                .get_reference_index()
                .get_var_reference_decl(&binder.file_id, var.get_range())
        })
        .collect()
}

pub(super) fn bind_multi_return_refs(
    binder: &mut FlowBinder,
    decl_ids: &[Option<LuaDeclId>],
    values: &[LuaExpr],
    position: TextSize,
    flow_id: FlowId,
) {
    let tail_call = values.last().and_then(|value| match value {
        LuaExpr::CallExpr(call_expr) => Some((values.len() - 1, call_expr.to_ptr())),
        _ => None,
    });

    for (i, decl_id) in decl_ids.iter().enumerate() {
        let Some(decl_id) = decl_id else {
            continue;
        };

        let reference = tail_call.as_ref().and_then(|(last_value_idx, call_expr)| {
            if i < *last_value_idx {
                return None;
            }

            Some(DeclMultiReturnRef {
                call_expr: call_expr.clone(),
                return_index: i - *last_value_idx,
            })
        });

        binder
            .decl_multi_return_ref
            .entry(*decl_id)
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
    // 这里使用悲观合流: 只有静态确认循环体会执行时, 才把循环体 flow 合到循环之后.
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

/// 这里是循环可达性的静态判断, 只接受最直观的字面量真假值.
///
/// 它不是完整的常量求值或路径推断, 动态表达式和复杂常量表达式会返回 unknown,
/// 后续按不能确认进入循环处理.
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
