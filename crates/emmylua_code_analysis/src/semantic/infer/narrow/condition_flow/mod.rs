mod binary_flow;
mod call_flow;
pub(in crate::semantic::infer::narrow) mod correlated_flow;
mod index_flow;

use self::{
    binary_flow::get_type_at_binary_expr,
    correlated_flow::{CorrelatedConditionNarrowing, prepare_var_from_return_overload_condition},
};
use emmylua_parser::{
    LuaAstNode, LuaChunk, LuaExpr, LuaIndexMemberExpr, LuaNameExpr, LuaUnaryExpr, UnaryOperator,
};

use crate::{
    DbIndex, FlowNode, FlowTree, InferFailReason, InferGuard, LuaInferCache, LuaSignatureCast,
    LuaSignatureId, LuaType,
    semantic::infer::{
        VarRefId,
        infer_index::infer_member_by_member_key,
        narrow::{
            ResultTypeOrContinue,
            condition_flow::{
                call_flow::get_type_at_call_expr, index_flow::get_type_at_index_expr,
            },
            get_single_antecedent,
            get_type_at_cast_flow::cast_type,
            get_type_at_flow::get_type_at_flow,
            narrow_down_type, narrow_false_or_nil, remove_false_or_nil,
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

#[derive(Debug, Clone)]
pub(in crate::semantic) enum ConditionFlowAction {
    Continue,
    Result(LuaType),
    Pending(PendingConditionNarrow),
}

impl From<ResultTypeOrContinue> for ConditionFlowAction {
    fn from(result_or_continue: ResultTypeOrContinue) -> Self {
        match result_or_continue {
            ResultTypeOrContinue::Continue => ConditionFlowAction::Continue,
            ResultTypeOrContinue::Result(result_type) => ConditionFlowAction::Result(result_type),
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::semantic) enum PendingConditionNarrow {
    Truthiness(InferConditionFlow),
    FieldTruthy {
        index: LuaIndexMemberExpr,
        condition_flow: InferConditionFlow,
    },
    SameVarColonCall {
        index: LuaIndexMemberExpr,
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
    Correlated(CorrelatedConditionNarrowing),
}

impl PendingConditionNarrow {
    pub(in crate::semantic::infer::narrow) fn apply(
        self,
        db: &DbIndex,
        cache: &mut LuaInferCache,
        antecedent_type: LuaType,
    ) -> LuaType {
        match self {
            PendingConditionNarrow::Truthiness(condition_flow) => match condition_flow {
                InferConditionFlow::FalseCondition => narrow_false_or_nil(db, antecedent_type),
                InferConditionFlow::TrueCondition => remove_false_or_nil(antecedent_type),
            },
            PendingConditionNarrow::FieldTruthy {
                index,
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
                        index.clone(),
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
                    match condition_flow {
                        InferConditionFlow::TrueCondition => LuaType::from_vec(result),
                        InferConditionFlow::FalseCondition => {
                            let target = LuaType::from_vec(result);
                            crate::TypeOps::Remove.apply(db, &antecedent_type, &target)
                        }
                    }
                }
            }
            PendingConditionNarrow::SameVarColonCall {
                index,
                condition_flow,
            } => {
                let Ok(member_type) = infer_member_by_member_key(
                    db,
                    cache,
                    &antecedent_type,
                    index,
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
                    signature_id,
                    signature_cast,
                    condition_flow,
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
                    signature_id,
                    signature_cast,
                    condition_flow,
                )
            }
            PendingConditionNarrow::Eq {
                right_expr_type,
                condition_flow,
            } => match condition_flow {
                InferConditionFlow::TrueCondition => {
                    let maybe_type =
                        crate::TypeOps::Intersect.apply(db, &antecedent_type, &right_expr_type);
                    if maybe_type.is_never() {
                        antecedent_type
                    } else {
                        maybe_type
                    }
                }
                InferConditionFlow::FalseCondition => {
                    crate::TypeOps::Remove.apply(db, &antecedent_type, &right_expr_type)
                }
            },
            PendingConditionNarrow::TypeGuard {
                narrow,
                condition_flow,
            } => match condition_flow {
                InferConditionFlow::TrueCondition => {
                    narrow_down_type(db, antecedent_type, narrow.clone(), None).unwrap_or(narrow)
                }
                InferConditionFlow::FalseCondition => {
                    crate::TypeOps::Remove.apply(db, &antecedent_type, &narrow)
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
pub fn get_type_at_condition_flow(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    condition: LuaExpr,
    condition_flow: InferConditionFlow,
) -> Result<ConditionFlowAction, InferFailReason> {
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
        LuaExpr::CallExpr(call_expr) => {
            get_type_at_call_expr(db, cache, var_ref_id, call_expr, condition_flow)
        }
        LuaExpr::IndexExpr(index_expr) => {
            get_type_at_index_expr(db, cache, var_ref_id, index_expr, condition_flow)
        }
        LuaExpr::TableExpr(_) | LuaExpr::LiteralExpr(_) | LuaExpr::ClosureExpr(_) => {
            Ok(ConditionFlowAction::Continue)
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
                return Ok(ConditionFlowAction::Continue);
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
) -> Result<ConditionFlowAction, InferFailReason> {
    let Some(name_var_ref_id) =
        get_var_expr_var_ref_id(db, cache, LuaExpr::NameExpr(name_expr.clone()))
    else {
        return Ok(ConditionFlowAction::Continue);
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

    Ok(ConditionFlowAction::Pending(
        PendingConditionNarrow::Truthiness(condition_flow),
    ))
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
) -> Result<ConditionFlowAction, InferFailReason> {
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
        let antecedent_discriminant_type = get_type_at_flow(
            db,
            tree,
            cache,
            root,
            &VarRefId::VarRef(decl_id),
            antecedent_flow_id,
        )?;
        let narrowed_discriminant_type = match condition_flow {
            InferConditionFlow::FalseCondition => {
                narrow_false_or_nil(db, antecedent_discriminant_type)
            }
            InferConditionFlow::TrueCondition => remove_false_or_nil(antecedent_discriminant_type),
        };

        if let Some(correlated_narrowing) = prepare_var_from_return_overload_condition(
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
            return Ok(ConditionFlowAction::Pending(
                PendingConditionNarrow::Correlated(correlated_narrowing),
            ));
        }
    }

    let Some(expr_ptr) = tree.get_decl_ref_expr(&decl_id) else {
        return Ok(ConditionFlowAction::Continue);
    };

    let Some(expr) = expr_ptr.to_node(root) else {
        return Ok(ConditionFlowAction::Continue);
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
) -> Result<ConditionFlowAction, InferFailReason> {
    let Some(inner_expr) = unary_expr.get_expr() else {
        return Ok(ConditionFlowAction::Continue);
    };

    let Some(op) = unary_expr.get_op_token() else {
        return Ok(ConditionFlowAction::Continue);
    };

    match op.get_op() {
        UnaryOperator::OpNot => {}
        _ => {
            return Ok(ConditionFlowAction::Continue);
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
