mod binary_flow;
mod call_flow;
pub(in crate::semantic::infer::narrow) mod correlated_flow;
mod index_flow;

use std::rc::Rc;

use self::{
    binary_flow::{get_type_at_binary_expr, narrow_eq_condition},
    correlated_flow::{
        CorrelatedConditionNarrowing, PendingCorrelatedCondition,
        prepare_var_from_return_overload_condition,
    },
};
use emmylua_parser::{LuaAstNode, LuaChunk, LuaExpr, LuaIndexMemberExpr, UnaryOperator};

use crate::{
    DbIndex, FlowId, FlowNode, FlowTree, InferFailReason, InferGuard, LuaArrayLen, LuaArrayType,
    LuaDeclId, LuaInferCache, LuaSignatureCast, LuaSignatureId, LuaType, TypeOps,
    semantic::infer::{
        VarRefId,
        infer_index::infer_member_by_member_key,
        narrow::{
            condition_flow::{
                call_flow::get_type_at_call_expr, index_flow::get_type_at_index_expr,
            },
            get_single_antecedent,
            get_type_at_cast_flow::cast_type,
            narrow_down_type, narrow_false_or_nil, remove_false_or_nil,
            var_ref_id::get_var_expr_var_ref_id,
        },
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InferConditionFlow {
    TrueCondition,
    FalseCondition,
}

#[derive(Debug, Clone)]
pub(in crate::semantic) enum ConditionSubquery {
    ArrayLen {
        var_ref_id: VarRefId,
        antecedent_flow_id: FlowId,
        // This is the effective narrowing polarity after rewrites like `not` and `~=`.
        subquery_condition_flow: InferConditionFlow,
        right_expr_type: LuaType,
        max_adjustment: i64,
    },
    FieldLiteralEq {
        var_ref_id: VarRefId,
        antecedent_flow_id: FlowId,
        subquery_condition_flow: InferConditionFlow,
        idx: LuaIndexMemberExpr,
        right_expr_type: LuaType,
    },
    Correlated {
        var_ref_id: VarRefId,
        antecedent_flow_id: FlowId,
        subquery_condition_flow: InferConditionFlow,
        discriminant_decl_id: LuaDeclId,
        condition_position: rowan::TextSize,
        narrow: CorrelatedDiscriminantNarrow,
        fallback_expr: Option<LuaExpr>,
    },
}

#[derive(Debug, Clone)]
pub(in crate::semantic) enum CorrelatedDiscriminantNarrow {
    Truthiness,
    TypeGuard {
        narrow: LuaType,
    },
    Eq {
        right_expr_type: LuaType,
        allow_literal_equivalence: bool,
    },
}

#[derive(Debug, Clone)]
pub(in crate::semantic) enum ConditionFlowAction {
    Continue,
    Result(LuaType),
    Pending(PendingConditionNarrow),
    NeedSubquery(ConditionSubquery),
    NeedCorrelated(PendingCorrelatedCondition),
}

#[derive(Debug, Clone)]
pub(in crate::semantic) enum PendingConditionNarrow {
    Truthiness(InferConditionFlow),
    FieldTruthy {
        idx: LuaIndexMemberExpr,
        condition_flow: InferConditionFlow,
    },
    SameVarColonCall {
        idx: LuaIndexMemberExpr,
        condition_flow: InferConditionFlow,
    },
    SignatureCast {
        signature_id: LuaSignatureId,
        condition_flow: InferConditionFlow,
    },
    Eq {
        right_expr_type: LuaType,
        condition_flow: InferConditionFlow,
    },
    TypeGuard {
        narrow: LuaType,
        condition_flow: InferConditionFlow,
    },
    Correlated(Rc<CorrelatedConditionNarrowing>),
}

impl PendingConditionNarrow {
    pub(in crate::semantic::infer::narrow) fn apply(
        &self,
        db: &DbIndex,
        cache: &mut LuaInferCache,
        antecedent_type: LuaType,
    ) -> LuaType {
        match self {
            PendingConditionNarrow::Truthiness(condition_flow) => match condition_flow.clone() {
                InferConditionFlow::FalseCondition => narrow_false_or_nil(db, antecedent_type),
                InferConditionFlow::TrueCondition => remove_false_or_nil(antecedent_type),
            },
            PendingConditionNarrow::FieldTruthy {
                idx,
                condition_flow,
            } => {
                let LuaType::Union(union_type) = &antecedent_type else {
                    return antecedent_type;
                };

                let union_types = union_type.into_vec();
                let mut result = vec![];
                for sub_type in &union_types {
                    let member_type = match infer_member_by_member_key(
                        db,
                        cache,
                        sub_type,
                        idx.clone(),
                        &InferGuard::new(),
                    ) {
                        Ok(member_type) => member_type,
                        Err(_) => continue,
                    };

                    if !member_type.is_always_falsy() {
                        result.push(sub_type.clone());
                    }
                }

                if result.is_empty() {
                    antecedent_type
                } else {
                    match condition_flow.clone() {
                        InferConditionFlow::TrueCondition => LuaType::from_vec(result),
                        InferConditionFlow::FalseCondition => {
                            let target = LuaType::from_vec(result);
                            crate::TypeOps::Remove.apply(db, &antecedent_type, &target)
                        }
                    }
                }
            }
            PendingConditionNarrow::SameVarColonCall {
                idx,
                condition_flow,
            } => {
                let Ok(member_type) = infer_member_by_member_key(
                    db,
                    cache,
                    &antecedent_type,
                    idx.clone(),
                    &InferGuard::new(),
                ) else {
                    return antecedent_type;
                };

                let LuaType::Signature(signature_id) = member_type else {
                    return antecedent_type;
                };

                let Some(signature_cast) = db.get_flow_index().get_signature_cast(&signature_id)
                else {
                    return antecedent_type;
                };

                if signature_cast.name != "self" {
                    return antecedent_type;
                }

                apply_signature_cast(
                    db,
                    antecedent_type,
                    signature_id.clone(),
                    signature_cast,
                    condition_flow.clone(),
                )
            }
            PendingConditionNarrow::SignatureCast {
                signature_id,
                condition_flow,
            } => {
                let Some(signature_cast) = db.get_flow_index().get_signature_cast(&signature_id)
                else {
                    return antecedent_type;
                };

                apply_signature_cast(
                    db,
                    antecedent_type,
                    signature_id.clone(),
                    signature_cast,
                    condition_flow.clone(),
                )
            }
            PendingConditionNarrow::Eq {
                right_expr_type,
                condition_flow,
            } => match condition_flow.clone() {
                InferConditionFlow::TrueCondition => {
                    let maybe_type =
                        crate::TypeOps::Intersect.apply(db, &antecedent_type, right_expr_type);
                    if maybe_type.is_never() {
                        antecedent_type
                    } else {
                        maybe_type
                    }
                }
                InferConditionFlow::FalseCondition => {
                    crate::TypeOps::Remove.apply(db, &antecedent_type, right_expr_type)
                }
            },
            PendingConditionNarrow::TypeGuard {
                narrow,
                condition_flow,
            } => match condition_flow.clone() {
                InferConditionFlow::TrueCondition => {
                    narrow_down_type(db, antecedent_type, narrow.clone(), None)
                        .unwrap_or_else(|| narrow.clone())
                }
                InferConditionFlow::FalseCondition => {
                    crate::TypeOps::Remove.apply(db, &antecedent_type, narrow)
                }
            },
            PendingConditionNarrow::Correlated(correlated_narrowing) => {
                correlated_narrowing.apply(db, antecedent_type)
            }
        }
    }
}

fn apply_signature_cast(
    db: &DbIndex,
    antecedent_type: LuaType,
    signature_id: LuaSignatureId,
    signature_cast: &LuaSignatureCast,
    condition_flow: InferConditionFlow,
) -> LuaType {
    let file_id = signature_id.get_file_id();
    let Some(syntax_tree) = db.get_vfs().get_syntax_tree(&file_id) else {
        return antecedent_type;
    };
    let signature_root = syntax_tree.get_chunk_node();

    let (cast_ptr, cast_flow) = match condition_flow {
        InferConditionFlow::TrueCondition => (&signature_cast.cast, condition_flow),
        InferConditionFlow::FalseCondition => (
            signature_cast
                .fallback_cast
                .as_ref()
                .unwrap_or(&signature_cast.cast),
            signature_cast
                .fallback_cast
                .as_ref()
                .map(|_| InferConditionFlow::TrueCondition)
                .unwrap_or(condition_flow),
        ),
    };
    let Some(cast_op_type) = cast_ptr.to_node(&signature_root) else {
        return antecedent_type;
    };

    cast_type(
        db,
        file_id,
        cast_op_type,
        antecedent_type.clone(),
        cast_flow,
    )
    .unwrap_or(antecedent_type)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn get_type_at_condition_flow(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    condition: LuaExpr,
    condition_flow: InferConditionFlow,
) -> Result<ConditionFlowAction, InferFailReason> {
    let mut condition = condition;
    let mut condition_flow = condition_flow;

    loop {
        match condition {
            LuaExpr::NameExpr(name_expr) => {
                let Some(name_var_ref_id) =
                    get_var_expr_var_ref_id(db, cache, LuaExpr::NameExpr(name_expr.clone()))
                else {
                    return Ok(ConditionFlowAction::Continue);
                };

                if name_var_ref_id == *var_ref_id {
                    return Ok(ConditionFlowAction::Pending(
                        PendingConditionNarrow::Truthiness(condition_flow),
                    ));
                }

                let Some(decl_id) = db
                    .get_reference_index()
                    .get_var_reference_decl(&cache.get_file_id(), name_expr.get_range())
                else {
                    return Ok(ConditionFlowAction::Continue);
                };

                if let Some(target_decl_id) = var_ref_id.get_decl_id_ref()
                    && tree.has_decl_multi_return_refs(&decl_id)
                    && tree.has_decl_multi_return_refs(&target_decl_id)
                {
                    let antecedent_flow_id = get_single_antecedent(flow_node)?;
                    let fallback_expr = tree
                        .get_decl_ref_expr(&decl_id)
                        .and_then(|expr_ptr| expr_ptr.to_node(root));
                    return Ok(ConditionFlowAction::NeedSubquery(
                        ConditionSubquery::Correlated {
                            var_ref_id: VarRefId::VarRef(decl_id),
                            antecedent_flow_id,
                            subquery_condition_flow: condition_flow,
                            discriminant_decl_id: decl_id,
                            condition_position: name_expr.get_position(),
                            narrow: CorrelatedDiscriminantNarrow::Truthiness,
                            fallback_expr,
                        },
                    ));
                }

                let Some(expr_ptr) = tree.get_decl_ref_expr(&decl_id) else {
                    return Ok(ConditionFlowAction::Continue);
                };
                let Some(expr) = expr_ptr.to_node(root) else {
                    return Ok(ConditionFlowAction::Continue);
                };
                condition = expr;
                continue;
            }
            LuaExpr::CallExpr(call_expr) => {
                return get_type_at_call_expr(db, cache, var_ref_id, call_expr, condition_flow);
            }
            LuaExpr::IndexExpr(index_expr) => {
                return get_type_at_index_expr(db, cache, var_ref_id, index_expr, condition_flow);
            }
            LuaExpr::TableExpr(_) | LuaExpr::LiteralExpr(_) | LuaExpr::ClosureExpr(_) => {
                return Ok(ConditionFlowAction::Continue);
            }
            LuaExpr::BinaryExpr(binary_expr) => {
                return get_type_at_binary_expr(
                    db,
                    tree,
                    cache,
                    root,
                    var_ref_id,
                    flow_node,
                    binary_expr,
                    condition_flow,
                );
            }
            LuaExpr::UnaryExpr(unary_expr) => {
                let Some(inner_expr) = unary_expr.get_expr() else {
                    return Ok(ConditionFlowAction::Continue);
                };
                let Some(op) = unary_expr.get_op_token() else {
                    return Ok(ConditionFlowAction::Continue);
                };
                if op.get_op() != UnaryOperator::OpNot {
                    return Ok(ConditionFlowAction::Continue);
                }

                condition = inner_expr;
                condition_flow = match condition_flow {
                    InferConditionFlow::TrueCondition => InferConditionFlow::FalseCondition,
                    InferConditionFlow::FalseCondition => InferConditionFlow::TrueCondition,
                };
                continue;
            }
            LuaExpr::ParenExpr(paren_expr) => {
                let Some(inner_expr) = paren_expr.get_expr() else {
                    return Ok(ConditionFlowAction::Continue);
                };
                condition = inner_expr;
                continue;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::semantic::infer::narrow) fn resolve_condition_subquery(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    subquery: ConditionSubquery,
    antecedent_type: LuaType,
) -> Result<ConditionFlowAction, InferFailReason> {
    match subquery {
        ConditionSubquery::ArrayLen {
            subquery_condition_flow,
            right_expr_type,
            max_adjustment,
            ..
        } => match (&antecedent_type, &right_expr_type) {
            (
                LuaType::Array(array_type),
                LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i),
            ) if matches!(subquery_condition_flow, InferConditionFlow::TrueCondition) => {
                let new_array_type = LuaArrayType::new(
                    array_type.get_base().clone(),
                    LuaArrayLen::Max(*i + max_adjustment),
                );
                Ok(ConditionFlowAction::Result(LuaType::Array(
                    new_array_type.into(),
                )))
            }
            _ => Ok(ConditionFlowAction::Continue),
        },
        ConditionSubquery::FieldLiteralEq {
            subquery_condition_flow,
            idx,
            right_expr_type,
            ..
        } => {
            let LuaType::Union(union_type) = antecedent_type else {
                return Ok(ConditionFlowAction::Continue);
            };

            let mut opt_result = None;
            let mut union_types = union_type.into_vec();
            for (i, sub_type) in union_types.iter().enumerate() {
                let member_type = match infer_member_by_member_key(
                    db,
                    cache,
                    sub_type,
                    idx.clone(),
                    &InferGuard::new(),
                ) {
                    Ok(member_type) => member_type,
                    Err(_) => continue,
                };
                if always_literal_equal(&member_type, &right_expr_type) {
                    opt_result = Some(i);
                }
            }

            let action = match subquery_condition_flow {
                InferConditionFlow::TrueCondition => opt_result
                    .map(|i| ConditionFlowAction::Result(union_types[i].clone()))
                    .unwrap_or(ConditionFlowAction::Continue),
                InferConditionFlow::FalseCondition => opt_result
                    .map(|i| {
                        union_types.remove(i);
                        ConditionFlowAction::Result(LuaType::from_vec(union_types))
                    })
                    .unwrap_or(ConditionFlowAction::Continue),
            };
            Ok(action)
        }
        ConditionSubquery::Correlated {
            antecedent_flow_id,
            subquery_condition_flow,
            discriminant_decl_id,
            condition_position,
            narrow,
            fallback_expr,
            ..
        } => {
            let narrowed_discriminant_type = match narrow {
                CorrelatedDiscriminantNarrow::Truthiness => match subquery_condition_flow {
                    InferConditionFlow::FalseCondition => narrow_false_or_nil(db, antecedent_type),
                    InferConditionFlow::TrueCondition => remove_false_or_nil(antecedent_type),
                },
                CorrelatedDiscriminantNarrow::TypeGuard { narrow } => match subquery_condition_flow
                {
                    InferConditionFlow::TrueCondition => {
                        narrow_down_type(db, antecedent_type, narrow.clone(), None)
                            .unwrap_or(narrow)
                    }
                    InferConditionFlow::FalseCondition => {
                        TypeOps::Remove.apply(db, &antecedent_type, &narrow)
                    }
                },
                CorrelatedDiscriminantNarrow::Eq {
                    right_expr_type,
                    allow_literal_equivalence,
                } => narrow_eq_condition(
                    db,
                    antecedent_type,
                    right_expr_type,
                    subquery_condition_flow,
                    allow_literal_equivalence,
                ),
            };

            let action = prepare_var_from_return_overload_condition(
                db,
                tree,
                cache,
                root,
                var_ref_id,
                discriminant_decl_id,
                condition_position,
                antecedent_flow_id,
                &narrowed_discriminant_type,
            )?;

            let Some(fallback_expr) = fallback_expr else {
                return Ok(action);
            };

            if !matches!(action, ConditionFlowAction::Continue) {
                return Ok(action);
            }

            get_type_at_condition_flow(
                db,
                tree,
                cache,
                root,
                var_ref_id,
                flow_node,
                fallback_expr,
                subquery_condition_flow,
            )
        }
    }
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
