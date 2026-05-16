use std::{ops::Deref, sync::Arc};

use hashbrown::{HashMap, HashSet};
use rowan::TextRange;

use crate::{
    DbIndex, GenericParam, GenericTpl, InFiled, LuaArrayType, LuaConditionalType, LuaFunctionType,
    LuaGenericType, LuaMappedType, LuaMemberKey, LuaMemberOwner, LuaObjectType, LuaTupleType,
    LuaType, LuaUnionType, TypeOps, TypeSubstitutor, VariadicType, instantiate_type_generic,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::semantic::generic) enum TplCandidateSource {
    Plain,
    ConstPreserving,
    Finalized,
}

pub(in crate::semantic::generic) fn finalize_inferred_tpl_candidate(
    db: &DbIndex,
    tpl: &GenericTpl,
    raw_candidate: &LuaType,
    candidate_source: TplCandidateSource,
    top_level: bool,
    return_top_level: bool,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    if candidate_source == TplCandidateSource::ConstPreserving {
        return raw_candidate.clone();
    }

    let primitive_constraint = tpl
        .get_constraint()
        .map(|constraint| {
            let constraint = instantiate_type_generic(db, constraint, substitutor);
            is_primitive_or_literal_type(&constraint)
        })
        .unwrap_or(false);
    let candidate = if primitive_constraint || !top_level || return_top_level {
        raw_candidate.clone()
    } else {
        widen_literal_type(raw_candidate.clone())
    };
    finalize_tpl_candidate_type(
        db,
        candidate,
        WideningContext::Root,
        &mut WideningGuard::default(),
    )
}

fn is_primitive_or_literal_type(ty: &LuaType) -> bool {
    match ty {
        LuaType::String
        | LuaType::Number
        | LuaType::Integer
        | LuaType::Boolean
        | LuaType::StringConst(_)
        | LuaType::DocStringConst(_)
        | LuaType::IntegerConst(_)
        | LuaType::DocIntegerConst(_)
        | LuaType::FloatConst(_)
        | LuaType::BooleanConst(_)
        | LuaType::DocBooleanConst(_) => true,
        LuaType::Tuple(tuple) => tuple.get_types().iter().any(is_primitive_or_literal_type),
        LuaType::Union(union) => union.into_vec().iter().any(is_primitive_or_literal_type),
        LuaType::MultiLineUnion(union) => union
            .get_unions()
            .iter()
            .any(|(ty, _)| is_primitive_or_literal_type(ty)),
        LuaType::Variadic(variadic) => match variadic.deref() {
            VariadicType::Base(base) => is_primitive_or_literal_type(base),
            VariadicType::Multi(types) => types.iter().any(is_primitive_or_literal_type),
        },
        LuaType::Call(call) => call.get_operands().iter().any(is_primitive_or_literal_type),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WideningContext {
    Root,
    UnionMember,
    ObjectProperty,
    ArrayElement,
    TupleElement,
    VariadicElement,
}

const MAX_WIDENING_DEPTH: u16 = 100;

#[derive(Default)]
pub struct WideningGuard {
    depth: u16,
    active_table_ids: HashSet<InFiled<TextRange>>,
}

impl WideningGuard {
    fn enter_level(&mut self) -> bool {
        if self.depth >= MAX_WIDENING_DEPTH {
            return false;
        }
        self.depth += 1;
        true
    }

    fn leave_level(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn enter_table(&mut self, table_id: &InFiled<TextRange>) -> bool {
        self.active_table_ids.insert(table_id.clone())
    }

    fn leave_table(&mut self, table_id: &InFiled<TextRange>) {
        self.active_table_ids.remove(table_id);
    }
}

fn finalize_tpl_candidate_type(
    db: &DbIndex,
    ty: LuaType,
    context: WideningContext,
    guard: &mut WideningGuard,
) -> LuaType {
    if !guard.enter_level() {
        return match ty {
            LuaType::TableConst(_) => LuaType::Table,
            ty => widen_literals_with_context(ty, context),
        };
    }

    let widened = match ty {
        LuaType::TableConst(table_id) => {
            table_const_to_object(db, table_id, guard).unwrap_or(LuaType::Table)
        }
        LuaType::Object(object) => {
            let fields = object
                .get_fields()
                .iter()
                .map(|(key, ty)| {
                    (
                        key.clone(),
                        finalize_tpl_candidate_type(
                            db,
                            ty.clone(),
                            WideningContext::ObjectProperty,
                            guard,
                        ),
                    )
                })
                .collect();
            let index_access = object
                .get_index_access()
                .iter()
                .map(|(key, value)| {
                    (
                        widen_type_with_context(
                            key.clone(),
                            WideningContext::ObjectProperty,
                            guard,
                        ),
                        finalize_tpl_candidate_type(
                            db,
                            value.clone(),
                            WideningContext::ObjectProperty,
                            guard,
                        ),
                    )
                })
                .collect();
            LuaType::Object(LuaObjectType::new_with_fields(fields, index_access).into())
        }
        LuaType::Array(array) => {
            let element_context = match context {
                WideningContext::TupleElement => WideningContext::TupleElement,
                _ => WideningContext::ArrayElement,
            };
            let base =
                finalize_tpl_candidate_type(db, array.get_base().clone(), element_context, guard);
            LuaType::Array(LuaArrayType::new(base, array.get_len().clone()).into())
        }
        LuaType::Tuple(tuple) => {
            let types = tuple
                .get_types()
                .iter()
                .cloned()
                .map(|ty| finalize_tpl_candidate_type(db, ty, WideningContext::TupleElement, guard))
                .collect();
            LuaType::Tuple(LuaTupleType::new(types, tuple.status).into())
        }
        LuaType::Union(union) => {
            let member_context = if matches!(context, WideningContext::Root) {
                WideningContext::Root
            } else {
                WideningContext::UnionMember
            };
            LuaType::Union(
                LuaUnionType::from_vec(
                    union
                        .into_vec()
                        .into_iter()
                        .map(|ty| finalize_tpl_candidate_type(db, ty, member_context, guard))
                        .collect(),
                )
                .into(),
            )
        }
        ty => widen_type_with_context(ty, context, guard),
    };

    guard.leave_level();
    widened
}

pub fn widen_type_with_context(
    ty: LuaType,
    context: WideningContext,
    guard: &mut WideningGuard,
) -> LuaType {
    if !guard.enter_level() {
        return widen_literals_with_context(ty, context);
    }

    let ty = widen_literals_with_context(ty, context);

    let widened = match ty {
        LuaType::Array(array) => {
            let element_context = match context {
                WideningContext::TupleElement => WideningContext::TupleElement,
                _ => WideningContext::ArrayElement,
            };
            let base = widen_type_with_context(array.get_base().clone(), element_context, guard);
            LuaType::Array(LuaArrayType::new(base, array.get_len().clone()).into())
        }
        LuaType::Tuple(tuple) => {
            let types = tuple
                .get_types()
                .iter()
                .cloned()
                .map(|ty| widen_type_with_context(ty, WideningContext::TupleElement, guard))
                .collect();
            LuaType::Tuple(LuaTupleType::new(types, tuple.status).into())
        }
        LuaType::Object(object) => {
            let fields = object
                .get_fields()
                .iter()
                .map(|(key, ty)| {
                    (
                        key.clone(),
                        widen_type_with_context(ty.clone(), WideningContext::ObjectProperty, guard),
                    )
                })
                .collect();
            let index_access = object
                .get_index_access()
                .iter()
                .map(|(key, value)| {
                    (
                        widen_type_with_context(
                            key.clone(),
                            WideningContext::ObjectProperty,
                            guard,
                        ),
                        widen_type_with_context(
                            value.clone(),
                            WideningContext::ObjectProperty,
                            guard,
                        ),
                    )
                })
                .collect();
            LuaType::Object(LuaObjectType::new_with_fields(fields, index_access).into())
        }
        LuaType::Union(union) => {
            let member_context = if matches!(context, WideningContext::Root) {
                WideningContext::Root
            } else {
                WideningContext::UnionMember
            };
            LuaType::Union(
                LuaUnionType::from_vec(
                    union
                        .into_vec()
                        .into_iter()
                        .map(|ty| widen_type_with_context(ty, member_context, guard))
                        .collect(),
                )
                .into(),
            )
        }
        LuaType::MultiLineUnion(multi) => LuaType::MultiLineUnion(
            crate::LuaMultiLineUnion::new(
                multi
                    .get_unions()
                    .iter()
                    .map(|(ty, description)| {
                        (
                            widen_type_with_context(
                                ty.clone(),
                                WideningContext::UnionMember,
                                guard,
                            ),
                            description.clone(),
                        )
                    })
                    .collect(),
            )
            .into(),
        ),
        LuaType::Intersection(intersection) => LuaType::Intersection(
            crate::LuaIntersectionType::new(
                intersection
                    .get_types()
                    .iter()
                    .cloned()
                    .map(|ty| widen_type_with_context(ty, WideningContext::UnionMember, guard))
                    .collect(),
            )
            .into(),
        ),
        LuaType::Variadic(variadic) => LuaType::Variadic(
            match variadic.deref() {
                VariadicType::Base(base) => VariadicType::Base(widen_type_with_context(
                    base.clone(),
                    WideningContext::VariadicElement,
                    guard,
                )),
                VariadicType::Multi(types) => VariadicType::Multi(
                    types
                        .iter()
                        .cloned()
                        .map(|ty| {
                            widen_type_with_context(ty, WideningContext::VariadicElement, guard)
                        })
                        .collect(),
                ),
            }
            .into(),
        ),
        LuaType::Generic(generic) => LuaType::Generic(
            LuaGenericType::new(
                generic.get_base_type_id(),
                generic
                    .get_params()
                    .iter()
                    .cloned()
                    .map(|ty| widen_type_with_context(ty, WideningContext::Root, guard))
                    .collect(),
            )
            .into(),
        ),
        LuaType::TableGeneric(params) => LuaType::TableGeneric(
            params
                .iter()
                .cloned()
                .map(|ty| widen_type_with_context(ty, WideningContext::Root, guard))
                .collect::<Vec<_>>()
                .into(),
        ),
        LuaType::DocFunction(func) => LuaType::DocFunction(
            LuaFunctionType::new(
                func.get_async_state(),
                func.is_colon_define(),
                func.is_variadic(),
                func.get_params()
                    .iter()
                    .map(|(name, ty)| {
                        (
                            name.clone(),
                            ty.clone().map(|ty| {
                                widen_type_with_context(ty, WideningContext::Root, guard)
                            }),
                        )
                    })
                    .collect(),
                widen_type_with_context(func.get_ret().clone(), WideningContext::Root, guard),
            )
            .into(),
        ),
        LuaType::TypeGuard(type_guard) => LuaType::TypeGuard(
            widen_type_with_context(type_guard.deref().clone(), WideningContext::Root, guard)
                .into(),
        ),
        LuaType::Conditional(conditional) => LuaType::Conditional(
            LuaConditionalType::new(
                widen_type_with_context(
                    conditional.get_checked_type().clone(),
                    WideningContext::Root,
                    guard,
                ),
                widen_type_with_context(
                    conditional.get_extends_type().clone(),
                    WideningContext::Root,
                    guard,
                ),
                widen_type_with_context(
                    conditional.get_true_type().clone(),
                    WideningContext::Root,
                    guard,
                ),
                widen_type_with_context(
                    conditional.get_false_type().clone(),
                    WideningContext::Root,
                    guard,
                ),
                conditional.get_infer_params().to_vec(),
                conditional.has_new,
            )
            .into(),
        ),
        LuaType::Mapped(mapped) => LuaType::Mapped(Arc::new(LuaMappedType::new(
            (
                mapped.param.0,
                GenericParam::new(
                    mapped.param.1.name.clone(),
                    mapped
                        .param
                        .1
                        .type_constraint
                        .clone()
                        .map(|ty| widen_type_with_context(ty, WideningContext::Root, guard)),
                    mapped
                        .param
                        .1
                        .default_type
                        .clone()
                        .map(|ty| widen_type_with_context(ty, WideningContext::Root, guard)),
                    mapped.param.1.attributes.clone(),
                ),
            ),
            widen_type_with_context(mapped.value.clone(), WideningContext::Root, guard),
            mapped.is_readonly,
            mapped.is_optional,
        ))),
        ty => ty,
    };

    guard.leave_level();
    widened
}

fn widen_literals_with_context(ty: LuaType, context: WideningContext) -> LuaType {
    match context {
        WideningContext::Root => ty,
        _ => widen_literal_type(ty),
    }
}

fn widen_literal_type(ty: LuaType) -> LuaType {
    match ty {
        LuaType::FloatConst(_) => LuaType::Number,
        LuaType::DocIntegerConst(_) | LuaType::IntegerConst(_) => LuaType::Integer,
        LuaType::DocStringConst(_) | LuaType::StringConst(_) => LuaType::String,
        LuaType::DocBooleanConst(_) | LuaType::BooleanConst(_) => LuaType::Boolean,
        ty => ty,
    }
}

fn table_const_to_object(
    db: &DbIndex,
    table_id: InFiled<TextRange>,
    guard: &mut WideningGuard,
) -> Option<LuaType> {
    let owner = LuaMemberOwner::Element(table_id.clone());
    let members = db.get_member_index().get_members(&owner)?;
    if !guard.enter_table(&table_id) {
        return Some(LuaType::Table);
    }
    let mut fields = HashMap::new();
    let mut index_access = Vec::new();

    for member in members {
        let value = db
            .get_type_index()
            .get_type_cache(&member.get_id().into())
            .map(|cache| cache.as_type().clone())
            .unwrap_or(LuaType::Unknown);
        let value = finalize_tpl_candidate_type(db, value, WideningContext::ObjectProperty, guard);

        match member.get_key() {
            LuaMemberKey::Name(_) | LuaMemberKey::Integer(_) => {
                fields
                    .entry(member.get_key().clone())
                    .and_modify(|prev| {
                        *prev = TypeOps::Union.apply(db, prev, &value);
                    })
                    .or_insert(value);
            }
            LuaMemberKey::ExprType(key) => {
                index_access.push((
                    widen_type_with_context(key.clone(), WideningContext::ObjectProperty, guard),
                    value,
                ));
            }
            LuaMemberKey::None => {}
        }
    }

    guard.leave_table(&table_id);

    Some(LuaType::Object(
        LuaObjectType::new_with_fields(fields, index_access).into(),
    ))
}
