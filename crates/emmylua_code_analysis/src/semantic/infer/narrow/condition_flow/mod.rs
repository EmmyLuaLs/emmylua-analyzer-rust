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
use emmylua_parser::{
    LuaAstNode, LuaCallExpr, LuaChunk, LuaExpr, LuaIndexMemberExpr, UnaryOperator,
};

use crate::{
    DbIndex, FlowId, FlowNode, FlowTree, InferFailReason, InferGuard, LuaArrayLen, LuaArrayType,
    LuaDeclId, LuaInferCache, LuaSignatureCast, LuaSignatureId, LuaType, TypeOps,
    semantic::infer::{
        InferResult, VarRefId,
        infer_index::{infer_member_by_key_type, infer_member_by_member_key},
        narrow::{
            condition_flow::{
                call_flow::{
                    get_type_at_call_expr, get_type_at_call_expr_by_func,
                    needs_deferred_receiver_method_lookup,
                },
                index_flow::get_type_at_index_expr,
            },
            get_single_antecedent,
            get_type_at_cast_flow::cast_type,
            narrow_down_type, narrow_false_or_nil, remove_false_or_nil,
            var_ref_id::get_var_expr_var_ref_id,
        },
    },
    semantic::type_check::is_sub_type_of,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InferConditionFlow {
    TrueCondition,
    FalseCondition,
}

impl InferConditionFlow {
    fn invert(self) -> Self {
        match self {
            Self::TrueCondition => Self::FalseCondition,
            Self::FalseCondition => Self::TrueCondition,
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::semantic) enum ExprTypeContinuation {
    Call {
        call_expr: LuaCallExpr,
        condition_flow: InferConditionFlow,
    },
    ReceiverMethodCall {
        idx: LuaIndexMemberExpr,
        call_expr: LuaCallExpr,
        condition_flow: InferConditionFlow,
    },
    ArrayLen {
        subquery_condition_flow: InferConditionFlow,
        max_adjustment: i64,
    },
    CorrelatedEq {
        var_ref_id: VarRefId,
        subquery_condition_flow: InferConditionFlow,
        discriminant_decl_id: LuaDeclId,
        condition_position: rowan::TextSize,
        allow_literal_equivalence: bool,
    },
    Eq {
        condition_flow: InferConditionFlow,
        true_result_is_rhs: bool,
    },
}

#[derive(Debug, Clone)]
pub(in crate::semantic) struct CorrelatedSubquery {
    var_ref_id: VarRefId,
    antecedent_flow_id: FlowId,
    subquery_condition_flow: InferConditionFlow,
    discriminant_decl_id: LuaDeclId,
    condition_position: rowan::TextSize,
    narrow: CorrelatedDiscriminantNarrow,
    fallback_expr: Option<LuaExpr>,
}

#[derive(Debug, Clone)]
pub(in crate::semantic) struct FieldLiteralSiblingSubquery {
    var_ref_id: VarRefId,
    discriminant_prefix_var_ref_id: VarRefId,
    antecedent_flow_id: FlowId,
    condition_flow: InferConditionFlow,
    idx: LuaIndexMemberExpr,
    right_expr_type: LuaType,
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
    NeedExprType {
        flow_id: FlowId,
        expr: LuaExpr,
        resume: ExprTypeContinuation,
    },
    NeedSubquery(CorrelatedSubquery),
    NeedFieldLiteralSibling(FieldLiteralSiblingSubquery),
    NeedCorrelated(PendingCorrelatedCondition),
}

#[derive(Debug, Clone)]
pub(in crate::semantic) enum PendingConditionNarrow {
    Truthiness(InferConditionFlow),
    ReceiverMethodCall {
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
    Field {
        idx: LuaIndexMemberExpr,
        key_type: Option<LuaType>,
        condition_flow: InferConditionFlow,
        kind: FieldConditionKind,
    },
    ArrayLen {
        right_expr_type: LuaType,
        condition_flow: InferConditionFlow,
        max_adjustment: i64,
    },
    TypeGuard {
        narrow: LuaType,
        condition_flow: InferConditionFlow,
    },
    NarrowTo(LuaType),
    Correlated(Rc<CorrelatedConditionNarrowing>),
}

#[derive(Debug, Clone)]
pub(in crate::semantic) enum FieldConditionKind {
    Truthy,
    LiteralEq { right_expr_type: LuaType },
}

impl PendingConditionNarrow {
    pub(in crate::semantic::infer::narrow) fn apply(
        &self,
        db: &DbIndex,
        cache: &mut LuaInferCache,
        antecedent_type: LuaType,
    ) -> LuaType {
        match self {
            PendingConditionNarrow::Truthiness(condition_flow) => match *condition_flow {
                InferConditionFlow::FalseCondition => narrow_false_or_nil(db, antecedent_type),
                InferConditionFlow::TrueCondition => remove_false_or_nil(antecedent_type),
            },
            PendingConditionNarrow::ReceiverMethodCall {
                idx,
                condition_flow,
            } => {
                let Ok(member_type) = cache.with_no_flow(|cache| {
                    infer_member_by_member_key(
                        db,
                        cache,
                        &antecedent_type,
                        idx.clone(),
                        &InferGuard::new(),
                    )
                }) else {
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
                    *condition_flow,
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
                    *condition_flow,
                )
            }
            PendingConditionNarrow::Eq {
                right_expr_type,
                condition_flow,
            } => narrow_eq_condition(
                db,
                antecedent_type,
                right_expr_type.clone(),
                *condition_flow,
                false,
            ),
            PendingConditionNarrow::Field {
                idx,
                key_type,
                condition_flow,
                kind,
            } => match kind {
                FieldConditionKind::Truthy => {
                    let narrowed = narrow_field_truthy(
                        db,
                        cache,
                        antecedent_type.clone(),
                        idx,
                        key_type.as_ref(),
                    );

                    match narrowed {
                        Some(truthy_type) => apply_field_truthy_condition(
                            db,
                            antecedent_type,
                            truthy_type,
                            *condition_flow,
                        ),
                        None => antecedent_type,
                    }
                }
                FieldConditionKind::LiteralEq { right_expr_type } => narrow_field_literal_eq(
                    db,
                    cache,
                    antecedent_type.clone(),
                    idx,
                    key_type.as_ref(),
                    right_expr_type,
                    *condition_flow,
                )
                .unwrap_or(antecedent_type),
            },
            PendingConditionNarrow::ArrayLen {
                right_expr_type,
                condition_flow,
                max_adjustment,
            } => match (&antecedent_type, right_expr_type) {
                (
                    LuaType::Array(array_type),
                    LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i),
                ) if matches!(condition_flow, InferConditionFlow::TrueCondition) => {
                    let new_array_type = LuaArrayType::new(
                        array_type.get_base().clone(),
                        LuaArrayLen::Max(*i + *max_adjustment),
                    );
                    LuaType::Array(new_array_type.into())
                }
                _ => antecedent_type,
            },
            PendingConditionNarrow::TypeGuard {
                narrow,
                condition_flow,
            } => match *condition_flow {
                InferConditionFlow::TrueCondition => {
                    narrow_type_guard(db, antecedent_type, narrow.clone())
                        .unwrap_or_else(|| narrow.clone())
                }
                InferConditionFlow::FalseCondition => {
                    TypeOps::Remove.apply(db, &antecedent_type, narrow)
                }
            },
            PendingConditionNarrow::NarrowTo(target_type) => {
                narrow_down_type(db, antecedent_type.clone(), target_type.clone(), None)
                    .unwrap_or(antecedent_type)
            }
            PendingConditionNarrow::Correlated(correlated_narrowing) => {
                correlated_narrowing.apply(db, antecedent_type)
            }
        }
    }
}

fn narrow_type_guard(db: &DbIndex, antecedent_type: LuaType, narrow: LuaType) -> Option<LuaType> {
    if antecedent_type == narrow {
        return Some(antecedent_type);
    }

    match (&antecedent_type, &narrow) {
        (
            LuaType::Def(source_id) | LuaType::Ref(source_id),
            LuaType::Def(target_id) | LuaType::Ref(target_id),
        ) => {
            if is_sub_type_of(db, source_id, target_id) {
                return Some(antecedent_type);
            }
            if is_sub_type_of(db, target_id, source_id) {
                return Some(narrow);
            }
        }
        (LuaType::Union(source_union), _) => {
            let narrowed = source_union
                .into_vec()
                .into_iter()
                .filter_map(|member| narrow_type_guard(db, member, narrow.clone()))
                .collect::<Vec<_>>();
            return (!narrowed.is_empty()).then_some(LuaType::from_vec(narrowed));
        }
        (LuaType::MultiLineUnion(source_union), _) => {
            let narrowed = source_union
                .get_unions()
                .iter()
                .filter_map(|(member, _)| narrow_type_guard(db, member.clone(), narrow.clone()))
                .collect::<Vec<_>>();
            return (!narrowed.is_empty()).then_some(LuaType::from_vec(narrowed));
        }
        (_, LuaType::Union(target_union)) => {
            let narrowed = target_union
                .into_vec()
                .into_iter()
                .filter_map(|target| narrow_type_guard(db, antecedent_type.clone(), target))
                .collect::<Vec<_>>();
            return (!narrowed.is_empty()).then_some(LuaType::from_vec(narrowed));
        }
        (_, LuaType::MultiLineUnion(target_union)) => {
            let narrowed = target_union
                .get_unions()
                .iter()
                .filter_map(|(target, _)| {
                    narrow_type_guard(db, antecedent_type.clone(), target.clone())
                })
                .collect::<Vec<_>>();
            return (!narrowed.is_empty()).then_some(LuaType::from_vec(narrowed));
        }
        _ => {}
    }

    narrow_down_type(db, antecedent_type, narrow, None)
}

pub(super) fn eq_condition_action(
    db: &DbIndex,
    var_ref_id: &VarRefId,
    right_expr_type: LuaType,
    condition_flow: InferConditionFlow,
    true_result_is_rhs: bool,
) -> ConditionFlowAction {
    if matches!(condition_flow, InferConditionFlow::FalseCondition) {
        return ConditionFlowAction::Pending(PendingConditionNarrow::Eq {
            right_expr_type,
            condition_flow,
        });
    }

    if true_result_is_rhs {
        return ConditionFlowAction::Result(right_expr_type);
    }

    // self is special; drop nil directly instead of replaying a normal equality narrow.
    if var_ref_id.is_self_ref() && !right_expr_type.is_nil() {
        return ConditionFlowAction::Result(TypeOps::Remove.apply(
            db,
            &right_expr_type,
            &LuaType::Nil,
        ));
    }

    ConditionFlowAction::Pending(PendingConditionNarrow::Eq {
        right_expr_type,
        condition_flow,
    })
}

fn narrow_field_truthy(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    antecedent_type: LuaType,
    idx: &LuaIndexMemberExpr,
    key_type: Option<&LuaType>,
) -> Option<LuaType> {
    let LuaType::Union(union_type) = &antecedent_type else {
        return None;
    };

    let union_types = union_type.into_vec();
    let mut result = vec![];
    for sub_type in &union_types {
        let member_type = match infer_pending_field_member(db, cache, &sub_type, idx, key_type) {
            Ok(member_type) => member_type,
            Err(_) => continue,
        };

        if !member_type.is_always_falsy() {
            result.push(sub_type.clone());
        }
    }

    (!result.is_empty()).then(|| LuaType::from_vec(result))
}

fn infer_pending_field_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type: &LuaType,
    idx: &LuaIndexMemberExpr,
    key_type: Option<&LuaType>,
) -> InferResult {
    cache.with_no_flow(|cache| {
        if let Some(key_type) = key_type {
            infer_member_by_key_type(
                db,
                cache,
                prefix_type,
                idx.clone(),
                key_type,
                &InferGuard::new(),
            )
        } else {
            infer_member_by_member_key(db, cache, prefix_type, idx.clone(), &InferGuard::new())
        }
    })
}

fn apply_field_truthy_condition(
    db: &DbIndex,
    antecedent_type: LuaType,
    truthy_type: LuaType,
    condition_flow: InferConditionFlow,
) -> LuaType {
    match condition_flow {
        InferConditionFlow::TrueCondition => truthy_type,
        InferConditionFlow::FalseCondition => {
            TypeOps::Remove.apply(db, &antecedent_type, &truthy_type)
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
                    return Ok(ConditionFlowAction::NeedSubquery(CorrelatedSubquery {
                        var_ref_id: VarRefId::VarRef(decl_id),
                        antecedent_flow_id,
                        subquery_condition_flow: condition_flow,
                        discriminant_decl_id: decl_id,
                        condition_position: name_expr.get_position(),
                        narrow: CorrelatedDiscriminantNarrow::Truthiness,
                        fallback_expr,
                    }));
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
                return get_type_at_call_expr(
                    db,
                    cache,
                    var_ref_id,
                    flow_node,
                    call_expr,
                    condition_flow,
                );
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
                condition_flow = condition_flow.invert();
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

struct CorrelatedSubqueryCtx<'a> {
    db: &'a DbIndex,
    tree: &'a FlowTree,
    cache: &'a mut LuaInferCache,
    root: &'a LuaChunk,
    var_ref_id: &'a VarRefId,
    flow_node: &'a FlowNode,
}

pub(in crate::semantic::infer::narrow) fn resolve_correlated_subquery(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    subquery: CorrelatedSubquery,
    antecedent_result: InferResult,
) -> Result<ConditionFlowAction, InferFailReason> {
    let mut ctx = CorrelatedSubqueryCtx {
        db,
        tree,
        cache,
        root,
        var_ref_id,
        flow_node,
    };

    subquery.resolve(&mut ctx, antecedent_result)
}

pub(in crate::semantic::infer::narrow) fn resolve_expr_type_continuation(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    var_ref_id: &VarRefId,
    antecedent_flow_id: FlowId,
    resume: ExprTypeContinuation,
    expr_type: LuaType,
) -> Result<ConditionFlowAction, InferFailReason> {
    match resume {
        ExprTypeContinuation::Call {
            call_expr,
            condition_flow,
        } => get_type_at_call_expr_by_func(
            db,
            cache,
            var_ref_id,
            call_expr,
            expr_type,
            condition_flow,
        ),
        ExprTypeContinuation::ReceiverMethodCall {
            idx,
            call_expr,
            condition_flow,
        } => resolve_receiver_method_call(
            db,
            cache,
            var_ref_id,
            expr_type,
            idx,
            call_expr,
            condition_flow,
        ),
        ExprTypeContinuation::ArrayLen {
            subquery_condition_flow,
            max_adjustment,
        } => Ok(ConditionFlowAction::Pending(
            PendingConditionNarrow::ArrayLen {
                right_expr_type: expr_type,
                condition_flow: subquery_condition_flow,
                max_adjustment,
            },
        )),
        ExprTypeContinuation::CorrelatedEq {
            var_ref_id,
            subquery_condition_flow,
            discriminant_decl_id,
            condition_position,
            allow_literal_equivalence,
        } => Ok(ConditionFlowAction::NeedSubquery(CorrelatedSubquery {
            var_ref_id,
            antecedent_flow_id,
            subquery_condition_flow,
            discriminant_decl_id,
            condition_position,
            narrow: CorrelatedDiscriminantNarrow::Eq {
                right_expr_type: expr_type,
                allow_literal_equivalence,
            },
            fallback_expr: None,
        })),
        ExprTypeContinuation::Eq {
            condition_flow,
            true_result_is_rhs,
        } => Ok(eq_condition_action(
            db,
            var_ref_id,
            expr_type,
            condition_flow,
            true_result_is_rhs,
        )),
    }
}

fn resolve_receiver_method_call(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    var_ref_id: &VarRefId,
    receiver_type: LuaType,
    idx: LuaIndexMemberExpr,
    call_expr: LuaCallExpr,
    condition_flow: InferConditionFlow,
) -> Result<ConditionFlowAction, InferFailReason> {
    let member_type = match cache.with_no_flow(|cache| {
        infer_member_by_member_key(db, cache, &receiver_type, idx.clone(), &InferGuard::new())
    }) {
        Ok(member_type) => member_type,
        Err(_) => return Ok(ConditionFlowAction::Continue),
    };

    if needs_deferred_receiver_method_lookup(&member_type) {
        return Ok(ConditionFlowAction::Pending(
            PendingConditionNarrow::ReceiverMethodCall {
                idx,
                condition_flow,
            },
        ));
    }

    get_type_at_call_expr_by_func(
        db,
        cache,
        var_ref_id,
        call_expr,
        member_type,
        condition_flow,
    )
}

fn narrow_field_literal_eq(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    antecedent_type: LuaType,
    idx: &LuaIndexMemberExpr,
    key_type: Option<&LuaType>,
    right_expr_type: &LuaType,
    condition_flow: InferConditionFlow,
) -> Option<LuaType> {
    let LuaType::Union(union_type) = antecedent_type else {
        return None;
    };

    let mut matched = Vec::new();
    let mut unmatched = Vec::new();
    let mut has_matched = false;
    for sub_type in union_type.into_vec() {
        let member_type = match infer_pending_field_member(db, cache, &sub_type, idx, key_type) {
            Ok(member_type) => member_type,
            Err(_) => {
                unmatched.push(sub_type);
                continue;
            }
        };
        if always_literal_equal(&member_type, right_expr_type) {
            has_matched = true;
            matched.push(sub_type.clone());
        } else {
            unmatched.push(sub_type.clone());
        }
    }

    let result = match condition_flow {
        InferConditionFlow::TrueCondition => matched,
        InferConditionFlow::FalseCondition => unmatched,
    };
    if !has_matched {
        None
    } else if result.is_empty() {
        Some(LuaType::Never)
    } else {
        Some(LuaType::from_vec(result))
    }
}

impl FieldLiteralSiblingSubquery {
    pub(in crate::semantic::infer::narrow) fn next_flow_query(&self) -> (&VarRefId, FlowId) {
        (
            &self.discriminant_prefix_var_ref_id,
            self.antecedent_flow_id,
        )
    }

    pub(in crate::semantic::infer::narrow) fn resolve(
        self,
        db: &DbIndex,
        cache: &mut LuaInferCache,
        antecedent_type: LuaType,
    ) -> Result<ConditionFlowAction, InferFailReason> {
        let Some(narrowed_prefix_type) = narrow_field_literal_eq(
            db,
            cache,
            antecedent_type,
            &self.idx,
            None,
            &self.right_expr_type,
            self.condition_flow,
        ) else {
            return Ok(ConditionFlowAction::Continue);
        };

        let Some(projected_type) = project_relative_member_type(
            db,
            &narrowed_prefix_type,
            &self.var_ref_id,
            &self.discriminant_prefix_var_ref_id,
        )?
        else {
            return Ok(ConditionFlowAction::Continue);
        };

        Ok(ConditionFlowAction::Pending(
            PendingConditionNarrow::NarrowTo(projected_type),
        ))
    }
}

fn project_relative_member_type(
    db: &DbIndex,
    prefix_type: &LuaType,
    field_ref_id: &VarRefId,
    prefix_ref_id: &VarRefId,
) -> Result<Option<LuaType>, InferFailReason> {
    let Some(path) = field_ref_id.relative_index_path(prefix_ref_id) else {
        return Ok(None);
    };
    if path.is_empty() {
        return Ok(Some(prefix_type.clone()));
    }
    if prefix_type.is_never() {
        return Ok(Some(LuaType::Never));
    }

    let mut current_type = prefix_type.clone();
    for member_key in path {
        let Some(projected_type) = project_member_type(db, &current_type, member_key)? else {
            return Ok(None);
        };
        current_type = projected_type;
    }
    Ok(Some(current_type))
}

fn project_member_type(
    db: &DbIndex,
    prefix_type: &LuaType,
    member_key: crate::LuaMemberKey,
) -> Result<Option<LuaType>, InferFailReason> {
    match prefix_type {
        LuaType::Never => return Ok(Some(LuaType::Never)),
        LuaType::Nil => return Ok(None),
        LuaType::Union(union_type) => {
            return project_union_member_type(db, union_type.into_vec(), member_key);
        }
        LuaType::MultiLineUnion(multi_union) => {
            let union_type = multi_union.to_union();
            if let LuaType::Union(union_type) = union_type {
                return project_union_member_type(db, union_type.into_vec(), member_key);
            }
        }
        _ => {}
    }

    let Some(members) =
        crate::semantic::member::find_members_with_key(db, prefix_type, member_key, true)
    else {
        return Ok(None);
    };
    if members.is_empty() {
        return Ok(None);
    }

    Ok(Some(LuaType::from_vec(
        members.into_iter().map(|member| member.typ).collect(),
    )))
}

fn project_union_member_type(
    db: &DbIndex,
    union_types: Vec<LuaType>,
    member_key: crate::LuaMemberKey,
) -> Result<Option<LuaType>, InferFailReason> {
    let mut result_type = LuaType::Never;
    let mut has_member = false;
    let mut has_missing_member = false;

    for sub_type in union_types {
        match project_member_type(db, &sub_type, member_key.clone())? {
            Some(member_type) => {
                has_member = true;
                result_type = TypeOps::Union.apply(db, &result_type, &member_type);
            }
            None => {
                has_missing_member = true;
            }
        }
    }

    if !has_member {
        return Ok(None);
    }
    if has_missing_member {
        result_type = TypeOps::Union.apply(db, &result_type, &LuaType::Nil);
    }
    Ok(Some(result_type))
}

impl CorrelatedSubquery {
    pub(in crate::semantic::infer::narrow) fn next_flow_query(&self) -> (&VarRefId, FlowId) {
        (&self.var_ref_id, self.antecedent_flow_id)
    }

    fn resolve(
        self,
        ctx: &mut CorrelatedSubqueryCtx<'_>,
        antecedent_result: InferResult,
    ) -> Result<ConditionFlowAction, InferFailReason> {
        let correlated = self;
        let antecedent_type = antecedent_result?;
        let narrowed_discriminant_type = match correlated.narrow {
            CorrelatedDiscriminantNarrow::Truthiness => match correlated.subquery_condition_flow {
                InferConditionFlow::FalseCondition => narrow_false_or_nil(ctx.db, antecedent_type),
                InferConditionFlow::TrueCondition => remove_false_or_nil(antecedent_type),
            },
            CorrelatedDiscriminantNarrow::TypeGuard { narrow } => {
                match correlated.subquery_condition_flow {
                    InferConditionFlow::TrueCondition => {
                        narrow_down_type(ctx.db, antecedent_type, narrow.clone(), None)
                            .unwrap_or(narrow)
                    }
                    InferConditionFlow::FalseCondition => {
                        TypeOps::Remove.apply(ctx.db, &antecedent_type, &narrow)
                    }
                }
            }
            CorrelatedDiscriminantNarrow::Eq {
                right_expr_type,
                allow_literal_equivalence,
            } => narrow_eq_condition(
                ctx.db,
                antecedent_type,
                right_expr_type,
                correlated.subquery_condition_flow,
                allow_literal_equivalence,
            ),
        };

        let action = prepare_var_from_return_overload_condition(
            ctx.db,
            ctx.tree,
            &mut *ctx.cache,
            ctx.root,
            ctx.var_ref_id,
            correlated.discriminant_decl_id,
            correlated.condition_position,
            correlated.antecedent_flow_id,
            &narrowed_discriminant_type,
        )?;

        let Some(fallback_expr) = correlated.fallback_expr else {
            return Ok(action);
        };

        if !matches!(action, ConditionFlowAction::Continue) {
            return Ok(action);
        }

        get_type_at_condition_flow(
            ctx.db,
            ctx.tree,
            &mut *ctx.cache,
            ctx.root,
            ctx.var_ref_id,
            ctx.flow_node,
            fallback_expr,
            correlated.subquery_condition_flow,
        )
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
