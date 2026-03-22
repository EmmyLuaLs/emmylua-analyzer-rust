mod binary_flow;
mod call_flow;
mod correlated_flow;
mod index_flow;

use self::correlated_flow::narrow_var_from_return_overload_condition;
use emmylua_parser::{LuaAstNode, LuaChunk, LuaExpr, LuaNameExpr, LuaUnaryExpr, UnaryOperator};

use crate::{
    DbIndex, FlowNode, FlowTree, InferFailReason, LuaInferCache, LuaType,
    semantic::infer::{
        VarRefId,
        narrow::{
            ResultTypeOrContinue,
            condition_flow::{
                binary_flow::get_type_at_binary_expr, call_flow::get_type_at_call_expr,
                index_flow::get_type_at_index_expr,
            },
            get_single_antecedent,
            get_type_at_flow::get_type_at_flow,
            narrow_false_or_nil, remove_false_or_nil,
            var_ref_id::get_var_expr_var_ref_id,
        },
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferConditionFlow {
    TrueCondition,
    FalseCondition,
}

impl InferConditionFlow {
    pub fn get_negated(&self) -> Self {
        match self {
            InferConditionFlow::TrueCondition => InferConditionFlow::FalseCondition,
            InferConditionFlow::FalseCondition => InferConditionFlow::TrueCondition,
        }
    }

    #[allow(unused)]
    pub fn is_true(&self) -> bool {
        matches!(self, InferConditionFlow::TrueCondition)
    }

    pub fn is_false(&self) -> bool {
        matches!(self, InferConditionFlow::FalseCondition)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn get_type_at_condition_flow(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    condition: LuaExpr,
    condition_flow: InferConditionFlow,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    match condition {
        LuaExpr::NameExpr(name_expr) => get_type_at_name_expr(
            db,
            tree,
            cache,
            root,
            var_ref_id,
            flow_node,
            name_expr,
            condition_flow,
        ),
        LuaExpr::CallExpr(call_expr) => get_type_at_call_expr(
            db,
            tree,
            cache,
            root,
            var_ref_id,
            flow_node,
            call_expr,
            condition_flow,
        ),
        LuaExpr::IndexExpr(index_expr) => get_type_at_index_expr(
            db,
            tree,
            cache,
            root,
            var_ref_id,
            flow_node,
            index_expr,
            condition_flow,
        ),
        LuaExpr::TableExpr(_) | LuaExpr::LiteralExpr(_) | LuaExpr::ClosureExpr(_) => {
            Ok(ResultTypeOrContinue::Continue)
        }
        LuaExpr::BinaryExpr(binary_expr) => get_type_at_binary_expr(
            db,
            tree,
            cache,
            root,
            var_ref_id,
            flow_node,
            binary_expr,
            condition_flow,
        ),
        LuaExpr::UnaryExpr(unary_expr) => get_type_at_unary_flow(
            db,
            tree,
            cache,
            root,
            var_ref_id,
            flow_node,
            unary_expr,
            condition_flow,
        ),
        LuaExpr::ParenExpr(paren_expr) => {
            let Some(inner_expr) = paren_expr.get_expr() else {
                return Ok(ResultTypeOrContinue::Continue);
            };

            get_type_at_condition_flow(
                db,
                tree,
                cache,
                root,
                var_ref_id,
                flow_node,
                inner_expr,
                condition_flow,
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn get_type_at_name_expr(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    name_expr: LuaNameExpr,
    condition_flow: InferConditionFlow,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    let Some(name_var_ref_id) =
        get_var_expr_var_ref_id(db, cache, LuaExpr::NameExpr(name_expr.clone()))
    else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    if name_var_ref_id != *var_ref_id {
        return get_type_at_name_ref(
            db,
            tree,
            cache,
            root,
            var_ref_id,
            flow_node,
            name_expr,
            condition_flow,
        );
    }

    let antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
    let antecedent_type = get_type_at_flow(db, tree, cache, root, var_ref_id, antecedent_flow_id)?;

    let result_type = match condition_flow {
        InferConditionFlow::FalseCondition => narrow_false_or_nil(db, antecedent_type),
        InferConditionFlow::TrueCondition => remove_false_or_nil(antecedent_type),
    };

    Ok(ResultTypeOrContinue::Result(result_type))
}

#[allow(clippy::too_many_arguments)]
fn get_type_at_name_ref(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    name_expr: LuaNameExpr,
    condition_flow: InferConditionFlow,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    let Some(decl_id) = db
        .get_reference_index()
        .get_var_reference_decl(&cache.get_file_id(), name_expr.get_range())
    else {
        return Ok(ResultTypeOrContinue::Continue);
    };
    let antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
    let antecedent_discriminant_type = get_type_at_flow(
        db,
        tree,
        cache,
        root,
        &VarRefId::VarRef(decl_id),
        antecedent_flow_id,
    )?;
    let narrowed_discriminant_type = match condition_flow {
        InferConditionFlow::FalseCondition => narrow_false_or_nil(db, antecedent_discriminant_type),
        InferConditionFlow::TrueCondition => remove_false_or_nil(antecedent_discriminant_type),
    };

    if let ResultTypeOrContinue::Result(result_type) = narrow_var_from_return_overload_condition(
        db,
        tree,
        cache,
        root,
        var_ref_id,
        flow_node,
        decl_id,
        name_expr.get_position(),
        &narrowed_discriminant_type,
    )? {
        return Ok(ResultTypeOrContinue::Result(result_type));
    }

    let Some(expr_ptr) = tree.get_decl_ref_expr(&decl_id) else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let Some(expr) = expr_ptr.to_node(root) else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    get_type_at_condition_flow(
        db,
        tree,
        cache,
        root,
        var_ref_id,
        flow_node,
        expr,
        condition_flow,
    )
}

pub(super) fn always_literal_equal(left: &LuaType, right: &LuaType) -> bool {
    match (left, right) {
        (LuaType::Union(union), other) => union
            .into_vec()
            .into_iter()
            .all(|candidate| always_literal_equal(&candidate, other)),
        (other, LuaType::Union(union)) => union
            .into_vec()
            .into_iter()
            .all(|candidate| always_literal_equal(other, &candidate)),
        (
            LuaType::StringConst(l) | LuaType::DocStringConst(l),
            LuaType::StringConst(r) | LuaType::DocStringConst(r),
        ) => l == r,
        (
            LuaType::BooleanConst(l) | LuaType::DocBooleanConst(l),
            LuaType::BooleanConst(r) | LuaType::DocBooleanConst(r),
        ) => l == r,
        (
            LuaType::IntegerConst(l) | LuaType::DocIntegerConst(l),
            LuaType::IntegerConst(r) | LuaType::DocIntegerConst(r),
        ) => l == r,
        _ => left == right,
    }
}

#[allow(clippy::too_many_arguments)]
fn get_type_at_unary_flow(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    unary_expr: LuaUnaryExpr,
    condition_flow: InferConditionFlow,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    let Some(inner_expr) = unary_expr.get_expr() else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    let Some(op) = unary_expr.get_op_token() else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    match op.get_op() {
        UnaryOperator::OpNot => {}
        _ => {
            return Ok(ResultTypeOrContinue::Continue);
        }
    }

    get_type_at_condition_flow(
        db,
        tree,
        cache,
        root,
        var_ref_id,
        flow_node,
        inner_expr,
        condition_flow.get_negated(),
    )
}
